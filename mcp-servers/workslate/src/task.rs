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
";

/// Apply schema migrations to an existing database. Runs after SCHEMA_SQL,
/// which is idempotent (CREATE TABLE IF NOT EXISTS). Migrations handle cases
/// where an older DB exists without newer columns.
///
/// Each migration must be idempotent — safe to re-run on an already-migrated DB.
pub fn migrate_db(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
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
    Ok(())
}
