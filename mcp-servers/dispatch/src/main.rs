//! dispatch — hierarchical delegation MCP server.
//!
//! Where `aside` asks another model family for *judgment* (read-only), dispatch
//! hands another agent *execution*: it runs Codex as a headless, WRITE-CAPABLE
//! subprocess that modifies files in a target directory, so a planning agent can
//! offload individual plan steps and keep working. MCP calls are request/response
//! but a delegated run takes minutes, so the model is submit → poll → cancel:
//! `dispatch_submit` returns an id immediately and the run continues detached.
//!
//! Safety is split: the server enforces mechanical invariants the model cannot
//! talk its way past (working_dir containment, sandbox ceiling, one-active-run
//! per dir); the *when to ask the user* policy lives in claude-agent-kit--dispatch.md.

mod backend;
mod executor;
mod lenient;
mod opencode;
mod params;
#[cfg(unix)]
mod pdeath_guard;
mod render;
mod rollout;
mod store;
#[cfg(windows)]
mod winjob;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime};

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
use serde_json::{Value, json};

use backend::Backend;
use params::{
    BackendsParams, CancelParams, ListParams, LogsParams, StatusParams, SteerParams, SubmitParams,
    WaitParams,
};

/// How long `dispatch_steer` waits for the parent run to actually terminate after
/// cancel before giving up (so the working_dir is free for the resume run).
const STEER_TERMINATE_WAIT_MS: u64 = 30_000;

/// `dispatch_wait` ceiling: a bounded long-poll, never an unbounded block — a held MCP
/// call would hit the client/harness request timeout. The caller re-invokes if it
/// times out. `WaitParams.timeout_ms` is clamped to this.
const WAIT_MAX_TIMEOUT_MS: u64 = 120_000;
const WAIT_DEFAULT_TIMEOUT_MS: u64 = 30_000;
const WAIT_POLL_INTERVAL_MS: u64 = 300;
const WAIT_LOG_TAIL_LINES: usize = 30;
const WAIT_LOG_BYTE_CAP: usize = 8 * 1024;

/// Tolerance subtracted from a legacy task's start time when validating its rollout by
/// mtime — absorbs clock / ordering skew between the DB timestamp and the file.
const FLOOR_TOLERANCE_SECS: u64 = 60;

// ── server ────────────────────────────────────────────────

#[derive(Clone)]
struct Dispatch {
    db: executor::DbHandle,
    registry: executor::Registry,
    /// Canonical project root — the default containment boundary for working_dir.
    project_root: PathBuf,
    /// Canonical extra roots from DISPATCH_EXTRA_ROOTS — the user's explicit opt-in
    /// to delegate outside the project tree.
    extra_roots: Arc<Vec<PathBuf>>,
    /// Whether `danger-full-access` is permitted (DISPATCH_ALLOW_DANGER).
    allow_danger: bool,
    /// This server process — written on every task it owns, read at peer startup
    /// to decide which stranded rows are safe to reconcile.
    owner_pid: i64,
    owner_instance: String,
    /// Backend --version strings probed once at boot, recorded on each run for audit.
    backend_versions: Arc<HashMap<String, Option<String>>>,
    /// Per-project state directory (dispatch.db plus dispatch-owned logs).
    state_dir: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Dispatch {
    #[tool(
        description = "Delegate ONE execution step to a coding-agent backend (codex or opencode) running headless and WRITE-CAPABLE in `working_dir` — it may modify files there. Runs ASYNCHRONOUSLY: returns a task id immediately; poll dispatch_status(id) for progress and the result. Provide a structured spec — objective (required), working_dir (required, absolute), and optional target_files / constraints / acceptance — plus optional free-form context / details; the server renders them into the backend prompt. working_dir is rejected unless it canonicalizes within the project root (widen with the DISPATCH_EXTRA_ROOTS env var). sandbox defaults to workspace-write; danger-full-access is rejected unless the server enables it. One active run per working_dir unless allow_concurrent=true. POLICY: initiate dispatch according to claude-agent-kit--dispatch-prefs.md (`conservative` / `preference-only` / `proactive`) — under `proactive` + `auto`, submit directly for suitable steps; this policy governs dispatch specifically and is not subject to the general write-capable delegation propose-and-wait default used elsewhere. APPROVAL: before the FIRST dispatch in a session, confirm working_dir + the step scope + the approval granularity (per-step vs batch) with the user when approval mode is ask; skip that confirmation only when approval mode is auto."
    )]
    async fn dispatch_submit(
        &self,
        Parameters(p): Parameters<SubmitParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        if p.objective.trim().is_empty() {
            return Ok(err_struct(ErrCode::InvalidParams, "objective is required"));
        }
        if p.working_dir.trim().is_empty() {
            return Ok(err_struct(
                ErrCode::InvalidParams,
                "working_dir is required",
            ));
        }

        let backend = match Backend::parse(p.backend.as_deref().unwrap_or("")) {
            Some(b) => b,
            None => {
                return Ok(err_struct(
                    ErrCode::UnknownBackend,
                    format!(
                        "unknown backend {:?}; supported: codex, opencode",
                        p.backend.as_deref().unwrap_or("")
                    ),
                ));
            }
        };

        let canon = match self.check_working_dir(&p.working_dir) {
            Ok(c) => c,
            Err(e) => return Ok(err_struct(ErrCode::InvalidWorkingDir, e)),
        };
        let canon_str = canon.to_string_lossy().to_string();

        let sandbox = match self.check_sandbox(p.sandbox.as_deref()) {
            Ok(s) => s,
            Err(e) => return Ok(err_struct(ErrCode::SandboxForbidden, e)),
        };

        let allow_concurrent = p.allow_concurrent.unwrap_or(false);
        let nonce = make_nonce(&self.owner_instance);
        let prompt = render::render_prompt(&p, &nonce);
        let spec_json = render::spec_json(&p);

        let id = {
            let mut conn = match self.lock_db() {
                Ok(c) => c,
                Err(e) => return Ok(e),
            };
            let new = store::NewTask {
                plan_id: nonempty(p.plan_id.clone()),
                backend: backend.as_str().to_string(),
                working_dir: canon_str.clone(),
                title: nonempty(p.title.clone()),
                spec_json,
                prompt: prompt.clone(),
                model: nonempty(p.model.clone()),
                reasoning_effort: nonempty(p.reasoning_effort.clone()),
                sandbox: sandbox.clone(),
                parent_id: None,
                nonce: Some(nonce.clone()),
                rollout_start_line: None,
            };
            let enforce_dir = if allow_concurrent {
                None
            } else {
                Some(canon_str.as_str())
            };
            match store::insert_queued(
                &mut conn,
                &new,
                self.owner_pid,
                &self.owner_instance,
                enforce_dir,
            ) {
                Ok(store::InsertOutcome::Created(id)) => id,
                Ok(store::InsertOutcome::Conflict(existing)) => {
                    return Ok(err_struct(
                        ErrCode::DirBusy,
                        format!(
                            "a dispatch ({existing}) is already active for {canon_str}; \
                         wait for it, cancel it, or pass allow_concurrent=true to override"
                        ),
                    ));
                }
                Err(e) => return Ok(err_struct(ErrCode::DbError, format!("db error: {e}"))),
            }
        };

        let job = executor::Job {
            id: id.clone(),
            backend,
            working_dir: canon,
            sandbox: sandbox.clone(),
            model: nonempty(p.model),
            reasoning_effort: nonempty(p.reasoning_effort),
            skip_git_repo_check: p.skip_git_repo_check.unwrap_or(false),
            prompt,
            backend_version: self.backend_version(backend),
            state_dir: self.state_dir.clone(),
            resume_session: None,
            nonce: Some(nonce),
            rollout_path: None,
        };
        executor::spawn(self.db.clone(), self.registry.clone(), job);

        Ok(json_ok(json!({
            "id": id,
            "status": store::STATUS_QUEUED,
            "backend": backend.as_str(),
            "working_dir": canon_str,
            "sandbox": sandbox,
            "plan_id": nonempty(p.plan_id),
            "note": "running asynchronously — poll dispatch_status(id); cancel with dispatch_cancel(id)",
        })))
    }

    #[tool(
        description = "Get the status and (when terminal) the captured result / error of a dispatched task by id. Statuses: queued, running, succeeded, failed, cancelled, interrupted."
    )]
    async fn dispatch_status(
        &self,
        Parameters(p): Parameters<StatusParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let id = p.id.trim();
        if id.is_empty() {
            return Ok(err_struct(ErrCode::InvalidParams, "id is required"));
        }
        let conn = match self.lock_db() {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };
        match store::get(&conn, id) {
            Ok(Some(row)) => Ok(json_ok(row.to_json(true))),
            Ok(None) => Ok(err_struct(
                ErrCode::NoSuchTask,
                format!("no task with id {id:?}"),
            )),
            Err(e) => Ok(err_struct(ErrCode::DbError, format!("db error: {e}"))),
        }
    }

    #[tool(
        description = "List dispatched tasks (insertion order), optionally filtered by plan_id and/or status. The captured result is omitted here — use dispatch_status(id) for a task's output."
    )]
    async fn dispatch_list(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let conn = match self.lock_db() {
            Ok(c) => c,
            Err(e) => return Ok(e),
        };
        match store::list(&conn, nonempty_ref(&p.plan_id), nonempty_ref(&p.status)) {
            Ok(rows) => {
                let tasks: Vec<Value> = rows.iter().map(|r| r.to_json(false)).collect();
                Ok(json_ok(json!({ "count": tasks.len(), "tasks": tasks })))
            }
            Err(e) => Ok(err_struct(ErrCode::DbError, format!("db error: {e}"))),
        }
    }

    #[tool(
        description = "Cancel a running dispatch by id, or every active step of a plan by plan_id (pass exactly one). Fires the cancellation token; the backend's process group is killed and the task transitions to cancelled. A task owned by a different session's server cannot be cancelled from here."
    )]
    async fn dispatch_cancel(
        &self,
        Parameters(p): Parameters<CancelParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        match (nonempty_ref(&p.id), nonempty_ref(&p.plan_id)) {
            (Some(id), None) => {
                let row = {
                    let conn = match self.lock_db() {
                        Ok(c) => c,
                        Err(e) => return Ok(e),
                    };
                    store::get(&conn, id)
                };
                match row {
                    Ok(Some(row)) => {
                        if !store::is_active(&row.status) {
                            return Ok(text_ok(format!(
                                "task {id} is already {} — nothing to cancel",
                                row.status
                            )));
                        }
                        if executor::request_cancel(&self.registry, id) {
                            Ok(text_ok(format!(
                                "cancellation requested for {id}; it will transition to cancelled shortly"
                            )))
                        } else {
                            Ok(text_ok(format!(
                                "task {id} is {} but is not running under this server instance \
                                 (another session may own it) — cannot cancel it from here",
                                row.status
                            )))
                        }
                    }
                    Ok(None) => Ok(err_struct(
                        ErrCode::NoSuchTask,
                        format!("no task with id {id:?}"),
                    )),
                    Err(e) => Ok(err_struct(ErrCode::DbError, format!("db error: {e}"))),
                }
            }
            (None, Some(plan)) => {
                let ids = {
                    let conn = match self.lock_db() {
                        Ok(c) => c,
                        Err(e) => return Ok(e),
                    };
                    store::active_ids_for_plan(&conn, plan)
                };
                match ids {
                    Ok(ids) if ids.is_empty() => {
                        Ok(text_ok(format!("no active tasks in plan {plan:?}")))
                    }
                    Ok(ids) => {
                        let mut cancelled = Vec::new();
                        let mut not_owned_here = Vec::new();
                        for id in ids {
                            if executor::request_cancel(&self.registry, &id) {
                                cancelled.push(id);
                            } else {
                                not_owned_here.push(id);
                            }
                        }
                        Ok(json_ok(json!({
                            "plan_id": plan,
                            "cancelled": cancelled,
                            "not_owned_here": not_owned_here,
                        })))
                    }
                    Err(e) => Ok(err_struct(ErrCode::DbError, format!("db error: {e}"))),
                }
            }
            (Some(_), Some(_)) => Ok(err_struct(
                ErrCode::InvalidParams,
                "pass exactly one of id or plan_id, not both",
            )),
            (None, None) => Ok(err_struct(
                ErrCode::InvalidParams,
                "pass either id or plan_id to cancel",
            )),
        }
    }

    #[tool(
        description = "List which backend CLIs (codex, opencode) are available on PATH, with their --version output. Call this when you're unsure a dispatch backend is installed on this machine."
    )]
    async fn dispatch_backends(
        &self,
        Parameters(_p): Parameters<BackendsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut report = Vec::new();
        for b in Backend::all() {
            let entry = match backend::which(b.binary()) {
                Some(path) => {
                    let ver = backend::version(*b)
                        .await
                        .unwrap_or_else(|| "(unknown)".to_string());
                    json!({
                        "backend": b.as_str(),
                        "available": true,
                        "path": path.display().to_string(),
                        "version": ver,
                    })
                }
                None => json!({
                    "backend": b.as_str(),
                    "available": false,
                    "path": null,
                    "version": null,
                }),
            };
            report.push(entry);
        }
        Ok(json_ok(json!({ "backends": report })))
    }

    #[tool(
        description = "Show a curated, live-updating timeline of what a delegated run is doing. Codex logs are read from codex's rollout; OpenCode logs are dispatch-owned normalized JSONL with native OpenCode events preserved. Noise (system prompts, token counts, encrypted codex reasoning, raw tool-call output) is filtered out; signal (user/backend messages, tool-call invocations, file edits, lifecycle, and plaintext OpenCode reasoning) is kept, and never truncated per-field — only the total response is size-capped (see line_start/line_end). Works WHILE the task is still running. Page with line_start/line_end (1-based; omitted = the tail) to avoid output limits — total_lines tells you how to page. kinds filters categories (lifecycle/messages/tools/edits/reasoning/tool_results; by default codex excludes reasoning, opencode includes it; tool_results — raw tool-call output — is excluded by default for both backends, request it explicitly). raw=true returns the underlying JSONL."
    )]
    async fn dispatch_logs(
        &self,
        Parameters(p): Parameters<LogsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let id = p.id.trim();
        if id.is_empty() {
            return Ok(err_struct(ErrCode::InvalidParams, "id is required"));
        }
        let row = {
            let conn = match self.lock_db() {
                Ok(c) => c,
                Err(e) => return Ok(e),
            };
            store::get(&conn, id)
        };
        let row = match row {
            Ok(Some(r)) => r,
            Ok(None) => {
                return Ok(err_struct(
                    ErrCode::NoSuchTask,
                    format!("no task with id {id:?}"),
                ));
            }
            Err(e) => return Ok(err_struct(ErrCode::DbError, format!("db error: {e}"))),
        };
        let rollout_path = match self.resolve_rollout(&row) {
            Some(p) => p,
            None => {
                return Ok(json_ok(json!({
                    "id": id, "status": row.status, "session_pending": true, "log": "",
                    "note": "no backend log associated yet — the run may not have written its log, or its association is still pending; try again shortly",
                })));
            }
        };
        let jsonl = match rollout::read_to_string(Path::new(&rollout_path)) {
            Ok(s) => s,
            Err(e) => {
                return Ok(err_struct(
                    ErrCode::RolloutUnreadable,
                    format!("could not read rollout {rollout_path}: {e}"),
                ));
            }
        };
        // For a steered task, codex appended its new turn to the inherited parent
        // rollout; skip the lines that predate the steer so logs show only this turn.
        let jsonl = trim_to_start_line(jsonl, row.rollout_start_line);
        let start = p.line_start.map(|n| n as usize);
        let end = p.line_end.map(|n| n as usize);

        if p.raw.unwrap_or(false) {
            let lines: Vec<String> = jsonl.lines().map(str::to_string).collect();
            let (text, s, e, capped) = rollout::window(&lines, start, end);
            return Ok(json_ok(json!({
                "id": id, "status": row.status, "raw": true, "session_pending": false,
                "rollout_path": rollout_path, "total_lines": lines.len(),
                "shown_lines": format!("{s}-{e}"), "byte_capped": capped, "log": text,
            })));
        }

        let kinds = p
            .kinds
            .unwrap_or_else(|| rollout::default_kinds(&row.backend));
        let rendered = rollout::curate(&jsonl, &kinds);
        let (text, s, e, capped) = rollout::window(&rendered.lines, start, end);
        Ok(json_ok(json!({
            "id": id, "status": row.status, "session_id": row.session_id,
            "session_pending": false, "rollout_path": rollout_path, "kinds": kinds,
            "total_lines": rendered.total, "shown_lines": format!("{s}-{e}"),
            "byte_capped": capped, "log": text,
        })))
    }

    #[tool(
        description = "Interrupt a delegated task and steer it with a NEW instruction, continuing the SAME backend session (its accumulated context + the files it already wrote are preserved when the backend supports sessions). If the task is still running it is cancelled first; then the backend resumes the session with your instruction. Creates a new linked task (parent_id = the steered task) so the turn history shows in dispatch_list. Returns the new id — poll dispatch_status / dispatch_logs. Use this for mid-flight 'no, do it this way instead' redirection."
    )]
    async fn dispatch_steer(
        &self,
        Parameters(p): Parameters<SteerParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let id = p.id.trim();
        if id.is_empty() {
            return Ok(err_struct(ErrCode::InvalidParams, "id is required"));
        }
        if p.instruction.trim().is_empty() {
            return Ok(err_struct(
                ErrCode::InvalidParams,
                "instruction is required",
            ));
        }
        let parent = {
            let conn = match self.lock_db() {
                Ok(c) => c,
                Err(e) => return Ok(e),
            };
            store::get(&conn, id)
        };
        let parent = match parent {
            Ok(Some(r)) => r,
            Ok(None) => {
                return Ok(err_struct(
                    ErrCode::NoSuchTask,
                    format!("no task with id {id:?}"),
                ));
            }
            Err(e) => return Ok(err_struct(ErrCode::DbError, format!("db error: {e}"))),
        };
        let session_id = match self.resolve_session_id(&parent) {
            Some(s) => s,
            None => {
                return Ok(err_struct(
                    ErrCode::SessionNotReady,
                    format!(
                        "no backend session recorded for {id} yet — it may not have started; check dispatch_status / dispatch_logs first"
                    ),
                ));
            }
        };

        // Interrupt if still active, then wait for it to actually terminate so the
        // working_dir is free before the resume run starts.
        if store::is_active(&parent.status) {
            executor::request_cancel(&self.registry, id);
            let mut waited_ms = 0u64;
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                waited_ms += 200;
                let still_active = if let Ok(conn) = self.db.lock() {
                    store::get(&conn, id)
                        .ok()
                        .flatten()
                        .map(|r| store::is_active(&r.status))
                        .unwrap_or(false)
                } else {
                    false
                };
                if !still_active {
                    break;
                }
                if waited_ms >= STEER_TERMINATE_WAIT_MS {
                    return Ok(err_struct(
                        ErrCode::DirBusy,
                        format!(
                            "{id} is still terminating after cancel; retry dispatch_steer shortly"
                        ),
                    ));
                }
            }
        }

        let instruction = p.instruction.trim().to_string();
        let prompt = format!(
            "Continue working in the same workspace on the SAME task. New instruction from the user:\n\n{instruction}\n\nWhen finished, end with a short summary of what you changed."
        );
        let spec_json = serde_json::to_string(&json!({
            "objective": instruction, "working_dir": parent.working_dir, "resume_of": id,
        }))
        .unwrap_or_else(|_| "{}".to_string());

        // The parent's rollout file: resumed backends append their new turn here. We use it
        // for the steered row's start-line boundary (logs show only the new turn) and to
        // set the steered row's identity immediately (below).
        let parent_rollout = self.resolve_rollout(&parent);
        let rollout_start_line = parent_rollout
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|s| s.lines().count() as i64);
        // Execution knobs the steered run inherits (overridable per call) — captured
        // once so they can be reused for the row, the job, and the echoed response.
        let eff_model = nonempty(p.model.clone()).or_else(|| parent.model.clone());
        let eff_effort =
            nonempty(p.reasoning_effort.clone()).or_else(|| parent.reasoning_effort.clone());
        let eff_sandbox = parent.sandbox.clone();

        let new_id = {
            let mut conn = match self.lock_db() {
                Ok(c) => c,
                Err(e) => return Ok(e),
            };
            let new = store::NewTask {
                plan_id: parent.plan_id.clone(),
                backend: parent.backend.clone(),
                working_dir: parent.working_dir.clone(),
                title: Some(format!("steer of {id}")),
                spec_json,
                prompt: prompt.clone(),
                model: eff_model.clone(),
                reasoning_effort: eff_effort.clone(),
                sandbox: eff_sandbox.clone(),
                parent_id: Some(id.to_string()),
                nonce: None,
                rollout_start_line,
            };
            match store::insert_queued(
                &mut conn,
                &new,
                self.owner_pid,
                &self.owner_instance,
                Some(parent.working_dir.as_str()),
            ) {
                Ok(store::InsertOutcome::Created(nid)) => nid,
                Ok(store::InsertOutcome::Conflict(existing)) => {
                    return Ok(err_struct(
                        ErrCode::DirBusy,
                        format!(
                            "another dispatch ({existing}) is active for {}; cancel it first",
                            parent.working_dir
                        ),
                    ));
                }
                Err(e) => return Ok(err_struct(ErrCode::DbError, format!("db error: {e}"))),
            }
        };

        // Record the steered row's identity now — we already know the resumed session and
        // the inherited rollout file — so dispatch_logs / dispatch_steer never fall back to
        // a cwd guess in the window before the executor records it.
        if let Some(rp) = parent_rollout.as_deref()
            && let Ok(conn) = self.db.lock()
        {
            let _ = store::set_session(&conn, &new_id, &session_id, rp);
        }

        let job = executor::Job {
            id: new_id.clone(),
            backend: Backend::parse(&parent.backend).unwrap_or(Backend::Codex),
            working_dir: PathBuf::from(&parent.working_dir),
            sandbox: eff_sandbox.clone(),
            model: eff_model.clone(),
            reasoning_effort: eff_effort.clone(),
            skip_git_repo_check: true,
            prompt,
            backend_version: self
                .backend_version(Backend::parse(&parent.backend).unwrap_or(Backend::Codex)),
            state_dir: self.state_dir.clone(),
            resume_session: Some(session_id.clone()),
            nonce: None,
            rollout_path: parent_rollout.as_deref().map(PathBuf::from),
        };
        executor::spawn(self.db.clone(), self.registry.clone(), job);

        Ok(json_ok(json!({
            "id": new_id,
            "parent_id": id,
            "status": store::STATUS_QUEUED,
            "resumed_session": session_id,
            "working_dir": parent.working_dir,
            "sandbox": eff_sandbox,
            "model": eff_model,
            "reasoning_effort": eff_effort,
            "note": "steering: the backend session was resumed with your new instruction (it inherits the echoed sandbox/model/reasoning_effort unless you overrode them) — poll dispatch_status / dispatch_logs",
        })))
    }

    #[tool(
        description = "Bounded long-poll: block until a dispatched task reaches a terminal status (succeeded / failed / cancelled / interrupted) or timeout_ms elapses (default 30s, capped at 120s), then return compact task status plus a small curated `log_tail` and `timed_out` flag. This is NOT an unbounded wait — a multi-minute run times out with `timed_out: true` and a non-terminal status. dispatch has NO push notification (unlike a backgrounded Agent/Workflow task, which auto-notifies on completion): if the task is still non-terminal, either re-invoke dispatch_wait now to keep blocking, or — if ending the turn — schedule a follow-up dispatch_status/dispatch_wait check first where a scheduling mechanism is available (e.g. ScheduleWakeup), or otherwise tell the user explicitly the task is still running and they'll need to ask you to check back. Ending the turn with nothing armed and no signal to the user strands the task with no way to learn it finished. Use dispatch_logs for the full timeline and dispatch_status for the full captured result/spec."
    )]
    async fn dispatch_wait(
        &self,
        Parameters(p): Parameters<WaitParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let id = p.id.trim().to_string();
        if id.is_empty() {
            return Ok(err_struct(ErrCode::InvalidParams, "id is required"));
        }
        let timeout_ms = p
            .timeout_ms
            .map(|t| (t as u64).clamp(WAIT_POLL_INTERVAL_MS, WAIT_MAX_TIMEOUT_MS))
            .unwrap_or(WAIT_DEFAULT_TIMEOUT_MS);

        let mut waited_ms = 0u64;
        loop {
            let row = {
                let conn = match self.lock_db() {
                    Ok(c) => c,
                    Err(e) => return Ok(e),
                };
                store::get(&conn, &id)
            };
            let row = match row {
                Ok(Some(r)) => r,
                Ok(None) => {
                    return Ok(err_struct(
                        ErrCode::NoSuchTask,
                        format!("no task with id {id:?}"),
                    ));
                }
                Err(e) => return Ok(err_struct(ErrCode::DbError, format!("db error: {e}"))),
            };
            if !store::is_active(&row.status) {
                let v = self.wait_json(&row, waited_ms, false);
                return Ok(json_ok(v));
            }
            if waited_ms >= timeout_ms {
                let mut v = self.wait_json(&row, waited_ms, true);
                v["note"] = json!(format!(
                    "still {} after {waited_ms}ms — re-invoke dispatch_wait to keep waiting; inspect log_tail below or call dispatch_logs for the full timeline",
                    row.status
                ));
                return Ok(json_ok(v));
            }
            tokio::time::sleep(std::time::Duration::from_millis(WAIT_POLL_INTERVAL_MS)).await;
            waited_ms += WAIT_POLL_INTERVAL_MS;
        }
    }
}

// ── guards + helpers (non-tool impl) ──────────────────────

impl Dispatch {
    fn backend_version(&self, backend: Backend) -> Option<String> {
        self.backend_versions
            .get(backend.as_str())
            .and_then(|v| v.clone())
    }

    fn lock_db(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, CallToolResult> {
        self.db
            .lock()
            .map_err(|e| err_struct(ErrCode::DbError, format!("database lock poisoned: {e}")))
    }

    fn wait_json(&self, row: &store::TaskRow, waited_ms: u64, timed_out: bool) -> Value {
        let mut v = row.to_json(false);
        v["timed_out"] = json!(timed_out);
        v["waited_ms"] = json!(waited_ms);
        v["has_result"] = json!(row.result.is_some());
        v["has_error"] = json!(row.error.is_some());
        if let Some(err) = row
            .error
            .as_deref()
            .filter(|_| !store::is_active(&row.status))
        {
            v["error_preview"] = json!(preview_oneline(err, 1_000));
        }
        v["log_tail"] = self.wait_log_tail(row);
        v["next"] = json!({
            "wait": format!("dispatch_wait(id={})", row.id),
            "logs": format!("dispatch_logs(id={}) for the full curated timeline", row.id),
            "status": format!("dispatch_status(id={}) for captured result/error/spec", row.id),
        });
        v
    }

    fn wait_log_tail(&self, row: &store::TaskRow) -> Value {
        let Some(rollout_path) = self.resolve_rollout(row) else {
            return json!({
                "session_pending": true,
                "total_lines": 0,
                "shown_lines": "0-0",
                "byte_capped": false,
                "text": "",
                    "note": "no backend log associated yet — the run may not have written its log, or its association is still pending",
            });
        };
        let jsonl = match rollout::read_to_string(Path::new(&rollout_path)) {
            Ok(s) => trim_to_start_line(s, row.rollout_start_line),
            Err(e) => {
                return json!({
                    "session_pending": false,
                    "total_lines": 0,
                    "shown_lines": "0-0",
                    "byte_capped": false,
                    "text": "",
                    "error": format!("could not read rollout {rollout_path}: {e}"),
                });
            }
        };
        let kinds = rollout::default_kinds(&row.backend);
        let rendered = rollout::curate(&jsonl, &kinds);
        let (text, s, e, capped) = rollout::window_with_limits(
            &rendered.lines,
            None,
            None,
            WAIT_LOG_TAIL_LINES,
            WAIT_LOG_BYTE_CAP,
        );
        json!({
            "session_pending": false,
            "total_lines": rendered.total,
            "shown_lines": format!("{s}-{e}"),
            "byte_capped": capped,
            "kinds": kinds,
            "text": text,
        })
    }

    /// Resolve a task's backend log path. A cached value is trusted only if it still
    /// validates as this task's (`rollout_is_ours`) — self-healing a row poisoned by the
    /// old cwd-guessing code. Otherwise it re-locates by identity (`locate_validated`).
    /// It never returns a bare cwd guess: with nothing matching it returns None (fail
    /// closed) rather than hand back another session's rollout.
    fn resolve_rollout(&self, row: &store::TaskRow) -> Option<String> {
        if let Some(p) = row.rollout_path.as_ref()
            && Path::new(p).exists()
            && self.rollout_is_ours(row, Path::new(p))
        {
            return Some(p.clone());
        }
        let (path, sid) = self.locate_validated(row)?;
        let path_str = path.to_string_lossy().to_string();
        if let Ok(conn) = self.db.lock() {
            let _ = store::set_session(&conn, &row.id, &sid, &path_str);
        }
        Some(path_str)
    }

    /// Resolve a task's backend session id (for `dispatch_steer`'s resume). The stored sid
    /// is trusted only if its cached rollout still validates as this task's — otherwise it
    /// is re-derived by identity, so a steer can never resume a poisoned / unrelated session.
    fn resolve_session_id(&self, row: &store::TaskRow) -> Option<String> {
        if let Some(s) = row.session_id.as_deref().filter(|s| !s.is_empty())
            && let Some(p) = row.rollout_path.as_deref()
            && Path::new(p).exists()
            && self.rollout_is_ours(row, Path::new(p))
        {
            return Some(s.to_string());
        }
        let (path, sid) = self.locate_validated(row)?;
        if let Ok(conn) = self.db.lock() {
            let _ = store::set_session(&conn, &row.id, &sid, &path.to_string_lossy());
        }
        Some(sid)
    }

    /// Whether the rollout at `path` belongs to this task, by an INDEPENDENT signal in
    /// priority order: the prompt nonce (fresh tasks); the inherited authoritative
    /// session id (steered tasks, identified by `parent_id`); else — a legacy row — cwd
    /// plus a not-older-than-start time floor. The floor is what avoids the circular case
    /// where a poisoned row's session_id was copied from the same stale file.
    fn rollout_is_ours(&self, row: &store::TaskRow, path: &Path) -> bool {
        if let Some(n) = row.nonce.as_deref().filter(|n| !n.is_empty()) {
            return rollout::rollout_has_nonce(path, n);
        }
        if row.parent_id.is_some() {
            // Steered task: associate ONLY by the inherited session id — never a cwd
            // guess, even in the window before the session id is recorded (fail closed).
            return row
                .session_id
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|sid| rollout::rollout_has_session_id(path, sid))
                .unwrap_or(false);
        }
        rollout::rollout_cwd_after(path, Path::new(&row.working_dir), self.task_floor(row))
    }

    /// Re-locate a task's rollout by an independent identity signal: the nonce (fresh —
    /// and ONLY the nonce, so a fresh task never falls back to a cwd guess before its
    /// rollout is written); the inherited session id (steered, by `parent_id`); else —
    /// a legacy row — the newest same-cwd rollout at or after the task's start.
    fn locate_validated(&self, row: &store::TaskRow) -> Option<(PathBuf, String)> {
        if let Some(n) = row.nonce.as_deref().filter(|n| !n.is_empty()) {
            return rollout::locate_by_nonce(Path::new(&row.working_dir), n);
        }
        if row.parent_id.is_some() {
            // Steered task: only the inherited session id; fail closed otherwise.
            return row
                .session_id
                .as_deref()
                .filter(|s| !s.is_empty())
                .and_then(|sid| rollout::locate_by_session_id(sid).map(|p| (p, sid.to_string())));
        }
        rollout::locate_new_by_cwd(
            Path::new(&row.working_dir),
            &HashSet::new(),
            self.task_floor(row),
        )
    }

    /// The lower time bound for a legacy row's rollout: the task's start (or creation),
    /// minus a tolerance for clock / ordering skew. None if it can't be parsed.
    fn task_floor(&self, row: &store::TaskRow) -> Option<SystemTime> {
        let raw = row.started_at.as_deref().unwrap_or(row.created_at.as_str());
        parse_sqlite_utc(raw)?.checked_sub(Duration::from_secs(FLOOR_TOLERANCE_SECS))
    }

    /// Containment guard: working_dir must be absolute, exist, be a directory, and
    /// canonicalize within the project root OR a user-allowlisted extra root. This
    /// is the real, model-proof boundary on a write-capable subprocess.
    fn check_working_dir(&self, raw: &str) -> Result<PathBuf, String> {
        let raw = raw.trim();
        let p = Path::new(raw);
        if !p.is_absolute() {
            return Err(format!("working_dir must be an absolute path, got {raw:?}"));
        }
        let canon = p
            .canonicalize()
            .map_err(|e| format!("working_dir {raw:?} cannot be resolved (does it exist?): {e}"))?;
        if !canon.is_dir() {
            return Err(format!(
                "working_dir {} is not a directory",
                canon.display()
            ));
        }
        let allowed = canon.starts_with(&self.project_root)
            || self.extra_roots.iter().any(|r| canon.starts_with(r));
        if !allowed {
            return Err(format!(
                "working_dir {} is outside the project root ({}) and any allowlisted root. \
                 dispatch only delegates within the project tree by default; if you intend this, \
                 add the root to the DISPATCH_EXTRA_ROOTS env var (an OS-path-list of absolute \
                 paths) for this server.",
                canon.display(),
                self.project_root.display()
            ));
        }
        Ok(canon)
    }

    /// Sandbox ceiling: workspace-write (default) and read-only are always allowed;
    /// danger-full-access requires the server to opt in via DISPATCH_ALLOW_DANGER.
    fn check_sandbox(&self, s: Option<&str>) -> Result<String, String> {
        let s = s
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .unwrap_or("workspace-write");
        match s {
            "read-only" | "workspace-write" => Ok(s.to_string()),
            "danger-full-access" => {
                if self.allow_danger {
                    Ok(s.to_string())
                } else {
                    Err(
                        "sandbox 'danger-full-access' is disabled on this server. Set \
                         DISPATCH_ALLOW_DANGER=1 to permit running codex with no sandbox."
                            .to_string(),
                    )
                }
            }
            other => Err(format!(
                "invalid sandbox {other:?}; allowed: read-only, workspace-write \
                 (or danger-full-access if the server enables it)"
            )),
        }
    }
}

// ── free helpers ──────────────────────────────────────────

fn text_ok(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(msg.into())])
}

fn json_ok(v: Value) -> CallToolResult {
    CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".to_string()),
    )])
}

/// Stable, machine-readable error categories returned alongside the human message, so a
/// calling agent can branch on `error.code` instead of parsing prose.
#[derive(Clone, Copy)]
enum ErrCode {
    InvalidParams,
    NoSuchTask,
    InvalidWorkingDir,
    SandboxForbidden,
    DirBusy,
    SessionNotReady,
    UnknownBackend,
    RolloutUnreadable,
    DbError,
}

impl ErrCode {
    fn as_str(self) -> &'static str {
        match self {
            ErrCode::InvalidParams => "invalid_params",
            ErrCode::NoSuchTask => "no_such_task",
            ErrCode::InvalidWorkingDir => "invalid_working_dir",
            ErrCode::SandboxForbidden => "sandbox_forbidden",
            ErrCode::DirBusy => "dir_busy",
            ErrCode::SessionNotReady => "session_not_ready",
            ErrCode::UnknownBackend => "unknown_backend",
            ErrCode::RolloutUnreadable => "rollout_unreadable",
            ErrCode::DbError => "db_error",
        }
    }
}

/// A structured error result — the `isError` content variant, carrying a stable `code`
/// plus the human-readable `message`.
fn err_struct(code: ErrCode, msg: impl Into<String>) -> CallToolResult {
    let msg = msg.into();
    let body = serde_json::to_string_pretty(&json!({
        "error": { "code": code.as_str(), "message": msg }
    }))
    .unwrap_or_else(|_| msg.clone());
    CallToolResult::error(vec![Content::text(body)])
}

fn nonempty(o: Option<String>) -> Option<String> {
    o.filter(|s| !s.trim().is_empty())
}

fn nonempty_ref(o: &Option<String>) -> Option<&str> {
    o.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn preview_oneline(s: &str, cap: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > cap {
        let cut: String = one.chars().take(cap).collect();
        format!("{cut}…")
    } else {
        one
    }
}

/// Monotonic counter for `make_nonce`.
static NONCE_SEQ: AtomicU64 = AtomicU64::new(0);

/// A per-task identity nonce: the server instance id (unique per process) plus a
/// monotonic counter, so it is unique across tasks and servers and effectively never
/// collides with prompt text. `render::render_prompt` embeds it; `rollout::locate_by_nonce`
/// matches it back to the rollout this task produced.
fn make_nonce(instance: &str) -> String {
    let n = NONCE_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{instance}-{n}")
}

/// Drop the first `start_line` raw rollout lines — a steered task's inherited parent
/// turns — so `dispatch_logs` shows only the new turn. A no-op when unset / non-positive.
fn trim_to_start_line(jsonl: String, start_line: Option<i64>) -> String {
    match start_line {
        Some(n) if n > 0 => jsonl
            .lines()
            .skip(n as usize)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => jsonl,
    }
}

/// Parse a SQLite `datetime('now')` string ("YYYY-MM-DD HH:MM:SS", UTC) into a
/// `SystemTime`, without pulling in a date crate. Returns None on any malformed field.
/// Uses Howard Hinnant's days-from-civil algorithm.
fn parse_sqlite_utc(s: &str) -> Option<SystemTime> {
    let (date, time) = s.trim().split_once(' ')?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let mo: i64 = d.next()?.parse().ok()?;
    let da: i64 = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let h: i64 = t.next()?.parse().ok()?;
    let mi: i64 = t.next()?.parse().ok()?;
    let se: i64 = t.next()?.parse().ok()?;
    if !(1..=12).contains(&mo)
        || !(1..=31).contains(&da)
        || !(0..=23).contains(&h)
        || !(0..=59).contains(&mi)
        || !(0..=60).contains(&se)
    {
        return None;
    }
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = (if yy >= 0 { yy } else { yy - 399 }) / 400;
    let yoe = yy - era * 400;
    let mp = (mo + 9) % 12; // Mar=0 … Feb=11
    let doy = (153 * mp + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + h * 3600 + mi * 60 + se;
    if secs < 0 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64))
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// State dir for dispatch.db — mirrors workslate's project-anchored layout but in
/// a `dispatch/` subdir so the two servers never share a file.
fn resolve_state_dir(fallback: &Path) -> PathBuf {
    let project_dir = std::env::var("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| fallback.to_path_buf());
    // Canonicalize so symlink / case / `..` aliases of one project don't split it
    // into multiple state dirs (which would defeat reconciliation + the dir guard).
    let project_dir = project_dir.canonicalize().unwrap_or(project_dir);
    let project_path = project_dir.to_string_lossy().replace('/', "-");
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    home.join(".claude")
        .join("projects")
        .join(&project_path)
        .join("dispatch")
}

fn parse_extra_roots() -> Vec<PathBuf> {
    match std::env::var_os("DISPATCH_EXTRA_ROOTS") {
        Some(v) => std::env::split_paths(&v)
            .filter_map(|p| p.canonicalize().ok())
            .collect(),
        None => Vec::new(),
    }
}

#[cfg(unix)]
fn process_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    // kill(pid, 0): 0 => alive & signalable; EPERM => alive but not ours; ESRCH => dead.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    winjob::process_alive(pid as u32)
}

#[cfg(not(any(unix, windows)))]
fn process_alive(_pid: i32) -> bool {
    // No portable liveness check; assume alive so a peer server's tasks are never
    // clobbered. A crashed non-unix server may leave a stale 'running' row.
    true
}

/// Boot reconciliation: a freshly started server owns no running child, so any
/// `queued`/`running` row whose owner process is gone is stranded — mark it
/// interrupted. Rows owned by a still-live peer server are left untouched.
fn reconcile(conn: &rusqlite::Connection) {
    let actives = match store::active_owners(conn) {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("dispatch: reconcile read failed: {e}");
            return;
        }
    };
    let mut reconciled = 0usize;
    for (id, owner_pid) in actives {
        let dead = match owner_pid {
            Some(pid) => !process_alive(pid as i32),
            None => true,
        };
        if dead {
            if let Err(e) = store::mark_interrupted(
                conn,
                &id,
                "owning dispatch server is no longer running (reconciled at startup)",
            ) {
                tracing::warn!("dispatch: reconcile mark_interrupted({id}) failed: {e}");
            } else {
                reconciled += 1;
            }
        }
    }
    if reconciled > 0 {
        tracing::info!("dispatch: reconciled {reconciled} stranded task(s) to interrupted");
    }
}

// ── ServerHandler ─────────────────────────────────────────

impl ServerHandler for Dispatch {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Hierarchical delegation tools. Where `aside` asks another model family for a \
             read-only second opinion, `dispatch` hands a coding-agent backend (codex or opencode) an \
             execution task: the backend runs headless and WRITE-CAPABLE, modifying \
             files under a target directory. Delegation is ASYNCHRONOUS — dispatch_submit returns \
             a task id immediately and the run continues in the background; poll dispatch_status, \
             enumerate with dispatch_list, and stop a run with dispatch_cancel. The server enforces \
             hard guards a misbehaving model cannot bypass: working_dir must canonicalize within \
             the project root (or a DISPATCH_EXTRA_ROOTS-allowlisted root), the sandbox ceiling \
             blocks danger-full-access unless DISPATCH_ALLOW_DANGER is set, and only one run is \
             allowed per working_dir unless allow_concurrent. The behavioral dispatch policy — \
             when to initiate dispatch (a `proactive` prefs policy means submitting directly, \
             without a propose-and-wait step), and whether to confirm working_dir, step scope, \
             and approval granularity before the first dispatch of a session — lives in \
             claude-agent-kit--dispatch.md and claude-agent-kit--dispatch-prefs.md.",
        )
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tcc = ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
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

/// Sync entrypoint: argv-sniffs for the hidden `__pdeath_guard` re-invocation
/// (Linux/macOS only — see `pdeath_guard`) BEFORE booting Tokio, since guard
/// mode never needs an async runtime and stays deliberately minimal-surface.
/// Everything else boots Tokio and runs the real server unchanged.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        let mut args = std::env::args_os();
        let _argv0 = args.next();
        if args.next().as_deref() == Some(std::ffi::OsStr::new("__pdeath_guard")) {
            return pdeath_guard::run(args);
        }
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_server())
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cwd = std::env::current_dir()?;
    let state_dir = resolve_state_dir(&cwd);
    tokio::fs::create_dir_all(&state_dir).await?;

    let db_path = state_dir.join("dispatch.db");
    let conn = rusqlite::Connection::open(&db_path)?;
    // busy_timeout before the WAL switch so a concurrent writer is waited out
    // rather than failing — multiple dispatch servers can share this DB.
    conn.execute_batch("PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL;")?;
    store::init(&conn)?;
    reconcile(&conn);

    let project_dir = std::env::var("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| cwd.clone());
    let project_root = project_dir.canonicalize().unwrap_or(project_dir);

    let owner_pid = std::process::id() as i64;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let owner_instance = format!("{owner_pid}-{nanos}");

    let mut backend_versions = HashMap::new();
    for b in Backend::all() {
        backend_versions.insert(b.as_str().to_string(), backend::version(*b).await);
    }

    let server = Dispatch {
        db: Arc::new(StdMutex::new(conn)),
        registry: Arc::new(StdMutex::new(HashMap::new())),
        project_root,
        extra_roots: Arc::new(parse_extra_roots()),
        allow_danger: env_truthy("DISPATCH_ALLOW_DANGER"),
        owner_pid,
        owner_instance,
        backend_versions: Arc::new(backend_versions),
        state_dir,
        tool_router: Dispatch::tool_router(),
    };

    let registry_for_shutdown = server.registry.clone();
    let transport = rmcp::transport::io::stdio();
    let running = server.serve(transport).await?;
    tokio::select! {
        r = running.waiting() => { r?; }
        _ = shutdown_signal() => {
            graceful_shutdown(&registry_for_shutdown).await;
        }
    }
    Ok(())
}

/// Resolves on Ctrl+C (all platforms) or SIGTERM (Unix only) — the
/// interceptable-termination cases. Does nothing for a hard `SIGKILL`, which
/// the native per-platform mechanisms (`pdeath_guard` on Linux/macOS, Job
/// Objects on Windows — see those modules) exist specifically to cover.
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("dispatch: failed to register SIGTERM handler: {e}");
            // Fall back to Ctrl+C alone rather than return immediately, which
            // would make this arm of the outer select! spuriously "win".
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// Cancels every in-flight task's token and waits, bounded, for their cleanup
/// to actually finish (each task removes itself from `registry` on completion
/// — see `executor::spawn`) before returning. Only reachable via interceptable
/// termination (see `shutdown_signal`); a hard `SIGKILL` never runs this.
async fn graceful_shutdown(registry: &executor::Registry) {
    let tokens: Vec<_> = match registry.lock() {
        Ok(reg) => reg.values().cloned().collect(),
        Err(_) => Vec::new(),
    };
    if tokens.is_empty() {
        return;
    }
    for t in &tokens {
        t.cancel();
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let empty = registry.lock().map(|reg| reg.is_empty()).unwrap_or(true);
        if empty {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sqlite_utc_epoch_known_dates_and_ordering() {
        assert_eq!(
            parse_sqlite_utc("1970-01-01 00:00:00"),
            Some(SystemTime::UNIX_EPOCH)
        );
        // 2001-09-09 01:46:40 UTC is exactly 1_000_000_000 epoch seconds.
        assert_eq!(
            parse_sqlite_utc("2001-09-09 01:46:40"),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000)),
        );
        assert!(parse_sqlite_utc("2026-06-27 02:36:23") > parse_sqlite_utc("2026-06-27 02:36:22"));
        assert!(parse_sqlite_utc("not-a-date").is_none());
        assert!(parse_sqlite_utc("2026-13-01 00:00:00").is_none());
        assert!(parse_sqlite_utc("2026-06-27 -1:00:00").is_none());
    }
}
