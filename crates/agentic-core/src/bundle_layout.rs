//! Writes one bundle's contents into a staging dir. Paths recorded in the
//! returned manifest point at the final `root`, which the staging dir becomes
//! through a single rename (see [`crate::bundle`]).

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::agent_version::{ResolvedCap, IGNORED_FILE};
use crate::bundle::{
    BundleManifest, BundleMcpServer, BundleSkill, Harness, Mount, RequiredCli, SkippedCapability,
};
use crate::hook_sync;
use crate::mcp;
use crate::model::{CapabilityItem, CapabilityKind, SuiteDefinition};
use crate::rule_sync::strip_frontmatter;
use crate::settings::slugify;

const PLUGIN_DIR: &str = "plugin";
const INSTRUCTIONS_FILE: &str = "instructions.md";

pub(crate) struct LayoutInput<'a> {
    /// Staging dir being written; becomes `root` on success.
    pub stage: &'a Path,
    pub root: &'a Path,
    pub suite: &'a SuiteDefinition,
    pub version: &'a str,
    pub harness: Harness,
    pub caps: &'a [ResolvedCap<'a>],
    pub probe: &'a (dyn Fn(&str) -> bool + Sync),
}

pub(crate) fn required_clis(
    suite: &SuiteDefinition,
    probe: &(dyn Fn(&str) -> bool + Sync),
) -> Vec<RequiredCli> {
    let mut out: Vec<RequiredCli> = Vec::new();
    for id in suite.agent.iter().flat_map(|a| &a.required_clis) {
        if out.iter().all(|c| &c.id != id) {
            out.push(RequiredCli {
                id: id.clone(),
                installed: probe(id),
            });
        }
    }
    out
}

pub(crate) fn write_layout(input: &LayoutInput<'_>) -> io::Result<BundleManifest> {
    let mut w = Writer::new(input);
    for cap in input.caps {
        match cap.item {
            None => w.skip(&cap.id, "not found"),
            Some(item) if !item.valid => w.skip(&cap.id, &item.validation_errors.join("; ")),
            Some(item) => w.add(&cap.id, item),
        }
    }
    w.finish()
}

struct Writer<'a> {
    input: &'a LayoutInput<'a>,
    plugin: bool,
    /// Where `skills/` (and in plugin mode every other dir) lives, relative to root.
    content: PathBuf,
    claimed: HashSet<(CapabilityKind, String)>,
    rules: Vec<(String, String)>,
    hook_entries: Vec<(String, Value)>,
    manifest: BundleManifest,
}

impl<'a> Writer<'a> {
    fn new(input: &'a LayoutInput<'a>) -> Self {
        let mount = input.harness.mount();
        let plugin = mount == Mount::Plugin;
        let content = if plugin {
            PathBuf::from(PLUGIN_DIR)
        } else {
            PathBuf::new()
        };
        let manifest = BundleManifest {
            agent_id: input.suite.id.clone(),
            name: input.suite.name.clone(),
            version: input.version.to_string(),
            harness: input.harness,
            mount,
            root: input.root.to_path_buf(),
            plugin_dir: plugin.then(|| input.root.join(PLUGIN_DIR)),
            instructions_file: None,
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            required_clis: Vec::new(),
            skipped: Vec::new(),
        };
        Writer {
            input,
            plugin,
            content,
            claimed: HashSet::new(),
            rules: Vec::new(),
            hook_entries: Vec::new(),
            manifest,
        }
    }

    fn skip(&mut self, capability: &str, reason: &str) {
        self.manifest.skipped.push(SkippedCapability {
            capability: capability.to_string(),
            reason: reason.to_string(),
        });
    }

    /// First claimant of a leaf name per kind wins; caps arrive in id order.
    fn claim(&mut self, id: &str, kind: CapabilityKind, leaf: &str) -> bool {
        let won = self.claimed.insert((kind, leaf.to_string()));
        if !won {
            self.skip(id, "name collision");
        }
        won
    }

    fn add(&mut self, id: &str, item: &CapabilityItem) {
        match item.kind {
            CapabilityKind::Skill => self.add_skill(id, item),
            CapabilityKind::Agent => self.add_plugin_file(id, item, "agents", "agent"),
            CapabilityKind::Command => self.add_plugin_file(id, item, "commands", "command"),
            CapabilityKind::Rule => self.add_rule(id, item),
            CapabilityKind::Hook => self.add_hook(id, item),
            CapabilityKind::Mcp => self.add_mcp(id, item),
        }
    }

    fn add_skill(&mut self, id: &str, item: &CapabilityItem) {
        let leaf = leaf_name(item);
        if !self.claim(id, item.kind, &leaf) {
            return;
        }
        let rel = self.content.join("skills").join(&leaf);
        if self.copy(id, &item.source_path, &rel) {
            self.manifest.skills.push(BundleSkill {
                name: leaf,
                path: self.input.root.join(rel),
            });
        }
    }

    fn add_plugin_file(&mut self, id: &str, item: &CapabilityItem, dir: &str, noun: &str) {
        if !self.plugin {
            self.skip(id, &format!("prompt mount has no {noun} support"));
            return;
        }
        let leaf = leaf_name(item);
        if self.claim(id, item.kind, &leaf) {
            let rel = self.content.join(dir).join(leaf);
            self.copy(id, &item.source_path, &rel);
        }
    }

    fn add_rule(&mut self, id: &str, item: &CapabilityItem) {
        match fs::read_to_string(&item.source_path) {
            Ok(text) => {
                let rel = id.strip_prefix("rule:").unwrap_or(id).to_string();
                self.rules.push((rel, strip_frontmatter(&text)));
            }
            Err(e) => self.skip(id, &format!("unreadable: {}", e.kind())),
        }
    }

    fn add_hook(&mut self, id: &str, item: &CapabilityItem) {
        if !self.plugin {
            self.skip(id, "prompt mount has no hook support");
            return;
        }
        let manifest = match hook_sync::load_manifest(&item.source_path) {
            Ok(m) => m,
            Err(e) => return self.skip(id, &e),
        };
        let tool = self.input.harness.tool();
        if !manifest.effective_targets().contains(&tool) {
            let reason = format!("hook does not target {}", self.input.harness.as_str());
            return self.skip(id, &reason);
        }
        let rel = self.content.join("hooks").join(&manifest.id);
        let mut notes = Vec::new();
        let entries = hook_sync::hook_entries(
            tool,
            &manifest,
            &self.input.root.join(&rel),
            None,
            &mut notes,
        );
        for note in notes {
            self.skip(id, &note);
        }
        if !entries.is_empty() && self.copy(id, &item.source_path, &rel) {
            self.hook_entries.extend(entries);
        }
    }

    fn add_mcp(&mut self, id: &str, item: &CapabilityItem) {
        let server = match mcp::load_manifest(&item.source_path) {
            Ok(s) => s,
            Err(e) => return self.skip(id, &e.to_string()),
        };
        if self.claim(id, item.kind, &server.name) {
            self.manifest.mcp_servers.push(BundleMcpServer {
                name: server.name,
                transport: server.transport,
                command: server.command,
                args: server.args,
                env_names: server.env,
                url: server.url,
            });
        }
    }

    /// Copy one capability; a failure skips it and removes the partial copy.
    fn copy(&mut self, id: &str, src: &Path, rel: &Path) -> bool {
        let dst = self.input.stage.join(rel);
        match copy_tree(src, &dst) {
            Ok(()) => true,
            Err(e) => {
                let _ = fs::remove_dir_all(&dst).or_else(|_| fs::remove_file(&dst));
                self.skip(id, &format!("copy failed: {}", e.kind()));
                false
            }
        }
    }

    fn finish(mut self) -> io::Result<BundleManifest> {
        let stage = self.input.stage;
        if self.plugin {
            let dir = match self.input.harness {
                Harness::Claude => ".claude-plugin",
                _ => ".cursor-plugin",
            };
            write_json(
                &stage.join(PLUGIN_DIR).join(dir).join("plugin.json"),
                &plugin_json(self.input.suite, self.input.version),
            )?;
        }
        if !self.hook_entries.is_empty() {
            let hooks = hooks_json(self.input.harness, std::mem::take(&mut self.hook_entries));
            write_json(&stage.join(&self.content).join("hooks/hooks.json"), &hooks)?;
        }
        let agent_text = self
            .input
            .suite
            .agent
            .as_ref()
            .and_then(|a| a.instructions.as_deref());
        if let Some(text) = compose_instructions(agent_text, &self.rules) {
            fs::write(stage.join(INSTRUCTIONS_FILE), text)?;
            self.manifest.instructions_file = Some(self.input.root.join(INSTRUCTIONS_FILE));
        }
        self.manifest.required_clis = required_clis(self.input.suite, self.input.probe);
        Ok(self.manifest)
    }
}

fn leaf_name(item: &CapabilityItem) -> String {
    item.relative_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| item.name.clone())
}

fn plugin_json(suite: &SuiteDefinition, version: &str) -> Value {
    let mut o = Map::new();
    o.insert(
        "name".into(),
        json!(format!("ehub-{}", slugify(&suite.name))),
    );
    o.insert("version".into(), json!(version));
    if let Some(description) = &suite.description {
        o.insert("description".into(), json!(description));
    }
    Value::Object(o)
}

fn hooks_json(harness: Harness, entries: Vec<(String, Value)>) -> Value {
    let mut by_event: Map<String, Value> = Map::new();
    for (event, entry) in entries {
        if let Value::Array(list) = by_event
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()))
        {
            list.push(entry);
        }
    }
    match harness {
        Harness::Cursor => json!({ "version": 1, "hooks": by_event }),
        _ => json!({ "hooks": by_event }),
    }
}

/// Agent instructions, then each rule behind a `<!-- rule:<id> -->` marker,
/// blank-line separated. `None` when there is nothing to say.
fn compose_instructions(agent: Option<&str>, rules: &[(String, String)]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(text) = agent.filter(|t| !t.trim().is_empty()) {
        parts.push(text.trim_end().to_string());
    }
    for (id, body) in rules {
        let marker = format!("<!-- rule:{id} -->");
        parts.push(if body.is_empty() {
            marker
        } else {
            format!("{marker}\n{body}")
        });
    }
    (!parts.is_empty()).then(|| format!("{}\n", parts.join("\n\n")))
}

fn write_json(path: &Path, value: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))
}

/// The capability's own path is followed. Inside it, a symlink to a file is
/// copied as that file's bytes; a symlink to a directory is dropped, so a
/// bundle never links back to, or loops through, the source.
fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    if fs::metadata(src)?.is_dir() {
        copy_dir(src, dst)
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst).map(|_| ())
    }
}

fn copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == IGNORED_FILE {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            if fs::metadata(&from).is_ok_and(|m| m.is_file()) {
                fs::copy(&from, &to)?;
            }
        } else if file_type.is_dir() {
            copy_dir(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_instructions_is_none_when_empty() {
        assert_eq!(compose_instructions(None, &[]), None);
        assert_eq!(compose_instructions(Some("  \n"), &[]), None);
    }

    #[test]
    fn compose_instructions_keeps_an_empty_rule_as_its_marker() {
        let rules = vec![("a.md".to_string(), String::new())];
        assert_eq!(
            compose_instructions(None, &rules).as_deref(),
            Some("<!-- rule:a.md -->\n")
        );
    }

    #[test]
    fn compose_instructions_keeps_leading_indentation_of_agent_text() {
        assert_eq!(
            compose_instructions(Some("    code\n\n"), &[]).as_deref(),
            Some("    code\n")
        );
    }
}
