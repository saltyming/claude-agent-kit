//! Hook subcommands for the doorbell + session-start bridge.
//!
//! Invoked from settings.json hooks:
//! - `workslate --hook=task` (PostToolUse): render the task-status footer AFTER a
//!   tool runs, so it reflects that tool's own effect (e.g. task_done shows done on
//!   the same call). Session-scoped, so it resolves from session_id alone.
//! - `workslate --hook=inbox` (PreToolUse): nudge BEFORE every tool call (and on
//!   calls that error or are denied, which PostToolUse skips) that unread messages
//!   await. Both resolve this Claude session via `session_context`, keyed by the
//!   composite (session_id, agent_id) identity, and print a
//!   `hookSpecificOutput.additionalContext` the agent sees on its next inference.
//! - `workslate --hook=session-start` (SessionStart): hand the agent its
//!   conversation session id so it can pass it to workslate_register/task_init
//!   — the MCP server's own env id does NOT match the hook's stdin session id.
//!
//! A doorbell must never break a tool call: every error path logs to stderr and
//! produces empty stdout (exit 0 = no injection).

use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::resolve_tasks_dir;
use crate::task::{load_tasks, render_task_footer};

#[derive(Clone, Copy)]
pub enum HookMode {
    Task,
    Inbox,
    SessionStart,
    SubagentStart,
}

/// Detect the `--hook=task` / `--hook=inbox` flag in argv.
pub fn parse_hook_mode(args: &[String]) -> Option<HookMode> {
    args.iter().find_map(|a| match a.as_str() {
        "--hook=task" => Some(HookMode::Task),
        "--hook=inbox" => Some(HookMode::Inbox),
        "--hook=session-start" => Some(HookMode::SessionStart),
        "--hook=subagent-start" => Some(HookMode::SubagentStart),
        _ => None,
    })
}

/// Run a doorbell hook. Prints at most one JSON object to stdout and never
/// surfaces an error to the caller — a failed doorbell must not block a tool.
pub fn run(mode: HookMode) {
    match try_run(mode) {
        Ok(Some(ctx)) => print_additional_context(event_name(mode), &ctx),
        Ok(None) => {}
        Err(e) => eprintln!("workslate hook error: {e}"),
    }
}

fn event_name(mode: HookMode) -> &'static str {
    match mode {
        HookMode::SessionStart => "SessionStart",
        HookMode::SubagentStart => "SubagentStart",
        // Task footer fires on PostToolUse so it reflects the just-run tool's own
        // effect; the inbox nudge stays on PreToolUse (must fire on every call,
        // including errors/denials, which PostToolUse skips).
        HookMode::Task => "PostToolUse",
        HookMode::Inbox => "PreToolUse",
    }
}

fn try_run(mode: HookMode) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let v: serde_json::Value = serde_json::from_str(&input)?;

    let session_id = v.get("session_id").and_then(|x| x.as_str()).unwrap_or("");
    if session_id.is_empty() {
        return Ok(None);
    }

    // agent_id is present only when the hook fires inside a subagent; empty for the
    // main session. It is the second half of the (session_id, agent_id) identity.
    let agent_id = v.get("agent_id").and_then(|x| x.as_str()).unwrap_or("");

    if let HookMode::SessionStart = mode {
        // Hand the agent its conversation session id so it can pass it to
        // workslate_task_init / workslate_register. No DB lookup needed.
        return Ok(Some(format!(
            "[workslate] session_id=`{session_id}` — to enable team task/inbox doorbells, \
             pass session_id=\"{session_id}\" to workslate_task_init and workslate_register."
        )));
    }

    if let HookMode::SubagentStart = mode {
        // Subagents do NOT fire SessionStart and share the parent's session_id, so
        // agent_id (subagent-only) is their identity discriminator. Hand them both ids
        // so they can pass them to workslate_task_init / workslate_register.
        return Ok(Some(format!(
            "[workslate] agent_id=`{agent_id}` session_id=`{session_id}` — you are a \
             subagent; pass BOTH agent_id=\"{agent_id}\" and session_id=\"{session_id}\" to \
             workslate_task_init and workslate_register to enable team task/inbox doorbells."
        )));
    }

    let cwd = v.get("cwd").and_then(|x| x.as_str()).unwrap_or(".");

    // Anchor on the same path logic the server uses so both resolve to the same
    // database (CLAUDE_PROJECT_DIR, with the hook-input cwd as fallback).
    let db_path = resolve_tasks_dir(std::path::Path::new(cwd)).join("workslate.db");
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;

    let exact: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT task_session, role FROM session_context \
             WHERE claude_session_id = ? AND agent_id = ?",
            rusqlite::params![session_id, agent_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    match mode {
        // Inbox is role-specific: it needs the exact (session_id, agent_id) row to
        // know whose unread to show. It runs on PreToolUse, whose stdin carries
        // agent_id, so the exact lookup resolves.
        HookMode::Inbox => {
            let (task_session, role) = match exact {
                Some(t) => t,
                None => return Ok(None),
            };
            Ok(inbox_doorbell(&conn, &task_session, role.as_deref()))
        }
        // Task is session-scoped: the footer shows the session's tasks regardless of
        // role. It runs on PostToolUse, whose stdin may omit agent_id, so fall back
        // from the exact row to the unique task_session for this claude_session_id
        // (render nothing if there are none or they disagree — never guess).
        HookMode::Task => {
            let task_session = match exact {
                Some((ts, _)) => ts,
                None => match session_scoped_task_session(&conn, session_id) {
                    Some(ts) => ts,
                    None => return Ok(None),
                },
            };
            Ok(task_doorbell(&conn, &task_session))
        }
        // SessionStart / SubagentStart return early above (before the DB lookup).
        HookMode::SessionStart | HookMode::SubagentStart => Ok(None),
    }
}

/// Resolve the task_session for a Claude session id when the exact
/// `(session_id, agent_id)` row is absent — e.g. a `PostToolUse` stdin that omits
/// `agent_id`. Returns the single task_session shared by that session id's rows,
/// or `None` if there are none, or more than one distinct task_session (ambiguous;
/// the task footer renders nothing rather than guess the wrong session).
fn session_scoped_task_session(conn: &Connection, session_id: &str) -> Option<String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT task_session FROM session_context WHERE claude_session_id = ?")
        .ok()?;
    let mut rows: Vec<String> = stmt
        .query_map(rusqlite::params![session_id], |r| r.get(0))
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    if rows.len() == 1 {
        rows.pop()
    } else {
        None
    }
}

/// Task-status doorbell: the same footer the MCP server used to append, now
/// surfaced on every tool call rather than only on workslate's own tools.
fn task_doorbell(conn: &Connection, task_session: &str) -> Option<String> {
    let tasks = load_tasks(conn, task_session, None).ok()?;
    if tasks.is_empty() {
        return None;
    }
    let footer = render_task_footer(&tasks, task_session, &[]);
    if footer.is_empty() {
        None
    } else {
        Some(footer)
    }
}

/// Inbox doorbell: one line nudging the agent to read unread messages. Repeats
/// every tool call until workslate_inbox_read clears them.
fn inbox_doorbell(conn: &Connection, task_session: &str, role: Option<&str>) -> Option<String> {
    let role = role?;
    let mut stmt = conn
        .prepare(
            "SELECT subject, urgent FROM messages \
             WHERE task_session = ? AND recipient_role = ? AND read_at IS NULL \
             ORDER BY id DESC",
        )
        .ok()?;
    let rows: Vec<(String, i64)> = stmt
        .query_map(rusqlite::params![task_session, role], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .ok()?
        .filter_map(|r| r.ok())
        .collect();
    if rows.is_empty() {
        return None;
    }
    let count = rows.len();
    let any_urgent = rows.iter().any(|(_, u)| *u != 0);
    let latest = &rows[0].0;
    let flag = if any_urgent { "🚨 URGENT — " } else { "" };
    Some(format!(
        "📨 {flag}{count} unread message(s) for role \"{role}\". \
         Latest: \"{latest}\". Read now with workslate_inbox_read(role=\"{role}\")."
    ))
}

fn print_additional_context(event_name: &str, ctx: &str) {
    let out = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": event_name,
            "additionalContext": ctx,
        }
    });
    println!("{out}");
}

// ── settings.json hook registration ──────────────────────

/// Path to the user's Claude Code settings file (`~/.claude/settings.json`).
fn settings_path() -> Option<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    Some(PathBuf::from(home).join(".claude").join("settings.json"))
}

/// Absolute path of this binary, used as the hook command so PreToolUse hooks
/// run without depending on PATH. Falls back to the bare name.
fn workslate_bin() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| "workslate".to_string())
}

fn read_settings(path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => Ok(serde_json::from_str(&s)?),
        _ => Ok(serde_json::json!({})),
    }
}

fn write_settings(path: &Path, root: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(root)?))?;
    Ok(())
}

/// True if a single hook handler is one of ours (a workslate doorbell command).
/// Matches by content (not exact path) so re-install after a path change still
/// replaces the old entry rather than duplicating it.
fn hook_is_ours(handler: &serde_json::Value) -> bool {
    handler
        .get("command")
        .and_then(|c| c.as_str())
        .map(|c| c.contains("workslate") && c.contains("--hook="))
        .unwrap_or(false)
}

/// Remove only our doorbell handlers from each PreToolUse matcher-group,
/// dropping a group only once it has no handlers left. This preserves unrelated
/// user hooks that happen to share a matcher-group with ours.
fn strip_our_hooks(pre: &mut Vec<serde_json::Value>) {
    pre.retain_mut(
        |group| match group.get_mut("hooks").and_then(|h| h.as_array_mut()) {
            Some(handlers) => {
                handlers.retain(|h| !hook_is_ours(h));
                !handlers.is_empty()
            }
            None => true,
        },
    );
}

/// All hook event arrays workslate manages in settings.json.
const WS_HOOK_EVENTS: &[&str] = &["PreToolUse", "PostToolUse", "SessionStart", "SubagentStart"];

/// Ensure `root.hooks.<event>` is an array and return a mutable handle to it.
fn event_array<'a>(
    root: &'a mut serde_json::Value,
    event: &str,
) -> Result<&'a mut Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let obj = root
        .as_object_mut()
        .ok_or("settings.json root is not a JSON object")?;
    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or("settings.json 'hooks' is not an object")?;
    let arr = hooks_obj
        .entry(event.to_string())
        .or_insert_with(|| serde_json::json!([]));
    arr.as_array_mut()
        .ok_or_else(|| format!("settings.json 'hooks.{event}' is not an array").into())
}

/// Install workslate's hooks into settings.json (idempotent): the PreToolUse inbox
/// doorbell, the PostToolUse task-status doorbell, and the SessionStart/SubagentStart
/// bridges. Existing user hooks are preserved — only workslate's own handlers are replaced.
pub fn install_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let path = settings_path().ok_or("could not resolve HOME for settings.json")?;
    let bin = workslate_bin();
    let mut root = read_settings(&path)?;
    {
        // Inbox doorbell on PreToolUse: a nudge that must fire before every tool call
        // AND on calls that error or are denied (PostToolUse fires only on success),
        // so a waiting message is never missed.
        let pre = event_array(&mut root, "PreToolUse")?;
        strip_our_hooks(pre);
        pre.push(serde_json::json!({
            "matcher": "*",
            "hooks": [
                { "type": "command", "command": format!("\"{bin}\" --hook=inbox") }
            ]
        }));
    }
    {
        // Task-status footer on PostToolUse so it reflects the just-completed tool's
        // own effect (e.g. workslate_task_done shows done on the same call, not one
        // call later). Session-scoped, so it resolves from session_id even if
        // PostToolUse stdin omits agent_id.
        let post = event_array(&mut root, "PostToolUse")?;
        strip_our_hooks(post);
        post.push(serde_json::json!({
            "matcher": "*",
            "hooks": [
                { "type": "command", "command": format!("\"{bin}\" --hook=task") }
            ]
        }));
    }
    {
        let ss = event_array(&mut root, "SessionStart")?;
        strip_our_hooks(ss);
        ss.push(serde_json::json!({
            "hooks": [
                { "type": "command", "command": format!("\"{bin}\" --hook=session-start") }
            ]
        }));
    }
    {
        // SubagentStart hands each subagent its agent_id (subagents do not fire
        // SessionStart and share the parent's session id). matcher "*" matches all
        // agent_types.
        let sa = event_array(&mut root, "SubagentStart")?;
        strip_our_hooks(sa);
        sa.push(serde_json::json!({
            "matcher": "*",
            "hooks": [
                { "type": "command", "command": format!("\"{bin}\" --hook=subagent-start") }
            ]
        }));
    }
    write_settings(&path, &root)?;
    println!(
        "Installed workslate hooks (PreToolUse inbox + PostToolUse task doorbells + SessionStart/SubagentStart bridges) into {}",
        path.display()
    );
    Ok(())
}

/// Remove all workslate hooks from settings.json (idempotent).
pub fn uninstall_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let path = settings_path().ok_or("could not resolve HOME for settings.json")?;
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_settings(&path)?;
    for event in WS_HOOK_EVENTS {
        if let Some(arr) = root
            .get_mut("hooks")
            .and_then(|h| h.get_mut(*event))
            .and_then(|a| a.as_array_mut())
        {
            strip_our_hooks(arr);
        }
    }
    write_settings(&path, &root)?;
    println!("Removed workslate hooks from {}", path.display());
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hook_mode_detects_flags() {
        assert!(matches!(
            parse_hook_mode(&["prog".into(), "--hook=task".into()]),
            Some(HookMode::Task)
        ));
        assert!(matches!(
            parse_hook_mode(&["--hook=inbox".into()]),
            Some(HookMode::Inbox)
        ));
        assert!(matches!(
            parse_hook_mode(&["--hook=session-start".into()]),
            Some(HookMode::SessionStart)
        ));
        assert!(matches!(
            parse_hook_mode(&["--hook=subagent-start".into()]),
            Some(HookMode::SubagentStart)
        ));
        assert!(parse_hook_mode(&["--something-else".into()]).is_none());
        assert!(parse_hook_mode(&[]).is_none());
    }

    #[test]
    fn hook_is_ours_matches_only_doorbell_commands() {
        let ours =
            serde_json::json!({ "type": "command", "command": "/abs/path/workslate --hook=task" });
        let theirs =
            serde_json::json!({ "type": "command", "command": "/usr/bin/prettier --write" });
        assert!(hook_is_ours(&ours));
        assert!(!hook_is_ours(&theirs));
    }

    #[test]
    fn strip_our_hooks_preserves_user_hooks_in_shared_group() {
        let mut pre = vec![serde_json::json!({
            "matcher": "*",
            "hooks": [
                { "type": "command", "command": "/abs/workslate --hook=task" },
                { "type": "command", "command": "/usr/bin/prettier" }
            ]
        })];
        strip_our_hooks(&mut pre);
        assert_eq!(pre.len(), 1);
        let handlers = pre[0]["hooks"].as_array().unwrap();
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0]["command"], "/usr/bin/prettier");
    }

    #[test]
    fn strip_our_hooks_drops_group_when_only_ours() {
        let mut pre = vec![serde_json::json!({
            "matcher": "*",
            "hooks": [{ "type": "command", "command": "/abs/workslate --hook=inbox" }]
        })];
        strip_our_hooks(&mut pre);
        assert!(pre.is_empty());
    }

    #[test]
    fn hook_is_ours_matches_subagent_start() {
        let ours = serde_json::json!({
            "type": "command", "command": "/abs/path/workslate --hook=subagent-start"
        });
        assert!(hook_is_ours(&ours));
    }

    #[test]
    fn event_name_task_is_posttooluse_inbox_is_pretooluse() {
        assert_eq!(event_name(HookMode::Task), "PostToolUse");
        assert_eq!(event_name(HookMode::Inbox), "PreToolUse");
        assert_eq!(event_name(HookMode::SessionStart), "SessionStart");
        assert_eq!(event_name(HookMode::SubagentStart), "SubagentStart");
    }

    #[test]
    fn session_scoped_task_session_resolves_unique_or_none() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session_context (\
                 claude_session_id TEXT NOT NULL, agent_id TEXT NOT NULL DEFAULT '', \
                 task_session TEXT NOT NULL, role TEXT, \
                 PRIMARY KEY (claude_session_id, agent_id));",
        )
        .unwrap();
        // Two agents share one session id AND one task_session -> unique resolution
        // (this is the team invariant; a PostToolUse without agent_id still resolves).
        conn.execute(
            "INSERT INTO session_context VALUES ('s', '', 'sess', 'leader')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_context VALUES ('s', 'aX', 'sess', 'teammate')",
            [],
        )
        .unwrap();
        assert_eq!(session_scoped_task_session(&conn, "s"), Some("sess".to_string()));
        // Unknown session id -> None.
        assert_eq!(session_scoped_task_session(&conn, "other"), None);
        // Ambiguous: two distinct task_sessions under one session id -> None (don't guess).
        conn.execute(
            "INSERT INTO session_context VALUES ('s', 'aY', 'sess2', 'tm2')",
            [],
        )
        .unwrap();
        assert_eq!(session_scoped_task_session(&conn, "s"), None);
    }
}
