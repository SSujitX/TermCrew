//! Whole-process-tree termination for spawned agents.
//!
//! `portable-pty` can only kill the direct child. Agents spawn their own
//! children (node, python, npm wrappers) that would otherwise outlive the
//! session and leak CPU until manually killed. This module attaches every
//! PTY child to an OS kill-tree primitive at spawn time:
//!
//! - Windows: the child is assigned to a Job Object with
//!   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Dropping the guard — or the
//!   backend process exiting and the handle being closed by the kernel —
//!   terminates the entire tree, including processes spawned before
//!   assignment completes... and any spawned later, since children inherit
//!   job membership.
//! - Unix: `portable-pty` calls `setsid()` in the child, so the child is a
//!   process-group leader (pgid == pid). We signal the whole group.

use tracing::{debug, warn};

pub struct ProcessGuard {
    inner: GuardInner,
}

enum GuardInner {
    #[cfg(windows)]
    Job(job::JobHandle),
    #[cfg(unix)]
    Group { pgid: i32 },
}

impl ProcessGuard {
    /// Attaches the freshly spawned process to a kill-tree primitive.
    /// Returns `None` when attachment failed; the caller should log and
    /// degrade to direct-child kill.
    pub fn attach(pid: u32) -> Option<Self> {
        match Self::try_attach(pid) {
            Ok(guard) => {
                debug!(pid, "Attached process to kill-tree guard");
                Some(guard)
            }
            Err(e) => {
                warn!(pid, error = %e, "Could not attach process to kill-tree guard");
                None
            }
        }
    }

    fn try_attach(pid: u32) -> Result<Self, String> {
        #[cfg(windows)]
        {
            let job = job::JobHandle::create().map_err(|e| format!("CreateJobObjectW: {e}"))?;
            job.assign(pid).map_err(|e| format!("AssignProcessToJobObject: {e}"))?;
            Ok(Self { inner: GuardInner::Job(job) })
        }
        #[cfg(unix)]
        {
            // kill(pid, 0) performs a liveness check without signalling.
            if unsafe { libc::kill(pid as i32, 0) } != 0 {
                return Err("child is not running (kill(pid, 0) failed)".to_string());
            }
            // portable-pty setsid()s the child, so it is a group leader.
            Ok(Self { inner: GuardInner::Group { pgid: pid as i32 } })
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = pid;
            Err("unsupported platform".to_string())
        }
    }

    /// Terminates the entire process tree. Safe to call more than once.
    pub fn kill(&self) {
        match &self.inner {
            #[cfg(windows)]
            GuardInner::Job(job) => {
                if let Err(e) = job.terminate() {
                    warn!(error = %e, "TerminateJobObject failed; falling back to direct-child kill");
                }
            }
            #[cfg(unix)]
            GuardInner::Group { pgid } => group::terminate_group(*pgid),
        }
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // Windows: closing the job handle fires KILL_ON_JOB_CLOSE in the kernel.
        #[cfg(unix)]
        if let GuardInner::Group { pgid } = &self.inner {
            // Tear-down path: no SIGTERM grace period, just reap the group.
            unsafe {
                libc::kill(-*pgid, libc::SIGKILL);
            }
        }
    }
}

#[cfg(windows)]
mod job {
    use std::mem::size_of;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

    /// Owns the job handle. Closing it is what triggers KILL_ON_JOB_CLOSE,
    /// so `Drop` must always run — even on panic unwind during shutdown.
    pub struct JobHandle(HANDLE);

    // The handle is only used through the thread-safe Win32 job API.
    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    impl JobHandle {
        pub fn create() -> windows::core::Result<Self> {
            let handle = unsafe { CreateJobObjectW(None, None)? };
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const core::ffi::c_void,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )?;
            }
            Ok(Self(handle))
        }

        pub fn assign(&self, pid: u32) -> windows::core::Result<()> {
            let process = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid)? };
            let result = unsafe { AssignProcessToJobObject(self.0, process) };
            unsafe { let _ = CloseHandle(process); }
            result
        }

        pub fn terminate(&self) -> windows::core::Result<()> {
            unsafe { TerminateJobObject(self.0, 1) }
        }
    }

    impl Drop for JobHandle {
        fn drop(&mut self) {
            unsafe {
                if let Err(e) = CloseHandle(self.0) {
                    tracing::warn!(error = %e, "Failed to close job object handle");
                }
            }
        }
    }
}

#[cfg(unix)]
mod group {
    use std::time::Duration;

    /// SIGTERM the group, then SIGKILL after a short grace period if any
    /// member is still alive. The direct child almost always handles the
    /// first signal; the escalation is for grandchildren that ignore it.
    pub fn terminate_group(pgid: i32) {
        unsafe {
            libc::kill(-pgid, libc::SIGTERM);
        }
        std::thread::Builder::new()
            .name("pgroup-reaper".into())
            .spawn(move || {
                for _ in 0..10 {
                    std::thread::sleep(Duration::from_millis(100));
                    // kill(-pgid, 0): ESRCH means no group member is left.
                    if unsafe { libc::kill(-pgid, 0) } != 0 {
                        return;
                    }
                }
                unsafe {
                    libc::kill(-pgid, libc::SIGKILL);
                }
            })
            .ok();
    }
}
