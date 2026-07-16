//! Messaging domain: param structs, schema, migrations, sender attribution.
//!
//! workslate's charter after v11 is mid-turn team messaging ONLY — the task
//! tracker (ws:/team: namespaces, footer doorbell) was retired in favor of the
//! harness-native task tools. What remains is the thing the harness does not
//! do: native SendMessage delivers at turn boundaries, so a busy teammate
//! cannot be steered mid-task; workslate's inbox + PreToolUse doorbell close
//! that gap. Message scope is the Claude session id itself (leader and
//! teammates share one), so there is no named-session bootstrap to remember.

use schemars::JsonSchema;
use serde::Deserialize;

// ── Param structs ────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegisterParams {
    /// Your role name (e.g. "backend-dev" — for a spawned teammate, use your
    /// agent name so native SendMessage traffic addressed to that name reaches
    /// your inbox too). The durable identity messages are addressed to.
    pub role: String,
    /// This Claude session's id — the value the workslate SessionStart hint gave
    /// you (`[workslate] session_id=...`). Pass it so the doorbell hooks (which see
    /// the conversation session id on their stdin) can resolve this agent. The
    /// MCP server's own env id does NOT match the hook's, so this must come from
    /// the hint. Falls back to the server env id only when omitted.
    pub session_id: Option<String>,
    /// This subagent's agent_id (from the workslate SubagentStart hint). With
    /// session_id it forms the composite identity; distinguishes a teammate from the
    /// parent session (they share CLAUDE_CODE_SESSION_ID). REQUIRED: the leader passes
    /// an explicit empty string (its `(session_id, "")` identity), teammates pass their
    /// SubagentStart agent_id. Omitting it is rejected — it would default to the leader's
    /// row and a teammate would overwrite the leader's role.
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MsgSendParams {
    /// Recipient role name (the registered role — for spawned teammates,
    /// their agent name).
    pub recipient: String,
    /// One-line subject shown in the recipient's inbox doorbell
    pub subject: String,
    /// Full message body, returned when the recipient reads the inbox
    pub body: String,
    /// Mark urgent so the doorbell flags it (🚨). JSON boolean — not a string.
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_bool")]
    pub urgent: Option<bool>,
    /// Sender label. When omitted, the sender is resolved from the caller's
    /// registered role via the composite (session_id, agent_id) identity in
    /// session_context (falling back to the in-process active_role cache only when
    /// that finds nothing). Pass session_id/agent_id below for correct attribution
    /// in a shared-process team.
    pub sender: Option<String>,
    /// This Claude session's id (from the workslate SessionStart/SubagentStart
    /// hint). Scopes the message AND, with agent_id, identifies THIS caller so its
    /// registered role can be used as the sender. Falls back to the server env id
    /// when omitted.
    pub session_id: Option<String>,
    /// This subagent's agent_id (from the workslate SubagentStart hint). The second
    /// half of the composite identity that tells a teammate apart from the parent
    /// session (they share CLAUDE_CODE_SESSION_ID). REQUIRED whenever a session_id is
    /// in effect: the leader passes an explicit empty string (its `(session_id, "")`
    /// identity), teammates pass their SubagentStart agent_id. Omitting it while a
    /// session is resolvable is rejected — an omitted agent_id would collapse to the
    /// leader's row and mis-attribute the message as the leader's.
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InboxReadParams {
    /// Your role name. Returns unread messages addressed to this role and marks them read.
    pub role: String,
    /// This Claude session's id (from the workslate hint) — the message scope.
    /// Falls back to the server env id when omitted.
    pub session_id: Option<String>,
}

// ── Schema ───────────────────────────────────────────────

/// Messaging schema. The retired task tables (`tasks`, `task_counters`) are
/// deliberately NOT dropped from existing databases — old data stays readable
/// with external tools — they are simply no longer created or touched.
///
/// `messages.task_session` (historical column name, kept for data
/// compatibility) now stores the Claude session id — the scope a leader and
/// its teammates share automatically.
pub const SCHEMA_SQL: &str = "\
CREATE TABLE IF NOT EXISTS messages (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    task_session   TEXT    NOT NULL,
    recipient_role TEXT    NOT NULL,
    sender         TEXT,
    subject        TEXT    NOT NULL,
    body           TEXT    NOT NULL,
    urgent         INTEGER NOT NULL DEFAULT 0,
    read_at        TEXT,
    created_at     TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_messages_inbox
    ON messages(task_session, recipient_role, read_at);
";

/// `session_context` DDL — kept separate from SCHEMA_SQL because v8.9 changed its
/// primary key from `claude_session_id` alone to the composite
/// `(claude_session_id, agent_id)`. `migrate_db` rebuilds the old shape and
/// (re)creates the table, so the single source of truth lives here.
/// `task_session` (historical name) holds the message scope — since v11 the
/// Claude session id itself.
pub const SESSION_CONTEXT_DDL: &str = "\
CREATE TABLE IF NOT EXISTS session_context (
    claude_session_id TEXT NOT NULL,
    agent_id          TEXT NOT NULL DEFAULT '',
    task_session      TEXT NOT NULL,
    role              TEXT,
    updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (claude_session_id, agent_id)
);
";

/// Apply schema migrations to an existing database. Runs after SCHEMA_SQL,
/// which is idempotent (CREATE TABLE IF NOT EXISTS). Migrations handle cases
/// where an older DB exists without newer columns.
///
/// Each migration must be idempotent — safe to re-run on an already-migrated DB.
pub fn migrate_db(conn: &mut rusqlite::Connection) -> rusqlite::Result<()> {
    // v9.3: drop the legacy buffers table (buffer-staging tools removed).
    // Idempotent — a no-op on a fresh DB where SCHEMA_SQL never created it.
    conn.execute("DROP TABLE IF EXISTS buffers", [])?;

    // v8.9: session_context identity is the composite (claude_session_id, agent_id).
    // Subagents share the parent's CLAUDE_CODE_SESSION_ID, so agent_id (handed to a
    // subagent by the SubagentStart hook) is what separates a teammate from the main
    // session. The old table keyed claude_session_id alone — rebuild it. The table is
    // ephemeral (agents re-register; messages are keyed by role/task_session, not by
    // session_context), so drop+recreate loses nothing durable. BEGIN IMMEDIATE
    // serializes concurrent MCP-server startups (leader + each subagent) and forces
    // the agent_id check to be re-evaluated under the write lock.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    {
        let cols: Vec<String> = {
            let mut stmt = tx.prepare("PRAGMA table_info(session_context)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        let table_exists = !cols.is_empty();
        let has_agent_id = cols.iter().any(|c| c == "agent_id");
        if table_exists && !has_agent_id {
            tx.execute("DROP TABLE session_context", [])?;
        }
        tx.execute_batch(SESSION_CONTEXT_DDL)?;
    }
    tx.commit()?;
    Ok(())
}

/// Upsert an agent's `(claude_session_id, agent_id) → (scope, role)` row.
///
/// `keep_existing_role=false` (workslate_register): the caller states its role —
/// overwrite. `keep_existing_role=true` (SubagentStart auto-registration): a
/// best-effort default that must never clobber a role an agent set itself.
pub fn upsert_session_context(
    conn: &rusqlite::Connection,
    claude_session_id: &str,
    agent_id: &str,
    scope: &str,
    role: Option<&str>,
    keep_existing_role: bool,
) -> rusqlite::Result<()> {
    let sql = if keep_existing_role {
        "INSERT INTO session_context (claude_session_id, agent_id, task_session, role, updated_at) \
         VALUES (?, ?, ?, ?, datetime('now')) \
         ON CONFLICT(claude_session_id, agent_id) DO UPDATE SET \
             task_session = excluded.task_session, \
             role = COALESCE(role, excluded.role), \
             updated_at = datetime('now')"
    } else {
        "INSERT INTO session_context (claude_session_id, agent_id, task_session, role, updated_at) \
         VALUES (?, ?, ?, ?, datetime('now')) \
         ON CONFLICT(claude_session_id, agent_id) DO UPDATE SET \
             task_session = excluded.task_session, \
             role = excluded.role, \
             updated_at = datetime('now')"
    };
    conn.execute(
        sql,
        rusqlite::params![claude_session_id, agent_id, scope, role],
    )?;
    Ok(())
}

/// Resolve the `sender` attribution for an outgoing message.
///
/// Priority:
/// 1. an explicit `sender` the caller passed,
/// 2. the caller's registered role, looked up by the composite
///    `(claude_session_id, agent_id)` identity in `session_context`,
/// 3. the in-process `active_role` cache — ONLY when no `session_id` was supplied
///    (a legacy/degraded path); when a `session_id` IS supplied, (2) is authoritative
///    and a miss yields no sender rather than the (process-shared) cache.
///
/// (2) is the source of truth in a shared-process team: an Agent-Team leader and
/// its teammates share one MCP server process, so a single in-process role cache
/// is last-writer-wins and cannot attribute the true caller. The composite key —
/// the same identity the doorbell hooks resolve — distinguishes them, so callers
/// pass their session_id/agent_id (from the SessionStart/SubagentStart hint) just
/// as they do for register.
pub fn resolve_sender(
    conn: &rusqlite::Connection,
    explicit: Option<String>,
    claude_session_id: Option<&str>,
    agent_id: Option<&str>,
    active_role_fallback: Option<String>,
) -> Option<String> {
    if let Some(s) = explicit {
        return Some(s);
    }
    // When the caller identifies itself (session_id supplied), the composite
    // session_context lookup is authoritative — return its result even on a miss
    // (None → NULL sender). Do NOT fall back to the process-shared active_role on a
    // miss: that cache is last-writer-wins across a leader and its teammates, so a
    // fallback here would reintroduce the mis-attribution this fix removes.
    if let Some(sid) = claude_session_id {
        // agent_id MUST be explicit to attribute a sender under a (shared) session id.
        // The leader and all teammates share one session_id; an OMITTED agent_id would
        // collapse to the leader's `(session_id, "")` row and mis-attribute the message
        // as sent by the leader. Refuse to guess (None → NULL sender). An explicit
        // empty string IS the leader's own identity and is honoured. The msg_send tool
        // rejects the omitted-agent_id case up front; this is defense in depth.
        let agent_id = agent_id?;
        return conn
            .query_row(
                "SELECT role FROM session_context WHERE claude_session_id = ? AND agent_id = ?",
                rusqlite::params![sid, agent_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
    }
    // Legacy path: no session id supplied (not running under the hooks). The cache
    // is the only signal available, so accept its best-effort answer.
    active_role_fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_drops_legacy_buffers_table() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        // Simulate a pre-9.3 DB that still has the buffers table (with a staged row).
        conn.execute_batch(
            "CREATE TABLE buffers (
                 name        TEXT PRIMARY KEY,
                 content     TEXT    NOT NULL,
                 file_path   TEXT,
                 depends_on  TEXT    NOT NULL DEFAULT '[]',
                 source_hash TEXT,
                 updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO buffers (name, content) VALUES ('leftover', 'staged content')",
            [],
        )
        .unwrap();

        migrate_db(&mut conn).unwrap();
        // Idempotent: a second run (matching a fresh DB where SCHEMA_SQL never
        // created the table) must not error.
        migrate_db(&mut conn).unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'buffers'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            !exists,
            "legacy buffers table should be dropped after migration"
        );
    }

    #[test]
    fn migrate_leaves_retired_task_tables_untouched() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        // A pre-v11 DB still carries the task tables with data. Retiring the
        // feature must not destroy that data — the tables are simply orphaned.
        conn.execute_batch(
            "CREATE TABLE tasks (session TEXT, namespace TEXT, id INTEGER, name TEXT);
             CREATE TABLE task_counters (session TEXT, namespace TEXT, next_id INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks VALUES ('old-session', 'ws', 1, 'legacy task')",
            [],
        )
        .unwrap();

        migrate_db(&mut conn).unwrap();

        let count: u32 = conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "retired task data must survive migration");
    }

    #[test]
    fn migrate_rebuilds_session_context_to_composite_pk() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        // Base schema (no session_context — it lives in SESSION_CONTEXT_DDL), then
        // the OLD pre-8.9 single-PK session_context shape to exercise the rebuild path.
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn.execute_batch(
            "CREATE TABLE session_context (
                 claude_session_id TEXT PRIMARY KEY,
                 task_session      TEXT NOT NULL,
                 role              TEXT,
                 updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_context (claude_session_id, task_session, role) \
             VALUES ('sid', 'sess', 'leader')",
            [],
        )
        .unwrap();

        migrate_db(&mut conn).unwrap();
        // Idempotent: a second run must not error or change the shape.
        migrate_db(&mut conn).unwrap();

        let cols: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(session_context)").unwrap();
            let rows = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert!(
            cols.iter().any(|c| c == "agent_id"),
            "agent_id column missing after migration"
        );

        // The composite key lets a leader (agent_id='') and a teammate (distinct
        // agent_id) coexist under the same claude_session_id without clobbering.
        conn.execute(
            "INSERT INTO session_context (claude_session_id, agent_id, task_session, role) \
             VALUES ('sid', '', 'sess', 'leader')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_context (claude_session_id, agent_id, task_session, role) \
             VALUES ('sid', 'agentX', 'sess', 'tm')",
            [],
        )
        .unwrap();
        let leader: String = conn
            .query_row(
                "SELECT role FROM session_context WHERE claude_session_id='sid' AND agent_id=''",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let tm: String = conn
            .query_row(
                "SELECT role FROM session_context WHERE claude_session_id='sid' AND agent_id='agentX'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leader, "leader");
        assert_eq!(tm, "tm");
    }

    #[test]
    fn migrate_creates_session_context_on_fresh_db() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        // No session_context yet — migrate_db must create it with the composite shape.
        migrate_db(&mut conn).unwrap();
        let cols: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(session_context)").unwrap();
            let rows = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert!(cols.iter().any(|c| c == "agent_id"));
        assert!(cols.iter().any(|c| c == "claude_session_id"));
    }

    #[test]
    fn upsert_session_context_respects_keep_existing_role() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SESSION_CONTEXT_DDL).unwrap();

        // Auto-registration (keep_existing_role=true) fills an absent row …
        upsert_session_context(&conn, "sid", "aX", "sid", Some("researcher"), true).unwrap();
        let role: Option<String> = conn
            .query_row(
                "SELECT role FROM session_context WHERE claude_session_id='sid' AND agent_id='aX'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(role.as_deref(), Some("researcher"));

        // … but never clobbers a role the agent set itself.
        upsert_session_context(&conn, "sid", "aX", "sid", Some("auto-name"), true).unwrap();
        let role: Option<String> = conn
            .query_row(
                "SELECT role FROM session_context WHERE claude_session_id='sid' AND agent_id='aX'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(role.as_deref(), Some("researcher"), "auto must not clobber");

        // An explicit register (keep_existing_role=false) DOES overwrite.
        upsert_session_context(&conn, "sid", "aX", "sid", Some("renamed"), false).unwrap();
        let role: Option<String> = conn
            .query_row(
                "SELECT role FROM session_context WHERE claude_session_id='sid' AND agent_id='aX'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(role.as_deref(), Some("renamed"));
    }

    #[test]
    fn resolve_sender_uses_composite_identity_then_fallback() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SESSION_CONTEXT_DDL).unwrap();
        // Leader (agent_id='') and teammate (distinct agent_id) under one session id.
        conn.execute(
            "INSERT INTO session_context (claude_session_id, agent_id, task_session, role) \
             VALUES ('sid', '', 'sess', 'leader')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_context (claude_session_id, agent_id, task_session, role) \
             VALUES ('sid', 'agentX', 'sess', 'teammate')",
            [],
        )
        .unwrap();

        // Explicit sender always wins.
        assert_eq!(
            resolve_sender(
                &conn,
                Some("explicit".into()),
                Some("sid"),
                Some(""),
                Some("fb".into())
            ),
            Some("explicit".into())
        );
        // Composite lookup: same session id, explicit agent_id='' -> leader.
        assert_eq!(
            resolve_sender(&conn, None, Some("sid"), Some(""), Some("fb".into())),
            Some("leader".into())
        );
        // Composite lookup: same session id, distinct agent_id -> teammate. A single
        // in-process role cache would mis-attribute this to the last registrant.
        assert_eq!(
            resolve_sender(&conn, None, Some("sid"), Some("agentX"), Some("fb".into())),
            Some("teammate".into())
        );
        // session_id supplied but no matching row -> None (NOT the process-shared
        // active_role, which would reintroduce mis-attribution on a miss).
        assert_eq!(
            resolve_sender(&conn, None, Some("sid"), Some("unknown"), Some("fb".into())),
            None
        );
        // session_id supplied but agent_id OMITTED -> None. An omitted agent_id must not
        // collapse to the leader's (session_id, "") row and mis-attribute the sender.
        assert_eq!(
            resolve_sender(&conn, None, Some("sid"), None, Some("fb".into())),
            None
        );
        // No session_id supplied at all -> legacy active_role fallback.
        assert_eq!(
            resolve_sender(&conn, None, None, None, Some("fb".into())),
            Some("fb".into())
        );
    }
}
