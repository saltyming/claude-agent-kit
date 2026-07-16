//! Hook subcommands for the messaging doorbell, the SendMessage bridge, and
//! the session-start bridges.
//!
//! Invoked from settings.json hooks:
//! - `workslate --hook=inbox` (PreToolUse, matcher `*`): nudge BEFORE every tool
//!   call (and on calls that error or are denied, which PostToolUse skips) that
//!   unread messages await — this is what makes steering land MID-TURN.
//! - `workslate --hook=send-bridge` (PostToolUse, matcher `SendMessage`): mirror a
//!   successful native SendMessage call into the workslate inbox, so the RECIPIENT's
//!   inbox doorbell can announce it before native turn-boundary delivery arrives.
//!   Senders keep using the native tool they are trained on; no workslate call needed.
//! - `workslate --hook=session-start` (SessionStart): hand the agent its
//!   conversation session id so it can pass it to workslate_register — the MCP
//!   server's own env id does NOT match the hook's stdin session id.
//! - `workslate --hook=subagent-start` (SubagentStart): hand a subagent its
//!   agent_id + session_id, and best-effort auto-register it under its agent
//!   name when the hook payload carries one.
//!
//! A hook must never break a tool call: every error path logs to stderr and
//! produces empty stdout (exit 0 = no injection).

use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::msg::upsert_session_context;
use crate::resolve_tasks_dir;

#[derive(Clone, Copy)]
pub enum HookMode {
    Inbox,
    SendBridge,
    SessionStart,
    SubagentStart,
    /// `--hook=task` from a pre-v11 settings.json that has not been re-installed.
    /// Recognized so the invocation exits quietly instead of falling through to
    /// an MCP-server start on a hook stdin; emits nothing.
    LegacyTask,
}

/// Detect the `--hook=…` flag in argv.
pub fn parse_hook_mode(args: &[String]) -> Option<HookMode> {
    args.iter().find_map(|a| match a.as_str() {
        "--hook=inbox" => Some(HookMode::Inbox),
        "--hook=send-bridge" => Some(HookMode::SendBridge),
        "--hook=session-start" => Some(HookMode::SessionStart),
        "--hook=subagent-start" => Some(HookMode::SubagentStart),
        "--hook=task" => Some(HookMode::LegacyTask),
        _ => None,
    })
}

/// Run a hook. Prints at most one JSON object to stdout and never surfaces an
/// error to the caller — a failed hook must not block a tool.
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
        // The inbox nudge stays on PreToolUse (must fire on every call, including
        // errors/denials, which PostToolUse skips); the bridge mirrors AFTER a
        // successful native send.
        HookMode::Inbox => "PreToolUse",
        HookMode::SendBridge | HookMode::LegacyTask => "PostToolUse",
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
        // workslate_register. No DB lookup needed.
        return Ok(Some(format!(
            "[workslate] session_id=`{session_id}` — to enable the mid-turn message doorbell, \
             pass session_id=\"{session_id}\" (and agent_id=\"\") to workslate_register."
        )));
    }

    let cwd = v.get("cwd").and_then(|x| x.as_str()).unwrap_or(".");

    // Anchor on the same path logic the server uses so both resolve to the same
    // database (CLAUDE_PROJECT_DIR, with the hook-input cwd as fallback).
    let db_path = resolve_tasks_dir(std::path::Path::new(cwd)).join("workslate.db");

    if let HookMode::SubagentStart = mode {
        // Subagents do NOT fire SessionStart and share the parent's session_id, so
        // agent_id (subagent-only) is their identity discriminator. Best-effort
        // auto-registration: when the payload names the agent, register it under
        // that name so bridged native SendMessage traffic reaches its inbox with
        // zero startup calls (never clobbering a role it set itself).
        let agent_name = subagent_name(&v);
        let auto = match (&agent_name, db_path.exists()) {
            (Some(name), true) => {
                Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
                    .ok()
                    .and_then(|conn| {
                        conn.busy_timeout(std::time::Duration::from_millis(2000))
                            .ok()?;
                        upsert_session_context(
                            &conn,
                            session_id,
                            agent_id,
                            session_id,
                            Some(name),
                            true,
                        )
                        .ok()
                    })
                    .map(|_| name.clone())
            }
            _ => None,
        };
        return Ok(Some(match auto {
            Some(name) => format!(
                "[workslate] agent_id=`{agent_id}` session_id=`{session_id}` — you are a \
                 subagent, auto-registered for team messaging as role \"{name}\". Unread \
                 messages will be announced by the inbox doorbell; drain them with \
                 workslate_inbox_read(role=\"{name}\", session_id=\"{session_id}\"). To use a \
                 different role, call workslate_register with BOTH ids."
            ),
            None => format!(
                "[workslate] agent_id=`{agent_id}` session_id=`{session_id}` — you are a \
                 subagent; to enable the mid-turn message doorbell, pass BOTH \
                 agent_id=\"{agent_id}\" and session_id=\"{session_id}\" to workslate_register."
            ),
        }));
    }

    if !db_path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;

    match mode {
        // Inbox is role-specific: it needs the exact (session_id, agent_id) row to
        // know whose unread to show. It runs on PreToolUse, whose stdin carries
        // agent_id, so the exact lookup resolves.
        HookMode::Inbox => {
            let exact: Option<(String, Option<String>)> = conn
                .query_row(
                    "SELECT task_session, role FROM session_context \
                     WHERE claude_session_id = ? AND agent_id = ?",
                    rusqlite::params![session_id, agent_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();
            let (scope, role) = match exact {
                Some(t) => t,
                None => return Ok(None),
            };
            Ok(inbox_doorbell(&conn, &scope, role.as_deref()))
        }
        // Mirror a successful native SendMessage into the inbox. Emits nothing
        // into the SENDER's context — the injection surface is the recipient's
        // inbox doorbell.
        HookMode::SendBridge => {
            bridge_native_send(&conn, session_id, agent_id, &v);
            Ok(None)
        }
        HookMode::LegacyTask => Ok(None),
        // SessionStart / SubagentStart returned early above.
        HookMode::SessionStart | HookMode::SubagentStart => Ok(None),
    }
}

/// The spawned agent's name from a SubagentStart payload, if the harness
/// provides one. Only name-like fields count — `agent_type` (e.g.
/// "general-purpose") is a capability class, not an identity, and
/// auto-registering under it would collide every teammate onto one role.
fn subagent_name(v: &serde_json::Value) -> Option<String> {
    for key in ["agent_name", "name"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str())
            && !s.trim().is_empty()
        {
            return Some(s.trim().to_string());
        }
    }
    None
}

/// Mirror one successful native SendMessage call into the workslate inbox so
/// the recipient's PreToolUse doorbell can announce it mid-turn (native
/// delivery only lands at the recipient's next turn boundary).
///
/// Best-effort by design: any missing/odd field means "mirror nothing" — the
/// native delivery path is unaffected either way. Protocol messages (JSON
/// objects like shutdown_request) are not mirrored; only plain-text sends.
fn bridge_native_send(
    conn: &Connection,
    session_id: &str,
    sender_agent_id: &str,
    v: &serde_json::Value,
) {
    let Some(tool_input) = v.get("tool_input") else {
        return;
    };
    let Some(to) = tool_input.get("to").and_then(|x| x.as_str()).map(str::trim) else {
        return;
    };
    if to.is_empty() {
        return;
    }
    // Only plain-text messages are worth a doorbell; protocol objects
    // (shutdown/plan-approval traffic) stay on the native channel alone.
    let Some(body) = tool_input.get("message").and_then(|x| x.as_str()) else {
        return;
    };
    let subject = tool_input
        .get("summary")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| truncate_chars(body, 80));

    // "main" addresses the main conversation — deliver to the leader's
    // registered role (its row is (session_id, "")). Skip when the leader
    // never registered: there is no doorbell identity to announce to.
    let recipient = if to == "main" {
        match lookup_role(conn, session_id, "") {
            Some(r) => r,
            None => return,
        }
    } else {
        to.to_string()
    };

    // Sender attribution from the SENDER's composite identity (the hook runs in
    // the sender's context). NULL when unregistered — honest, not guessed.
    let sender = lookup_role(conn, session_id, sender_agent_id);

    let _ = conn.execute(
        "INSERT INTO messages (task_session, recipient_role, sender, subject, body, urgent) \
         VALUES (?, ?, ?, ?, ?, 0)",
        rusqlite::params![session_id, recipient, sender, subject, body],
    );
}

fn lookup_role(conn: &Connection, session_id: &str, agent_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT role FROM session_context WHERE claude_session_id = ? AND agent_id = ?",
        rusqlite::params![session_id, agent_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

fn truncate_chars(s: &str, cap: usize) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > cap {
        let cut: String = one.chars().take(cap).collect();
        format!("{cut}…")
    } else {
        one
    }
}

/// Inbox doorbell: one line nudging the agent to read unread messages. Repeats
/// every tool call until workslate_inbox_read clears them.
fn inbox_doorbell(conn: &Connection, scope: &str, role: Option<&str>) -> Option<String> {
    let role = role?;
    let mut stmt = conn
        .prepare(
            "SELECT subject, urgent FROM messages \
             WHERE task_session = ? AND recipient_role = ? AND read_at IS NULL \
             ORDER BY id DESC",
        )
        .ok()?;
    let rows: Vec<(String, i64)> = stmt
        .query_map(rusqlite::params![scope, role], |r| {
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

/// Marker embedded at the start of the Stop verify hook's `prompt` text.
/// `hook_is_ours` uses it to recognize a `type:"agent"` entry (which has a
/// `prompt` field, not `command`) as ours on reinstall/uninstall — the
/// content-based `command` check below can't see it, since this hook type
/// never invokes the workslate binary at all (Claude Code's own runtime
/// spawns the verifier subagent directly from `prompt`).
const STOP_VERIFY_MARKER: &str = "[workslate-task-verify]";

/// True if a single hook handler is one of ours: either a workslate hook
/// `command` (matched by content, not exact path, so re-install after a path
/// change still replaces the old entry rather than duplicating it), or the
/// Stop verify hook's `type:"agent"` `prompt` entry (matched by the embedded
/// marker, since that entry has no `command` field at all).
fn hook_is_ours(handler: &serde_json::Value) -> bool {
    let command_ours = handler
        .get("command")
        .and_then(|c| c.as_str())
        .map(|c| c.contains("workslate") && c.contains("--hook="))
        .unwrap_or(false);
    let prompt_ours = handler
        .get("prompt")
        .and_then(|c| c.as_str())
        .map(|c| c.contains(STOP_VERIFY_MARKER))
        .unwrap_or(false);
    command_ours || prompt_ours
}

/// Remove only our handlers from each matcher-group, dropping a group only
/// once it has no handlers left. This preserves unrelated user hooks that
/// happen to share a matcher-group with ours.
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

/// All hook event arrays workslate manages in settings.json, including the
/// Stop verify hook installed by `install_hooks()`; `uninstall_hooks()` cleans
/// up every event listed here.
const WS_HOOK_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "SessionStart",
    "SubagentStart",
    "Stop",
];

/// The Stop verify hook's prompt: spawns an independent verifier subagent
/// (via Claude Code's native `type:"agent"` hook mechanism, response schema
/// `{"ok": bool, "reason"}` per the hooks doc) that spot-checks the turn's
/// completion claims against real repository state before Claude is allowed
/// to end its turn — never trusting the conversation's own self-report.
/// Deliberately standalone: it reads the transcript and the repo directly,
/// with no MCP-tool dependency. `$ARGUMENTS` is substituted by Claude Code
/// with the hook input JSON — it is the only channel through which the
/// verifier learns session_id/transcript_path/stop_hook_active/cwd.
const STOP_VERIFY_PROMPT: &str = "\
[workslate-task-verify] You are an independent verification subagent spawned by a
Stop hook. Decide whether the assistant may end its turn, by checking REAL
repository/system state — never by trusting the conversation's own claims of
success.

Hook input JSON (session_id, cwd, transcript_path, stop_hook_active):
$ARGUMENTS

Rules:
1. If `stop_hook_active` is true in the input above, respond {\"ok\": true}
   immediately — a prior Stop hook already blocked this turn once; do not loop.
2. Read the tail of the transcript at `transcript_path` to find what the assistant
   claimed to have completed THIS turn (tasks finished, tests passing, builds green,
   files changed). If the turn made no completion claims — conversational,
   investigative, or explicitly-unfinished work — respond {\"ok\": true}.
3. Verify the load-bearing claims against real state: read the files named, run the
   tests/commands cited, check actual output. Spot-check; do not re-derive
   everything.
4. If the claims hold, respond {\"ok\": true}. If a claim is false or unverifiable —
   e.g. work reported complete while its build or test still fails — respond
   {\"ok\": false, \"reason\": \"<specifically what is wrong or unproven>\"}.

Your final message must be ONLY {\"ok\": true} or {\"ok\": false, \"reason\": \"...\"}
— no other fields, no surrounding prose.";

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
/// doorbell, the PostToolUse SendMessage bridge, the SessionStart/SubagentStart
/// bridges, and the anti-self-grading Stop verify hook. Existing user hooks are
/// preserved — only workslate's own handlers are replaced (including the retired
/// pre-v11 PostToolUse task footer, which the content-based strip removes).
///
/// The Stop hook is a `type:"agent"` entry (not `type:"command"` like the rest) —
/// Claude Code has no mechanism for one hook entry to gate whether a sibling entry
/// even fires, so it spawns a verifier subagent on every Stop event, in every
/// session, project-wide. That cost is accepted here by design (this is a
/// single-user personal toolchain, not a distribution with many installs to
/// weigh) — see claude-agent-kit--task-execution.md for the tradeoff and the
/// `/goal` relationship.
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
        // SendMessage bridge on PostToolUse: mirrors each successful native send
        // into the inbox so the recipient's doorbell can announce it mid-turn.
        // Fires only on the SendMessage tool — matcher-scoped, not "*".
        let post = event_array(&mut root, "PostToolUse")?;
        strip_our_hooks(post);
        post.push(serde_json::json!({
            "matcher": "SendMessage",
            "hooks": [
                { "type": "command", "command": format!("\"{bin}\" --hook=send-bridge") }
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
        // SessionStart and share the parent's session id) and best-effort
        // auto-registers named teammates. matcher "*" matches all agent_types.
        let sa = event_array(&mut root, "SubagentStart")?;
        strip_our_hooks(sa);
        sa.push(serde_json::json!({
            "matcher": "*",
            "hooks": [
                { "type": "command", "command": format!("\"{bin}\" --hook=subagent-start") }
            ]
        }));
    }
    {
        // Anti-self-grading verify hook on Stop: spawns an independent verifier
        // subagent before Claude is allowed to end a turn. See STOP_VERIFY_PROMPT's
        // own doc comment for the full design rationale.
        let stop = event_array(&mut root, "Stop")?;
        strip_our_hooks(stop);
        stop.push(serde_json::json!({
            "hooks": [
                { "type": "agent", "prompt": STOP_VERIFY_PROMPT, "timeout": 120 }
            ]
        }));
    }
    write_settings(&path, &root)?;
    println!(
        "Installed workslate hooks (PreToolUse inbox doorbell + PostToolUse SendMessage \
         bridge + SessionStart/SubagentStart bridges + Stop anti-self-grading verify hook) \
         into {}",
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
    use crate::msg::{SCHEMA_SQL, SESSION_CONTEXT_DDL};

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn.execute_batch(SESSION_CONTEXT_DDL).unwrap();
        conn
    }

    #[test]
    fn parse_hook_mode_detects_flags() {
        assert!(matches!(
            parse_hook_mode(&["prog".into(), "--hook=inbox".into()]),
            Some(HookMode::Inbox)
        ));
        assert!(matches!(
            parse_hook_mode(&["--hook=send-bridge".into()]),
            Some(HookMode::SendBridge)
        ));
        assert!(matches!(
            parse_hook_mode(&["--hook=session-start".into()]),
            Some(HookMode::SessionStart)
        ));
        assert!(matches!(
            parse_hook_mode(&["--hook=subagent-start".into()]),
            Some(HookMode::SubagentStart)
        ));
        // A stale pre-v11 settings.json still invokes --hook=task; it must be
        // recognized (and quietly no-op) rather than fall through to an MCP
        // server start on hook stdin.
        assert!(matches!(
            parse_hook_mode(&["--hook=task".into()]),
            Some(HookMode::LegacyTask)
        ));
        assert!(parse_hook_mode(&["--something-else".into()]).is_none());
        assert!(parse_hook_mode(&[]).is_none());
    }

    #[test]
    fn event_names_route_hooks_to_the_right_events() {
        assert_eq!(event_name(HookMode::Inbox), "PreToolUse");
        assert_eq!(event_name(HookMode::SendBridge), "PostToolUse");
        assert_eq!(event_name(HookMode::LegacyTask), "PostToolUse");
        assert_eq!(event_name(HookMode::SessionStart), "SessionStart");
        assert_eq!(event_name(HookMode::SubagentStart), "SubagentStart");
    }

    #[test]
    fn subagent_name_reads_name_fields_only() {
        assert_eq!(
            subagent_name(&serde_json::json!({"agent_name": "researcher"})),
            Some("researcher".to_string())
        );
        assert_eq!(
            subagent_name(&serde_json::json!({"name": " backend-dev "})),
            Some("backend-dev".to_string())
        );
        // agent_type is a capability class, not an identity — never a role.
        assert_eq!(
            subagent_name(&serde_json::json!({"agent_type": "general-purpose"})),
            None
        );
        assert_eq!(subagent_name(&serde_json::json!({"agent_name": ""})), None);
        assert_eq!(subagent_name(&serde_json::json!({})), None);
    }

    #[test]
    fn bridge_mirrors_plain_sends_and_skips_protocol_objects() {
        let conn = mem_db();
        // Sender registered as team-lead (leader row), recipient is a teammate name.
        conn.execute(
            "INSERT INTO session_context (claude_session_id, agent_id, task_session, role) \
             VALUES ('sid', '', 'sid', 'team-lead')",
            [],
        )
        .unwrap();

        bridge_native_send(
            &conn,
            "sid",
            "",
            &serde_json::json!({
                "tool_input": {"to": "researcher", "summary": "assign task 1", "message": "start on task #1"}
            }),
        );
        let (recipient, sender, subject, body, urgent): (
            String,
            Option<String>,
            String,
            String,
            i64,
        ) = conn
            .query_row(
                "SELECT recipient_role, sender, subject, body, urgent FROM messages",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(recipient, "researcher");
        assert_eq!(sender.as_deref(), Some("team-lead"));
        assert_eq!(subject, "assign task 1");
        assert_eq!(body, "start on task #1");
        assert_eq!(urgent, 0, "bridged messages are never urgent");

        // Protocol objects (shutdown/plan traffic) are NOT mirrored.
        bridge_native_send(
            &conn,
            "sid",
            "",
            &serde_json::json!({
                "tool_input": {"to": "researcher", "message": {"type": "shutdown_request"}}
            }),
        );
        let count: u32 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "protocol objects must not be mirrored");
    }

    #[test]
    fn bridge_routes_main_to_leader_role_and_falls_back_to_body_subject() {
        let conn = mem_db();
        conn.execute(
            "INSERT INTO session_context (claude_session_id, agent_id, task_session, role) \
             VALUES ('sid', '', 'sid', 'team-lead')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_context (claude_session_id, agent_id, task_session, role) \
             VALUES ('sid', 'aX', 'sid', 'researcher')",
            [],
        )
        .unwrap();

        // A teammate (agent aX) sends to "main" with no summary.
        bridge_native_send(
            &conn,
            "sid",
            "aX",
            &serde_json::json!({
                "tool_input": {"to": "main", "message": "task 1 complete, moving to task 2"}
            }),
        );
        let (recipient, sender, subject): (String, Option<String>, String) = conn
            .query_row(
                "SELECT recipient_role, sender, subject FROM messages",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(recipient, "team-lead", "'main' routes to the leader's role");
        assert_eq!(sender.as_deref(), Some("researcher"));
        assert_eq!(subject, "task 1 complete, moving to task 2");

        // With NO leader row, a send to "main" mirrors nothing (no doorbell identity).
        let conn2 = mem_db();
        bridge_native_send(
            &conn2,
            "sid",
            "aX",
            &serde_json::json!({"tool_input": {"to": "main", "message": "x"}}),
        );
        let count: u32 = conn2
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn hook_is_ours_matches_only_workslate_commands() {
        let ours = serde_json::json!({ "type": "command", "command": "/abs/path/workslate --hook=send-bridge" });
        let legacy =
            serde_json::json!({ "type": "command", "command": "/abs/path/workslate --hook=task" });
        let theirs =
            serde_json::json!({ "type": "command", "command": "/usr/bin/prettier --write" });
        assert!(hook_is_ours(&ours));
        assert!(
            hook_is_ours(&legacy),
            "pre-v11 task footer entries must still be recognized so reinstall removes them"
        );
        assert!(!hook_is_ours(&theirs));
    }

    #[test]
    fn strip_our_hooks_preserves_user_hooks_in_shared_group() {
        let mut pre = vec![serde_json::json!({
            "matcher": "*",
            "hooks": [
                { "type": "command", "command": "/abs/workslate --hook=inbox" },
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
    fn hook_is_ours_matches_stop_verify_agent_prompt_entry() {
        // type:"agent" entries have a `prompt` field, not `command` — the
        // command-based check alone would miss this and duplicate/orphan the
        // entry on reinstall or fail to clean it up on uninstall.
        let ours = serde_json::json!({
            "type": "agent",
            "prompt": STOP_VERIFY_PROMPT,
            "timeout": 120
        });
        assert!(hook_is_ours(&ours));

        let theirs = serde_json::json!({
            "type": "agent",
            "prompt": "Some unrelated user-authored Stop hook prompt.",
            "timeout": 60
        });
        assert!(!hook_is_ours(&theirs));
    }

    #[test]
    fn stop_verify_hook_block_is_idempotent_and_uninstallable() {
        let dir =
            std::env::temp_dir().join(format!("workslate-stop-hook-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{}").unwrap();

        // Exercise the same Stop-block logic install_hooks() runs, against this
        // scratch settings.json path via the internal event_array/strip_our_hooks
        // helpers directly (install_hooks() itself resolves $HOME, which we don't
        // want to touch in a unit test) — simulate that block inline.
        let mut root = read_settings(&path).unwrap();
        {
            let stop = event_array(&mut root, "Stop").unwrap();
            strip_our_hooks(stop);
            stop.push(serde_json::json!({
                "hooks": [
                    { "type": "agent", "prompt": STOP_VERIFY_PROMPT, "timeout": 120 }
                ]
            }));
        }
        write_settings(&path, &root).unwrap();

        // Re-run: idempotent, not duplicated.
        let mut root2 = read_settings(&path).unwrap();
        {
            let stop = event_array(&mut root2, "Stop").unwrap();
            strip_our_hooks(stop);
            stop.push(serde_json::json!({
                "hooks": [
                    { "type": "agent", "prompt": STOP_VERIFY_PROMPT, "timeout": 120 }
                ]
            }));
        }
        write_settings(&path, &root2).unwrap();

        let after_reinstall = read_settings(&path).unwrap();
        let stop_arr = after_reinstall["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_arr.len(), 1, "reinstall must replace, not duplicate");

        // WS_HOOK_EVENTS includes "Stop", so the general uninstall path covers it.
        assert!(WS_HOOK_EVENTS.contains(&"Stop"));
        let mut root3 = read_settings(&path).unwrap();
        for event in WS_HOOK_EVENTS {
            if let Some(arr) = root3
                .get_mut("hooks")
                .and_then(|h| h.get_mut(*event))
                .and_then(|a| a.as_array_mut())
            {
                strip_our_hooks(arr);
            }
        }
        write_settings(&path, &root3).unwrap();
        let after_uninstall = read_settings(&path).unwrap();
        let stop_arr_after = after_uninstall["hooks"]["Stop"].as_array().unwrap();
        assert!(
            stop_arr_after.is_empty(),
            "uninstall must remove the Stop verify hook"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
