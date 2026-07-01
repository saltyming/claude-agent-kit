//! Windows process-orphaning protection via Job Objects.
//!
//! Unlike Linux/macOS (see `pdeath_guard.rs`), Windows needs no guard process:
//! `AssignProcessToJobObject` + `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` makes the
//! **kernel** the reactor — job membership propagates to children automatically,
//! and the whole job is torn down when the job handle's last reference closes,
//! with no process needing to stay alive to react. This module wraps that
//! mechanism directly around the child `spawn_child`/`spawn_server` already
//! produces; no re-invocation indirection is needed.

use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// A kill-on-close Job Object. Dropping it closes the handle; if no process was
/// ever assigned, that's a no-op teardown. Job Object handles are safe to use
/// from any thread (single logical owner enforced by the registry's mutex, not
/// by any thread-affinity in the Win32 API itself), so this is `Send`.
struct WinJob(HANDLE);

unsafe impl Send for WinJob {}

impl WinJob {
    fn new_kill_on_close() -> std::io::Result<Self> {
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            let err = std::io::Error::last_os_error();
            unsafe {
                CloseHandle(job);
            }
            return Err(err);
        }
        Ok(WinJob(job))
    }

    fn assign(&self, process: HANDLE) -> std::io::Result<()> {
        if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn terminate(&self, exit_code: u32) {
        unsafe {
            TerminateJobObject(self.0, exit_code);
        }
    }
}

impl Drop for WinJob {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn registry() -> &'static StdMutex<HashMap<u32, WinJob>> {
    static REG: OnceLock<StdMutex<HashMap<u32, WinJob>>> = OnceLock::new();
    REG.get_or_init(|| StdMutex::new(HashMap::new()))
}

/// Creates a kill-on-close Job Object, assigns the just-spawned process
/// (`process_handle`, from `Child::raw_handle()`) to it, and registers it under
/// `pid` so `terminate`/`release` can find it later. Called right after a
/// successful spawn.
///
/// Failure here is treated as fatal by the caller (`spawn_child`/`spawn_server`
/// kill the child and return an error) rather than silently continuing to run
/// write-capable work with no orphan protection — see `backend::spawn_child`'s
/// doc comment and the plan's explicit fail-closed decision for this case.
pub fn protect(pid: u32, process_handle: HANDLE) -> std::io::Result<()> {
    let job = WinJob::new_kill_on_close()?;
    job.assign(process_handle)?;
    if let Ok(mut reg) = registry().lock() {
        reg.insert(pid, job);
    }
    Ok(())
}

/// Terminates the Job Object registered for `pid` (killing the whole subtree —
/// the process plus any descendants that inherited job membership) and removes
/// the registry entry. Mirrors `backend::kill_process_group`'s Unix behavior at
/// the call site: an active-cancellation teardown.
pub fn terminate(pid: u32) {
    let job = registry().lock().ok().and_then(|mut reg| reg.remove(&pid));
    if let Some(job) = job {
        job.terminate(1);
    }
}

/// Removes the registry entry for `pid` WITHOUT terminating — used on a
/// natural-completion reap path, where the process already exited on its own
/// and the Job Object is no longer needed. Avoids unbounded registry growth
/// without spuriously calling `TerminateJobObject` on an already-gone process.
pub fn release(pid: u32) {
    if let Ok(mut reg) = registry().lock() {
        reg.remove(&pid);
    }
}

/// Real Windows liveness check backing `main.rs::process_alive`, so the
/// existing boot-time `reconcile()` backstop can actually mark a Windows row
/// `interrupted` when its owning server is gone (previously always assumed
/// alive — see the CHANGELOG entry for this delivery).
pub fn process_alive(pid: u32) -> bool {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut code as *mut u32);
        CloseHandle(handle);
        ok != 0 && code as i32 == STILL_ACTIVE
    }
}
