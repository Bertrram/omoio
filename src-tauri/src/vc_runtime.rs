//! Microsoft's Visual C++ runtime, which RPCS3, Cemu and Dolphin all need.
//!
//! Each is built with Visual Studio and links its C++ library as DLLs
//! (msvcp140.dll, vcruntime140.dll, vcruntime140_1.dll) that Windows doesn't
//! come with. Without them, starting the emulator puts up Windows' own "was
//! not found" box, one for each try, and the emulator never runs (issue #15).
//! Omoio itself doesn't need them: Tauri links the runtime into Omoio's own
//! program.
//!
//! The runtime's installer writes its version under
//! `HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64`. A program
//! built with Visual Studio 2022 17.10 or later can crash on a runtime older
//! than 14.40, the one that came with it, so an older one is updated too.
//! Where the key is missing but the DLLs are in System32, put there by some
//! other program's installer, they are taken as they are.
//!
//! The installer needs administrator rights, so Windows asks the user first.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

/// Microsoft's own link to the newest x64 runtime for Visual Studio 2015 to
/// 2022 and later.
const INSTALLER_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";
/// The oldest runtime the emulators are taken to run on: 14.40.
const OLDEST: (u32, u32) = (14, 40);

const DECLINED: &str = "The emulators need Microsoft's Visual C++ runtime, and it wasn't installed. Try again and choose Yes when Windows asks.";
const OFFLINE: &str = "Couldn't download Microsoft's Visual C++ runtime, which the emulators need. Check the internet connection and try again.";

/// Whether the runtime the emulators need is installed.
pub fn present() -> bool {
    match installed_version() {
        Some(version) => new_enough(version),
        None => dlls_in_system32(),
    }
}

fn new_enough((major, minor): (u32, u32)) -> bool {
    (major, minor) >= OLDEST
}

/// Installs the runtime when it is missing or too old, asking Windows for
/// administrator rights, and waits for it. Blocks, so it is called off the
/// window's thread.
pub fn ensure() -> Result<(), String> {
    // Two emulators being installed at once ask once.
    static TURN: Mutex<()> = Mutex::new(());
    let _turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
    if present() {
        return Ok(());
    }
    let installer = tauri::async_runtime::block_on(download())?;
    let ran = run_elevated(&installer);
    let _ = std::fs::remove_file(&installer);
    ran?;
    if present() {
        Ok(())
    } else {
        Err("Microsoft's Visual C++ runtime didn't install. Restart the computer and try again.".to_string())
    }
}

/// `ensure` from an async command, on a thread of its own.
pub async fn ensure_async() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(ensure).await.map_err(|e| e.to_string())?
}

/// Downloads Microsoft's installer to the temporary folder. Microsoft's link
/// goes on to its download servers, and the file is kept only when it came
/// from one of them.
async fn download() -> Result<PathBuf, String> {
    let response = reqwest::Client::new()
        .get(INSTALLER_URL)
        .header("User-Agent", "Omoio")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| OFFLINE.to_string())?;
    if !from_microsoft(response.url()) {
        return Err(OFFLINE.to_string());
    }
    let bytes = response.bytes().await.map_err(|_| OFFLINE.to_string())?;
    let path = std::env::temp_dir().join("omoio-vc_redist.x64.exe");
    std::fs::write(&path, &bytes).map_err(|_| "Couldn't keep Microsoft's Visual C++ runtime installer.".to_string())?;
    Ok(path)
}

fn from_microsoft(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host == "microsoft.com" || host.ends_with(".microsoft.com"))
}

/// The installed runtime's major and minor version, from what its installer
/// wrote. `None` when it wrote nothing, or says it is not installed.
#[cfg(windows)]
fn installed_version() -> Option<(u32, u32)> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD};

    let read = |value: PCWSTR| -> Option<u32> {
        let mut data = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // Fills in the u32 handed over, of the size given.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!(r"SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64"),
                value,
                RRF_RT_REG_DWORD,
                None,
                Some((&mut data as *mut u32).cast()),
                Some(&mut size),
            )
        };
        (status == ERROR_SUCCESS).then_some(data)
    };
    if read(w!("Installed"))? != 1 {
        return None;
    }
    Some((read(w!("Major"))?, read(w!("Minor"))?))
}

#[cfg(not(windows))]
fn installed_version() -> Option<(u32, u32)> {
    None
}

#[cfg(windows)]
fn dlls_in_system32() -> bool {
    let Some(windows) = std::env::var_os("SystemRoot") else {
        return false;
    };
    let system32 = Path::new(&windows).join("System32");
    ["msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll"]
        .iter()
        .all(|dll| system32.join(dll).is_file())
}

#[cfg(not(windows))]
fn dlls_in_system32() -> bool {
    true
}

/// Runs the installer as administrator, with its own progress window and no
/// restart, and waits for it. Windows asks the user first.
#[cfg(windows)]
fn run_elevated(installer: &Path) -> Result<(), String> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    /// What the installer ends with when all went well, when a newer runtime
    /// was there already, and when it wants a restart it was told not to do.
    const DONE: [u32; 3] = [0, 1638, 3010];
    /// Long enough for a slow machine, short enough not to wait for ever on
    /// an installer that hangs.
    const TEN_MINUTES: u32 = 10 * 60 * 1000;

    let file = HSTRING::from(installer);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: w!("/install /passive /norestart"),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // Fills in the struct handed over. Fails when the user says no.
    unsafe { ShellExecuteExW(&mut info) }.map_err(|_| DECLINED.to_string())?;
    if info.hProcess.is_invalid() {
        return Err(DECLINED.to_string());
    }
    let mut code = u32::MAX;
    // The process handle asked for above, waited on, read and closed.
    let finished = unsafe {
        let finished = WaitForSingleObject(info.hProcess, TEN_MINUTES) == WAIT_OBJECT_0
            && GetExitCodeProcess(info.hProcess, &mut code).is_ok();
        let _ = CloseHandle(info.hProcess);
        finished
    };
    if !finished {
        return Err("Microsoft's Visual C++ runtime is taking too long to install. Try again once it has finished.".to_string());
    }
    if DONE.contains(&code) {
        Ok(())
    } else {
        Err(format!("Microsoft's Visual C++ runtime didn't install (code {code}). Try again, or install it from {INSTALLER_URL}"))
    }
}

#[cfg(not(windows))]
fn run_elevated(_installer: &Path) -> Result<(), String> {
    Err("Only supported on Windows.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runtime_from_before_visual_studio_17_10_is_too_old() {
        assert!(!new_enough((14, 38)));
        assert!(!new_enough((14, 0)));
        assert!(new_enough((14, 40)));
        assert!(new_enough((14, 44)));
        assert!(new_enough((15, 0)));
    }

    #[test]
    fn the_installer_is_kept_only_from_microsoft() {
        let url = |text: &str| reqwest::Url::parse(text).unwrap();
        assert!(from_microsoft(&url("https://download.visualstudio.microsoft.com/download/pr/x/vc_redist.x64.exe")));
        assert!(!from_microsoft(&url("http://download.visualstudio.microsoft.com/vc_redist.x64.exe")));
        assert!(!from_microsoft(&url("https://microsoft.com.example.org/vc_redist.x64.exe")));
        assert!(!from_microsoft(&url("https://notmicrosoft.com/vc_redist.x64.exe")));
    }
}
