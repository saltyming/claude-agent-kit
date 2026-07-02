mod hooks;
mod lenient;
mod task;

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use rmcp::{
    RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, Content, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    tool, tool_router,
};
use tokio::sync::RwLock;

use task::{
    InboxReadParams, MsgSendParams, Namespace, RegisterParams, SCHEMA_SQL, TaskClearParams,
    TaskCreateParams, TaskDoneParams, TaskId, TaskInitParams, TaskListParams, TaskStatus,
    TaskUpdateParams, load_tasks, migrate_db, recompute_blocked_status, resolve_sender,
    serialize_depends_on,
};
// ── Workslate server ──────────────────────────────────────

#[derive(Clone)]
struct Workslate {
    db: Arc<StdMutex<rusqlite::Connection>>,
    tasks_dir: PathBuf,
    active_session: Arc<RwLock<Option<String>>>,
    /// This session's registered role (set by workslate_register, seeded by
    /// workslate_task_init from the DB). The default sender for msg_send — under the
    /// composite (session_id, agent_id) identity the env session id alone cannot
    /// disambiguate a subagent from its parent, so the role is cached in-process.
    active_role: Arc<RwLock<Option<String>>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Workslate {
    fn new(db: rusqlite::Connection, tasks_dir: PathBuf) -> Self {
        Self {
            db: Arc::new(StdMutex::new(db)),
            tasks_dir,
            active_session: Arc::new(RwLock::new(None)),
            active_role: Arc::new(RwLock::new(None)),
            tool_router: Self::tool_router(),
        }
    }

    // ── Task tools ────────────────────────────────────────

    #[tool(
        description = "Create a task. namespace: 'ws' (default) or 'team'. Returns namespaced ID like ws:1 or team:3."
    )]
    async fn workslate_task_create(
        &self,
        Parameters(params): Parameters<TaskCreateParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await {
            return Ok(e);
        }
        let session = self.active_session.read().await.clone().unwrap();
        let ns_str = params.namespace.as_deref().unwrap_or("ws");
        let ns = match Namespace::parse(ns_str) {
            Ok(n) => n,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let deps: Vec<TaskId> = if let Some(ref dep_strs) = params.depends_on {
            let mut parsed = Vec::new();
            for s in dep_strs {
                match TaskId::parse(s) {
                    Ok(tid) => parsed.push(tid),
                    Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
                }
            }
            parsed
        } else {
            vec![]
        };

        let mut conn = match self.lock_db() {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };

        // Validate dependencies BEFORE opening a transaction: an early return
        // inside an open transaction would leak it onto the shared connection
        // and corrupt later writes from this and other agent instances.
        for dep in &deps {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM tasks WHERE session = ? AND namespace = ? AND id = ?",
                    rusqlite::params![session, dep.namespace.as_str(), dep.id],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if !exists {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "depends_on references non-existent task: {}",
                    dep
                ))]));
            }
        }

        let status = if deps.is_empty() {
            "pending"
        } else {
            "blocked"
        };
        let deps_json = serialize_depends_on(&deps);

        // RAII transaction: any early return below drops `tx`, rolling back.
        let tx = conn
            .transaction()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        tx.execute(
            "INSERT OR IGNORE INTO task_counters (session, namespace, next_id) VALUES (?, ?, 1)",
            rusqlite::params![session, ns.as_str()],
        )
        .ok();
        let id: u32 = tx
            .query_row(
                "SELECT next_id FROM task_counters WHERE session = ? AND namespace = ?",
                rusqlite::params![session, ns.as_str()],
                |row| row.get(0),
            )
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        tx.execute(
            "INSERT INTO tasks (session, namespace, id, name, description, status, owner, depends_on) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![session, ns.as_str(), id, params.name, params.description, status, params.owner, deps_json],
        ).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        tx.execute(
            "UPDATE task_counters SET next_id = ? WHERE session = ? AND namespace = ?",
            rusqlite::params![id + 1, session, ns.as_str()],
        )
        .ok();

        tx.commit()
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        if let Err(e) = recompute_blocked_status(&conn, &session) {
            tracing::warn!("Failed to recompute blocked status: {}", e);
        }

        let task_id = TaskId { namespace: ns, id };
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} created: {}",
            task_id, params.name
        ))]))
    }

    #[tool(
        description = "Mark a task as done. ID format: 3, ws:3, or team:3. Automatically unblocks dependent tasks."
    )]
    async fn workslate_task_done(
        &self,
        Parameters(params): Parameters<TaskDoneParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await {
            return Ok(e);
        }
        let session = self.active_session.read().await.clone().unwrap();
        let tid = match TaskId::parse(&params.id) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        let conn = match self.lock_db() {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };
        let affected = conn.execute(
            "UPDATE tasks SET status = 'done', updated_at = datetime('now') WHERE session = ? AND namespace = ? AND id = ?",
            rusqlite::params![session, tid.namespace.as_str(), tid.id],
        ).unwrap_or(0);

        if affected == 0 {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Task {} not found",
                tid
            ))]));
        }

        let name: String = conn
            .query_row(
                "SELECT name FROM tasks WHERE session = ? AND namespace = ? AND id = ?",
                rusqlite::params![session, tid.namespace.as_str(), tid.id],
                |row| row.get(0),
            )
            .unwrap_or_default();

        if let Err(e) = recompute_blocked_status(&conn, &session) {
            tracing::warn!("Failed to recompute blocked status: {}", e);
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} done: {}",
            tid, name
        ))]))
    }

    #[tool(
        description = "Update a task's status, description, or owner. ID format: 3, ws:3, or team:3."
    )]
    async fn workslate_task_update(
        &self,
        Parameters(params): Parameters<TaskUpdateParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await {
            return Ok(e);
        }
        let session = self.active_session.read().await.clone().unwrap();
        let tid = match TaskId::parse(&params.id) {
            Ok(t) => t,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        if let Some(ref s) = params.status
            && TaskStatus::parse(s).is_err()
        {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Invalid status '{}'. Must be: pending, in_progress, done, blocked",
                s
            ))]));
        }

        let conn = match self.lock_db() {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };

        let (cur_status, cur_desc, cur_owner): (String, Option<String>, Option<String>) = match conn.query_row(
            "SELECT status, description, owner FROM tasks WHERE session = ? AND namespace = ? AND id = ?",
            rusqlite::params![session, tid.namespace.as_str(), tid.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ) {
            Ok(vals) => vals,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Task {} not found", tid
                ))]));
            }
            Err(e) => {
                return Err(rmcp::ErrorData::internal_error(e.to_string(), None));
            }
        };

        let new_status = params.status.unwrap_or(cur_status);
        let new_desc = params.description.or(cur_desc);
        let new_owner = match params.owner {
            Some(ref o) if o.is_empty() => None,
            Some(o) => Some(o),
            None => cur_owner,
        };

        conn.execute(
            "UPDATE tasks SET status = ?, description = ?, owner = ?, updated_at = datetime('now') WHERE session = ? AND namespace = ? AND id = ?",
            rusqlite::params![new_status, new_desc, new_owner, session, tid.namespace.as_str(), tid.id],
        ).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        if let Err(e) = recompute_blocked_status(&conn, &session) {
            tracing::warn!("Failed to recompute blocked status: {}", e);
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Task {} updated",
            tid
        ))]))
    }

    #[tool(description = "List tasks. Optional namespace filter: 'ws', 'team', or omit for all.")]
    async fn workslate_task_list(
        &self,
        Parameters(params): Parameters<TaskListParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await {
            return Ok(e);
        }
        let session = self.active_session.read().await.clone().unwrap();

        let conn = match self.lock_db() {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };
        let tasks = load_tasks(&conn, &session, params.namespace.as_deref())
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        if tasks.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No tasks")]));
        }

        let mut lines = Vec::new();
        for task in &tasks {
            let icon = match task.status {
                TaskStatus::Done => "\u{2713}",
                TaskStatus::InProgress => "\u{2192}",
                TaskStatus::Pending => " ",
                TaskStatus::Blocked => "\u{2298}",
            };
            let owner_str = task
                .owner
                .as_ref()
                .map(|o| format!(" (owner: {})", o))
                .unwrap_or_default();
            let mut line = format!("{} {}. {}{}", icon, task.display_id(), task.name, owner_str);
            if task.status == TaskStatus::InProgress {
                line.push_str("  \u{2190} in_progress");
            }
            if task.status == TaskStatus::Blocked && !task.depends_on.is_empty() {
                let dep_ids: Vec<String> = task.depends_on.iter().map(|d| d.display()).collect();
                line.push_str(&format!("  (blocked by: {})", dep_ids.join(", ")));
            }
            if let Some(ref desc) = task.description {
                line.push_str(&format!("\n    {}", desc));
            }
            lines.push(line);
        }

        Ok(CallToolResult::success(vec![Content::text(
            lines.join("\n"),
        )]))
    }

    #[tool(description = "Clear tasks. Optional namespace: 'ws', 'team', or omit to clear all.")]
    async fn workslate_task_clear(
        &self,
        Parameters(params): Parameters<TaskClearParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await {
            return Ok(e);
        }
        let session = self.active_session.read().await.clone().unwrap();

        let conn = match self.lock_db() {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };
        let count: u32 = if let Some(ref ns) = params.namespace {
            let c = conn
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE session = ? AND namespace = ?",
                    rusqlite::params![session, ns],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            conn.execute(
                "DELETE FROM tasks WHERE session = ? AND namespace = ?",
                rusqlite::params![session, ns],
            )
            .ok();
            conn.execute(
                "UPDATE task_counters SET next_id = 1 WHERE session = ? AND namespace = ?",
                rusqlite::params![session, ns],
            )
            .ok();
            c
        } else {
            let c = conn
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE session = ?",
                    rusqlite::params![session],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            conn.execute(
                "DELETE FROM tasks WHERE session = ?",
                rusqlite::params![session],
            )
            .ok();
            conn.execute(
                "DELETE FROM task_counters WHERE session = ?",
                rusqlite::params![session],
            )
            .ok();
            c
        };

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Cleared {} task(s)",
            count
        ))]))
    }

    #[tool(description = "Switch to a named task session. Creates or opens the session in SQLite.")]
    async fn workslate_task_init(
        &self,
        Parameters(params): Parameters<TaskInitParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let json_path = self.tasks_dir.join(format!("tasks-{}.json", params.name));

        let task_count = {
            let conn = match self.lock_db() {
                Ok(c) => c,
                Err(e) => return Ok(e),
            };

            let existing_count: u32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE session = ?",
                    rusqlite::params![params.name],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            if existing_count == 0
                && let Ok(json) = std::fs::read_to_string(&json_path)
                && let Ok(old_store) = serde_json::from_str::<serde_json::Value>(&json)
                && let Some(tasks) = old_store.get("tasks").and_then(|t| t.as_array())
            {
                for task_val in tasks {
                    let id = task_val.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let name = task_val.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let desc = task_val.get("description").and_then(|v| v.as_str());
                    let status = task_val
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("pending");
                    let deps = task_val
                        .get("depends_on")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            let ids: Vec<TaskId> = arr
                                .iter()
                                .filter_map(|v| v.as_u64())
                                .map(|n| TaskId {
                                    namespace: Namespace::Ws,
                                    id: n as u32,
                                })
                                .collect();
                            serialize_depends_on(&ids)
                        })
                        .unwrap_or_else(|| "[]".to_string());

                    conn.execute(
                                    "INSERT OR IGNORE INTO tasks (session, namespace, id, name, description, status, depends_on) VALUES (?, 'ws', ?, ?, ?, ?, ?)",
                                    rusqlite::params![params.name, id, name, desc, status, deps],
                                ).ok();
                }
                let next_id = old_store
                    .get("next_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32;
                conn.execute(
                                "INSERT OR REPLACE INTO task_counters (session, namespace, next_id) VALUES (?, 'ws', ?)",
                                rusqlite::params![params.name, next_id],
                            ).ok();
                tracing::info!("Migrated session '{}' from JSON to SQLite", params.name);
            }

            conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE session = ?",
                rusqlite::params![params.name],
                |row| row.get(0),
            )
            .unwrap_or(0u32)
        };

        *self.active_session.write().await = Some(params.name.clone());

        // Record this Claude session's task_session in session_context so the
        // doorbell hooks can resolve it even before workslate_register sets a
        // role. Keeps any existing role. Best-effort: skipped when
        // CLAUDE_CODE_SESSION_ID is unset (e.g. not running under Claude Code).
        if let Some(claude_sid) = params
            .session_id
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(current_claude_session_id)
        {
            let agent_id = params.agent_id.clone().unwrap_or_default();
            // Scope the DB guard so it drops before the async active_role write below
            // (a std MutexGuard held across .await makes the tool future !Send).
            let preserved: Option<String> = if let Ok(conn) = self.lock_db() {
                conn.execute(
                    "INSERT INTO session_context (claude_session_id, agent_id, task_session, role, updated_at) \
                     VALUES (?, ?, ?, NULL, datetime('now')) \
                     ON CONFLICT(claude_session_id, agent_id) DO UPDATE SET \
                         task_session = excluded.task_session, updated_at = datetime('now')",
                    rusqlite::params![claude_sid, agent_id, params.name],
                ).ok();
                conn.query_row(
                    "SELECT role FROM session_context WHERE claude_session_id = ? AND agent_id = ?",
                    rusqlite::params![claude_sid, agent_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten()
            } else {
                None
            };
            // Seed active_role from a role preserved across an MCP restart so msg_send
            // sender attribution survives a task_init that does not re-call register.
            if let Some(r) = preserved {
                *self.active_role.write().await = Some(r);
            }
        }

        let msg = if task_count > 0 {
            format!(
                "Switched to session '{}' ({} tasks)",
                params.name, task_count
            )
        } else {
            format!("Created new session '{}'", params.name)
        };
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(description = "List all available task sessions")]
    async fn workslate_task_sessions(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let active = self.active_session.read().await.clone();
        let conn = self.db.lock().map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Database lock poisoned: {}", e), None)
        })?;

        let mut stmt = conn.prepare(
            "SELECT session, namespace, COUNT(*), SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END) \
             FROM tasks GROUP BY session, namespace ORDER BY session, namespace"
        ).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        let rows: Vec<(String, String, u32, u32)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
            .filter_map(|r| r.ok())
            .collect();

        if rows.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No sessions")]));
        }

        let mut sessions: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for (session, ns, total, done) in &rows {
            sessions
                .entry(session.clone())
                .or_default()
                .push(format!("{}:[{}/{}]", ns, done, total));
        }

        let mut lines = Vec::new();
        for (session, counters) in &sessions {
            let is_active = active.as_ref().map(|a| a == session).unwrap_or(false);
            let marker = if is_active { " \u{2190} active" } else { "" };
            lines.push(format!("  {} {}{}", session, counters.join(" "), marker));
        }

        Ok(CallToolResult::success(vec![Content::text(
            lines.join("\n"),
        )]))
    }

    // ── Team messaging tools ──────────────────────────────

    #[tool(
        description = "Register your role name for the active task session so the inbox/task doorbell hooks can resolve which agent this Claude session is. Teammates call this once on startup (with the same session name the leader used), alongside workslate_inbox_read. Pass agent_id explicitly: the leader passes an empty agent_id (its own identity), teammates pass their SubagentStart agent_id — omitting it is rejected, since it would default to the leader's row and a teammate would overwrite the leader's role."
    )]
    async fn workslate_register(
        &self,
        Parameters(params): Parameters<RegisterParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await {
            return Ok(e);
        }
        let session = self.active_session.read().await.clone().unwrap();
        let claude_sid = match params
            .session_id
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(current_claude_session_id)
        {
            Some(s) => s,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "No session id: pass session_id (the value from the workslate SessionStart hint) or run under Claude Code.".to_string(),
                )]));
            }
        };
        // HARD GUARD: agent_id must be explicit. The leader and all teammates share one
        // session_id; an omitted agent_id defaults to "" and writes the main session's
        // `(session_id, "")` row — its ON CONFLICT DO UPDATE overwrites `role`, so a
        // teammate that forgets agent_id would CLOBBER the leader's `team-lead` role.
        // The leader passes an empty agent_id (its own identity); teammates pass theirs.
        if params.agent_id.is_none() {
            return Ok(CallToolResult::error(vec![Content::text(
                "workslate_register: agent_id is required — pass it explicitly. The leader \
                 and all teammates share one session_id, so an omitted agent_id defaults to \
                 the main session's row (empty agent_id) and a teammate would overwrite the \
                 leader's role. The leader passes an empty agent_id (its own identity); \
                 teammates pass the agent_id from their SubagentStart hint."
                    .to_string(),
            )]));
        }
        let agent_id = params.agent_id.clone().unwrap_or_default();
        {
            // Scope the DB guard so it is released before the async active_role write.
            let conn = match self.lock_db() {
                Ok(c) => c,
                Err(e) => return Ok(e),
            };
            conn.execute(
                "INSERT INTO session_context (claude_session_id, agent_id, task_session, role, updated_at) \
                 VALUES (?, ?, ?, ?, datetime('now')) \
                 ON CONFLICT(claude_session_id, agent_id) DO UPDATE SET \
                     task_session = excluded.task_session, role = excluded.role, updated_at = datetime('now')",
                rusqlite::params![claude_sid, agent_id, session, params.role],
            ).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        }
        *self.active_role.write().await = Some(params.role.clone());
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Registered as '{}' in session '{}' (session id: {}, agent_id: {})",
            params.role,
            session,
            claude_sid,
            if agent_id.is_empty() {
                "<main>"
            } else {
                agent_id.as_str()
            }
        ))]))
    }

    #[tool(
        description = "Send a message to a teammate's role inbox in the active task session. The recipient sees a one-line doorbell on their next tool call and reads the body with workslate_inbox_read. Set urgent=true for mid-task steering that should interrupt. Pass your own session_id AND agent_id (the SessionStart/SubagentStart hint values) for correct sender attribution — when a session is in effect an omitted agent_id is rejected, since the leader and teammates share one session_id and a missing agent_id would mis-attribute the message as 'team-lead' (the leader passes an empty agent_id)."
    )]
    async fn workslate_msg_send(
        &self,
        Parameters(params): Parameters<MsgSendParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await {
            return Ok(e);
        }
        let session = self.active_session.read().await.clone().unwrap();
        let urgent: i64 = if params.urgent.unwrap_or(false) { 1 } else { 0 };
        // Read the active_role fallback BEFORE locking the DB: the async RwLock read
        // must not happen while the std MutexGuard is held (that makes the future !Send).
        let active_role_fallback = self.active_role.read().await.clone();
        // Resolve THIS caller's composite identity. The passed session_id (from the
        // SessionStart/SubagentStart hint) is what keys session_context; the env id is
        // a degraded fallback that does not match those rows. agent_id tells a teammate
        // apart from the leader within one shared MCP server process.
        let claude_sid = params
            .session_id
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(current_claude_session_id);
        // HARD GUARD: when a session_id is in effect (passed or env-resolved) the caller
        // is in a (possibly shared) team session where agent_id is the only discriminator.
        // An OMITTED agent_id defaults to the leader's `(session_id, "")` row, silently
        // mis-attributing this message as sent by `team-lead`. Require it explicitly — the
        // leader passes agent_id="" (its own identity), teammates pass their SubagentStart
        // agent_id. An explicit `sender` is the caller taking responsibility, so it bypasses.
        if claude_sid.is_some() && params.agent_id.is_none() && params.sender.is_none() {
            return Ok(CallToolResult::error(vec![Content::text(
                "workslate_msg_send: a session_id is in effect but agent_id was omitted. \
                 The leader and all teammates share one session_id, so agent_id is what \
                 attributes the sender — omitting it would mis-attribute this message as \
                 sent by 'team-lead'. Pass agent_id explicitly: the leader passes an empty \
                 agent_id (the main session), teammates pass the agent_id from their \
                 SubagentStart hint (or pass an explicit `sender`)."
                    .to_string(),
            )]));
        }
        let conn = match self.lock_db() {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };
        // Sender attribution via the composite (session_id, agent_id) identity, not a
        // single in-process role cache (last-writer-wins when leader + teammates share
        // this one server process). A miss yields NULL, never the cache; an omitted
        // agent_id is rejected by the guard above before reaching here.
        let sender = resolve_sender(
            &conn,
            params.sender,
            claude_sid.as_deref(),
            params.agent_id.as_deref(),
            active_role_fallback,
        );
        conn.execute(
            "INSERT INTO messages (task_session, recipient_role, sender, subject, body, urgent) \
             VALUES (?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                session,
                params.recipient,
                sender,
                params.subject,
                params.body,
                urgent
            ],
        )
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Message sent to '{}': {}",
            params.recipient, params.subject
        ))]))
    }

    #[tool(
        description = "Read and mark-read all unread messages addressed to your role in the active task session. Call on startup and whenever the inbox doorbell reports unread messages. Atomic: concurrent reads will not double-deliver."
    )]
    async fn workslate_inbox_read(
        &self,
        Parameters(params): Parameters<InboxReadParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if let Err(e) = self.require_session().await {
            return Ok(e);
        }
        let session = self.active_session.read().await.clone().unwrap();
        let conn = match self.lock_db() {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };

        // Single atomic UPDATE ... RETURNING: marks unread messages read and
        // returns them in one statement, so two concurrent readers of the same
        // role cannot both receive the same message.
        let mut stmt = conn
            .prepare(
                "UPDATE messages SET read_at = datetime('now') \
             WHERE task_session = ? AND recipient_role = ? AND read_at IS NULL \
             RETURNING id, sender, subject, body, urgent, created_at",
            )
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let mut msgs: Vec<(i64, Option<String>, String, String, i64, String)> = stmt
            .query_map(rusqlite::params![session, params.role], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        msgs.sort_by_key(|m| m.0);

        if msgs.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No unread messages for '{}'",
                params.role
            ))]));
        }

        let mut out = format!("{} unread message(s) for '{}':\n", msgs.len(), params.role);
        for (_, sender, subject, body, urgent, created) in &msgs {
            let flag = if *urgent != 0 { "🚨 " } else { "" };
            let from = sender.as_deref().unwrap_or("unknown");
            out.push_str(&format!(
                "\n{}[{}]  from {}  ({})\n{}\n",
                flag, subject, from, created, body
            ));
        }
        Ok(CallToolResult::success(vec![Content::text(out)]))
    }
}

// ── Task helpers ──────────────────────────────────────────

impl Workslate {
    fn lock_db(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, CallToolResult> {
        self.db.lock().map_err(|e| {
            CallToolResult::error(vec![Content::text(format!(
                "Database lock poisoned: {}",
                e
            ))])
        })
    }

    async fn require_session(&self) -> Result<(), CallToolResult> {
        let session = self.active_session.read().await;
        if session.is_none() {
            return Err(CallToolResult::error(vec![Content::text(
                "No active task session. Call workslate_task_init(name) first.".to_string(),
            )]));
        }
        Ok(())
    }
}

// ── ServerHandler (manual, replaces #[tool_handler]) ──────

impl ServerHandler for Workslate {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "SQLite-backed task tracking with ws: and team: namespaces, named resumable sessions, \
             and WAL concurrency for multiple agents. \
             Team messaging: workslate_register (map your session to a role), workslate_msg_send, \
             and workslate_inbox_read. When the PreToolUse doorbell hooks are installed, task status \
             and unread-message alerts are injected before every tool call.",
        )
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tcc = ToolCallContext::new(self, request, context);
        let result = self.tool_router.call(tcc).await?;
        Ok(result)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }
}

// ── main ──────────────────────────────────────────────────

/// The Claude Code session id this MCP server process is serving, read from
/// the `CLAUDE_CODE_SESSION_ID` env var Claude Code injects when it spawns the
/// server. Keys `session_context` so the doorbell hooks (which receive the same
/// session id on their stdin) can resolve this session to a role.
fn current_claude_session_id() -> Option<String> {
    std::env::var("CLAUDE_CODE_SESSION_ID")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Resolve the workslate data directory for the current project.
///
/// Shared by the MCP server (launched in the project cwd) and the hook
/// subcommands (spawned by Claude Code with an arbitrary cwd). Both anchor on
/// `CLAUDE_PROJECT_DIR` so the path stays stable even if the session cwd
/// diverges from the MCP's launch cwd; `fallback` (current_dir for the server,
/// hook-input cwd for hooks) is used only when `CLAUDE_PROJECT_DIR` is unset.
/// The path encoding matches the historical current_dir-based layout, so
/// existing databases are found unchanged when CLAUDE_PROJECT_DIR == cwd.
pub(crate) fn resolve_tasks_dir(fallback: &std::path::Path) -> PathBuf {
    let project_dir = std::env::var("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| fallback.to_path_buf());
    let project_path = project_dir.to_string_lossy().replace('/', "-");
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    home.join(".claude")
        .join("projects")
        .join(&project_path)
        .join("workslate")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `workslate --hook=task|inbox`: PreToolUse doorbell. Sync, prints hook
    // JSON to stdout and exits without starting the MCP server.
    let args: Vec<String> = std::env::args().collect();
    if let Some(mode) = hooks::parse_hook_mode(&args) {
        hooks::run(mode);
        return Ok(());
    }
    if args.iter().any(|a| a == "--install-hooks") {
        return hooks::install_hooks();
    }
    if args.iter().any(|a| a == "--uninstall-hooks") {
        return hooks::uninstall_hooks();
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cwd = std::env::current_dir()?;
    let tasks_dir = resolve_tasks_dir(&cwd);
    tokio::fs::create_dir_all(&tasks_dir).await?;

    let old_db_path = tasks_dir.join("workslate-tasks.db");
    let db_path = tasks_dir.join("workslate.db");
    if old_db_path.exists() && !db_path.exists() {
        std::fs::rename(&old_db_path, &db_path).ok();
    }
    let mut conn = rusqlite::Connection::open(&db_path)?;
    // busy_timeout first so the WAL-mode switch below waits out a concurrent
    // writer instead of failing with "database is locked" — multiple teammate
    // MCP servers can open this DB at the same moment.
    conn.execute_batch("PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL;")?;
    conn.execute_batch(SCHEMA_SQL)?;
    migrate_db(&mut conn)?;

    let server = Workslate::new(conn, tasks_dir);
    let transport = rmcp::transport::io::stdio();
    let running = server.serve(transport).await?;
    running.waiting().await?;
    Ok(())
}
