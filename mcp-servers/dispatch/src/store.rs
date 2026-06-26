//! SQLite persistence for dispatch tasks (`dispatch.db`).
//!
//! Mirrors workslate's connection discipline: a single rusqlite `Connection`
//! lives behind a std `Mutex` in the server struct; WAL + `busy_timeout` let
//! multiple dispatch server processes (one per Claude session) share one
//! project DB. Every function here is synchronous and takes `&Connection` (or
//! `&mut Connection` where a transaction is needed); callers lock the mutex
//! briefly and never hold the guard across an `.await` — that keeps the
//! detached executor future `Send`.

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Value, json};

// ── status constants ──────────────────────────────────────

pub const STATUS_QUEUED: &str = "queued";
pub const STATUS_RUNNING: &str = "running";
pub const STATUS_SUCCEEDED: &str = "succeeded";
pub const STATUS_FAILED: &str = "failed";
pub const STATUS_CANCELLED: &str = "cancelled";
pub const STATUS_INTERRUPTED: &str = "interrupted";

/// A task that is still owned by a live executor (queued or running). Used by
/// the per-working_dir concurrency guard and by boot reconciliation.
pub fn is_active(status: &str) -> bool {
    matches!(status, STATUS_QUEUED | STATUS_RUNNING)
}

// ── schema ────────────────────────────────────────────────

pub const SCHEMA_SQL: &str = "\
CREATE TABLE IF NOT EXISTS dispatch_tasks (
    id               TEXT PRIMARY KEY,
    plan_id          TEXT,
    backend          TEXT NOT NULL DEFAULT 'codex',
    working_dir      TEXT NOT NULL,
    title            TEXT,
    spec_json        TEXT NOT NULL,
    prompt           TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'queued',
    model            TEXT,
    reasoning_effort TEXT,
    sandbox          TEXT NOT NULL DEFAULT 'workspace-write',
    backend_version  TEXT,
    argv             TEXT,
    owner_pid        INTEGER,
    owner_instance   TEXT,
    child_pid        INTEGER,
    exit_code        INTEGER,
    result           TEXT,
    error            TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    started_at       TEXT,
    finished_at      TEXT,
    session_id       TEXT,
    rollout_path     TEXT,
    parent_id        TEXT,
    nonce            TEXT,
    rollout_start_line INTEGER
);
CREATE INDEX IF NOT EXISTS idx_dispatch_plan_status ON dispatch_tasks(plan_id, status);
CREATE INDEX IF NOT EXISTS idx_dispatch_dir_status  ON dispatch_tasks(working_dir, status);
CREATE TABLE IF NOT EXISTS dispatch_counters (
    scope   TEXT PRIMARY KEY,
    next_id INTEGER NOT NULL DEFAULT 1
);
";

const COLS: &str = "id, plan_id, backend, working_dir, title, spec_json, prompt, status, \
model, reasoning_effort, sandbox, backend_version, argv, owner_pid, owner_instance, child_pid, \
exit_code, result, error, created_at, started_at, finished_at, session_id, rollout_path, parent_id, \
nonce, rollout_start_line";

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)?;
    // Idempotent upgrade for DBs created before these columns existed. ALTER ...
    // ADD COLUMN errors when the column already exists (fresh DB from SCHEMA_SQL
    // above) — that specific failure is expected and ignored.
    for (col, decl) in [
        ("session_id", "TEXT"),
        ("rollout_path", "TEXT"),
        ("parent_id", "TEXT"),
        ("nonce", "TEXT"),
        ("rollout_start_line", "INTEGER"),
    ] {
        let _ = conn.execute(
            &format!("ALTER TABLE dispatch_tasks ADD COLUMN {col} {decl}"),
            [],
        );
    }
    Ok(())
}

// ── row types ─────────────────────────────────────────────

/// Fields supplied at submit time (the id is allocated by `insert_queued`).
#[derive(Debug, Clone)]
pub struct NewTask {
    pub plan_id: Option<String>,
    pub backend: String,
    pub working_dir: String,
    pub title: Option<String>,
    pub spec_json: String,
    pub prompt: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub sandbox: String,
    pub parent_id: Option<String>,
    /// Per-task identity marker embedded in the rendered prompt, used to positively
    /// match the codex rollout this task produced (None for steer/resume tasks, which
    /// already know their session id).
    pub nonce: Option<String>,
    /// For a steer/resume task: the rollout line count at resume time, so dispatch_logs
    /// can show only the new turn rather than the whole inherited parent session.
    pub rollout_start_line: Option<i64>,
}

/// A full row read back from the DB.
#[derive(Debug, Clone)]
pub struct TaskRow {
    pub id: String,
    pub plan_id: Option<String>,
    pub backend: String,
    pub working_dir: String,
    pub title: Option<String>,
    pub spec_json: String,
    pub prompt: String,
    pub status: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub sandbox: String,
    pub backend_version: Option<String>,
    pub argv: Option<String>,
    pub owner_pid: Option<i64>,
    pub owner_instance: Option<String>,
    pub child_pid: Option<i64>,
    pub exit_code: Option<i64>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub session_id: Option<String>,
    pub rollout_path: Option<String>,
    pub parent_id: Option<String>,
    pub nonce: Option<String>,
    pub rollout_start_line: Option<i64>,
}

fn row_from(r: &Row) -> rusqlite::Result<TaskRow> {
    Ok(TaskRow {
        id: r.get(0)?,
        plan_id: r.get(1)?,
        backend: r.get(2)?,
        working_dir: r.get(3)?,
        title: r.get(4)?,
        spec_json: r.get(5)?,
        prompt: r.get(6)?,
        status: r.get(7)?,
        model: r.get(8)?,
        reasoning_effort: r.get(9)?,
        sandbox: r.get(10)?,
        backend_version: r.get(11)?,
        argv: r.get(12)?,
        owner_pid: r.get(13)?,
        owner_instance: r.get(14)?,
        child_pid: r.get(15)?,
        exit_code: r.get(16)?,
        result: r.get(17)?,
        error: r.get(18)?,
        created_at: r.get(19)?,
        started_at: r.get(20)?,
        finished_at: r.get(21)?,
        session_id: r.get(22)?,
        rollout_path: r.get(23)?,
        parent_id: r.get(24)?,
        nonce: r.get(25)?,
        rollout_start_line: r.get(26)?,
    })
}

impl TaskRow {
    /// JSON view for tool output. `include_result` controls whether the (large)
    /// captured stdout/error is inlined — list responses omit it, status
    /// includes it.
    pub fn to_json(&self, include_result: bool) -> Value {
        let mut v = json!({
            "id": self.id,
            "plan_id": self.plan_id,
            "backend": self.backend,
            "working_dir": self.working_dir,
            "title": self.title,
            "status": self.status,
            "model": self.model,
            "reasoning_effort": self.reasoning_effort,
            "sandbox": self.sandbox,
            "backend_version": self.backend_version,
            "child_pid": self.child_pid,
            "exit_code": self.exit_code,
            "created_at": self.created_at,
            "started_at": self.started_at,
            "finished_at": self.finished_at,
            "parent_id": self.parent_id,
        });
        if include_result {
            v["result"] = json!(self.result);
            v["error"] = json!(self.error);
            v["argv"] = json!(self.argv);
            v["prompt"] = json!(self.prompt);
            v["spec"] = serde_json::from_str::<Value>(&self.spec_json)
                .unwrap_or_else(|_| json!(self.spec_json));
            v["owner_pid"] = json!(self.owner_pid);
            v["owner_instance"] = json!(self.owner_instance);
            v["session_id"] = json!(self.session_id);
            v["rollout_path"] = json!(self.rollout_path);
            v["rollout_start_line"] = json!(self.rollout_start_line);
        }
        v
    }
}

// ── lifecycle ─────────────────────────────────────────────

/// Result of a guarded insert.
pub enum InsertOutcome {
    /// Inserted; carries the new id (e.g. `d-7`).
    Created(String),
    /// Rejected: an active (queued/running) task already targets this working_dir.
    /// Carries that task's id.
    Conflict(String),
}

/// Allocate the next sequential id and insert the task as `queued`, atomically.
///
/// When `enforce_unique_dir` is `Some(dir)`, the per-working_dir concurrency guard
/// is applied *inside the same transaction*: the check and the insert are atomic so
/// two processes cannot both pass the check and both insert. The transaction is
/// `IMMEDIATE` (write lock taken at BEGIN), so a concurrent submit for the same dir
/// blocks on `busy_timeout` until this one commits, then sees the row and is
/// rejected — a `DEFERRED` transaction would take no lock until its first write,
/// letting both submits read an empty result and both insert.
pub fn insert_queued(
    conn: &mut Connection,
    t: &NewTask,
    owner_pid: i64,
    owner_instance: &str,
    enforce_unique_dir: Option<&str>,
) -> rusqlite::Result<InsertOutcome> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Some(dir) = enforce_unique_dir
        && let Some(existing) = active_for_dir(&tx, dir)?
    {
        // tx drops here → rollback; no counter id is burned.
        return Ok(InsertOutcome::Conflict(existing));
    }
    tx.execute(
        "INSERT OR IGNORE INTO dispatch_counters (scope, next_id) VALUES ('global', 1)",
        [],
    )?;
    let n: i64 = tx.query_row(
        "SELECT next_id FROM dispatch_counters WHERE scope='global'",
        [],
        |r| r.get(0),
    )?;
    let id = format!("d-{}", n);
    tx.execute(
        "INSERT INTO dispatch_tasks \
         (id, plan_id, backend, working_dir, title, spec_json, prompt, status, \
          model, reasoning_effort, sandbox, owner_pid, owner_instance, parent_id, \
          nonce, rollout_start_line) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            id,
            t.plan_id,
            t.backend,
            t.working_dir,
            t.title,
            t.spec_json,
            t.prompt,
            STATUS_QUEUED,
            t.model,
            t.reasoning_effort,
            t.sandbox,
            owner_pid,
            owner_instance,
            t.parent_id,
            t.nonce,
            t.rollout_start_line,
        ],
    )?;
    tx.execute(
        "UPDATE dispatch_counters SET next_id = ?1 WHERE scope='global'",
        params![n + 1],
    )?;
    tx.commit()?;
    Ok(InsertOutcome::Created(id))
}

/// Transition queued → running once the child process exists.
pub fn mark_running(
    conn: &Connection,
    id: &str,
    child_pid: Option<i64>,
    argv: &str,
    backend_version: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE dispatch_tasks SET status = ?1, child_pid = ?2, argv = ?3, \
         backend_version = ?4, started_at = datetime('now') WHERE id = ?5",
        params![STATUS_RUNNING, child_pid, argv, backend_version, id],
    )?;
    Ok(())
}

/// Record the codex session id + rollout file path once the executor has located
/// them (used by `dispatch_logs` to read the live log and `dispatch_steer` to resume).
pub fn set_session(
    conn: &Connection,
    id: &str,
    session_id: &str,
    rollout_path: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE dispatch_tasks SET session_id = ?1, rollout_path = ?2 WHERE id = ?3",
        params![session_id, rollout_path, id],
    )?;
    Ok(())
}

/// Write a terminal status (succeeded / failed / cancelled) plus captured output.
pub fn finish(
    conn: &Connection,
    id: &str,
    status: &str,
    exit_code: Option<i64>,
    result: Option<&str>,
    error: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE dispatch_tasks SET status = ?1, exit_code = ?2, result = ?3, error = ?4, \
         finished_at = datetime('now') WHERE id = ?5",
        params![status, exit_code, result, error, id],
    )?;
    Ok(())
}

/// Mark a stranded task (owner process died) as interrupted.
pub fn mark_interrupted(conn: &Connection, id: &str, reason: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE dispatch_tasks SET status = ?1, error = ?2, finished_at = datetime('now') \
         WHERE id = ?3",
        params![STATUS_INTERRUPTED, reason, id],
    )?;
    Ok(())
}

// ── reads ─────────────────────────────────────────────────

pub fn get(conn: &Connection, id: &str) -> rusqlite::Result<Option<TaskRow>> {
    conn.query_row(
        &format!("SELECT {COLS} FROM dispatch_tasks WHERE id = ?1"),
        params![id],
        row_from,
    )
    .optional()
}

/// List tasks, optionally filtered by plan_id and/or status. `rowid` ordering
/// preserves insertion order (the TEXT id sorts lexically, which is wrong).
pub fn list(
    conn: &Connection,
    plan_id: Option<&str>,
    status: Option<&str>,
) -> rusqlite::Result<Vec<TaskRow>> {
    let sql = format!(
        "SELECT {COLS} FROM dispatch_tasks \
         WHERE (?1 IS NULL OR plan_id = ?1) AND (?2 IS NULL OR status = ?2) \
         ORDER BY rowid"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![plan_id, status], row_from)?;
    rows.collect()
}

/// Returns the id of an active (queued/running) task already targeting this
/// canonical working_dir, if any — the per-dir concurrency guard.
pub fn active_for_dir(conn: &Connection, working_dir: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT id FROM dispatch_tasks WHERE working_dir = ?1 \
         AND status IN ('queued', 'running') ORDER BY rowid LIMIT 1",
        params![working_dir],
        |r| r.get(0),
    )
    .optional()
}

/// Active task ids belonging to a plan — for cancel-by-plan.
pub fn active_ids_for_plan(conn: &Connection, plan_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM dispatch_tasks WHERE plan_id = ?1 AND status IN ('queued', 'running') \
         ORDER BY rowid",
    )?;
    let rows = stmt.query_map(params![plan_id], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// (id, owner_pid) for every active task — boot reconciliation reads this and
/// the caller decides which owners are dead (liveness lives in main, which has
/// the platform-specific `kill(pid, 0)`).
pub fn active_owners(conn: &Connection) -> rusqlite::Result<Vec<(String, Option<i64>)>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner_pid FROM dispatch_tasks WHERE status IN ('queued', 'running')",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn sample_task() -> NewTask {
        NewTask {
            plan_id: None,
            backend: "codex".to_string(),
            working_dir: "/tmp/dispatch-guard".to_string(),
            title: None,
            spec_json: "{}".to_string(),
            prompt: "noop".to_string(),
            model: None,
            reasoning_effort: None,
            sandbox: "workspace-write".to_string(),
            parent_id: None,
            nonce: None,
            rollout_start_line: None,
        }
    }

    fn open(path: &std::path::Path) -> Connection {
        let c = Connection::open(path).unwrap();
        c.execute_batch("PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL;")
            .unwrap();
        c
    }

    /// The one-active-run-per-working_dir guard must hold across *separate*
    /// connections racing to insert: IMMEDIATE serializes them so exactly one
    /// wins and the other sees the conflict. (A DEFERRED transaction would let
    /// both read an empty active-dir result and both insert.)
    #[test]
    fn one_active_run_per_dir_is_atomic_across_connections() {
        let path =
            std::env::temp_dir().join(format!("dispatch-guard-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        init(&open(&path)).unwrap();

        let (p1, p2) = (path.clone(), path.clone());
        let h1 = thread::spawn(move || {
            insert_queued(
                &mut open(&p1),
                &sample_task(),
                1,
                "i1",
                Some("/tmp/dispatch-guard"),
            )
            .unwrap()
        });
        let h2 = thread::spawn(move || {
            insert_queued(
                &mut open(&p2),
                &sample_task(),
                2,
                "i2",
                Some("/tmp/dispatch-guard"),
            )
            .unwrap()
        });
        let r1 = h1.join().unwrap();
        let r2 = h2.join().unwrap();

        let created = [&r1, &r2]
            .iter()
            .filter(|r| matches!(r, InsertOutcome::Created(_)))
            .count();
        let conflict = [&r1, &r2]
            .iter()
            .filter(|r| matches!(r, InsertOutcome::Conflict(_)))
            .count();
        assert_eq!(created, 1, "exactly one racing insert should be created");
        assert_eq!(conflict, 1, "the other should see the active-dir conflict");

        let _ = std::fs::remove_file(&path);
    }

    /// allow_concurrent (enforce_unique_dir = None) bypasses the guard entirely.
    #[test]
    fn allow_concurrent_skips_the_guard() {
        let path = std::env::temp_dir().join(format!(
            "dispatch-concurrent-test-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut c = open(&path);
        init(&c).unwrap();
        let a = insert_queued(&mut c, &sample_task(), 1, "i1", None).unwrap();
        let b = insert_queued(&mut c, &sample_task(), 1, "i1", None).unwrap();
        assert!(matches!(a, InsertOutcome::Created(_)));
        assert!(matches!(b, InsertOutcome::Created(_)));
        let _ = std::fs::remove_file(&path);
    }
}
