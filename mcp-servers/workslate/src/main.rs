mod hooks;
mod lenient;
mod msg;

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

use msg::{
    InboxReadParams, MsgSendParams, RegisterParams, SCHEMA_SQL, migrate_db, resolve_sender,
    upsert_session_context,
};

// ── Workslate server ──────────────────────────────────────

#[derive(Clone)]
struct Workslate {
    db: Arc<StdMutex<rusqlite::Connection>>,
    /// This session's registered role (set by workslate_register). The default
    /// sender for msg_send only on the legacy no-session path — under the
    /// composite (session_id, agent_id) identity the env session id alone cannot
    /// disambiguate a subagent from its parent, so session_context is authoritative.
    active_role: Arc<RwLock<Option<String>>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Workslate {
    fn new(db: rusqlite::Connection) -> Self {
        Self {
            db: Arc::new(StdMutex::new(db)),
            active_role: Arc::new(RwLock::new(None)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Register this agent for team messaging — the one-call bootstrap. Maps the composite (session_id, agent_id) identity to your role so (a) messages you send are attributed to you and (b) the PreToolUse inbox doorbell announces unread messages to you mid-turn. Message scope is the Claude session itself — the leader and its teammates share one session_id, so there is no separate session name to pass or remember. session_id comes from the [workslate] SessionStart/SubagentStart hint. agent_id is REQUIRED and explicit: the leader passes an empty string (its own identity); teammates pass the agent_id from their SubagentStart hint — omitting it would overwrite the leader's row. Spawned teammates may already be auto-registered under their agent name by the SubagentStart hook; calling register again just updates the role."
    )]
    async fn workslate_register(
        &self,
        Parameters(params): Parameters<RegisterParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let claude_sid = match params
            .session_id
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(current_claude_session_id)
        {
            Some(s) => s,
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "No session id: pass session_id (the value from the [workslate] SessionStart/SubagentStart hint) or run under Claude Code.".to_string(),
                )]));
            }
        };
        // HARD GUARD: agent_id must be explicit. The leader and all teammates share one
        // session_id; an omitted agent_id defaults to "" and writes the main session's
        // `(session_id, "")` row — its upsert overwrites `role`, so a teammate that
        // forgets agent_id would CLOBBER the leader's role. The leader passes an empty
        // agent_id (its own identity); teammates pass theirs.
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
            // Scope == the Claude session id: the one namespace the whole team shares.
            upsert_session_context(
                &conn,
                &claude_sid,
                &agent_id,
                &claude_sid,
                Some(&params.role),
                false,
            )
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        }
        *self.active_role.write().await = Some(params.role.clone());
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Registered as '{}' (session id: {}, agent_id: {}) — the inbox doorbell is armed; \
             unread messages will be announced before your tool calls.",
            params.role,
            claude_sid,
            if agent_id.is_empty() {
                "<main>"
            } else {
                agent_id.as_str()
            }
        ))]))
    }

    #[tool(
        description = "Send a message directly to a teammate's role inbox. The recipient sees a one-line doorbell on their next tool call — even MID-TURN, which native SendMessage (turn-boundary delivery) cannot do — and reads the body with workslate_inbox_read. Set urgent=true for steering that should interrupt. NOTE: routine messages can just use native SendMessage — the send-bridge hook mirrors those into this inbox automatically; call this tool directly when you need the urgent flag or a custom sender label. Pass your own session_id AND agent_id (the hint values): the leader passes an empty agent_id, teammates pass theirs — when a session is in effect an omitted agent_id is rejected (it would mis-attribute the sender as the leader)."
    )]
    async fn workslate_msg_send(
        &self,
        Parameters(params): Parameters<MsgSendParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let urgent: i64 = if params.urgent.unwrap_or(false) { 1 } else { 0 };
        // Read the active_role fallback BEFORE locking the DB: the async RwLock read
        // must not happen while the std MutexGuard is held (that makes the future !Send).
        let active_role_fallback = self.active_role.read().await.clone();
        // Resolve THIS caller's composite identity. The passed session_id (from the
        // SessionStart/SubagentStart hint) is what keys session_context AND scopes the
        // message; the env id is a degraded fallback that does not match those rows.
        let claude_sid = params
            .session_id
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(current_claude_session_id);
        let Some(scope) = claude_sid.clone() else {
            return Ok(CallToolResult::error(vec![Content::text(
                "No session id: pass session_id (the value from the [workslate] hint) so the \
                 message lands in this session's scope."
                    .to_string(),
            )]));
        };
        // HARD GUARD: the caller is in a (possibly shared) team session where agent_id
        // is the only discriminator. An OMITTED agent_id defaults to the leader's
        // `(session_id, "")` row, silently mis-attributing this message as sent by the
        // leader. Require it explicitly — the leader passes agent_id="" (its own
        // identity), teammates pass their SubagentStart agent_id. An explicit `sender`
        // is the caller taking responsibility, so it bypasses.
        if params.agent_id.is_none() && params.sender.is_none() {
            return Ok(CallToolResult::error(vec![Content::text(
                "workslate_msg_send: agent_id was omitted. The leader and all teammates \
                 share one session_id, so agent_id is what attributes the sender — omitting \
                 it would mis-attribute this message as sent by the leader. Pass agent_id \
                 explicitly: the leader passes an empty agent_id (the main session), \
                 teammates pass the agent_id from their SubagentStart hint (or pass an \
                 explicit `sender`)."
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
                scope,
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
        description = "Read and mark-read all unread messages addressed to your role in this Claude session's scope. Call it when the inbox doorbell reports unread messages (and once after registering). Atomic: concurrent reads will not double-deliver. Note: messages mirrored from native SendMessage by the send-bridge hook will ALSO arrive through native delivery at your next turn boundary — reading them here early is for mid-turn steering; seeing one twice is expected, not a re-send."
    )]
    async fn workslate_inbox_read(
        &self,
        Parameters(params): Parameters<InboxReadParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let Some(scope) = params
            .session_id
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(current_claude_session_id)
        else {
            return Ok(CallToolResult::error(vec![Content::text(
                "No session id: pass session_id (the value from the [workslate] hint) to \
                 select this session's message scope."
                    .to_string(),
            )]));
        };
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
            .query_map(rusqlite::params![scope, params.role], |row| {
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

// ── Helpers ───────────────────────────────────────────────

impl Workslate {
    fn lock_db(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, CallToolResult> {
        self.db.lock().map_err(|e| {
            CallToolResult::error(vec![Content::text(format!(
                "Database lock poisoned: {}",
                e
            ))])
        })
    }
}

// ── ServerHandler (manual, replaces #[tool_handler]) ──────

impl ServerHandler for Workslate {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Mid-turn team messaging for Agent Teams. Native SendMessage delivers only at turn \
             boundaries — a busy teammate cannot be steered mid-task. workslate closes that gap: \
             a PostToolUse bridge hook mirrors native SendMessage calls into a shared SQLite \
             inbox, and a PreToolUse doorbell announces unread messages to the recipient before \
             its next tool call. Message scope is the Claude session id (leader and teammates \
             share one), so there is no session name to set up. Tools: workslate_register \
             (one-call bootstrap — map this agent's (session_id, agent_id) to a role; spawned \
             teammates may already be auto-registered under their agent name), workslate_msg_send \
             (direct send — mainly for urgent=true steering), workslate_inbox_read (drain \
             unread).",
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
    // `workslate --hook=…`: doorbell / bridge subcommands. Sync, prints hook
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

    let server = Workslate::new(conn);
    let transport = rmcp::transport::io::stdio();
    let running = server.serve(transport).await?;
    running.waiting().await?;
    Ok(())
}
