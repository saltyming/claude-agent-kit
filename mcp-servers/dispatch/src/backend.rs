//! Backend CLI adapters for dispatch.
//!
//! Unlike `aside` (read-only Q&A), dispatch runs the backend **write-capable**:
//! codex executes in `-s workspace-write` and may modify files in the target
//! directory. The argv template per backend is localised in `build_command`, so
//! future CLI syntax drift is a single-site change; a future agent is a new
//! `Backend` variant plus one match arm.
//!
//! This module owns the subprocess mechanics — command construction, spawn,
//! capped stdout/stderr capture, process-group teardown on cancel. The
//! orchestration around it (the cancellation registry, the SQLite write-back)
//! lives in `executor.rs`, which is backend-agnostic.
//!
//! There is intentionally no wall-clock timeout — a delegated step can take
//! many minutes. Cancellation is explicit via `dispatch_cancel`, which fires
//! the `CancellationToken` this module selects on.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// stdout cap: dispatched runs are agent transcripts and can be verbose, so this
/// is larger than aside's 50 KB. Overflow is drained (so the child never blocks
/// on a full pipe) but discarded, and a truncation marker is recorded.
const MAX_STDOUT: usize = 200 * 1024;
const MAX_STDERR: usize = 16 * 1024;

/// Which coding-agent CLI we delegate to. Codex is the only backend today; a
/// future agent is a new variant + a match arm in `build_command`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Codex,
}

impl Backend {
    pub fn binary(&self) -> &'static str {
        match self {
            Backend::Codex => "codex",
        }
    }

    /// Stable string used in params, the DB `backend` column, and tool output.
    pub fn as_str(&self) -> &'static str {
        self.binary()
    }

    /// Parse the `backend` param. Empty/absent defaults to Codex.
    pub fn parse(s: &str) -> Option<Backend> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "codex" => Some(Backend::Codex),
            _ => None,
        }
    }

    pub fn all() -> &'static [Backend] {
        &[Backend::Codex]
    }
}

/// Everything `build_command` needs except the prompt (which is written to the
/// child's stdin, not passed on argv).
pub struct SpawnSpec<'a> {
    pub working_dir: &'a Path,
    pub sandbox: &'a str,
    pub model: Option<&'a str>,
    pub reasoning_effort: Option<&'a str>,
    pub skip_git_repo_check: bool,
    /// When `Some(session_id)`, continue an existing codex session
    /// (`codex exec resume <id>`) instead of starting a fresh one — the basis of
    /// `dispatch_steer`. The accumulated conversation context is preserved.
    pub resume_session: Option<&'a str>,
}

/// Build the `Command` for a backend. stdio, process group, and kill_on_drop are
/// configured by `spawn_child`. Returns the command plus the argv vector (for
/// audit recording). The prompt is intentionally absent from argv.
pub fn build_command(backend: Backend, spec: &SpawnSpec) -> (Command, Vec<String>) {
    match backend {
        Backend::Codex => {
            // codex -C <dir> -s <sandbox> -a never [-m MODEL] [-c model_reasoning_effort=EFF]
            //       exec [resume <sid>] [--skip-git-repo-check]   (prompt on stdin)
            //   -C <dir>:     working root, set explicitly (and recorded in argv) to match
            //                 the cmd.current_dir below — so the rollout's cwd is unambiguous.
            //   -s <sandbox>: workspace-write lets codex edit files under cwd; read-only is
            //                 read-only; danger-full-access drops the sandbox (server-gated).
            //   -a never:     non-interactive — never pause for an approval prompt.
            //   exec:         non-interactive subcommand. With no positional PROMPT, codex reads
            //                 instructions from stdin, which dodges OS argv-length limits.
            //   --skip-git-repo-check: permit running when working_dir is not a git repo.
            let mut args: Vec<String> = vec![
                "-C".into(),
                spec.working_dir.to_string_lossy().into_owned(),
                "-s".into(),
                spec.sandbox.into(),
                "-a".into(),
                "never".into(),
            ];
            if let Some(m) = spec.model {
                args.push("-m".into());
                args.push(m.into());
            }
            if let Some(eff) = spec.reasoning_effort {
                args.push("-c".into());
                args.push(format!("model_reasoning_effort={}", eff));
            }
            args.push("exec".into());
            if let Some(sid) = spec.resume_session {
                // codex exec resume <session_id>: continue the prior session with the
                // new prompt (on stdin), preserving its accumulated context.
                args.push("resume".into());
                args.push(sid.into());
            }
            if spec.skip_git_repo_check {
                args.push("--skip-git-repo-check".into());
            }

            let mut cmd = Command::new(backend.binary());
            cmd.args(&args);
            cmd.current_dir(spec.working_dir);

            let mut argv = vec![backend.binary().to_string()];
            argv.extend(args);
            (cmd, argv)
        }
    }
}

/// A spawned child plus the data the executor needs to record `running`.
pub struct Spawned {
    pub child: tokio::process::Child,
    pub child_pid: Option<u32>,
    pub argv: Vec<String>,
}

/// Outcome of awaiting a spawned child to completion (or cancellation).
pub enum RunOutcome {
    Done {
        exit_code: Option<i32>,
        success: bool,
        stdout: String,
        stdout_total: usize,
        stdout_truncated: bool,
        stderr: String,
        stderr_truncated: bool,
    },
    /// The child was killed because the cancellation token fired.
    Cancelled,
    /// Awaiting the child's exit status failed (rare; not a spawn failure).
    WaitFailed(String),
}

/// Spawn the backend child: pipe stdio, put it in its own process group (so the
/// whole subtree can be torn down on cancel), arm `kill_on_drop`, and stream the
/// rendered prompt to stdin in a detached writer (a large prompt can't deadlock
/// against the child filling stdout). Returns immediately after spawn.
pub fn spawn_child(backend: Backend, spec: &SpawnSpec, prompt: &str) -> Result<Spawned, String> {
    if which(backend.binary()).is_none() {
        return Err(format!(
            "backend `{}` not found on PATH — {}",
            backend.binary(),
            install_hint(backend)
        ));
    }

    let (mut cmd, argv) = build_command(backend, spec);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);
    #[cfg(unix)]
    {
        // New process group with pgid == child pid, so `kill(-pgid, …)` on cancel
        // reaps codex AND any shells / test runners it spawned. kill_on_drop only
        // reaps the direct child.
        cmd.process_group(0);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn {} failed: {}", backend.binary(), e))?;
    let child_pid = child.id();

    if let Some(mut stdin) = child.stdin.take() {
        let bytes = prompt.as_bytes().to_vec();
        tokio::spawn(async move {
            let _ = stdin.write_all(&bytes).await;
            let _ = stdin.shutdown().await; // close → EOF so codex stops reading
        });
    }

    Ok(Spawned {
        child,
        child_pid,
        argv,
    })
}

/// Await the child, capturing capped stdout/stderr, or kill its process group if
/// the cancellation token fires first.
pub async fn capture(spawned: Spawned, ct: &CancellationToken) -> RunOutcome {
    let Spawned {
        mut child,
        child_pid,
        argv: _,
    } = spawned;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_task = tokio::spawn(read_capped_opt(stdout, MAX_STDOUT));
    let err_task = tokio::spawn(read_capped_opt(stderr, MAX_STDERR));

    // Select cancellation against the child's own exit. The cancel arm touches only
    // the copied `child_pid`, never `child`, so there is no second `&mut child`
    // borrow; when cancel wins, the `child.wait()` future is dropped *un-reaped*.
    let status: Option<std::io::Result<std::process::ExitStatus>> = tokio::select! {
        biased;
        _ = ct.cancelled() => None,
        r = child.wait() => Some(r),
    };

    if status.is_none() {
        // Cancelled: the child is still LIVE here (wait did not complete), so kill
        // its whole process group while the pid is unambiguously ours, THEN reap it.
        // Killing before reaping means the group signal can never hit a reused pgid.
        if let Some(p) = child_pid {
            kill_process_group(p);
        }
        let _ = child.wait().await;
    }

    // Drain the capped readers. Caveat: on a *natural* exit where a descendant
    // inherited stdout and outlives codex, this await blocks until that descendant
    // closes the pipe, so the row could linger in `running`. codex `exec` does not
    // leave lingering daemons, and the cancel path's group-kill closes the fds — so
    // this is an accepted, documented edge, not a wall-clock timeout.
    let out = out_task.await.unwrap_or_else(|_| Capped::empty());
    let err = err_task.await.unwrap_or_else(|_| Capped::empty());

    match status {
        None => RunOutcome::Cancelled,
        Some(Ok(st)) => RunOutcome::Done {
            exit_code: st.code(),
            success: st.success(),
            stdout: out.text,
            stdout_total: out.total,
            stdout_truncated: out.truncated,
            stderr: err.text,
            stderr_truncated: err.truncated,
        },
        Some(Err(e)) => RunOutcome::WaitFailed(format!("wait failed: {}", e)),
    }
}

// ── capped capture ────────────────────────────────────────

struct Capped {
    text: String,
    total: usize,
    truncated: bool,
}

impl Capped {
    fn empty() -> Self {
        Capped {
            text: String::new(),
            total: 0,
            truncated: false,
        }
    }
}

async fn read_capped_opt<R: AsyncRead + Unpin>(reader: Option<R>, cap: usize) -> Capped {
    match reader {
        Some(r) => read_capped(r, cap)
            .await
            .unwrap_or_else(|_| Capped::empty()),
        None => Capped::empty(),
    }
}

/// Read a stream into a `cap`-bounded buffer. Bytes past the cap are drained
/// (kept reading so the child never blocks on a full pipe) but discarded.
async fn read_capped<R: AsyncRead + Unpin>(mut r: R, cap: usize) -> std::io::Result<Capped> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut total = 0usize;
    loop {
        let n = r.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        total += n;
        if buf.len() < cap {
            let take = (cap - buf.len()).min(n);
            buf.extend_from_slice(&chunk[..take]);
        }
        // else: drain and discard the overflow.
    }
    let truncated = total > buf.len();
    Ok(Capped {
        text: String::from_utf8_lossy(&buf).into_owned(),
        total,
        truncated,
    })
}

// ── process-group teardown ────────────────────────────────

#[cfg(unix)]
fn kill_process_group(pid: u32) {
    // `spawn_child` made the child its own group leader (process_group(0)), so the
    // group id equals the child pid. A negative pid signals the whole group.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_process_group(_pid: u32) {
    // Best-effort on non-unix: kill_on_drop reaps the direct child when the
    // capture future is dropped. Process-group teardown is unix-only.
}

// ── discovery ─────────────────────────────────────────────

/// Minimal PATH lookup — returns Some(path) if the binary is executable.
pub fn which(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{}.exe", binary));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

/// Ask the backend CLI for its `--version` string. Returns `None` if missing.
pub async fn version(backend: Backend) -> Option<String> {
    let _ = which(backend.binary())?;
    let output = Command::new(backend.binary())
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;
    let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !out.is_empty() {
        return Some(out);
    }
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if err.is_empty() { None } else { Some(err) }
}

pub fn install_hint(backend: Backend) -> String {
    match backend {
        Backend::Codex => {
            "install codex CLI (`npm i -g @openai/codex`; see https://github.com/openai/codex)"
                .to_string()
        }
    }
}
