use std::collections::HashSet;
use std::fmt;

use schemars::JsonSchema;
use serde::Deserialize;

// ── Namespace + TaskId ───────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Namespace {
    Ws,
    Team,
}

impl Namespace {
    pub fn as_str(&self) -> &str {
        match self {
            Namespace::Ws => "ws",
            Namespace::Team => "team",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "ws" => Ok(Namespace::Ws),
            "team" => Ok(Namespace::Team),
            other => Err(format!(
                "Unknown namespace '{}'. Must be 'ws' or 'team'",
                other
            )),
        }
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId {
    pub namespace: Namespace,
    pub id: u32,
}

impl TaskId {
    pub fn parse(s: &str) -> Result<Self, String> {
        if let Some((ns, id_str)) = s.split_once(':') {
            let namespace = Namespace::parse(ns)?;
            let id = id_str
                .parse::<u32>()
                .map_err(|_| format!("Invalid task ID number: '{}'", id_str))?;
            Ok(TaskId { namespace, id })
        } else {
            let id = s
                .parse::<u32>()
                .map_err(|_| format!("Invalid task ID: '{}'. Use N, ws:N, or team:N", s))?;
            Ok(TaskId {
                namespace: Namespace::Ws,
                id,
            })
        }
    }

    pub fn display(&self) -> String {
        format!("{}:{}", self.namespace.as_str(), self.id)
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.id)
    }
}

// ── Task status ──────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
}

impl TaskStatus {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(TaskStatus::Pending),
            "in_progress" => Ok(TaskStatus::InProgress),
            "done" => Ok(TaskStatus::Done),
            "blocked" => Ok(TaskStatus::Blocked),
            other => Err(format!(
                "Invalid status '{}'. Must be: pending, in_progress, done, blocked",
                other
            )),
        }
    }
}

// ── Task (loaded from SQLite) ────────────────────────────

#[derive(Debug, Clone)]
pub struct Task {
    pub namespace: Namespace,
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub owner: Option<String>,
    pub depends_on: Vec<TaskId>,
}

impl Task {
    pub fn display_id(&self) -> String {
        format!("{}:{}", self.namespace, self.id)
    }
}

// ── SQLite helpers ───────────────────────────────────────

pub fn parse_depends_on(json_str: &str) -> Vec<TaskId> {
    serde_json::from_str::<Vec<String>>(json_str)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| TaskId::parse(s).ok())
        .collect()
}

pub fn serialize_depends_on(deps: &[TaskId]) -> String {
    let strings: Vec<String> = deps.iter().map(|d| d.display()).collect();
    serde_json::to_string(&strings).unwrap_or_else(|_| "[]".to_string())
}

pub fn recompute_blocked_status(
    conn: &rusqlite::Connection,
    session: &str,
) -> rusqlite::Result<()> {
    let done_ids: HashSet<String> = {
        let mut stmt =
            conn.prepare("SELECT namespace, id FROM tasks WHERE session = ? AND status = 'done'")?;
        let rows = stmt.query_map(rusqlite::params![session], |row| {
            let ns: String = row.get(0)?;
            let id: u32 = row.get(1)?;
            Ok(format!("{}:{}", ns, id))
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut stmt = conn.prepare(
        "SELECT namespace, id, depends_on FROM tasks WHERE session = ? AND status IN ('pending', 'blocked')",
    )?;
    let updatable: Vec<(String, u32, String)> = stmt
        .query_map(rusqlite::params![session], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut update_stmt = conn.prepare(
        "UPDATE tasks SET status = ?, updated_at = datetime('now') WHERE session = ? AND namespace = ? AND id = ?",
    )?;

    for (ns, id, deps_json) in &updatable {
        let deps = parse_depends_on(deps_json);
        let new_status = if deps.is_empty() || deps.iter().all(|d| done_ids.contains(&d.display()))
        {
            "pending"
        } else {
            "blocked"
        };
        update_stmt.execute(rusqlite::params![new_status, session, ns, id])?;
    }

    Ok(())
}

pub fn load_tasks(
    conn: &rusqlite::Connection,
    session: &str,
    namespace_filter: Option<&str>,
) -> rusqlite::Result<Vec<Task>> {
    let sql = if namespace_filter.is_some() {
        "SELECT namespace, id, name, description, status, owner, depends_on \
         FROM tasks WHERE session = ? AND namespace = ? ORDER BY namespace, id"
    } else {
        "SELECT namespace, id, name, description, status, owner, depends_on \
         FROM tasks WHERE session = ? ORDER BY namespace, id"
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = if let Some(ns) = namespace_filter {
        stmt.query_map(rusqlite::params![session, ns], row_to_task)?
    } else {
        stmt.query_map(rusqlite::params![session], row_to_task)?
    };

    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    let ns_str: String = row.get(0)?;
    let id: u32 = row.get(1)?;
    let name: String = row.get(2)?;
    let description: Option<String> = row.get(3)?;
    let status_str: String = row.get(4)?;
    let owner: Option<String> = row.get(5)?;
    let deps_json: String = row.get(6)?;

    Ok(Task {
        namespace: Namespace::parse(&ns_str).unwrap_or(Namespace::Ws),
        id,
        name,
        description,
        status: TaskStatus::parse(&status_str).unwrap_or(TaskStatus::Pending),
        owner,
        depends_on: parse_depends_on(&deps_json),
    })
}

// ── Task param structs ───────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskCreateParams {
    /// Name/title of the task
    pub name: String,
    /// Optional description with more detail
    pub description: Option<String>,
    /// Task IDs this depends on (JSON array of strings, e.g. `["ws:1", "team:2"]`).
    /// Must be a JSON array — do NOT pass a stringified array like `"[\"ws:1\"]"`.
    /// Supports ID forms: "3", "ws:3", "team:2".
    #[serde(default, deserialize_with = "crate::lenient::lenient_opt_vec_string")]
    pub depends_on: Option<Vec<String>>,
    /// Namespace: "ws" (default) or "team"
    pub namespace: Option<String>,
    /// Owner name (for team tasks — who owns/claims this task)
    pub owner: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskDoneParams {
    /// Task ID: "3", "ws:3", or "team:3"
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskUpdateParams {
    /// Task ID: "3", "ws:3", or "team:3"
    pub id: String,
    /// New status: pending, in_progress, done, blocked
    pub status: Option<String>,
    /// New description
    pub description: Option<String>,
    /// New owner (for team tasks)
    pub owner: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskInitParams {
    /// Name of the task session (e.g., "auth-refactor")
    pub name: String,
    /// This Claude session's id (from the workslate SessionStart hint). Pass it
    /// so the task-status doorbell can resolve this session; falls back to the
    /// server env id when omitted. See RegisterParams.session_id.
    pub session_id: Option<String>,
    /// This subagent's agent_id (from the workslate SubagentStart hint). Subagents
    /// share the parent's CLAUDE_CODE_SESSION_ID, so agent_id is what distinguishes a
    /// teammate from the main session in the composite (claude_session_id, agent_id)
    /// identity. Empty/omitted for the main session (its hook stdin carries none).
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskListParams {
    /// Filter by namespace: "ws", "team", or omit for all
    pub namespace: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskClearParams {
    /// Clear only this namespace: "ws", "team", or omit to clear all
    pub namespace: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegisterParams {
    /// Your role name (e.g. "backend-dev"). The durable identity that messages
    /// are addressed to; maps this Claude session to the role for the doorbell hooks.
    pub role: String,
    /// This Claude session's id — the value the workslate SessionStart hint gave
    /// you (`[workslate] session_id=...`). Pass it so the doorbell hooks (which see
    /// the conversation session id on their stdin) can resolve this session. The
    /// MCP server's own env id does NOT match the hook's, so this must come from
    /// the hint. Falls back to the server env id only when omitted.
    pub session_id: Option<String>,
    /// This subagent's agent_id (from the workslate SubagentStart hint). With
    /// session_id it forms the composite identity; distinguishes a teammate from the
    /// parent session (they share CLAUDE_CODE_SESSION_ID). Empty/omitted for the main
    /// session.
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MsgSendParams {
    /// Recipient role name (the owner/role the message is addressed to)
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
    /// hint). With agent_id, identifies THIS caller so its registered role can be
    /// used as the sender. Needed for correct attribution in a shared-process team
    /// (leader + teammates share one MCP server, so the in-process role cache alone
    /// is last-writer-wins). Falls back to the server env id when omitted.
    pub session_id: Option<String>,
    /// This subagent's agent_id (from the workslate SubagentStart hint). The second
    /// half of the composite identity that tells a teammate apart from the parent
    /// session (they share CLAUDE_CODE_SESSION_ID). Empty/omitted for the main session.
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InboxReadParams {
    /// Your role name. Returns unread messages addressed to this role and marks them read.
    pub role: String,
}

// ── Task footer rendering ────────────────────────────────

pub fn render_task_footer(tasks: &[Task], session: &str, buffer_names: &[String]) -> String {
    if tasks.is_empty() && buffer_names.is_empty() {
        return String::new();
    }

    let ws_total = tasks.iter().filter(|t| t.namespace == Namespace::Ws).count();
    let ws_done = tasks
        .iter()
        .filter(|t| t.namespace == Namespace::Ws && t.status == TaskStatus::Done)
        .count();
    let team_total = tasks
        .iter()
        .filter(|t| t.namespace == Namespace::Team)
        .count();
    let team_done = tasks
        .iter()
        .filter(|t| t.namespace == Namespace::Team && t.status == TaskStatus::Done)
        .count();

    let mut counters = Vec::new();
    if ws_total > 0 {
        counters.push(format!("ws:[{}/{}]", ws_done, ws_total));
    }
    if team_total > 0 {
        counters.push(format!("team:[{}/{}]", team_done, team_total));
    }
    let counter_str = counters.join(" ");

    let mut lines = Vec::new();
    lines.push(format!(
        "── Tasks ({}) {} ──────────────────────────",
        session, counter_str
    ));

    let total_done = ws_done + team_done;
    if total_done >= 3 {
        let mut parts = Vec::new();
        if ws_done > 0 {
            parts.push(format!("{} ws", ws_done));
        }
        if team_done > 0 {
            parts.push(format!("{} team", team_done));
        }
        lines.push(format!("  ✓ {} done", parts.join(", ")));
    } else {
        for task in tasks.iter().filter(|t| t.status == TaskStatus::Done) {
            lines.push(format!("  ✓ {}. {}", task.display_id(), task.name));
        }
    }

    let mut remaining_slots: usize = 3;
    for task in tasks
        .iter()
        .filter(|t| t.status == TaskStatus::InProgress)
    {
        if remaining_slots == 0 {
            break;
        }
        let owner_str = task
            .owner
            .as_ref()
            .map(|o| format!(" (owner: {})", o))
            .unwrap_or_default();
        lines.push(format!(
            "  → {}.{}  {} ← in_progress",
            task.display_id(),
            task.name,
            owner_str
        ));
        remaining_slots -= 1;
    }

    let pending_blocked: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::Blocked)
        .collect();

    let show_count = remaining_slots.min(pending_blocked.len());
    for task in pending_blocked.iter().take(show_count) {
        let owner_str = task
            .owner
            .as_ref()
            .map(|o| format!(" (owner: {})", o))
            .unwrap_or_default();
        let mut line = format!("    {}.{}{}", task.display_id(), task.name, owner_str);
        if task.status == TaskStatus::Blocked && !task.depends_on.is_empty() {
            let dep_ids: Vec<String> = task.depends_on.iter().map(|d| d.display()).collect();
            line.push_str(&format!("  (blocked by: {})", dep_ids.join(", ")));
        }
        lines.push(line);
    }

    let hidden = pending_blocked.len().saturating_sub(show_count);
    if hidden > 0 {
        lines.push(format!("    ... and {} more", hidden));
    }

    if !buffer_names.is_empty() {
        const MAX_NAMES: usize = 5;
        let mut sorted: Vec<&String> = buffer_names.iter().collect();
        sorted.sort();
        let shown: Vec<String> = sorted
            .iter()
            .take(MAX_NAMES)
            .map(|s| (*s).clone())
            .collect();
        let overflow = sorted.len().saturating_sub(MAX_NAMES);
        let list = if overflow > 0 {
            format!("{}, +{} more", shown.join(", "), overflow)
        } else {
            shown.join(", ")
        };
        lines.push(format!(
            "── Buffers: {} staged ({}) ──",
            buffer_names.len(),
            list
        ));
    }

    lines.push("──────────────────────────────────────────".to_string());
    lines.join("\n")
}

// ── Schema initialization ────────────────────────────────

pub const SCHEMA_SQL: &str = "\
CREATE TABLE IF NOT EXISTS tasks (
    session    TEXT    NOT NULL,
    namespace  TEXT    NOT NULL DEFAULT 'ws',
    id         INTEGER NOT NULL,
    name       TEXT    NOT NULL,
    description TEXT,
    status     TEXT    NOT NULL DEFAULT 'pending',
    owner      TEXT,
    depends_on TEXT    NOT NULL DEFAULT '[]',
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (session, namespace, id)
);

CREATE INDEX IF NOT EXISTS idx_tasks_session_ns_status
    ON tasks(session, namespace, status);

CREATE TABLE IF NOT EXISTS task_counters (
    session   TEXT NOT NULL,
    namespace TEXT NOT NULL,
    next_id   INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (session, namespace)
);

CREATE TABLE IF NOT EXISTS buffers (
    name        TEXT PRIMARY KEY,
    content     TEXT    NOT NULL,
    file_path   TEXT,
    depends_on  TEXT    NOT NULL DEFAULT '[]',
    source_hash TEXT,
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

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
const SESSION_CONTEXT_DDL: &str = "\
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
    // v8.3: add buffers.source_hash for stale buffer detection
    let has_source_hash = {
        let mut stmt = conn.prepare("PRAGMA table_info(buffers)")?;
        let mut found = false;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let col_name: String = row.get(1)?;
            if col_name == "source_hash" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_source_hash {
        conn.execute("ALTER TABLE buffers ADD COLUMN source_hash TEXT", [])?;
    }

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
/// as they do for register/task_init.
pub fn resolve_sender(
    conn: &rusqlite::Connection,
    explicit: Option<String>,
    claude_session_id: Option<&str>,
    agent_id: &str,
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
    fn migrate_rebuilds_session_context_to_composite_pk() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        // Base schema (no session_context — it now lives in SESSION_CONTEXT_DDL), then
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
            resolve_sender(&conn, Some("explicit".into()), Some("sid"), "", Some("fb".into())),
            Some("explicit".into())
        );
        // Composite lookup: same session id, agent_id='' -> leader.
        assert_eq!(
            resolve_sender(&conn, None, Some("sid"), "", Some("fb".into())),
            Some("leader".into())
        );
        // Composite lookup: same session id, distinct agent_id -> teammate. A single
        // in-process role cache would mis-attribute this to the last registrant.
        assert_eq!(
            resolve_sender(&conn, None, Some("sid"), "agentX", Some("fb".into())),
            Some("teammate".into())
        );
        // session_id supplied but no matching row -> None (NOT the process-shared
        // active_role, which would reintroduce mis-attribution on a miss).
        assert_eq!(
            resolve_sender(&conn, None, Some("sid"), "unknown", Some("fb".into())),
            None
        );
        // No session_id supplied at all -> legacy active_role fallback.
        assert_eq!(
            resolve_sender(&conn, None, None, "", Some("fb".into())),
            Some("fb".into())
        );
    }
}
