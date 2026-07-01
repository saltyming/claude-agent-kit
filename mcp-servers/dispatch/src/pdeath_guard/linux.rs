//! Linux parent-death detection via `PR_SET_PDEATHSIG` delivered through a
//! `signalfd`, rather than a raw async-signal handler.
//!
//! `PR_SET_PDEATHSIG` is a passive, kernel-pushed primitive (no polling): once
//! armed, the kernel sends the chosen signal to this process when its current
//! parent dies. It is deliberately armed with `SIGTERM`, a catchable signal —
//! not `SIGKILL` directly on the guard — because a `SIGKILL`-by-kernel on the
//! guard alone would not cascade to its children; code needs to run and do the
//! group-kill. `SIGCHLD` is watched on the same fd so both "parent died" and
//! "backend exited" can be waited on with one blocking `read()`.

use std::os::fd::RawFd;
use std::process::Child;

use super::Outcome;

pub struct Watcher {
    sfd: RawFd,
}

/// Arms `PR_SET_PDEATHSIG(SIGTERM)` on this process and sets up a `signalfd`
/// that also watches `SIGCHLD`, so `wait_for_either` can block on a single fd
/// for both "parent died" and "backend exited".
pub fn arm_parent_watch(_dispatch_pid: u32) -> Result<Watcher, Box<dyn std::error::Error>> {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGCHLD);
        // Block both on this thread first so they queue for signalfd instead of
        // being delivered asynchronously (or terminating us outright, for TERM).
        if libc::sigprocmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) != 0 {
            return Err(format!(
                "pdeath_guard: sigprocmask failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }
        let sfd = libc::signalfd(-1, &set, libc::SFD_CLOEXEC);
        if sfd < 0 {
            return Err(format!(
                "pdeath_guard: signalfd failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }
        // Arm LAST, after signalfd is ready to receive — if this returns -1, fail
        // the guard outright rather than proceed unprotected. PR_SET_PDEATHSIG is
        // a per-calling-thread attribute; this is fine because the guard is a
        // single-threaded, non-Tokio binary and never crosses threads afterward.
        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0) != 0 {
            return Err(format!(
                "pdeath_guard: prctl(PR_SET_PDEATHSIG) failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }
        Ok(Watcher { sfd })
    }
}

pub fn wait_for_either(
    _dispatch_pid: u32,
    _child_pid: u32,
    child: &mut Child,
    watcher: Watcher,
) -> Result<Outcome, Box<dyn std::error::Error>> {
    loop {
        let mut info: libc::signalfd_siginfo = unsafe { std::mem::zeroed() };
        let n = unsafe {
            libc::read(
                watcher.sfd,
                &mut info as *mut libc::signalfd_siginfo as *mut libc::c_void,
                std::mem::size_of::<libc::signalfd_siginfo>(),
            )
        };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue; // EINTR — retry the blocking read
            }
            return Err(format!("pdeath_guard: signalfd read failed: {err}").into());
        }
        match info.ssi_signo as i32 {
            libc::SIGCHLD => {
                // try_wait rather than assuming this exact signal instance was
                // "our" child — SIGCHLD can coalesce, but we only ever have one
                // child, so a spurious wake here just loops back to the read.
                match child.try_wait()? {
                    Some(status) => return Ok(Outcome::ChildExited(status)),
                    None => continue,
                }
            }
            libc::SIGTERM => return Ok(Outcome::ParentDied),
            _ => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    // Exercises the real prctl+signalfd arm + wait path end-to-end (not a
    // mock). `_dispatch_pid` is unused by this platform's `arm_parent_watch`
    // (PR_SET_PDEATHSIG ties to whatever the real current parent is, which for
    // a `cargo test` process stays alive for the test's duration), so this only
    // exercises the SIGCHLD/child-exit branch — the SIGTERM/parent-death branch
    // is exercised indirectly by `pdeath_guard.rs`'s own doc-verified logic and
    // the macOS sibling test, since triggering a real SIGTERM here would mean
    // killing the test harness's own parent.
    //
    // Runs the arm+spawn+wait dance in a forked child rather than directly on
    // the test thread: `sigprocmask` blocks SIGCHLD only on the calling
    // thread, but `cargo test`'s harness is itself multi-threaded (main
    // thread + worker pool). Any other thread still has SIGCHLD unblocked
    // with its default (Ignore) disposition, so the kernel can hand `true`'s
    // exit signal to one of THOSE threads and silently discard it before our
    // signalfd ever sees it — hanging the blocking `read()` forever. (This is
    // exactly what happened the first time this test ran on real Linux CI.)
    // A real `dispatch __pdeath_guard` process is genuinely single-threaded
    // (see the module doc), so blocking "the calling thread" there really
    // does block the whole process; `fork()` recreates that single-threaded
    // guarantee here, since only the forking thread survives into the child.
    #[test]
    fn wait_for_either_detects_natural_child_exit() {
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork failed: {}", std::io::Error::last_os_error());

        if pid == 0 {
            // Child: exactly one thread exists here, so the signalfd
            // approach is correct — this mirrors the real guard process.
            let ok = std::panic::catch_unwind(|| {
                let watcher = arm_parent_watch(0).expect("arm_parent_watch");
                let mut child = Command::new("true").spawn().expect("spawn `true`");
                let child_pid = child.id();
                matches!(
                    wait_for_either(0, child_pid, &mut child, watcher).expect("wait_for_either"),
                    Outcome::ChildExited(status) if status.success()
                )
            })
            .unwrap_or(false);
            std::process::exit(if ok { 0 } else { 1 });
        }

        // Parent: poll with WNOHANG instead of a blocking waitpid, so a
        // regression of the exact bug this fork-based isolation fixes fails
        // the test within a bounded time instead of wedging CI again.
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            let mut raw_status: i32 = 0;
            let r = unsafe { libc::waitpid(pid, &mut raw_status, libc::WNOHANG) };
            assert!(
                r >= 0,
                "waitpid failed: {}",
                std::io::Error::last_os_error()
            );
            if r == pid {
                break raw_status;
            }
            if Instant::now() >= deadline {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                    libc::waitpid(pid, &mut raw_status, 0);
                }
                panic!(
                    "forked test child did not exit within 10s — likely a hang regression in wait_for_either"
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        };

        assert!(
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
            "forked child reported failure (raw status={status:#x})"
        );
    }
}
