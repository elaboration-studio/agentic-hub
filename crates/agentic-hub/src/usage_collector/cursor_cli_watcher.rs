use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use agentic_core::model::ToolId;
use agentic_core::paths::home_dir;
use agentic_core::settings::Settings;
use agentic_core::usage_tracer_enabled;
use notify::{recommended_watcher, Event, RecursiveMode, Watcher};
use serde_json::{json, Value};

use crate::usage_attribution::AttributionState;

use super::persist_event_batch;

const DEBOUNCE: Duration = Duration::from_millis(150);
const MAX_APPEND_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct CursorCliPrompt {
    query: String,
    byte_offset: u64,
}

pub(super) fn start(attribution: Arc<AttributionState>) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    let root = cursor_projects_root();
    if !root.is_dir() {
        return;
    }
    thread::spawn(move || watcher_loop(root, attribution));
}

fn watcher_loop(root: PathBuf, attribution: Arc<AttributionState>) {
    let mut offsets = seed_offsets(&root);
    let (tx, rx) = mpsc::channel();
    let mut watcher = match recommended_watcher(move |result: notify::Result<Event>| {
        if result.as_ref().is_ok_and(relevant_event) {
            let _ = tx.send(());
        }
    }) {
        Ok(watcher) => watcher,
        Err(_) => return,
    };
    if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
        return;
    }
    sync_transcripts(&root, &mut offsets, &attribution);

    while rx.recv().is_ok() {
        loop {
            match rx.recv_timeout(DEBOUNCE) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        sync_transcripts(&root, &mut offsets, &attribution);
    }
}

fn relevant_event(event: &Event) -> bool {
    event.paths.iter().any(|path| {
        path.components()
            .any(|component| component.as_os_str() == "agent-transcripts")
    })
}

fn sync_transcripts(
    root: &Path,
    offsets: &mut HashMap<PathBuf, u64>,
    attribution: &AttributionState,
) {
    let enabled =
        Settings::load().is_ok_and(|settings| usage_tracer_enabled(&settings, ToolId::Cursor));
    for (path, conversation_id) in discover_transcripts(root) {
        let current_len = match fs::metadata(&path) {
            Ok(metadata) => metadata.len(),
            Err(_) => continue,
        };
        let offset = offsets.get(&path).copied().unwrap_or_default();
        if !enabled {
            offsets.insert(path, current_len);
            continue;
        }
        let Some(workspace) = cursor_cli_workspace(&conversation_id) else {
            continue;
        };
        let Ok((next_offset, prompts)) = read_appended_prompts(&path, offset) else {
            continue;
        };
        offsets.insert(path, next_offset);
        for prompt in prompts {
            let raw = json!({
                "hook_event_name": "UserPromptSubmit",
                "generation_id": format!(
                    "cursor-cli:{conversation_id}:{}",
                    prompt.byte_offset
                ),
                "workspace_roots": [workspace.to_string_lossy()],
                "prompt": prompt.query,
                "invocation_type": "cursor_cli_transcript",
            });
            let batch = attribution.normalize(&raw, "cursor");
            let _ = persist_event_batch(batch, "cursor");
        }
    }
}

fn seed_offsets(root: &Path) -> HashMap<PathBuf, u64> {
    discover_transcripts(root)
        .into_iter()
        .filter_map(|(path, _)| {
            fs::metadata(&path)
                .ok()
                .map(|metadata| (path, metadata.len()))
        })
        .collect()
}

fn discover_transcripts(root: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    let Ok(projects) = fs::read_dir(root) else {
        return found;
    };
    for project in projects.flatten().filter(entry_is_dir) {
        let transcript_root = project.path().join("agent-transcripts");
        let Ok(sessions) = fs::read_dir(transcript_root) else {
            continue;
        };
        for session in sessions.flatten().filter(entry_is_dir) {
            let conversation_id = session.file_name().to_string_lossy().into_owned();
            if !valid_component(&conversation_id) {
                continue;
            }
            let transcript = session.path().join(format!("{conversation_id}.jsonl"));
            if fs::symlink_metadata(&transcript)
                .is_ok_and(|metadata| metadata.file_type().is_file())
            {
                found.push((transcript, conversation_id));
            }
        }
    }
    found
}

fn entry_is_dir(entry: &fs::DirEntry) -> bool {
    entry.file_type().is_ok_and(|file_type| file_type.is_dir())
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn read_appended_prompts(path: &Path, offset: u64) -> io::Result<(u64, Vec<CursorCliPrompt>)> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let offset = offset.min(file_len);
    let append_len = file_len.saturating_sub(offset);
    if append_len > MAX_APPEND_BYTES {
        return Ok((file_len, Vec::new()));
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::with_capacity(usize::try_from(append_len).unwrap_or_default());
    file.take(MAX_APPEND_BYTES + 1).read_to_end(&mut bytes)?;
    let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') else {
        return Ok((offset, Vec::new()));
    };
    let complete = &bytes[..=last_newline];
    let mut prompts = Vec::new();
    let mut line_offset = offset;
    for line in complete.split_inclusive(|byte| *byte == b'\n') {
        let content = line.strip_suffix(b"\n").unwrap_or(line);
        if let Ok(value) = serde_json::from_slice::<Value>(content) {
            if value.get("role").and_then(Value::as_str) == Some("user") {
                if let Some(text) = cursor_cli_message_text(&value) {
                    if let Some(query) = extract_user_query(text) {
                        prompts.push(CursorCliPrompt {
                            query: query.to_string(),
                            byte_offset: line_offset,
                        });
                    }
                }
            }
        }
        line_offset = line_offset.saturating_add(line.len() as u64);
    }
    Ok((offset.saturating_add(complete.len() as u64), prompts))
}

fn cursor_cli_message_text(value: &Value) -> Option<&str> {
    value
        .get("message")?
        .get("content")?
        .as_array()?
        .iter()
        .find(|block| block.get("type").and_then(Value::as_str) == Some("text"))?
        .get("text")?
        .as_str()
}

fn extract_user_query(text: &str) -> Option<&str> {
    const OPEN: &str = "<user_query>";
    const CLOSE: &str = "</user_query>";
    if let Some(start) = text.find(OPEN) {
        let content = &text[start + OPEN.len()..];
        let end = content.find(CLOSE)?;
        return non_empty(content[..end].trim());
    }
    if text.contains("<manually_attached_skills>") {
        return None;
    }
    non_empty(text.trim())
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn cursor_cli_workspace(conversation_id: &str) -> Option<PathBuf> {
    cursor_cli_workspace_at(&home_dir().join(".cursor").join("chats"), conversation_id)
}

fn cursor_cli_workspace_at(chats: &Path, conversation_id: &str) -> Option<PathBuf> {
    if !valid_component(conversation_id) {
        return None;
    }
    for group in fs::read_dir(chats).ok()?.flatten().filter(entry_is_dir) {
        let meta = group.path().join(conversation_id).join("meta.json");
        if !fs::symlink_metadata(&meta)
            .is_ok_and(|metadata| metadata.file_type().is_file() && metadata.len() <= 64 * 1024)
        {
            continue;
        }
        let value: Value = serde_json::from_slice(&fs::read(meta).ok()?).ok()?;
        let cwd = value.get("cwd").and_then(Value::as_str)?.trim();
        if !cwd.is_empty() {
            return Some(PathBuf::from(cwd));
        }
    }
    None
}

fn cursor_projects_root() -> PathBuf {
    home_dir().join(".cursor").join("projects")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn cursor_cli_wrapper_extracts_only_the_user_query() {
        let wrapped = "<manually_attached_skills>\nExample /not-a-real-invocation\n</manually_attached_skills>\n<user_query>\n/mr-content explain this\n</user_query>";

        assert_eq!(
            extract_user_query(wrapped),
            Some("/mr-content explain this")
        );
    }

    #[test]
    fn appended_cursor_cli_turn_is_emitted_once_from_its_byte_offset() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("session.jsonl");
        fs::write(
            &transcript,
            "{\"role\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"old\"}]}}\n",
        )
        .unwrap();
        let offset = fs::metadata(&transcript).unwrap().len();
        fs::write(
            &transcript,
            "{\"role\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"old\"}]}}\n{\"role\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"<user_query>\\n/mr-content explain this\\n</user_query>\"}]}}\n",
        )
        .unwrap();

        let (next_offset, prompts) = read_appended_prompts(&transcript, offset).unwrap();

        assert_eq!(
            prompts,
            vec![CursorCliPrompt {
                query: "/mr-content explain this".to_string(),
                byte_offset: offset,
            }]
        );
        assert_eq!(next_offset, fs::metadata(&transcript).unwrap().len());
    }

    #[test]
    fn cursor_cli_workspace_requires_matching_cli_chat_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let conversation_id = "44ddb24f-2b31-4b0e-aa09-22ca21a10b41";
        let meta_dir = dir.path().join("profile").join(conversation_id);
        fs::create_dir_all(&meta_dir).unwrap();
        fs::write(
            meta_dir.join("meta.json"),
            r#"{"cwd":"/Users/example/project"}"#,
        )
        .unwrap();

        assert_eq!(
            cursor_cli_workspace_at(dir.path(), conversation_id),
            Some(std::path::PathBuf::from("/Users/example/project"))
        );
    }
}
