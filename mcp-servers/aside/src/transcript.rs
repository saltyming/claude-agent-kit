//! Optional transcript forwarding.
//!
//! Claude Code stores each project's session log as a `<uuid>.jsonl` file
//! under `~/.claude/projects/{dashed-cwd}/` where `{dashed-cwd}` is the
//! absolute CWD with `/` replaced by `-` (same convention workslate uses for
//! its own state). When `include_transcript=true` we locate the most recent
//! file there and render a trimmed tail as plain text for the backend.
//!
//! This touches an internal Claude Code implementation detail — the JSONL
//! schema is not a documented stable API. The code degrades gracefully: a
//! missing directory, missing file, unparseable lines, or unknown content
//! shapes all produce a `TranscriptUnavailable` or a best-effort partial
//! render rather than an error. The caller's `question` and `context` always
//! reach the backend even when transcript rendering fails.

use std::path::{Path, PathBuf};

use serde_json::Value;

const DEFAULT_TAIL: usize = 80;
const MAX_RENDERED_BYTES: usize = 100 * 1024; // 100 KB

/// Result of attempting to build a transcript section for the prompt.
pub enum TranscriptOutcome {
    /// Transcript rendered successfully. `rendered` is the plain text to
    /// splice into the prompt; a `[transcript truncated: ...]` header is
    /// already prepended when trimming occurred.
    Ok { rendered: String },
    /// No transcript directory / file found for this project. The caller
    /// should proceed without transcript and surface the reason so the user
    /// can pass `context` explicitly if they need it.
    Unavailable(String),
}

/// Compute the directory Claude Code uses for this project's state, given
/// the current working directory.
pub fn project_state_dir(cwd: &Path, home: &Path) -> PathBuf {
    let dashed = cwd.to_string_lossy().replace('/', "-");
    home.join(".claude").join("projects").join(dashed)
}

/// Locate the newest `.jsonl` file in `dir`, by modification time.
fn newest_jsonl(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        match &best {
            None => best = Some((path, mtime)),
            Some((_, prev)) if mtime > *prev => best = Some((path, mtime)),
            _ => {}
        }
    }
    best.map(|(p, _)| p)
}

/// Render a transcript file's tail as plain text suitable for prompt
/// inclusion. `tail` defaults to `DEFAULT_TAIL` when `None`.
///
/// Byte budget: after rendering, if the result exceeds `MAX_RENDERED_BYTES`,
/// messages are dropped from the **front** (keeping the most recent) and a
/// `[transcript truncated: kept last K of M messages]` header is prepended.
pub fn render_transcript(cwd: &Path, home: &Path, tail: Option<u32>) -> TranscriptOutcome {
    let dir = project_state_dir(cwd, home);
    if !dir.exists() {
        return TranscriptOutcome::Unavailable(format!(
            "no transcript directory at {} — is this a Claude Code project?",
            dir.display()
        ));
    }
    let path = match newest_jsonl(&dir) {
        Some(p) => p,
        None => {
            return TranscriptOutcome::Unavailable(format!(
                "no .jsonl files in {}",
                dir.display()
            ));
        }
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return TranscriptOutcome::Unavailable(format!(
                "cannot read {}: {}",
                path.display(),
                e
            ));
        }
    };

    let want_tail = tail.map(|n| n as usize).unwrap_or(DEFAULT_TAIL).max(1);

    let messages: Vec<String> = content
        .lines()
        .filter_map(render_entry)
        .collect();
    let total = messages.len();
    if total == 0 {
        return TranscriptOutcome::Unavailable(format!(
            "transcript at {} has no renderable messages",
            path.display()
        ));
    }

    let start = total.saturating_sub(want_tail);
    let mut kept: Vec<&str> = messages[start..].iter().map(|s| s.as_str()).collect();
    let mut kept_count = kept.len();

    // Render, and byte-budget trim from the front if needed.
    let mut rendered = kept.join("\n\n");
    let mut trimmed = false;
    while rendered.len() > MAX_RENDERED_BYTES && kept.len() > 1 {
        kept.remove(0);
        kept_count -= 1;
        rendered = kept.join("\n\n");
        trimmed = true;
    }

    if trimmed || kept_count < total {
        let header = format!(
            "[transcript truncated: kept last {} of {} messages]",
            kept_count, total
        );
        rendered = format!("{}\n\n{}", header, rendered);
    }

    TranscriptOutcome::Ok { rendered }
}

/// Render one JSONL entry into a short plain-text block, or `None` if it
/// doesn't look like a message (e.g. summary / system entries).
fn render_entry(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type")?.as_str()?;
    match ty {
        "user" | "assistant" => {
            let msg = v.get("message")?;
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or(ty);
            let content = msg.get("content")?;
            let body = render_content(content)?;
            if body.trim().is_empty() {
                return None;
            }
            Some(format!("[{}] {}", role, body))
        }
        _ => None,
    }
}

fn render_content(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let mut parts: Vec<String> = Vec::new();
            for item in items {
                if let Some(s) = item.as_str() {
                    parts.push(s.to_string());
                    continue;
                }
                let ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match ty {
                    "text" => {
                        if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                            parts.push(t.to_string());
                        }
                    }
                    "tool_use" => {
                        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                        parts.push(format!("[tool_use: {}]", name));
                    }
                    "tool_result" => {
                        parts.push("[tool_result]".to_string());
                    }
                    "thinking" => {
                        // Don't leak thinking blocks to external advisors.
                        parts.push("[thinking]".to_string());
                    }
                    _ => {}
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}
