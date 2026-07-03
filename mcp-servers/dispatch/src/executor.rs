//! Detached execution + cancellation registry.
//!
//! `submit` inserts a `queued` row, then calls [`spawn`], which fires a detached
//! tokio task and returns immediately — that is what makes dispatch asynchronous
//! despite MCP being request/response. The task:
//!   1. spawns the backend child (`backend::spawn_child`),
//!   2. transitions the row `queued` → `running` once the child exists (so cancel
//!      / status never race a row with no pid yet),
//!   3. awaits capped capture under a `CancellationToken`,
//!   4. writes the terminal status + captured output back to SQLite,
//!   5. deregisters its token.
//!
//! The connection is a std `Mutex`; every DB touch is a short synchronous helper
//! that locks, writes, and drops the guard — never held across an `.await`, so
//! the task future stays `Send`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use tokio_util::sync::CancellationToken;

use crate::backend::{self, Backend};
use crate::errkind::{self, BackendErrorKind};
use crate::opencode;
use crate::render;
use crate::store;

pub type DbHandle = Arc<StdMutex<rusqlite::Connection>>;
pub type Registry = Arc<StdMutex<HashMap<String, CancellationToken>>>;

/// How many times `record_rollout` polls for the rollout file (×150ms) before giving
/// up — covers codex's spawn → first-write gap. A miss is recoverable later at read
/// time via the stored nonce / session id, so this need not be generous.
const ROLLOUT_LOCATE_ATTEMPTS: usize = 40;

/// Owned, `'static` description of one delegated run (the borrowed
/// `backend::SpawnSpec` is rebuilt from this inside the task).
pub struct Job {
    pub id: String,
    pub backend: Backend,
    pub working_dir: PathBuf,
    pub sandbox: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub skip_git_repo_check: bool,
    pub prompt: String,
    pub backend_version: Option<String>,
    pub state_dir: PathBuf,
    /// When set, resume this backend session id instead of starting fresh (the
    /// `dispatch_steer` follow-up path).
    pub resume_session: Option<String>,
    /// Per-task identity marker embedded in the prompt (fresh submits only); lets
    /// `record_rollout` positively match the rollout this run produced. None for resume.
    pub nonce: Option<String>,
    /// For backends with dispatch-owned logs (OpenCode), a resumed task appends to
    /// the parent log instead of discovering an external rollout.
    pub rollout_path: Option<PathBuf>,
    /// Ordered fallback chain: on a transient backend error (per `errkind::classify`),
    /// retry the SAME task against the next model here. None/empty for
    /// `dispatch_steer` follow-ups — a resumed session stays on one model.
    pub model_fallback: Option<Vec<String>>,
}

/// Register a cancellation token for `job.id` and fire the detached run task.
pub fn spawn(db: DbHandle, registry: Registry, job: Job) {
    let ct = CancellationToken::new();
    if let Ok(mut reg) = registry.lock() {
        reg.insert(job.id.clone(), ct.clone());
    }
    tokio::spawn(async move {
        run(&db, &job, &ct).await;
        if let Ok(mut reg) = registry.lock() {
            reg.remove(&job.id);
        }
    });
}

/// Request cancellation of a running task. Returns true if a live token was
/// found and fired (the task writes `cancelled` and deregisters itself).
pub fn request_cancel(registry: &Registry, id: &str) -> bool {
    if let Ok(reg) = registry.lock()
        && let Some(tok) = reg.get(id)
    {
        tok.cancel();
        return true;
    }
    false
}

/// One fallback attempt's classified failure, kept for `fallback_history`.
struct AttemptRecord {
    model: String,
    kind: BackendErrorKind,
    detail: String,
}

/// Retry loop: try `job.model`, then each entry in `job.model_fallback` in
/// order, on a transient backend error (`errkind::classify`). Reuses the same
/// task row/id for every attempt — never re-enters `insert_queued` (the row
/// is already `running`, and the one-run-per-`working_dir` guard lives only
/// at insert time; a retry that resubmitted would trip its own prior
/// attempt's `dir_busy`).
async fn run(db: &DbHandle, job: &Job, ct: &CancellationToken) {
    if ct.is_cancelled() {
        db_finish(
            db,
            &job.id,
            store::STATUS_CANCELLED,
            None,
            None,
            Some("cancelled before the backend started"),
        );
        return;
    }

    let attempts: Vec<Option<String>> = std::iter::once(job.model.clone())
        .chain(job.model_fallback.iter().flatten().cloned().map(Some))
        .collect();
    let last_idx = attempts.len() - 1;
    let mut history: Vec<AttemptRecord> = Vec::new();

    for (idx, model) in attempts.iter().enumerate() {
        if ct.is_cancelled() {
            db_finish(
                db,
                &job.id,
                store::STATUS_CANCELLED,
                None,
                None,
                Some("cancelled before the backend started"),
            );
            return;
        }

        let outcome = run_attempt(db, job, model.as_deref(), idx, ct).await;

        if matches!(outcome, backend::RunOutcome::Cancelled) {
            finish_with_history(db, &job.id, outcome, model.as_deref(), &history);
            return;
        }

        if let Some(text) = failure_text(&outcome) {
            let kind = errkind::classify(&text);
            tracing::info!(
                "dispatch: {} attempt {}/{} model={:?} failed kind={} detail={:.200}",
                job.id,
                idx + 1,
                attempts.len(),
                model,
                kind.as_str(),
                text
            );
            if kind.is_retry_worthy() && idx != last_idx {
                history.push(AttemptRecord {
                    model: model.clone().unwrap_or_else(|| "(backend default)".into()),
                    kind,
                    detail: text,
                });
                continue;
            }
        }
        finish_with_history(db, &job.id, outcome, model.as_deref(), &history);
        return;
    }
}

/// One backend invocation for `model` (the `idx`-th attempt) — the
/// pre-refactor body of `run`, extracted so the retry loop can call it once
/// per fallback-chain entry. Each attempt mints its OWN nonce and takes its
/// own pre-spawn rollout snapshot (see the doc comment on the nonce-swap
/// below) rather than reusing attempt 0's — reusing one nonce across attempts
/// was an earlier design that turned out to be an actual bug: `locate_by_nonce`
/// has no snapshot/floor exclusion and returns on the first match, so a
/// retry's `record_rollout` could transiently match a still-on-disk PRIOR
/// attempt's (already-failed) rollout before its own newer one appears.
async fn run_attempt(
    db: &DbHandle,
    job: &Job,
    model: Option<&str>,
    idx: usize,
    ct: &CancellationToken,
) -> backend::RunOutcome {
    let spec = backend::SpawnSpec {
        working_dir: &job.working_dir,
        sandbox: &job.sandbox,
        model,
        reasoning_effort: job.reasoning_effort.as_deref(),
        skip_git_repo_check: job.skip_git_repo_check,
        resume_session: job.resume_session.as_deref(),
    };

    // Attempt 0 keeps the job's own nonce/prompt exactly as submitted (no
    // swap needed); a fallback retry (idx > 0) mints a fresh nonce and swaps
    // it into a copy of the prompt, so its own record_rollout call can never
    // match an earlier attempt's still-on-disk rollout.
    let (attempt_nonce, attempt_prompt): (Option<String>, std::borrow::Cow<'_, str>) =
        if idx == 0 || job.nonce.is_none() {
            (
                job.nonce.clone(),
                std::borrow::Cow::Borrowed(job.prompt.as_str()),
            )
        } else {
            let base = job.nonce.as_deref().unwrap_or("");
            let new_nonce = format!("{base}-retry{idx}");
            let swapped = job.prompt.replace(
                &render::nonce_marker(base),
                &render::nonce_marker(&new_nonce),
            );
            (Some(new_nonce), std::borrow::Cow::Owned(swapped))
        };

    if job.backend == Backend::Opencode {
        let ospec = opencode::RunSpec {
            id: &job.id,
            working_dir: &job.working_dir,
            sandbox: &job.sandbox,
            model,
            reasoning_effort: job.reasoning_effort.as_deref(),
            prompt: &attempt_prompt,
            state_dir: &job.state_dir,
            backend_version: job.backend_version.as_deref(),
            resume_session: job.resume_session.as_deref(),
            rollout_path: job.rollout_path.as_deref(),
        };
        return opencode::run(db, ospec, ct).await;
    }

    // Snapshot the rollouts that already exist BEFORE spawning THIS attempt,
    // so its own record_rollout call can require a file that did not exist
    // yet — never a pre-existing same-cwd session, including a prior
    // fallback attempt's own (already-failed) rollout.
    let snapshot = crate::rollout::session_snapshot();

    let spawned = match backend::spawn_child(job.backend, &spec, &attempt_prompt) {
        Ok(s) => s,
        Err(e) => return backend::RunOutcome::WaitFailed(e),
    };

    let child_pid = spawned.child_pid.map(|p| p as i64);
    let argv_json = serde_json::to_string(&spawned.argv).unwrap_or_default();
    db_mark_running(
        db,
        &job.id,
        child_pid,
        &argv_json,
        job.backend_version.as_deref(),
    );

    // Record the rollout codex just created (path + session id) so dispatch_logs
    // can tail it live and dispatch_steer can resume the session.
    record_rollout(
        db,
        &job.id,
        &job.working_dir,
        attempt_nonce.as_deref(),
        job.resume_session.as_deref(),
        &snapshot,
    )
    .await;

    backend::capture(spawned, ct).await
}

/// Extract the text `errkind::classify` should judge from a non-success
/// outcome. `None` for a success or a cancellation (neither is a failure to
/// classify — cancellation is handled separately, before this is called).
fn failure_text(outcome: &backend::RunOutcome) -> Option<String> {
    match outcome {
        backend::RunOutcome::Done {
            success: false,
            stderr,
            exit_code,
            ..
        } => Some(format!("exit_code={:?} stderr={}", exit_code, stderr)),
        backend::RunOutcome::WaitFailed(e) => Some(e.clone()),
        backend::RunOutcome::Done { success: true, .. } | backend::RunOutcome::Cancelled => None,
    }
}

/// Write the terminal outcome via the existing `finish_outcome` (unchanged),
/// then — only when at least one fallback attempt was tried and discarded —
/// record which model actually produced the result plus the discarded
/// attempts' history via the narrow `store::set_fallback_result` addition.
fn finish_with_history(
    db: &DbHandle,
    id: &str,
    outcome: backend::RunOutcome,
    final_model: Option<&str>,
    history: &[AttemptRecord],
) {
    finish_outcome(db, id, outcome);
    if !history.is_empty() {
        let history_json = serde_json::to_string(
            &history
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "model": a.model,
                        "error_kind": a.kind.as_str(),
                        "detail": a.detail.chars().take(2000).collect::<String>(),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());
        db_set_fallback_result(db, id, final_model, &history_json);
    }
}

fn db_set_fallback_result(db: &DbHandle, id: &str, final_model: Option<&str>, history_json: &str) {
    match db.lock() {
        Ok(conn) => {
            if let Err(e) = store::set_fallback_result(&conn, id, final_model, history_json) {
                tracing::warn!("dispatch: set_fallback_result({id}) failed: {e}");
            }
        }
        Err(e) => tracing::warn!("dispatch: db lock poisoned in set_fallback_result: {e}"),
    }
}

fn finish_outcome(db: &DbHandle, id: &str, outcome: backend::RunOutcome) {
    match outcome {
        backend::RunOutcome::Done {
            exit_code,
            success,
            stdout,
            stdout_total,
            stdout_truncated,
            stderr,
            stderr_truncated,
        } => {
            let status = if success {
                store::STATUS_SUCCEEDED
            } else {
                store::STATUS_FAILED
            };
            let result = build_result(&stdout, stdout_total, stdout_truncated);
            let error = if success {
                None
            } else {
                Some(build_error(&stderr, stderr_truncated, exit_code))
            };
            db_finish(
                db,
                id,
                status,
                exit_code.map(|c| c as i64),
                Some(&result),
                error.as_deref(),
            );
        }
        backend::RunOutcome::Cancelled => {
            db_finish(
                db,
                id,
                store::STATUS_CANCELLED,
                None,
                None,
                Some("cancelled by request; the backend process group was killed"),
            );
        }
        backend::RunOutcome::WaitFailed(e) => {
            db_finish(db, id, store::STATUS_FAILED, None, None, Some(&e));
        }
    }
}

/// Locate the rollout this run produced and persist its path + session id (so
/// dispatch_logs can tail it and dispatch_steer can resume). codex writes the rollout
/// at startup, so a short retry covers the spawn → first-write gap. The match is
/// **positive**, never "newest same-cwd": a resume run is found by its already-known
/// session id; a fresh run by its prompt nonce (falling back to the pre-spawn snapshot
/// diff only when there is no nonce). The DB lock is held only for the brief
/// `set_session`, never across the sleep.
async fn record_rollout(
    db: &DbHandle,
    id: &str,
    working_dir: &Path,
    nonce: Option<&str>,
    resume_session: Option<&str>,
    snapshot: &HashSet<PathBuf>,
) {
    for _ in 0..ROLLOUT_LOCATE_ATTEMPTS {
        let found = match resume_session {
            // Resume: the session id is known — locate its rollout deterministically by
            // id (codex appends the new turn to the same file).
            Some(sid) => crate::rollout::locate_by_session_id(sid).map(|p| (p, sid.to_string())),
            // Fresh: prefer the nonce (positive identity, robust to a concurrent
            // same-cwd codex); fall back to the snapshot diff only when there's no nonce.
            None => match nonce {
                Some(n) => crate::rollout::locate_by_nonce(working_dir, n),
                None => crate::rollout::locate_new_by_cwd(working_dir, snapshot, None),
            },
        };
        if let Some((path, sid)) = found {
            if let Ok(conn) = db.lock()
                && let Err(e) = store::set_session(&conn, id, &sid, &path.to_string_lossy())
            {
                tracing::warn!("dispatch: set_session({id}) failed: {e}");
            }
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    tracing::warn!("dispatch: could not locate codex rollout for {id}");
}

fn build_result(stdout: &str, total: usize, truncated: bool) -> String {
    if truncated {
        format!(
            "{stdout}\n\n[stdout truncated; captured first {} of {} bytes]",
            stdout.len(),
            total
        )
    } else {
        stdout.to_string()
    }
}

fn build_error(stderr: &str, truncated: bool, exit_code: Option<i32>) -> String {
    let mut s = String::new();
    if let Some(c) = exit_code {
        s.push_str(&format!("exit code {c}\n"));
    }
    if stderr.is_empty() {
        s.push_str("(no stderr captured)");
    } else {
        s.push_str(stderr);
        if truncated {
            s.push_str("\n[stderr truncated]");
        }
    }
    s
}

// ── DB helpers: lock briefly, write, drop the guard before any await ──

fn db_mark_running(db: &DbHandle, id: &str, pid: Option<i64>, argv: &str, ver: Option<&str>) {
    match db.lock() {
        Ok(conn) => {
            if let Err(e) = store::mark_running(&conn, id, pid, argv, ver) {
                tracing::warn!("dispatch: mark_running({id}) failed: {e}");
            }
        }
        Err(e) => tracing::warn!("dispatch: db lock poisoned in mark_running: {e}"),
    }
}

fn db_finish(
    db: &DbHandle,
    id: &str,
    status: &str,
    exit_code: Option<i64>,
    result: Option<&str>,
    error: Option<&str>,
) {
    match db.lock() {
        Ok(conn) => {
            if let Err(e) = store::finish(&conn, id, status, exit_code, result, error) {
                tracing::warn!("dispatch: finish({id}) failed: {e}");
            }
        }
        Err(e) => tracing::warn!("dispatch: db lock poisoned in finish: {e}"),
    }
}
