pub mod account;
pub mod compat;
pub mod controllers;
pub mod firmware;
pub mod fixes;
pub mod game_config;
pub mod graphics;
pub mod launch;
#[cfg(windows)]
pub mod overlay;
pub mod packages;
pub mod patches;
#[cfg(windows)]
pub mod portal;
pub mod saves;
pub mod updates;

use crate::core::types::Progress;
use futures_util::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

const RELEASES_API: &str = "https://api.github.com/repos/RPCS3/rpcs3-binaries-win/releases/latest";
const USER_AGENT: &str = "Omoio";

#[derive(Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

// RPCS3 ships as a portable folder, not an installer - extracting the
// archive here is the whole install.
pub fn install_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let local_data = app.path().local_data_dir().map_err(|e| e.to_string())?;
    Ok(local_data.join("Omoio").join("rpcs3"))
}

fn exe_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(install_dir(app)?.join("rpcs3.exe"))
}

/// The version `rpcs3.exe --version` last gave, with the size and time of the
/// file that gave it. Asking means starting RPCS3, which holds its files open
/// while it runs, and an update that met one of those files part way through
/// stopped half done (2 October 2026). So RPCS3 is asked again only when its
/// program has changed, and never while an update is replacing it.
static KNOWN: Mutex<Option<(u64, SystemTime, String)>> = Mutex::new(None);

/// Set while `install` replaces RPCS3's files.
static INSTALLING: AtomicBool = AtomicBool::new(false);

/// Clears `INSTALLING` however `install` ends, an error included.
struct Installing;

impl Drop for Installing {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::Relaxed);
    }
}

pub fn detect_version(app: &AppHandle) -> Option<String> {
    let exe = exe_path(app).ok()?;
    let meta = std::fs::metadata(&exe).ok()?;
    let stamp = (meta.len(), meta.modified().ok()?);
    // Held while RPCS3 answers, so screens asking at once start it once.
    let mut known = KNOWN.lock().unwrap();
    let installing = INSTALLING.load(Ordering::Relaxed);
    if let Some((len, time, version)) = known.as_ref() {
        if installing || (*len, *time) == stamp {
            return Some(version.clone());
        }
    }
    if installing {
        return None;
    }
    let version = if running(&exe) {
        let log = std::fs::read_to_string(install_dir(app).ok()?.join("log").join("RPCS3.log")).ok()?;
        log_version(&log)?
    } else {
        read_version(&exe)?
    };
    *known = Some((stamp.0, stamp.1, version.clone()));
    Some(version)
}

/// The processes running Omoio's RPCS3. It allows one copy of itself at a
/// time: a second, even one asked only for `--version`, puts up a "Fatal
/// Error" window over the game and waits there until someone presses OK, and
/// whatever started it waits with it.
fn copies(exe: &Path) -> (sysinfo::System, Vec<sysinfo::Pid>) {
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

fn running(exe: &Path) -> bool {
    !copies(exe).1.is_empty()
}

/// For anything that starts RPCS3 to do one job and waits for it, such as
/// installing firmware or a package.
pub fn refuse_while_running(app: &AppHandle) -> Result<(), String> {
    if running(&exe_path(app)?) {
        return Err("Close the game first. RPCS3 can only do one thing at a time.".to_string());
    }
    Ok(())
}

/// Ends every copy of RPCS3 still running before a game starts: the game
/// before, still on its way out, or one whose window Omoio lost, hidden with
/// nothing to close it by. The new game would otherwise meet RPCS3's "Fatal
/// Error" window. Starting a game ends the one before it anyway.
fn end_running(exe: &Path) {
    let (system, ours) = copies(exe);
    for pid in &ours {
        if let Some(process) = system.process(*pid) {
            process.kill();
        }
    }
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ours.is_empty() && running(exe) && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// The version a running RPCS3 gives on its log's first line, "RPCS3
/// v0.0.43-20240-5f8dd1de Alpha | master", in the form `--version` gives it.
/// The log is rewritten at every start, so it is the running copy's.
fn log_version(log: &str) -> Option<String> {
    let first = log.trim_start_matches('\u{feff}').lines().next()?;
    let mut words = first.split_whitespace();
    (words.next()? == "RPCS3").then_some(())?;
    words.next()?.strip_prefix('v').map(str::to_string)
}

pub fn open_in_explorer(folder: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(folder)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        Err("Only supported on Windows.".to_string())
    }
}

// RPCS3 is a console-less GUI binary; without this flag every call to it
// flashes a console window over whatever the user is looking at.
fn command(exe: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

// Asking the binary itself rather than trusting whatever we last installed:
// the installed build's actual behaviour is what counts.
fn read_version(exe: &Path) -> Option<String> {
    let output = command(exe).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // "RPCS3 0.0.42-19884-3ef20ebb Alpha" -> "0.0.42-19884-3ef20ebb"
    text.split_whitespace().nth(1).map(|s| s.to_string())
}

async fn latest_release(client: &reqwest::Client) -> Result<Release, String> {
    let response = client
        .get(RELEASES_API)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub returned {}", response.status()));
    }
    response.json::<Release>().await.map_err(|e| e.to_string())
}

/// The version a build's archive carries in its name, in the form
/// `--version` prints: `rpcs3-v0.0.42-19985-6ba56a52_win64_msvc.7z` is
/// `0.0.42-19985-6ba56a52`.
fn version_from_archive(name: &str) -> Option<&str> {
    name.strip_prefix("rpcs3-v")?.strip_suffix("_win64_msvc.7z")
}

pub async fn newest_version() -> Result<String, String> {
    let release = latest_release(&reqwest::Client::new()).await?;
    release
        .assets
        .iter()
        .find_map(|asset| version_from_archive(&asset.name))
        .map(str::to_string)
        .ok_or_else(|| "No Windows build found in the latest RPCS3 release".to_string())
}

pub async fn install(app: AppHandle, cancel: Arc<AtomicBool>) -> Result<String, String> {
    let client = reqwest::Client::new();

    emit(&app, "checking", 0, 0);
    let release = latest_release(&client).await?;

    let archive = release
        .assets
        .iter()
        .find(|a| a.name.ends_with("_win64_msvc.7z"))
        .ok_or("No Windows build found in the latest RPCS3 release")?;
    let checksum_name = format!("{}.sha256", archive.name);
    let checksum_asset = release
        .assets
        .iter()
        .find(|a| a.name == checksum_name)
        .ok_or("No checksum file found for the RPCS3 build")?;

    let dest_dir = install_dir(&app)?;
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    remove_old_archives(&dest_dir);
    let archive_path = dest_dir.join(&archive.name);

    download(&client, &archive.browser_download_url, &archive_path, &app, &cancel).await?;
    if cancel.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&archive_path);
        return Err("cancelled".to_string());
    }

    emit(&app, "verifying", 0, 1);
    let expected = client
        .get(&checksum_asset.browser_download_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?
        .trim()
        .to_lowercase();
    let actual = sha256_hex(&archive_path)?;
    if actual != expected {
        let _ = std::fs::remove_file(&archive_path);
        return Err("Downloaded file failed checksum verification".to_string());
    }

    emit(&app, "extracting", 0, 1);
    // Its files are held open while it runs, and an update that met one part
    // way through would leave RPCS3 half old, half new.
    refuse_while_running(&app)?;
    INSTALLING.store(true, Ordering::Relaxed);
    let _installing = Installing;
    let extract_dir = dest_dir.clone();
    let archive_path_clone = archive_path.clone();
    let unpacked = tauri::async_runtime::spawn_blocking(move || {
        sevenz_rust2::decompress_file(&archive_path_clone, &extract_dir)
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&archive_path);
    unpacked.map_err(|e| e.to_string())?;

    let exe = exe_path(&app)?;
    let version = read_version(&exe).ok_or("RPCS3 installed but did not report a version")?;
    emit(&app, "done", 1, 1);
    Ok(version)
}

async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    app: &AppHandle,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let response = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let total = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(dest).await.map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        emit(app, "downloading", downloaded, total);
    }

    Ok(())
}

/// Archives an earlier update left behind when it stopped part way. Each is
/// as big as RPCS3 itself.
fn remove_old_archives(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if version_from_archive(name).is_some() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn emit(app: &AppHandle, stage: &str, bytes: u64, total: u64) {
    let _ = app.emit(
        "rpcs3-install-progress",
        Progress {
            stage: stage.to_string(),
            bytes,
            total,
        },
    );
}

fn sha256_hex(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// RPCS3 as the PS3's emulator, for the parts of Omoio that work across
/// consoles. Each method hands on to the modules above.
pub struct Rpcs3;

impl super::EmulatorBackend for Rpcs3 {
    fn console(&self) -> crate::core::console::Console {
        crate::core::console::Console::Ps3
    }

    fn name(&self) -> &'static str {
        "RPCS3"
    }

    fn newest_version(&self) -> futures_util::future::BoxFuture<'static, Result<String, String>> {
        Box::pin(newest_version())
    }

    fn features(&self) -> crate::core::console::Features {
        crate::core::console::Features {
            updates: true,
            packs: true,
            settings: true,
            saves: true,
            compatibility: true,
            portal: true,
            // Omoio switches RPCS3's background input off before each game.
            quiet_behind: true,
        }
    }

    fn portal_figures(&self, pid: u32) -> Result<Vec<String>, String> {
        portal::figures(pid)
    }

    fn portal_load(&self, pid: u32, slot: usize, figure: &Path) -> Result<Vec<String>, String> {
        portal::load(pid, slot, figure)
    }

    fn portal_clear(&self, pid: u32, slot: usize) -> Result<Vec<String>, String> {
        portal::clear(pid, slot)
    }

    fn portal_characters(&self, pid: u32) -> Result<Vec<crate::core::figures::Character>, String> {
        portal::characters(pid)
    }

    fn portal_create(
        &self,
        pid: u32,
        slot: usize,
        character: &crate::core::figures::Character,
        file: &Path,
    ) -> Result<Vec<String>, String> {
        portal::create(pid, slot, character, file)
    }

    fn is_game_window(&self, title: &str) -> bool {
        title.starts_with(portal::GAME_TITLE)
    }

    fn tidy_window(&self, pid: u32, game: isize) {
        portal::tidy(pid, game);
    }

    /// Omoio's menus keep the game deaf by taking the keyboard from its
    /// window, since its background input is off. When it may hear again,
    /// the game gets the keyboard back if one of RPCS3's own windows has it,
    /// as the portal's file windows take it, which Omoio's own hand-over
    /// can't take back from them.
    fn hush(&self, pid: u32, hushed: bool) -> Result<(), String> {
        if !hushed {
            portal::give_back_to_game(pid);
        }
        Ok(())
    }

    /// A PS3 game is a folder of plain files, which is all the picture
    /// reader needs.
    fn readable_copy(
        &self,
        _app: &AppHandle,
        game: &crate::core::library::Game,
        _title_of: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<PathBuf> {
        game.path.is_dir().then(|| game.path.clone())
    }

    fn game_settings(
        &self,
        app: &AppHandle,
        game: &crate::core::library::Game,
    ) -> Result<crate::core::game_settings::GameSettings, String> {
        Ok(crate::core::game_settings::GameSettings {
            emulator: "RPCS3".to_string(),
            options: game_config::catalogue(app),
            chosen: game_config::read(app, &game.title_id),
            reasons: fixes::setting_reasons(&game.title_id)
                .into_iter()
                .map(|(key, reason)| (key.to_string(), reason.to_string()))
                .collect(),
            common_groups: ["Video", "Core", "Audio"].map(String::from).to_vec(),
        })
    }

    fn set_game_settings(
        &self,
        app: &AppHandle,
        game: &crate::core::library::Game,
        chosen: &crate::core::game_settings::Chosen,
    ) -> Result<(), String> {
        game_config::write(app, &game.title_id, chosen)
    }

    fn recognises(&self, path: &Path) -> bool {
        crate::import::recognises(path)
    }

    fn identify(&self, path: &Path) -> Result<crate::core::library::Game, String> {
        crate::import::identify(path).map_err(|e| e.to_string())
    }

    fn identify_packed(
        &self,
        names: &[String],
        read: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Option<crate::core::import_warning::Imported> {
        crate::import::identify_packed(names, read)
    }

    fn icon(&self, _app: &AppHandle, game: &crate::core::library::Game) -> Option<Vec<u8>> {
        std::fs::read(crate::import::icon_path(&game.path)?).ok()
    }

    fn button_names(&self) -> &'static [(&'static str, &'static str)] {
        &controllers::BUTTON_NAMES
    }

    fn write_layout(
        &self,
        app: &AppHandle,
        title_id: &str,
        players: &[crate::core::pad_layout::Player],
    ) -> Result<(), String> {
        controllers::write(app, title_id, players)
    }

    fn unfound_pad(&self, app: &AppHandle, title_id: &str) -> Option<String> {
        controllers::unfound(app, title_id)
    }

    fn forget_layout(&self, app: &AppHandle, title_id: &str) -> Result<(), String> {
        controllers::forget(app, title_id)
    }

    fn existing_layouts(&self, app: &AppHandle) -> Vec<(String, Vec<crate::core::pad_layout::Player>)> {
        controllers::existing(app)
    }

    fn tune_picture(
        &self,
        app: &AppHandle,
        display_height: u32,
        graphics_memory: u64,
    ) -> Result<Option<u32>, String> {
        graphics::apply(app, display_height, graphics_memory)
    }

    fn launch(&self, app: &AppHandle, game: &crate::core::library::Game) -> Result<u32, String> {
        launch::launch(app, game)
    }

    fn detect_version(&self, app: &AppHandle) -> Option<String> {
        detect_version(app)
    }

    fn log_file(&self, app: &AppHandle) -> Option<PathBuf> {
        install_dir(app).ok().map(|dir| dir.join("log").join("RPCS3.log"))
    }

    fn catalogue(&self, app: &AppHandle) -> Option<Vec<crate::core::catalogue::Entry>> {
        compat::entries(app)
    }

    fn refresh_catalogue<'a>(
        &'a self,
        app: &'a AppHandle,
        cancel: &'a AtomicBool,
    ) -> futures_util::future::BoxFuture<'a, Result<usize, String>> {
        Box::pin(compat::refresh(app, cancel))
    }

    fn catalogue_source(&self) -> (&'static str, &'static str) {
        ("PS3 results from RPCS3", "https://rpcs3.net/compatibility")
    }

    fn apply_fixes(
        &self,
        app: &AppHandle,
        game: &crate::core::library::Game,
        applied: &[String],
    ) -> Vec<&'static str> {
        let version = game.running_version().unwrap_or_default();
        fixes::apply(app, &game.title_id, version, applied)
    }

    /// RPCS3's community patches, matched against the version that runs, so
    /// a game with an official update sees the patches written for it.
    fn community_packs(
        &self,
        app: &AppHandle,
        title_id: &str,
        game: Option<&crate::core::library::Game>,
    ) -> crate::core::community::Packs {
        let version = game.and_then(|g| g.running_version()).unwrap_or_default();
        crate::core::community::Packs {
            have_list: patches::have_catalogue(app),
            source: "the RPCS3 community".to_string(),
            waiting: None,
            packs: patches::for_title(app, title_id, version).into_iter().map(patches::as_pack).collect(),
        }
    }

    fn set_community_pack(
        &self,
        app: &AppHandle,
        game: &crate::core::library::Game,
        change: &crate::core::community::PackChange,
    ) -> Result<(), String> {
        let version = game.running_version().unwrap_or_default();
        let patch = patches::for_title(app, &game.title_id, version)
            .into_iter()
            .find(|patch| patches::pack_id(patch) == change.id)
            .ok_or("That patch isn't in the list any more. Download the latest patches.")?;
        patches::set_enabled(app, &patch, &game.title_id, version, change.on)
    }

    fn refresh_community<'a>(
        &'a self,
        app: &'a AppHandle,
        _cancel: &'a std::sync::atomic::AtomicBool,
    ) -> futures_util::future::BoxFuture<'a, Result<usize, String>> {
        Box::pin(patches::refresh(app))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_builds_version_is_read_from_its_archive_name() {
        assert_eq!(
            version_from_archive("rpcs3-v0.0.42-19985-6ba56a52_win64_msvc.7z"),
            Some("0.0.42-19985-6ba56a52")
        );
        assert_eq!(version_from_archive("rpcs3-v0.0.42-19985-6ba56a52_win64_msvc.7z.sha256"), None);
    }

    #[test]
    fn a_running_rpcs3s_version_is_read_from_its_log() {
        let log = "\u{feff}RPCS3 v0.0.43-20240-5f8dd1de Alpha | master\nArchitecture: x64\n";
        assert_eq!(log_version(log), Some("0.0.43-20240-5f8dd1de".to_string()));
        assert_eq!(log_version("RPCS3 v0.0.43-20240-5f8dd1de Alpha"), Some("0.0.43-20240-5f8dd1de".to_string()));
        assert_eq!(log_version("·! 0:00:00.00000 SYS: something"), None);
        assert_eq!(log_version(""), None);
    }

    #[test]
    fn an_old_updates_archive_is_cleared_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("omoio-rpcs3-archives-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["rpcs3-v0.0.43-20160-0850e0ff_win64_msvc.7z", "rpcs3.exe", "notes.7z"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        remove_old_archives(&dir);
        assert!(!dir.join("rpcs3-v0.0.43-20160-0850e0ff_win64_msvc.7z").exists());
        assert!(dir.join("rpcs3.exe").exists());
        assert!(dir.join("notes.7z").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
