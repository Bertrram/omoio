//! Getting Dolphin: the release Omoio was checked against (release.rs),
//! downloaded from Dolphin's own server, checked, and unpacked into Omoio's
//! folder, where it keeps everything of its own beside itself.

use super::release;
use crate::core::types::Progress;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

/// Dolphin.exe has no way to say its version without opening its window, so
/// the release Omoio installed is kept here instead, as it is for Cemu.
const VERSION_FILE: &str = "omoio-version.txt";

pub fn install_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let local_data = app.path().local_data_dir().map_err(|e| e.to_string())?;
    Ok(local_data.join("Omoio").join("dolphin"))
}

pub fn exe_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(install_dir(app)?.join("Dolphin.exe"))
}

pub fn tool_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(install_dir(app)?.join("DolphinTool.exe"))
}

/// Where Dolphin keeps its settings, saves and logs. A file named
/// `portable.txt` beside Dolphin.exe has it keep them in a `User` folder
/// beside itself rather than in %APPDATA%, where the user may already have a
/// Dolphin of their own, and then it writes nothing to the registry either
/// (`SetUserDirectory` in Source/Core/UICommon/UICommon.cpp and
/// `PORTABLE_USER_DIR` in Common/CommonPaths.h, Dolphin 2609a, read
/// 8 October 2026).
pub fn user_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(install_dir(app)?.join("User"))
}

pub fn detect_version(app: &AppHandle) -> Option<String> {
    let dir = install_dir(app).ok()?;
    if !dir.join("Dolphin.exe").is_file() {
        return None;
    }
    let version = std::fs::read_to_string(dir.join(VERSION_FILE)).ok()?;
    Some(version.trim().to_string()).filter(|version| !version.is_empty())
}

fn emit(app: &AppHandle, stage: &str, bytes: u64, total: u64) {
    let _ = app.emit(
        "dolphin-install-progress",
        Progress {
            stage: stage.to_string(),
            bytes,
            total,
        },
    );
}

/// Whether Omoio's own Dolphin is running, in a game or on its own. Its
/// files are held open while it runs, and an update that met one part way
/// through would leave it half old, half new.
pub fn running(app: &AppHandle) -> bool {
    let Ok(exe) = exe_path(app) else {
        return false;
    };
    !copies(&exe).1.is_empty()
}

/// The processes running Omoio's Dolphin, by the file they were started
/// from, the same way backends/rpcs3 finds RPCS3's.
pub fn copies(exe: &Path) -> (sysinfo::System, Vec<sysinfo::Pid>) {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let ours = system
        .processes()
        .iter()
        .filter(|(_, process)| process.exe().is_some_and(|own| own.as_os_str().eq_ignore_ascii_case(exe.as_os_str())))
        .map(|(&pid, _)| pid)
        .collect();
    (system, ours)
}

/// Downloads the release Omoio was checked against, checks it is that file,
/// and unpacks it into Omoio's folder. Returns the version installed.
pub async fn install(app: AppHandle, cancel: Arc<AtomicBool>) -> Result<String, String> {
    emit(&app, "checking", 0, 0);
    let dir = install_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let archive = dir.join(release::FILE);

    let download = reqwest::Client::new()
        .get(release::DOWNLOAD)
        .header("User-Agent", "Omoio")
        .send()
        .await
        .map_err(|_| "Couldn't download Dolphin. Check your internet connection and try again.".to_string())?;
    if !download.status().is_success() {
        return Err("Dolphin's download server isn't handing it out right now. Try again in a while.".to_string());
    }
    let total = download.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&archive).await.map_err(|e| e.to_string())?;
    let mut stream = download.bytes_stream();
    let mut done: u64 = 0;
    let mut hasher = Sha256::new();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&archive);
            return Err("cancelled".to_string());
        }
        let chunk = chunk.map_err(|_| "The download stopped part way. Try again.".to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        hasher.update(&chunk);
        done += chunk.len() as u64;
        emit(&app, "downloading", done, total);
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    emit(&app, "verifying", 0, 1);
    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != release::SHA256 {
        let _ = std::fs::remove_file(&archive);
        return Err("The Dolphin download wasn't the file Omoio expects. Try again in a while.".to_string());
    }

    if running(&app) {
        let _ = std::fs::remove_file(&archive);
        return Err("Close Dolphin first. It can't be replaced while it runs.".to_string());
    }
    emit(&app, "extracting", 0, 1);
    let (from, into) = (archive.clone(), dir.clone());
    let unpacked = tauri::async_runtime::spawn_blocking(move || unpack(&from, &into))
        .await
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&archive);
    unpacked?;

    std::fs::write(dir.join("portable.txt"), "").map_err(|e| e.to_string())?;
    std::fs::write(dir.join(VERSION_FILE), release::VERSION).map_err(|e| e.to_string())?;

    emit(&app, "done", 1, 1);
    detect_version(&app).ok_or_else(|| "Dolphin unpacked, but Dolphin.exe isn't where it should be.".to_string())
}

/// The archive holds one folder, `Dolphin-x64`, around everything. It is
/// dropped, so Dolphin.exe sits in Omoio's folder whatever the release. The
/// archive carries nothing of the `User` folder, so settings and saves stay
/// as they are through an update.
fn unpack(archive: &Path, dest: &Path) -> Result<(), String> {
    let top = dest.join(release::TOP_FOLDER);
    sevenz_rust2::decompress_file_with_extract_fn(archive, dest, |entry, reader, path| {
        // The library has already refused any name that climbs out of `dest`.
        match path.strip_prefix(&top) {
            Ok(inner) if !inner.as_os_str().is_empty() => {
                sevenz_rust2::default_entry_extract_fn(entry, reader, &dest.join(inner))
            }
            _ => Ok(true),
        }
    })
    .map_err(|_| "The Dolphin download is damaged. Try again.".to_string())
}

/// Dolphin is a program with windows; without this every start of it from
/// Omoio would flash a console window too. DolphinTool, which is a console
/// program, runs out of sight the same way.
pub fn command(exe: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Ends every copy of Omoio's Dolphin still running before a game starts:
/// the game before, still on its way out, or one whose window Omoio lost,
/// with its main window hidden and nothing to close it by. Each is asked to
/// close first, as Omoio's Stop asks, so it writes out what it holds
/// (`backends::close`), and ended if it doesn't.
pub fn end_running(exe: &Path) {
    let (system, ours) = copies(exe);
    for pid in &ours {
        if crate::backends::close(&super::WII, pid.as_u32()) {
            continue;
        }
        if let Some(process) = system.process(*pid) {
            process.kill();
        }
    }
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ours.is_empty() && !copies(exe).1.is_empty() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the release archive on disk, so it runs only by hand:
    /// `OMOIO_DOLPHIN_7Z=<dolphin-2609a-x64.7z> cargo test unpacks_the_release -- --ignored`
    #[test]
    #[ignore]
    fn unpacks_the_release() {
        let archive = PathBuf::from(std::env::var("OMOIO_DOLPHIN_7Z").unwrap());
        let dest = std::env::temp_dir().join(format!("omoio-dolphin-unpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        let started = std::time::Instant::now();
        unpack(&archive, &dest).unwrap();
        println!("unpacked in {:?}", started.elapsed());
        assert!(dest.join("Dolphin.exe").is_file());
        assert!(dest.join("DolphinTool.exe").is_file());
        assert!(dest.join("Sys").join("GameSettings").is_dir());
        assert!(!dest.join(release::TOP_FOLDER).exists(), "the top folder is dropped");
        let _ = std::fs::remove_dir_all(&dest);
    }
}
