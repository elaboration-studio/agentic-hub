use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::*;
use crate::agent_spec::AgentSpec;
use crate::model::SuiteCapabilityRef;
use crate::scanner::scan;

pub(super) const CTO: &str = "cto-1";
pub(super) const BARE: &str = "bare-1";

pub(super) fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

pub(super) fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub(super) fn gh_only(id: &str) -> bool {
    id == "gh"
}

pub(super) fn set_mtime(path: &Path, t: SystemTime) {
    fs::File::open(path).unwrap().set_modified(t).unwrap();
}

fn suite(id: &str, name: &str, caps: &[&str], agent: Option<AgentSpec>) -> SuiteDefinition {
    SuiteDefinition {
        id: id.into(),
        name: name.into(),
        description: Some("delivery".into()),
        capabilities: caps.iter().map(|c| SuiteCapabilityRef::bare(*c)).collect(),
        is_base: false,
        agent,
        created_at: "t".into(),
        updated_at: "t".into(),
    }
}

/// A source root with every kind, a skill-name collision, and three suites:
/// the CTO agent, a bare suite, and a base suite the bundles must ignore.
pub(super) struct Fixture {
    pub dir: tempfile::TempDir,
    pub items: Vec<CapabilityItem>,
    pub suites: Vec<SuiteDefinition>,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        write(&src.join("skills/cto/tdd/SKILL.md"), "# cto tdd");
        write(&src.join("skills/cto/tdd/scripts/run.sh"), "echo run");
        write(&src.join("skills/dev/tdd/SKILL.md"), "# dev tdd");
        write(&src.join("skills/review/SKILL.md"), "# review");
        write(&src.join("agents/reviewer.md"), "# reviewer");
        write(&src.join("commands/ship.md"), "# ship");
        write(
            &src.join("rules/core.md"),
            "---\nname: core\n---\n\nBe precise.\n",
        );
        write(&src.join("rules/style.md"), "Short words.\n");
        write(
            &src.join("hooks/audit/hook.json"),
            r#"{ "id": "audit", "command": "${HOOK_DIR}/audit.sh", "timeout": 5,
                 "events": [{ "name": "PreToolUse", "matcher": "Bash" }, { "name": "Notification" }] }"#,
        );
        write(&src.join("hooks/audit/audit.sh"), "echo audit");
        write(
            &src.join("mcp/github/mcp.json"),
            r#"{ "$schema": "agentic-hub.mcp.v1", "name": "github", "transport": "stdio",
                 "command": "github-mcp", "args": ["stdio"], "env": ["GITHUB_TOKEN"] }"#,
        );

        let cto = suite(
            CTO,
            "Arno's CTO",
            &[
                "skill:cto/tdd",
                "skill:dev/tdd",
                "agent:reviewer.md",
                "command:ship.md",
                "rule:core.md",
                "rule:style.md",
                "hook:audit",
                "mcp:github",
                "skill:gone",
            ],
            Some(AgentSpec {
                emoji: Some("⚒️".into()),
                instructions: Some("Lead.".into()),
                required_clis: vec!["gh".into(), "nope".into()],
            }),
        );
        let bare = suite(BARE, "Bare", &["skill:cto/tdd"], None);
        let mut base = suite("base-1", "Base", &["skill:review"], None);
        base.is_base = true;

        let mut fixture = Fixture {
            dir,
            items: Vec::new(),
            suites: vec![cto, bare, base],
        };
        fixture.rescan();
        fixture
    }

    pub fn src(&self) -> PathBuf {
        self.dir.path().join("src")
    }

    pub fn bundles_root(&self) -> PathBuf {
        self.dir.path().join("home/.agentic-hub/bundles")
    }

    pub fn rescan(&mut self) {
        self.items = scan(&self.src()).items;
    }

    pub fn ctx(&self) -> BundleContext<'_> {
        BundleContext {
            bundles_root: self.bundles_root(),
            suites: &self.suites,
            items: &self.items,
            probe: &gh_only,
            now: SystemTime::now(),
            gc_max_age: GC_MAX_AGE,
        }
    }

    pub fn build(&self, suite_id: &str, harness: Harness) -> BundleManifest {
        build_bundle(&self.ctx(), suite_id, harness).unwrap()
    }
}

pub(super) fn skipped(m: &BundleManifest, capability: &str) -> Vec<String> {
    m.skipped
        .iter()
        .filter(|s| s.capability == capability)
        .map(|s| s.reason.clone())
        .collect()
}
