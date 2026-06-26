//! Deterministic prompt rendering from the structured task spec.
//!
//! Both the rendered prompt (sent to the backend on stdin) and a JSON capture of
//! the structured fields (`spec_json`, stored for audit / re-render) are produced
//! here, so the prompt a delegated run actually received is always reconstructable.

use serde_json::json;

use crate::params::SubmitParams;

/// Short execution framing so the backend treats the body as a task to implement
/// (it runs write-capable), not a question to answer.
const PREAMBLE: &str = "You are an autonomous coding agent delegated ONE self-contained task. \
Work in the current directory, make the changes required to satisfy the task, and keep your \
edits within the stated scope. When finished, end with a short summary of what you changed.";

/// Compose the structured spec into a single codex prompt. Sections are emitted
/// only when present, in a stable order. The `nonce` is appended as a trailing marker
/// so dispatch can positively identify the rollout this task produces.
pub fn render_prompt(p: &SubmitParams, nonce: &str) -> String {
    let mut parts: Vec<String> = vec![PREAMBLE.to_string()];
    parts.push(format!("# Objective\n\n{}", p.objective.trim()));
    push_list(&mut parts, "Target files", &p.target_files);
    push_list(&mut parts, "Constraints", &p.constraints);
    push_list(&mut parts, "Acceptance criteria", &p.acceptance);
    push_text(&mut parts, "Context", p.context.as_deref());
    push_text(&mut parts, "Details", p.details.as_deref());
    // Identity marker: codex records the prompt verbatim as a `user_message`, so this
    // lets `rollout::locate_by_nonce` match the rollout this task produced even when a
    // sibling codex (e.g. aside) shares the cwd. One unobtrusive trailing line.
    parts.push(nonce_marker(nonce));
    parts.join("\n\n")
}

/// The prompt marker carrying the per-task nonce. `rollout::locate_by_nonce` matches
/// the bare `nonce` as a substring, so the brackets are only for human legibility.
pub fn nonce_marker(nonce: &str) -> String {
    format!("[dispatch-task: {nonce}]")
}

/// Emit a bulleted `# <title>` section when the list is present and non-empty.
fn push_list(parts: &mut Vec<String>, title: &str, items: &Option<Vec<String>>) {
    if let Some(items) = items
        && !items.is_empty()
    {
        let body = items
            .iter()
            .map(|i| format!("- {}", i.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!("# {title}\n\n{body}"));
    }
}

/// Emit a free-form `# <title>` section when the text is present and non-blank.
fn push_text(parts: &mut Vec<String>, title: &str, text: Option<&str>) {
    if let Some(t) = text {
        let t = t.trim();
        if !t.is_empty() {
            parts.push(format!("# {title}\n\n{t}"));
        }
    }
}

/// Capture the structured fields as a JSON string for the `spec_json` audit column.
pub fn spec_json(p: &SubmitParams) -> String {
    serde_json::to_string(&json!({
        "objective": p.objective,
        "working_dir": p.working_dir,
        "title": p.title,
        "target_files": p.target_files,
        "constraints": p.constraints,
        "acceptance": p.acceptance,
        "context": p.context,
        "details": p.details,
    }))
    .unwrap_or_else(|_| "{}".to_string())
}
