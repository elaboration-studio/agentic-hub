//! Deterministic content version of a suite-as-agent: the first 12 hex chars
//! of a SHA-256 over the suite definition (minus timestamps), the agent block,
//! the sorted capability ids, and every capability's source bytes. See
//! `docs/tech/modules/agent-bundles.md`.

use std::fs;
use std::path::Path;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::internal_hooks;
use crate::model::{CapabilityItem, SuiteDefinition};

/// Bump when the on-disk bundle layout changes, so every version moves.
pub const BUNDLE_LAYOUT_VERSION: u32 = 1;

/// Finder writes these into any folder it opens; they are never content.
pub(crate) const IGNORED_FILE: &str = ".DS_Store";

const VERSION_HEX_CHARS: usize = 12;

/// One suite capability ref resolved against a scan (`None` = not found).
#[derive(Debug, Clone)]
pub struct ResolvedCap<'a> {
    pub id: String,
    pub item: Option<&'a CapabilityItem>,
}

/// The suite's refs resolved against `items`, deduped and sorted by id. When
/// one id appears twice, the first ref that resolves wins.
pub fn resolve_caps<'a>(
    suite: &SuiteDefinition,
    items: &'a [CapabilityItem],
) -> Vec<ResolvedCap<'a>> {
    let mut out: Vec<ResolvedCap<'a>> = Vec::new();
    for r in &suite.capabilities {
        let item = items
            .iter()
            .find(|i| !internal_hooks::is_internal_item(i) && r.matches_item(i));
        match out.iter_mut().find(|c| c.id == r.cap) {
            Some(existing) => existing.item = existing.item.or(item),
            None => out.push(ResolvedCap {
                id: r.cap.clone(),
                item,
            }),
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn agent_version(suite: &SuiteDefinition, items: &[CapabilityItem]) -> String {
    let mut h = Sha256::new();
    frame(&mut h, "layout", &BUNDLE_LAYOUT_VERSION.to_be_bytes());
    let definition = json!({
        "id": suite.id,
        "name": suite.name,
        "description": suite.description,
        "agent": suite.agent,
    });
    frame(&mut h, "suite", canonical_json(&definition).as_bytes());
    for cap in resolve_caps(suite, items) {
        match cap.item {
            Some(item) => {
                frame(&mut h, "cap", cap.id.as_bytes());
                hash_source(&mut h, &item.source_path);
            }
            None => frame(&mut h, "cap", format!("missing:{}", cap.id).as_bytes()),
        }
    }
    format!("{:x}", h.finalize())
        .chars()
        .take(VERSION_HEX_CHARS)
        .collect()
}

/// Length-prefixed `tag` + `bytes`, so adjacent fields can never run together.
fn frame(h: &mut Sha256, tag: &str, bytes: &[u8]) {
    h.update((tag.len() as u64).to_be_bytes());
    h.update(tag.as_bytes());
    h.update((bytes.len() as u64).to_be_bytes());
    h.update(bytes);
}

/// JSON with object keys sorted at every depth, independent of serde_json's
/// map ordering feature.
fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let fields: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| format!("{}:{}", Value::String(k.clone()), canonical_json(v)))
                .collect();
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        other => other.to_string(),
    }
}

/// A capability's own path is followed (a symlinked skill folder is the
/// skill); anything inside it is not.
fn hash_source(h: &mut Sha256, path: &Path) {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => hash_dir(h, path, ""),
        Ok(_) => hash_file(h, path, ""),
        Err(_) => frame(h, "unreadable", b""),
    }
}

fn hash_dir(h: &mut Sha256, dir: &Path, rel: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        frame(h, "unreadable", rel.as_bytes());
        return;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != IGNORED_FILE)
        .collect();
    names.sort();
    for name in names {
        let path = dir.join(&name);
        let child = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = fs::read_link(&path)
                    .map(|t| t.to_string_lossy().into_owned())
                    .unwrap_or_default();
                frame(h, "link", child.as_bytes());
                frame(h, "target", target.as_bytes());
            }
            Ok(meta) if meta.is_dir() => {
                frame(h, "dir", child.as_bytes());
                hash_dir(h, &path, &child);
            }
            Ok(_) => hash_file(h, &path, &child),
            Err(_) => frame(h, "unreadable", child.as_bytes()),
        }
    }
}

fn hash_file(h: &mut Sha256, path: &Path, rel: &str) {
    frame(h, "file", rel.as_bytes());
    match fs::read(path) {
        Ok(bytes) => frame(h, "bytes", &bytes),
        Err(_) => frame(h, "unreadable", b""),
    }
}

#[cfg(test)]
#[path = "agent_version_tests.rs"]
mod tests;
