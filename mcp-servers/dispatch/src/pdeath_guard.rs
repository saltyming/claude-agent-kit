//! Parent-death guard: a hidden re-invocation of the `dispatch` binary
//! (`dispatch __pdeath_guard <dispatch_pid> -- <real_binary> <arg>...`) that sits
//! between dispatch and a backend child so the whole subtree dies with dispatch,
//! even under a hard `SIGKILL` of dispatch itself.
//!
//! Why a guard, not a bare `PR_SET_PDEATHSIG` on the backend directly: the
//! primitive only arms the ONE process that calls it — it does not cover a
//! descendant the backend spawns mid-run (e.g. a shell or test runner). Turning
//! "parent died" into "kill the whole group" needs code we control reacting to
//! that signal, and that code can't live inside codex/opencode's own binary
//! since we don't control their source. So this guard sits in between:
//! - it becomes the pgid leader in place of the backend (dispatch already put
//!   the guard in its own process group when spawning it)
//! - the real backend inherits that pgid without calling `process_group(0)`
//!   itself
//! - on parent death, the guard kills the whole group
//! - on backend exit, the guard reaps it and mirrors the exit status onto
//!   itself, so `capture()`/`opencode::run()` need zero changes — they see
//!   this as "the child's" exit status either way
//!
//! Deliberately synchronous / no Tokio: the guard's entire job is a couple of
//! blocking syscalls, so keeping its runtime surface minimal keeps its own
//! failure modes minimal. `main.rs` never boots the Tokio runtime for this path.

use std::ffi::{OsStr, OsString};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Stdio};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
use linux::{arm_parent_watch, wait_for_either};
#[cfg(target_os = "macos")]
use macos::{arm_parent_watch, wait_for_either};

enum Outcome {
    ChildExited(ExitStatus),
    ParentDied,
}

/// Entry point for `dispatch __pdeath_guard <dispatch_pid> -- <argv...>`.
/// `args` is the remainder of argv after the `__pdeath_guard` token itself
/// (see `main.rs`'s argv-sniff before Tokio boots).
pub fn run(args: std::env::ArgsOs) -> Result<(), Box<dyn std::error::Error>> {
    let (dispatch_pid, argv) = parse_args(args)?;
    run_inner(dispatch_pid, &argv)
}

fn parse_args(args: std::env::ArgsOs) -> Result<(u32, Vec<OsString>), Box<dyn std::error::Error>> {
    let mut args = args.peekable();
    let pid_arg = args
        .next()
        .ok_or("pdeath_guard: missing <dispatch_pid> argument")?;
    let dispatch_pid: u32 = pid_arg
        .to_str()
        .and_then(|s| s.parse().ok())
        .ok_or("pdeath_guard: <dispatch_pid> must be a positive integer")?;
    if args.next().as_deref() != Some(OsStr::new("--")) {
        return Err("pdeath_guard: expected `--` before the real backend argv".into());
    }
    let argv: Vec<OsString> = args.collect();
    if argv.is_empty() {
        return Err("pdeath_guard: missing real backend argv after `--`".into());
    }
    Ok((dispatch_pid, argv))
}

fn run_inner(dispatch_pid: u32, argv: &[OsString]) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Arm the parent-death primitive FIRST, before spawning the (write-capable)
    //    real backend. Arming, then rechecking liveness, closes the race where
    //    dispatch dies in the gap between a liveness check and the arm call — a
    //    check-then-arm ordering would leave that gap open instead.
    let watcher = arm_parent_watch(dispatch_pid)?;

    // 2. Re-check parent liveness immediately after arming. This only catches the
    //    (now much narrower) case of dispatch dying before the arm call above —
    //    everything after this point is covered by the armed watch itself.
    if !parent_is(dispatch_pid) {
        return Err("pdeath_guard: parent already gone before spawn — aborting".into());
    }

    // 3. Spawn the real backend. No process_group(0) here: the guard is already
    //    the pgid leader (dispatch spawned it with process_group(0)), so the real
    //    backend inherits that pgid by default. No stdio redirection: fd 0/1/2
    //    pass straight through from the guard, which are already the exact pipe
    //    ends dispatch created for the backend.
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("pdeath_guard: spawn {:?} failed: {e}", argv[0]))?;
    let child_pid = child.id();

    // 4. Block until either the child exits or the parent dies, whichever first.
    let outcome = wait_for_either(dispatch_pid, child_pid, &mut child, watcher)?;

    match outcome {
        Outcome::ChildExited(status) => {
            // Re-check parent liveness once more immediately before mirroring —
            // closes the narrow race where the child-exit event "won" the wait
            // while dispatch was also dying. If the parent is ALSO gone now,
            // prioritize the group-kill cleanup path over naively mirroring the
            // child's exit, so a descendant the child spawned isn't missed.
            if !parent_is(dispatch_pid) {
                kill_group();
            }
            mirror_exit(status);
        }
        Outcome::ParentDied => kill_group(),
    }
}

/// Kills the whole process group (guard + backend + any real descendants
/// sharing the pgid) and never returns.
fn kill_group() -> ! {
    unsafe {
        libc::kill(-(std::process::id() as i32), libc::SIGKILL);
    }
    // SIGKILL to our own group terminates this process too; reaching here is
    // only possible if that somehow failed (e.g. already reaped away), so exit
    // explicitly rather than fall through.
    std::process::exit(137);
}

fn parent_is(dispatch_pid: u32) -> bool {
    unsafe { libc::getppid() == dispatch_pid as libc::pid_t }
}

/// Mirrors the child's exit status onto the guard's own exit, so callers see
/// this as "the child's" exit status regardless of the guard indirection.
fn mirror_exit(status: ExitStatus) -> ! {
    if let Some(code) = status.code() {
        std::process::exit(code);
    }
    if let Some(sig) = status.signal() {
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
        // raise() only returns if the signal didn't terminate us — fall back to
        // a faithful shell-convention exit code.
        std::process::exit(128 + sig);
    }
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn osv(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    // parse_args takes std::env::ArgsOs, which cannot be constructed directly in
    // a unit test — these tests exercise the OsString-level parsing logic that
    // parse_args wraps, via a local re-implementation kept in lockstep. The real
    // entry point (`run`) is covered by the guard exit-status integration tests
    // in this module's sibling platform files.
    fn parse(items: &[&str]) -> Result<(u32, Vec<OsString>), String> {
        let mut it = osv(items).into_iter().peekable();
        let pid_arg = it.next().ok_or("missing <dispatch_pid> argument")?;
        let dispatch_pid: u32 = pid_arg
            .to_str()
            .and_then(|s| s.parse().ok())
            .ok_or("<dispatch_pid> must be a positive integer")?;
        if it.next().as_deref() != Some(OsStr::new("--")) {
            return Err("expected `--` before the real backend argv".into());
        }
        let argv: Vec<OsString> = it.collect();
        if argv.is_empty() {
            return Err("missing real backend argv after `--`".into());
        }
        Ok((dispatch_pid, argv))
    }

    #[test]
    fn parses_valid_argv() {
        let (pid, argv) = parse(&["1234", "--", "codex", "exec"]).unwrap();
        assert_eq!(pid, 1234);
        assert_eq!(argv, osv(&["codex", "exec"]));
    }

    #[test]
    fn rejects_missing_pid() {
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn rejects_non_numeric_pid() {
        assert!(parse(&["not-a-pid", "--", "codex"]).is_err());
    }

    #[test]
    fn rejects_missing_separator() {
        assert!(parse(&["1234", "codex"]).is_err());
    }

    #[test]
    fn rejects_empty_backend_argv() {
        assert!(parse(&["1234", "--"]).is_err());
    }

    #[test]
    fn parent_is_matches_real_parent_pid() {
        let real_parent = unsafe { libc::getppid() } as u32;
        assert!(parent_is(real_parent));
        assert!(!parent_is(real_parent.wrapping_add(1)));
    }
}
