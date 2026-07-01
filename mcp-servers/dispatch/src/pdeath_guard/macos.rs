//! macOS parent-death detection via `kqueue`/`EVFILT_PROC`/`NOTE_EXIT`.
//!
//! macOS has no passive kernel-pushed "notify me when my parent dies" primitive
//! (unlike Linux's `PR_SET_PDEATHSIG`), so this uses an active watch: register
//! `EVFILT_PROC`/`NOTE_EXIT` interest on both the dispatch pid and the real
//! backend's pid, then block on one `kevent()` call for whichever fires first.

use std::os::fd::RawFd;
use std::process::{Child, ExitStatus};

use super::Outcome;

pub struct Watcher {
    kq: RawFd,
}

enum Registered {
    Armed,
    /// The target was already gone before the watch could be armed (`ESRCH`).
    AlreadyGone,
}

fn register(kq: RawFd, pid: u32) -> Result<Registered, Box<dyn std::error::Error>> {
    let mut kev: libc::kevent = unsafe { std::mem::zeroed() };
    kev.ident = pid as usize;
    kev.filter = libc::EVFILT_PROC;
    kev.flags = libc::EV_ADD | libc::EV_ONESHOT;
    kev.fflags = libc::NOTE_EXIT;
    let r = unsafe { libc::kevent(kq, &kev, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
    if r < 0 {
        let err = std::io::Error::last_os_error();
        // ESRCH: the target is already gone — not a hard failure. The caller's
        // post-arm liveness recheck (for the parent) or immediate reap (for the
        // child) handles this case explicitly.
        if err.raw_os_error() == Some(libc::ESRCH) {
            return Ok(Registered::AlreadyGone);
        }
        return Err(format!("pdeath_guard: kevent register(pid={pid}) failed: {err}").into());
    }
    Ok(Registered::Armed)
}

/// Arms the parent-death watch. `dispatch_pid`'s registration failing with
/// `ESRCH` is not treated as fatal here — `run_inner`'s immediate post-arm
/// `parent_is` recheck independently detects "parent already gone" and aborts
/// before the backend is ever spawned, so there is no gap either way.
pub fn arm_parent_watch(dispatch_pid: u32) -> Result<Watcher, Box<dyn std::error::Error>> {
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return Err(format!(
            "pdeath_guard: kqueue() failed: {}",
            std::io::Error::last_os_error()
        )
        .into());
    }
    register(kq, dispatch_pid)?;
    Ok(Watcher { kq })
}

pub fn wait_for_either(
    dispatch_pid: u32,
    child_pid: u32,
    child: &mut Child,
    watcher: Watcher,
) -> Result<Outcome, Box<dyn std::error::Error>> {
    // Register the child's exit watch now that we know its pid. If the child
    // already exited in the brief window before we could arm this (a very fast
    // exit), reap it directly instead of waiting on a watch that will never fire.
    if let Registered::AlreadyGone = register(watcher.kq, child_pid)? {
        let status = child.wait()?;
        return Ok(Outcome::ChildExited(status));
    }

    // Sized for both possible events (parent-death, child-exit) in one batch.
    let mut events: [libc::kevent; 2] = unsafe { std::mem::zeroed() };
    loop {
        let n = unsafe {
            libc::kevent(
                watcher.kq,
                std::ptr::null(),
                0,
                events.as_mut_ptr(),
                events.len() as i32,
                std::ptr::null(),
            )
        };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue; // EINTR — retry the blocking wait
            }
            return Err(format!("pdeath_guard: kevent wait failed: {err}").into());
        }

        let mut parent_died = false;
        let mut child_exited = false;
        for ev in &events[..n as usize] {
            if ev.ident as u32 == dispatch_pid {
                parent_died = true;
            }
            if ev.ident as u32 == child_pid {
                child_exited = true;
            }
        }

        // Prioritize parent-death cleanup if both fired in the same batch —
        // otherwise a descendant the child spawned could be missed by naively
        // reaping the child and returning before the group-kill runs.
        if parent_died {
            return Ok(Outcome::ParentDied);
        }
        if child_exited {
            let status: ExitStatus = child.wait()?;
            return Ok(Outcome::ChildExited(status));
        }
        // Neither matched (shouldn't happen — EV_ONESHOT only fires for what we
        // registered) — loop and wait again defensively rather than assume.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    // Exercises the real kqueue arm + wait path end-to-end (not a mock): watches
    // our own pid as a stand-in "dispatch_pid" (stays alive for the whole test,
    // so only the child-exit branch can fire) and confirms a genuinely spawned
    // child's natural exit is detected and its status correctly reaped.
    #[test]
    fn wait_for_either_detects_natural_child_exit() {
        let my_pid = std::process::id();
        let watcher = arm_parent_watch(my_pid).expect("arm_parent_watch");
        let mut child = Command::new("true").spawn().expect("spawn `true`");
        let child_pid = child.id();
        match wait_for_either(my_pid, child_pid, &mut child, watcher).expect("wait_for_either") {
            Outcome::ChildExited(status) => assert!(status.success()),
            Outcome::ParentDied => panic!("spuriously reported parent death"),
        }
    }

    #[test]
    fn wait_for_either_reaps_child_that_exits_before_registration() {
        // `false`/`true` exit almost immediately, so this exercises the
        // AlreadyGone/ESRCH branch in `register` at least some of the time — not
        // deterministically (it's a real race), but across runs it covers both
        // the ESRCH-at-register-time path and the normal kevent-wins path.
        let my_pid = std::process::id();
        let watcher = arm_parent_watch(my_pid).expect("arm_parent_watch");
        let mut child = Command::new("true").spawn().expect("spawn `true`");
        std::thread::sleep(std::time::Duration::from_millis(50));
        let child_pid = child.id();
        match wait_for_either(my_pid, child_pid, &mut child, watcher).expect("wait_for_either") {
            Outcome::ChildExited(status) => assert!(status.success()),
            Outcome::ParentDied => panic!("spuriously reported parent death"),
        }
    }
}
