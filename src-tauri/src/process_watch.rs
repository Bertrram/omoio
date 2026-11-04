//! The game's emulator, held by a handle of Omoio's own from the moment it
//! starts.
//!
//! A handle tells whether the process still runs without starting tasklist
//! to look for its id, and once it has ended, the code it ended with: what
//! it gave ExitProcess or returned from main, or the exception's own code
//! when an exception nothing handled ended it ("GetExitCodeProcess
//! function", Remarks, and "Terminating a Process", Microsoft Learn, both
//! read 4 November 2026). What the code means is for core::ending.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::Child;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE,
};

/// A game's emulator as its backend started it: the id the rest of Omoio
/// knows it by, and the process itself.
pub struct Launched {
    pub pid: u32,
    /// `None` only when Windows wouldn't open Omoio's own child, which
    /// leaves the id alone to go by.
    pub process: Option<Watched>,
}

impl Launched {
    /// For a backend's `launch`, before the spawned `child` is dropped or
    /// handed on (see `Watched::from_child`).
    pub fn from_child(child: &Child) -> Launched {
        Launched {
            pid: child.id(),
            process: Watched::from_child(child),
        }
    }
}

/// One process, held open, so its exit code and times stay readable after
/// it has gone: the process object lasts as long as any handle to it does
/// ("Terminating a Process", Microsoft Learn, read 4 November 2026).
pub struct Watched {
    pub pid: u32,
    /// Closed by Rust when the `Watched` is dropped, whichever way that
    /// comes about.
    handle: OwnedHandle,
    /// When Windows created the process, a FILETIME as one number:
    /// 100-nanosecond steps since 1 January 1601, UTC
    /// ("GetProcessTimes function", Microsoft Learn, read 4 November 2026).
    /// Windows' own reports of a crash name the process by its id and this
    /// time, which is how core::ending tells one is about this process.
    pub created: u64,
}

impl Watched {
    /// Opens the process `child` is with no more rights than waiting for
    /// its end and reading its exit code and times take: SYNCHRONIZE for
    /// the wait, PROCESS_QUERY_LIMITED_INFORMATION for GetExitCodeProcess
    /// and GetProcessTimes ("Process Security and Access Rights", Microsoft
    /// Learn, read 4 November 2026), rather than a copy of the spawn's own
    /// handle, which has every right.
    ///
    /// Done while `child` still holds that handle. An id passes to another
    /// process only once the last handle to the process has closed (Raymond
    /// Chen, "When does a process ID become available for reuse?", The Old
    /// New Thing, read 4 November 2026), so this one is still the
    /// emulator's, even one that failed the moment it started. Opened by id
    /// once the child had been dropped, it could be gone or be another's.
    pub fn from_child(child: &Child) -> Option<Watched> {
        let pid = child.id();
        let opened = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
        // Owned from here on, so it is closed even if the times can't be read.
        let handle = unsafe { OwnedHandle::from_raw_handle(opened.0) };
        let mut created = FILETIME::default();
        let (mut exited, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { GetProcessTimes(HANDLE(handle.as_raw_handle()), &mut created, &mut exited, &mut kernel, &mut user) }
            .ok()?;
        Some(Watched {
            pid,
            handle,
            created: (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
        })
    }

    /// Whether the process still runs. Only a wait that timed out says it
    /// does; one that failed counts as an end, so nothing waits for ever on
    /// a process Windows won't say anything about.
    pub fn running(&self) -> bool {
        let waited = unsafe { WaitForSingleObject(self.raw(), 0) };
        waited == WAIT_TIMEOUT
    }

    /// The code the process ended with, once it has; `None` while it runs
    /// and when Windows won't say. Read only once the handle is signalled:
    /// before that GetExitCodeProcess gives STILL_ACTIVE, 259, which a
    /// process may also end with ("GetExitCodeProcess function", Remarks,
    /// Microsoft Learn, read 4 November 2026; `cmd /c exit 259` does).
    pub fn exit_code(&self) -> Option<u32> {
        if unsafe { WaitForSingleObject(self.raw(), 0) } != WAIT_OBJECT_0 {
            return None;
        }
        let mut code = 0u32;
        unsafe { GetExitCodeProcess(self.raw(), &mut code) }.ok()?;
        Some(code)
    }

    /// `created` as a time, for asking Windows' logs what came after it.
    pub fn started(&self) -> SystemTime {
        // 1601 to 1970 is 134,774 days, 11,644,473,600 seconds.
        const UNIX_EPOCH_AS_FILETIME: u64 = 116_444_736_000_000_000;
        let since_epoch = self.created.saturating_sub(UNIX_EPOCH_AS_FILETIME);
        UNIX_EPOCH + Duration::from_nanos(since_epoch.saturating_mul(100))
    }

    fn raw(&self) -> HANDLE {
        HANDLE(self.handle.as_raw_handle())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    /// Runs `cmd /c exit <code>` and reads the code it ended with through
    /// the handle alone, the child dropped at once, as RPCS3's and
    /// Dolphin's are once `launch` returns.
    fn ended_with(code: &str) -> Option<u32> {
        let child = Command::new("cmd.exe")
            .args(["/c", "exit", code])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let watched = Watched::from_child(&child).expect("a handle to Omoio's own child");
        drop(child);
        let give_up = Instant::now() + Duration::from_secs(10);
        while watched.running() && Instant::now() < give_up {
            std::thread::sleep(Duration::from_millis(20));
        }
        watched.exit_code()
    }

    #[test]
    fn reads_the_code_of_an_exception() {
        // cmd takes the code signed; this is 0xC0000005, an access
        // violation, with nothing crashed and nothing reported to Windows.
        assert_eq!(ended_with("-1073741819"), Some(0xC000_0005));
    }

    #[test]
    fn tells_an_end_with_259_from_a_process_still_running() {
        assert_eq!(ended_with("259"), Some(259));
    }

    #[test]
    fn gives_no_code_while_the_process_runs() {
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let launched = Launched::from_child(&child);
        assert_eq!(launched.pid, child.id());
        let watched = launched.process.expect("a handle to Omoio's own child");
        assert!(watched.running());
        assert_eq!(watched.exit_code(), None);
        let age = SystemTime::now().duration_since(watched.started());
        assert!(age.is_ok_and(|age| age < Duration::from_secs(60)));
        // Rust's own kill ends it with 1 (`TerminateProcess(handle, 1)` in
        // std's sys/process/windows.rs, Rust 1.98.1), as taskkill /F does.
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!watched.running());
        assert_eq!(watched.exit_code(), Some(1));
    }
}
