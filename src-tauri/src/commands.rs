use crate::archive;
use crate::backends::rpcs3;
use crate::core::console::Console;
use crate::core::import_warning::{self, Imported, List, Warning};
use crate::core::library::Library;
use crate::core::playlog::{self, Session as PlaySession};
use crate::core::settings::Settings;
use crate::core::types::{GameEntry, HardwareInfo, Progress};
use crate::hardware;
use crate::import;
use crate::session::{Playing, Session};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub fn get_hardware_info() -> HardwareInfo {
    hardware::detect()
}

#[derive(Default)]
pub struct InstallState {
    cancel: Arc<AtomicBool>,
    cancel_import: Arc<AtomicBool>,
    cancel_update: Arc<AtomicBool>,
    cancel_compat: Arc<AtomicBool>,
    cancel_cemu: Arc<AtomicBool>,
    cancel_community: Arc<AtomicBool>,
    /// Emulators whose files are being replaced right now.
    installing: std::sync::Mutex<Vec<Console>>,
}

/// Marks an emulator as being installed until it is dropped, so a game cannot
/// start on files that are half replaced. Dropping it also covers an install
/// that fails or is stopped part way.
struct Installing<'a> {
    state: &'a InstallState,
    console: Console,
}

impl Drop for Installing<'_> {
    fn drop(&mut self) {
        self.state.installing.lock().unwrap().retain(|c| *c != self.console);
    }
}

impl InstallState {
    fn begin_install(&self, console: Console) -> Result<Installing<'_>, String> {
        let mut busy = self.installing.lock().unwrap();
        if busy.contains(&console) {
            return Err("It's already being installed.".to_string());
        }
        busy.push(console);
        Ok(Installing { state: self, console })
    }

    fn is_installing(&self, console: Console) -> bool {
        self.installing.lock().unwrap().contains(&console)
    }
}

/// An emulator's files cannot be replaced under a game it is running.
fn refuse_while_playing(app: &AppHandle, console: Console) -> Result<(), String> {
    if app.state::<Session>().playing().is_some_and(|p| p.console == console) {
        return Err("Close the game first. The emulator can't be replaced while it runs one.".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn get_rpcs3_version(app: AppHandle) -> Option<String> {
    rpcs3::detect_version(&app)
}

#[tauri::command]
pub async fn install_rpcs3(app: AppHandle, state: State<'_, InstallState>) -> Result<String, String> {
    refuse_while_playing(&app, Console::Ps3)?;
    let _installing = state.begin_install(Console::Ps3)?;
    state.cancel.store(false, Ordering::Relaxed);
    let cancel = state.cancel.clone();
    rpcs3::install(app, cancel).await
}

#[tauri::command]
pub fn cancel_rpcs3_install(state: State<'_, InstallState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn get_firmware_version(app: AppHandle) -> Option<String> {
    rpcs3::firmware::detect_version(&app)
}

fn omoio_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app.path().data_dir().map_err(|e| e.to_string())?;
    Ok(data.join("Omoio"))
}

fn library_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(omoio_data_dir(app)?.join("library.json"))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(omoio_data_dir(app)?.join("settings.json"))
}

#[tauri::command]
pub fn get_games_folder(app: AppHandle) -> Result<Option<String>, String> {
    Ok(Settings::load(&settings_path(&app)?)
        .games_folder
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn set_games_folder(app: AppHandle, path: String) -> Result<(), String> {
    let file = settings_path(&app)?;
    let mut settings = Settings::load(&file);
    settings.games_folder = Some(PathBuf::from(path));
    settings.save(&file)
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(Settings::load(&settings_path(&app)?))
}

#[tauri::command]
pub fn set_start_fullscreen(app: AppHandle, on: bool) -> Result<(), String> {
    let file = settings_path(&app)?;
    let mut settings = Settings::load(&file);
    settings.start_fullscreen = on;
    settings.save(&file)
}

#[tauri::command]
pub fn set_start_in_big_picture(app: AppHandle, on: bool) -> Result<(), String> {
    let file = settings_path(&app)?;
    let mut settings = Settings::load(&file);
    settings.start_in_big_picture = on;
    settings.save(&file)
}

/// Whether Big Picture is up, and whether a game is waiting behind it.
#[tauri::command]
pub fn big_picture(app: AppHandle) -> crate::big_picture::State {
    crate::big_picture::state(&app)
}

#[tauri::command]
pub fn set_big_picture(app: AppHandle, on: bool) -> Result<(), String> {
    crate::big_picture::set(&app, on)
}

/// Puts the game waiting behind Big Picture back on the screen.
#[tauri::command]
pub fn resume_game(app: AppHandle) {
    crate::big_picture::resume(&app);
}

/// The pads plugged in, so Big Picture names buttons as printed on them.
#[tauri::command]
pub fn pads_connected() -> Vec<crate::core::pad_layout::Pad> {
    crate::pads::connected()
}

#[tauri::command]
pub fn set_keep_sessions(app: AppHandle, keep: usize) -> Result<(), String> {
    let file = settings_path(&app)?;
    let mut settings = Settings::load(&file);
    settings.keep_sessions = keep.clamp(1, 200);
    settings.save(&file)
}

#[tauri::command]
pub fn cancel_import(state: State<'_, InstallState>) {
    state.cancel_import.store(true, Ordering::Relaxed);
}

/// Unpacks a compressed dump into the games folder, then imports what came out.
#[tauri::command]
pub async fn import_archive(
    app: AppHandle,
    path: String,
    state: State<'_, InstallState>,
) -> Result<GameEntry, String> {
    state.cancel_import.store(false, Ordering::Relaxed);
    let cancel = state.cancel_import.clone();

    let games_folder = Settings::load(&settings_path(&app)?)
        .games_folder
        .ok_or("Choose a games folder first.")?;

    tauri::async_runtime::spawn_blocking(move || {
        let source = PathBuf::from(&path);
        let kind = archive::detect_kind(&source)
            .ok_or("That file isn't a .7z or .zip archive.")?;

        // Answered from the names inside, before anything is unpacked: a dump
        // that needs a key is refused in seconds rather than after the wait.
        let names = archive::names(&source, kind)?;
        if let Some(why) = crate::backends::refuses(&app, &names) {
            return Err(why);
        }

        // Unpacking the same archive again would leave a second copy of the
        // whole game behind the replaced library entry.
        if let Some(game) = crate::backends::identify_packed(&names, &|name| archive::read_small(&source, kind, name)) {
            let library = Library::load(&library_path(&app)?);
            if library.games().iter().any(|had| had.title_id == game.title_id && had.path.exists()) {
                return Err("That game is already in your library.".to_string());
            }
        }

        let needed = archive::unpacked_size(&source, kind)?;
        let free = free_space(&games_folder);
        if free.is_some_and(|free| free < needed) {
            return Err(format!(
                "This game needs {} GB unpacked and the drive has less than that free.",
                needed / 1024u64.pow(3)
            ));
        }

        let name = source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .ok_or("That file has no name.")?;
        let dest = archive::free_folder(&games_folder, &name);

        let mut last_sent = 0u64;
        let outcome = archive::extract(&source, kind, &dest, &cancel, &mut |done| {
            // One event per percent rather than per chunk: 3900 files would
            // otherwise flood the interface with redundant redraws.
            let step = (needed / 100).max(1);
            if done - last_sent >= step || done >= needed {
                last_sent = done;
                emit_import_progress(&app, "unpacking", done, needed);
            }
        })?;
        if outcome.is_err() {
            return Err("cancelled".to_string());
        }

        emit_import_progress(&app, "identifying", needed, needed);
        let game = match crate::backends::identify(&dest) {
            Ok(game) => game,
            Err(e) => {
                // Nothing importable came out, so don't leave it behind.
                let _ = std::fs::remove_dir_all(&dest);
                return Err(e.to_string());
            }
        };

        let library_file = library_path(&app)?;
        let mut library = Library::load(&library_file);
        library.upsert(game.clone());
        library.save(&library_file)?;
        Ok(entry(&app, game))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn emit_import_progress(app: &AppHandle, stage: &str, bytes: u64, total: u64) {
    let _ = app.emit(
        "import-progress",
        Progress {
            stage: stage.to_string(),
            bytes,
            total,
        },
    );
}

#[cfg(windows)]
fn free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    // The folder may not exist yet, so ask about the nearest parent that does.
    let mut probe = path;
    while !probe.exists() {
        probe = probe.parent()?;
    }
    let wide: Vec<u16> = probe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0u64;
    unsafe {
        GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut available), None, None).ok()?;
    }
    Some(available)
}

#[cfg(not(windows))]
fn free_space(_path: &Path) -> Option<u64> {
    None
}

/// The game's own picture, kept under our own folder. A game imported before
/// its picture could be read gets it here, on a later load of the library.
fn cached_cover(app: &AppHandle, game: &crate::core::library::Game) -> Option<String> {
    let covers = omoio_data_dir(app).ok()?.join("covers");
    let backend = crate::backends::for_console(game.console)?;
    let kept = crate::covers::keep_own(&covers, &game.title_id, || backend.icon(app, game))?;
    Some(kept.to_string_lossy().into_owned())
}

/// The picture for a game: RAWG's cover when the user has switched that on
/// and one was found, otherwise the dump's own picture.
fn cover_for(
    app: &AppHandle,
    game: &crate::core::library::Game,
) -> (Option<String>, Option<&'static str>) {
    let rawg_on = settings_path(app)
        .map(|file| Settings::load(&file).covers)
        .unwrap_or(false);
    if rawg_on {
        if let Ok(dir) = omoio_data_dir(app) {
            let path = crate::covers::cached_path(&dir.join("covers"), &game.title_id);
            if path.is_file() {
                return (Some(path.to_string_lossy().into_owned()), Some("rawg"));
            }
        }
    }
    let own = cached_cover(app, game);
    let source = own.as_ref().map(|_| "dump");
    (own, source)
}

fn entry(app: &AppHandle, game: crate::core::library::Game) -> GameEntry {
    let (cover, cover_source) = cover_for(app, &game);
    let features = crate::backends::for_console(game.console)
        .map(|backend| backend.features())
        .unwrap_or_default();
    GameEntry {
        set_up: game.is_set_up(),
        // A folder for most games, a single file for a Wii U disc image.
        available: game.is_set_up() && game.path.exists(),
        cover,
        cover_source,
        features,
        portal_menu: crate::core::figures::offers_portal_menu(features, game.console, &game.title),
        game,
    }
}

/// Notes down a game the user says they own, without files behind it yet.
///
/// Nothing is downloaded and nothing is looked for. It is a placeholder that
/// says what to do next, which is to import the game's own files.
#[tauri::command]
pub fn add_to_library(
    app: AppHandle,
    console: crate::core::console::Console,
    title_id: String,
    title: String,
) -> Result<(), String> {
    let file = library_path(&app)?;
    let mut library = Library::load(&file);
    if library.games().iter().any(|g| g.title_id == title_id) {
        return Err("That game is already in your library.".into());
    }
    library.upsert(crate::core::library::Game::not_set_up(console, title_id, title));
    library.save(&file)
}

#[derive(serde::Serialize)]
pub struct CatalogueConsole {
    pub console: crate::core::console::Console,
    pub name: &'static str,
}

#[derive(serde::Serialize)]
pub struct CatalogueSource {
    pub label: &'static str,
    pub url: &'static str,
}

#[derive(serde::Serialize)]
pub struct CatalogueView {
    /// False until at least one console's list has been downloaded.
    pub have_list: bool,
    /// Every console Omoio runs, to choose between.
    pub consoles: Vec<CatalogueConsole>,
    /// Consoles whose list has not been downloaded yet.
    pub missing: Vec<crate::core::console::Console>,
    pub total: usize,
    pub shown: Vec<crate::core::catalogue::Listing>,
    /// Who published the lists on screen, for the credit under them.
    pub sources: Vec<CatalogueSource>,
}

/// Every console's list, put together and narrowed by the filter.
#[tauri::command]
pub fn catalogue(
    app: AppHandle,
    filter: crate::core::catalogue::Filter,
) -> Result<CatalogueView, String> {
    use crate::core::catalogue::{fold, group, pick};

    let mut entries = Vec::new();
    let mut consoles = Vec::new();
    let mut missing = Vec::new();
    let mut sources = Vec::new();
    for backend in crate::backends::all() {
        consoles.push(CatalogueConsole {
            console: backend.console(),
            name: backend.console().short(),
        });
        match backend.catalogue(&app) {
            Some(found) => {
                entries.extend(found);
                let (label, url) = backend.catalogue_source();
                sources.push(CatalogueSource { label, url });
            }
            None => missing.push(backend.console()),
        }
    }

    let (total, mut shown) = pick(group(entries), &filter);

    // A game counts as owned through any of its releases, or by its name
    // where the list carries no ids.
    let library = Library::load(&library_path(&app)?);
    for listing in &mut shown {
        listing.features = crate::backends::for_console(listing.console)
            .map(|backend| backend.features())
            .unwrap_or_default();
        let name = fold(&listing.name);
        listing.owned = library.games().iter().any(|game| {
            game.console == listing.console
                && (listing.releases.iter().any(|r| r.title_id == game.title_id)
                    || fold(&game.title) == name)
        });
    }

    Ok(CatalogueView {
        have_list: !sources.is_empty(),
        consoles,
        missing,
        total,
        shown,
        sources,
    })
}

#[tauri::command]
pub fn list_games(app: AppHandle) -> Result<Vec<GameEntry>, String> {
    let library = Library::load(&library_path(&app)?);
    Ok(library.games().iter().cloned().map(|g| entry(&app, g)).collect())
}

/// Every console's compatibility list, from the copies Omoio keeps. A list
/// never downloaded is empty, so nothing here waits on the network.
fn compat_lists(app: &AppHandle) -> Vec<List> {
    crate::backends::all()
        .iter()
        .map(|backend| List::new(backend.console(), backend.name(), backend.catalogue(app).unwrap_or_default()))
        .collect()
}

#[derive(serde::Serialize)]
pub struct ImportCheck {
    /// False when the game couldn't be told before importing it, as with an
    /// archive whose game files sit deep inside, so the check waits until it
    /// has been unpacked.
    pub checked: bool,
    pub warning: Option<Warning>,
}

/// What to warn about before a game is imported: whether its emulator rates
/// it below playing well, and whether another console has a version that
/// plays well. Answered before the long part of an import, unpacking an
/// archive, starts.
#[tauri::command]
pub async fn import_check(app: AppHandle, path: String) -> Result<ImportCheck, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(&path);
        let game = match archive::detect_kind(&path) {
            Some(kind) => {
                let names = archive::names(&path, kind).unwrap_or_default();
                // Refused anyway, so there is nothing to warn about.
                if crate::backends::refuses(&app, &names).is_some() {
                    return ImportCheck { checked: true, warning: None };
                }
                match crate::backends::identify_packed(&names, &|name| archive::read_small(&path, kind, name)) {
                    Some(game) => game,
                    None => return ImportCheck { checked: false, warning: None },
                }
            }
            None => {
                let refused = crate::backends::refuses(&app, &crate::backends::names_in(&path)).is_some();
                match crate::backends::identify(&path) {
                    Ok(game) if !refused => Imported::of(&game),
                    // The import itself says what is wrong.
                    _ => return ImportCheck { checked: true, warning: None },
                }
            }
        };
        ImportCheck {
            checked: true,
            warning: import_warning::warning(&game, &compat_lists(&app)),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// The same warning for a game already in the library, for an archive that
/// could only be told once it was unpacked, and for several imported at once.
#[tauri::command]
pub async fn game_warning(app: AppHandle, title_id: String) -> Result<Option<Warning>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let library = Library::load(&library_path(&app)?);
        Ok(library
            .games()
            .iter()
            .find(|game| game.title_id == title_id)
            .and_then(|game| import_warning::warning(&Imported::of(game), &compat_lists(&app))))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn import_game(app: AppHandle, path: String) -> Result<GameEntry, String> {
    // Measuring a dump means walking every file in it, so this stays off the
    // UI thread.
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(why) = crate::backends::refuses(&app, &crate::backends::names_in(Path::new(&path))) {
            return Err(why);
        }
        let game = crate::backends::identify(Path::new(&path))?;
        let library_file = library_path(&app)?;
        let mut library = Library::load(&library_file);
        library.upsert(game.clone());
        library.save(&library_file)?;
        Ok(entry(&app, game))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Why the game would start with nobody answering player 1, asked before
/// Play so the person can plug a pad in, or start it anyway. `None` when all
/// is well.
#[tauri::command]
pub fn launch_warning(app: AppHandle, title_id: String) -> Option<String> {
    let game = Library::load(&library_path(&app).ok()?)
        .games()
        .iter()
        .find(|g| g.title_id == title_id)
        .cloned()?;
    let backend = crate::backends::for_console(game.console)?;
    crate::controllers::launch_warning(&app, backend, &game.title_id)
}

#[tauri::command]
pub fn launch_game(app: AppHandle, title_id: String) -> Result<(), String> {
    let library = Library::load(&library_path(&app)?);
    let game = library
        .games()
        .iter()
        .find(|g| g.title_id == title_id)
        .ok_or("That game isn't in your library any more.")?;

    if app.state::<InstallState>().is_installing(game.console) {
        let name = crate::backends::for_console(game.console).map_or("The emulator", |b| b.name());
        return Err(format!("{name} is being updated. Try again in a minute."));
    }

    // Cemu may be busy making a copy of a game to read figure pictures from.
    if game.console == crate::core::console::Console::WiiU && crate::figure_pictures::copying() {
        return Err("Cemu is busy getting figure pictures. Try again when that is done, or stop it.".to_string());
    }

    // One game at a time: starting another stops the one already running.
    app.state::<Session>().stop();
    let backend = crate::backends::for_console(game.console)
        .ok_or("Omoio can't start games for this console yet.")?;
    // The known fixes for this game go in before it starts, each only once,
    // so one the user switched off afterwards stays off.
    let settings_file = settings_path(&app)?;
    let mut settings = Settings::load(&settings_file);
    let already = settings
        .applied_fixes
        .get(&game.title_id)
        .cloned()
        .unwrap_or_default();
    let applied = backend.apply_fixes(&app, game, &already);
    if !applied.is_empty() {
        settings
            .applied_fixes
            .entry(game.title_id.clone())
            .or_default()
            .extend(applied.iter().map(|id| id.to_string()));
        let _ = settings.save(&settings_file);
    }
    // Ready before the emulator starts: every player has their pad and
    // buttons, a pad plugged in for the first time works, and the picture
    // fits this machine.
    backend.prepare(&app, game);
    crate::controllers::before_launch(&app, backend, &game.title_id);
    tune_picture(&app, backend);
    let pid = backend.launch(&app, game)?;
    let session = app.state::<Session>();
    session.begin(
        pid,
        Playing {
            title_id: game.title_id.clone(),
            title: game.title.clone(),
            console: game.console,
        },
    );
    session.set_fullscreen(Settings::load(&settings_path(&app)?).start_fullscreen);
    crate::session::watch(app.clone(), pid);
    crate::portal_menu::watch(app.clone(), pid);
    Ok(())
}

/// The settings Omoio offers per game, what this game is currently set to, and
/// the reason for any Omoio sets itself.
#[tauri::command]
pub fn game_settings(app: AppHandle, title_id: String) -> Result<crate::core::game_settings::GameSettings, String> {
    let (backend, game) = game_and_emulator(&app, &title_id)?;
    backend.game_settings(&app, &game)
}

/// Saves only what was chosen. Clearing everything puts the game back on its
/// emulator's own settings.
#[tauri::command]
pub fn set_game_settings(
    app: AppHandle,
    title_id: String,
    chosen: crate::core::game_settings::Chosen,
) -> Result<(), String> {
    let (backend, game) = game_and_emulator(&app, &title_id)?;
    backend.set_game_settings(&app, &game, &chosen)
}

/// A game in the library and the emulator that runs it.
fn game_and_emulator(
    app: &AppHandle,
    title_id: &str,
) -> Result<(&'static dyn crate::backends::EmulatorBackend, crate::core::library::Game), String> {
    let game = Library::load(&library_path(app)?)
        .games()
        .iter()
        .find(|game| game.title_id == title_id)
        .cloned()
        .ok_or("That game isn't in the library any more.")?;
    let backend = crate::backends::for_console(game.console).ok_or("No emulator runs this game.")?;
    Ok((backend, game))
}

#[tauri::command]
pub fn stop_game(app: AppHandle) {
    app.state::<Session>().stop();
}

/// Where Omoio keeps things, so the Settings screen can point at them and open
/// them. Every one of these is somewhere a person might need to go digging
/// when something has gone wrong.
#[derive(serde::Serialize)]
pub struct Places {
    data: String,
    library: String,
    settings: String,
    logs: String,
    covers: String,
    figures: String,
    rpcs3: String,
    games_folder: Option<String>,
}

#[tauri::command]
pub fn get_places(app: AppHandle) -> Result<Places, String> {
    let data = omoio_data_dir(&app)?;
    let text = |p: PathBuf| p.to_string_lossy().into_owned();
    Ok(Places {
        library: text(data.join("library.json")),
        settings: text(data.join("settings.json")),
        logs: text(data.join("logs")),
        covers: text(data.join("covers")),
        figures: text(crate::portal_menu::folder(&app)?),
        rpcs3: text(rpcs3::install_dir(&app)?),
        games_folder: Settings::load(&settings_path(&app)?)
            .games_folder
            .map(|p| p.to_string_lossy().into_owned()),
        data: text(data),
    })
}

/// Opens a folder in Explorer. Creates it first if it isn't there yet, so the
/// button never just does nothing.
#[tauri::command]
pub fn reveal_folder(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().map(Path::to_path_buf).ok_or("No such folder.")?
    };
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    rpcs3::open_in_explorer(&folder)
}

/// Empties the library list. The games themselves are the user's and are never
/// touched; this only forgets them.
#[tauri::command]
pub fn forget_all_games(app: AppHandle) -> Result<(), String> {
    Library::default().save(&library_path(&app)?)
}

#[tauri::command]
pub fn clear_session_logs(app: AppHandle) -> Result<(), String> {
    let logs = omoio_data_dir(&app)?.join("logs");
    if logs.is_dir() {
        std::fs::remove_dir_all(&logs).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn sessions_index(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(omoio_data_dir(app)?.join("logs").join("sessions.json"))
}

/// Every session we kept, newest first. Optionally just one game's.
#[tauri::command]
pub fn list_sessions(app: AppHandle, title_id: Option<String>) -> Result<Vec<PlaySession>, String> {
    let text = std::fs::read_to_string(sessions_index(&app)?).unwrap_or_default();
    let all: Vec<PlaySession> = serde_json::from_str(&text).unwrap_or_default();
    Ok(match title_id {
        Some(id) => all.into_iter().filter(|s| s.title_id == id).collect(),
        None => all,
    })
}

/// The whole log for one session, for when the errors alone are not enough.
#[tauri::command]
pub fn read_session_log(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|_| "That log isn't there any more.".to_string())
}

/// A question about this session, ready to paste wherever it helps.
#[tauri::command]
pub fn session_prompt(app: AppHandle, log_file: String) -> Result<String, String> {
    let text = std::fs::read_to_string(sessions_index(&app)?).unwrap_or_default();
    let all: Vec<PlaySession> = serde_json::from_str(&text).unwrap_or_default();
    let session = all
        .iter()
        .find(|s| s.log_file == log_file)
        .ok_or_else(|| "That session isn't in the list any more.".to_string())?;

    // What is set for this game now, rather than what was set when it ran.
    // Anyone advising needs to know what has already been tried.
    let applied: Vec<(String, String)> = rpcs3::game_config::read(&app, &session.title_id)
        .into_iter()
        .map(|(key, value)| (key.split('\n').collect::<Vec<_>>().join(" / "), value))
        .collect();

    Ok(playlog::troubleshooting_prompt(session, &applied))
}

#[tauri::command]
pub fn playing_game(app: AppHandle) -> Option<Playing> {
    app.state::<Session>().playing()
}

#[tauri::command]
pub fn set_game_fullscreen(app: AppHandle, fullscreen: bool) {
    let session = app.state::<Session>();
    session.set_fullscreen(fullscreen);
    if let Some(window) = app.get_webview_window("main") {
        crate::session::place(&window, &session);
    }
    // The button that asked took the keyboard from the game.
    session.focus_game();
}

#[tauri::command]
pub fn remove_game(app: AppHandle, title_id: String) -> Result<(), String> {
    let library_file = library_path(&app)?;
    let mut library = Library::load(&library_file);
    // Only the entry goes; the user's files are theirs and stay where they are.
    if library.remove(&title_id) {
        library.save(&library_file)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn install_firmware(app: AppHandle, path: String) -> Result<String, String> {
    // Unpacking firmware takes long enough to block the UI thread, so it runs
    // on the blocking pool.
    tauri::async_runtime::spawn_blocking(move || {
        rpcs3::firmware::install(&app, std::path::Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What we can say about a game's compatibility, already turned into words.
/// The interface never sees a raw status string it would have to interpret.
#[derive(serde::Serialize)]
pub struct CompatView {
    pub known: bool,
    pub label: String,
    pub tone: String,
    pub explanation: String,
    pub checked: String,
    /// True when we have never downloaded the list, or the copy is old.
    pub stale: bool,
    pub have_list: bool,
}

#[tauri::command]
pub fn game_compatibility(app: AppHandle, title_id: String) -> CompatView {
    let (entry, stale) = rpcs3::compat::look_up(&app, &title_id);
    let have_list = rpcs3::compat::have_list(&app);

    match entry.as_ref().and_then(|e| {
        rpcs3::compat::describe(&e.status).map(|d| (e, d))
    }) {
        Some((entry, (label, tone, explanation))) => CompatView {
            known: true,
            label: label.into(),
            tone: tone.into(),
            explanation: explanation.into(),
            checked: entry.date.clone(),
            stale,
            have_list,
        },
        None => CompatView {
            known: false,
            label: if have_list { "No result".into() } else { "Not checked".into() },
            tone: "mute".into(),
            explanation: if have_list {
                "Nobody has reported on this game yet.".into()
            } else {
                "Get the compatibility list to see how well this game runs.".into()
            },
            checked: String::new(),
            stale,
            have_list,
        },
    }
}

/// Getting RPCS3's list means an export plus a page-at-a-time pass for the
/// names, around 22 seconds, and the Cemu wiki's takes a few more, so it
/// reports progress and can be stopped. `console` narrows it to one list.
#[tauri::command]
pub async fn refresh_compatibility(
    app: AppHandle,
    state: State<'_, InstallState>,
    console: Option<crate::core::console::Console>,
) -> Result<usize, String> {
    state.cancel_compat.store(false, Ordering::Relaxed);
    let cancel = state.cancel_compat.clone();
    let mut count = 0;
    let mut failed = None;
    for backend in crate::backends::all() {
        if console.is_some_and(|wanted| wanted != backend.console()) {
            continue;
        }
        match backend.refresh_catalogue(&app, &cancel).await {
            Ok(found) => count += found,
            Err(err) if err == "cancelled" => return Err(err),
            // One list failing is no reason to throw away the others.
            Err(err) => {
                failed.get_or_insert(err);
            }
        }
    }
    match failed {
        Some(err) if count == 0 => Err(err),
        _ => Ok(count),
    }
}

#[tauri::command]
pub fn cancel_compatibility(state: State<'_, InstallState>) {
    state.cancel_compat.store(true, Ordering::Relaxed);
}

/// What the community publishes for a game, from the emulator its console
/// uses. A game from the catalogue isn't in the library, so it names its
/// console.
#[tauri::command]
pub fn community_packs(app: AppHandle, title_id: String, console: Option<Console>) -> crate::core::community::Packs {
    let game = library_path(&app)
        .ok()
        .and_then(|file| Library::load(&file).games().iter().find(|g| g.title_id == title_id).cloned());
    let Some(backend) = game.as_ref().map(|g| g.console).or(console).and_then(crate::backends::for_console) else {
        return crate::core::community::Packs::default();
    };
    backend.community_packs(&app, &title_id, game.as_ref())
}

#[tauri::command]
pub fn set_community_pack(
    app: AppHandle,
    title_id: String,
    change: crate::core::community::PackChange,
) -> Result<(), String> {
    let (backend, game) = game_and_emulator(&app, &title_id)?;
    backend.set_community_pack(&app, &game, &change)
}

/// Downloads the newest packs for one console's emulator, with progress, and
/// can be stopped.
#[tauri::command]
pub async fn refresh_community(app: AppHandle, state: State<'_, InstallState>, console: Console) -> Result<usize, String> {
    state.cancel_community.store(false, Ordering::Relaxed);
    let cancel = state.cancel_community.clone();
    let backend = crate::backends::for_console(console).ok_or("There are no community packs for this console.")?;
    backend.refresh_community(&app, &cancel).await
}

#[tauri::command]
pub fn cancel_community(state: State<'_, InstallState>) {
    state.cancel_community.store(true, Ordering::Relaxed);
}

#[derive(serde::Serialize, Clone)]
pub struct ScanProgress {
    pub stage: String,
    pub done: usize,
    pub total: usize,
    pub title: String,
}

#[derive(serde::Serialize)]
pub struct ScanResult {
    pub added: usize,
    pub already_there: usize,
    pub not_games: usize,
    pub cancelled: bool,
    /// The games added that may not run well, said once at the end rather
    /// than asked about one at a time across a whole drive.
    pub warnings: Vec<Warning>,
}

/// Imports every dump under a folder in one pass.
///
/// Anything already in the library is left alone rather than replaced, so a
/// second scan over the same drive is harmless and quick.
#[tauri::command]
pub async fn scan_folder(
    app: AppHandle,
    path: String,
    state: State<'_, InstallState>,
) -> Result<ScanResult, String> {
    state.cancel_import.store(false, Ordering::Relaxed);
    let cancel = state.cancel_import.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&path);
        if !root.is_dir() {
            return Err("That folder isn't there.".to_string());
        }

        let mut seen = 0usize;
        let dumps = import::find_dumps(&root, &cancel, &mut |_| {
            seen += 1;
            let _ = app.emit(
                "scan-progress",
                ScanProgress {
                    stage: "looking".into(),
                    done: seen,
                    total: 0,
                    title: String::new(),
                },
            );
        });

        let library_file = library_path(&app)?;
        let mut library = Library::load(&library_file);
        let mut result = ScanResult {
            added: 0,
            already_there: 0,
            not_games: 0,
            cancelled: false,
            warnings: Vec::new(),
        };
        let mut added = Vec::new();

        let total = dumps.len();
        for (done, dump) in dumps.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                result.cancelled = true;
                break;
            }
            match crate::backends::identify(dump) {
                Ok(game) => {
                    if library.games().iter().any(|g| g.title_id == game.title_id) {
                        result.already_there += 1;
                    } else {
                        let _ = app.emit(
                            "scan-progress",
                            ScanProgress {
                                stage: "reading".into(),
                                done: done + 1,
                                total,
                                title: game.title.clone(),
                            },
                        );
                        added.push(Imported::of(&game));
                        library.upsert(game);
                        result.added += 1;
                    }
                }
                // A folder that is not a game, or is an update rather than a
                // title, is not a failure worth stopping a whole drive for.
                Err(_) => result.not_games += 1,
            }
        }

        if result.added > 0 {
            library.save(&library_file)?;
            let lists = compat_lists(&app);
            result.warnings = added.iter().filter_map(|game| import_warning::warning(game, &lists)).collect();
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn game_updates(title_id: String) -> Result<Vec<rpcs3::updates::Update>, String> {
    rpcs3::updates::available(&title_id).await
}

#[tauri::command]
pub fn cancel_update(state: State<'_, InstallState>) {
    state.cancel_update.store(true, Ordering::Relaxed);
}

/// How far an update run has got, across every package in it.
#[derive(Clone, serde::Serialize)]
pub struct UpdateProgress {
    /// The version being downloaded or installed right now.
    pub version: String,
    /// Which package this is, counted from 1, and how many there are.
    pub step: usize,
    pub steps: usize,
    /// Bytes downloaded across the whole run, and the size of the whole run.
    pub bytes: u64,
    pub total: u64,
    /// True once this package is downloaded and RPCS3 is installing it.
    pub installing: bool,
}

/// Takes a game up to `update`, installing every update before it that is not
/// installed yet, oldest first. RPCS3 refuses a package whose previous one is
/// missing, so asking for the newest means installing them all.
///
/// The version reached is recorded against the game after each one, because
/// the update lives in the emulator's storage and the dump keeps reporting the
/// version it shipped with. That also means a run that stops part way carries
/// on from where it got to next time.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    title_id: String,
    update: rpcs3::updates::Update,
    state: State<'_, InstallState>,
) -> Result<(), String> {
    state.cancel_update.store(false, Ordering::Relaxed);
    let cancel = state.cancel_update.clone();

    let file = library_path(&app)?;
    // What RPCS3 itself has installed comes first; the library's note of it
    // can fall behind.
    let installed = rpcs3::updates::installed_version(&app, &title_id)
        .or_else(|| {
            Library::load(&file)
                .games()
                .iter()
                .find(|g| g.title_id == title_id)
                .and_then(|g| g.update_version.clone().or_else(|| g.version.clone()))
        })
        .unwrap_or_default();
    // Already there, so only the library's note needs catching up.
    if installed == update.version {
        let mut library = Library::load(&file);
        if let Some(game) = library.get_mut(&title_id) {
            game.update_version = Some(installed);
            library.save(&file)?;
        }
        return Ok(());
    }
    let published = rpcs3::updates::available(&title_id).await?;
    let mut steps = rpcs3::updates::chain(&published, &installed, &update.version);
    // Going back to an older version is one package on its own.
    if steps.is_empty() {
        steps.push(update.clone());
    }

    // Updating is the moment saves are worth keeping: going back to an older
    // version afterwards is possible, but a save written by the newer one may
    // not load on it. Told to back up and given no way to, people would not.
    // A failure here is not a reason to refuse the update; it is reported and
    // the update goes ahead.
    if let Err(e) = rpcs3::saves::back_up(&app, &title_id) {
        let _ = app.emit("saves-backup-failed", e);
    }

    let whole: u64 = steps.iter().map(|s| s.size).sum();
    let count = steps.len();
    let mut before = 0u64;
    for (at, step) in steps.iter().enumerate() {
        let emitter = app.clone();
        let version = step.version.clone();
        let mut last = 0u64;
        rpcs3::updates::install(&app, step, &cancel, move |done, total| {
            // One event per percent of this package, not per chunk.
            let every = (total / 100).max(1);
            if done - last >= every || done >= total {
                last = done;
                let _ = emitter.emit(
                    "update-progress",
                    UpdateProgress {
                        version: version.clone(),
                        step: at + 1,
                        steps: count,
                        bytes: before + done,
                        total: whole,
                        installing: done >= total,
                    },
                );
            }
        })
        .await?;
        before += step.size;

        let mut library = Library::load(&file);
        if let Some(game) = library.get_mut(&title_id) {
            game.update_version = Some(step.version.clone());
            library.save(&file)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn game_saves(app: AppHandle, title_id: String) -> (bool, Vec<rpcs3::saves::Backup>) {
    (
        rpcs3::saves::has_saves(&app, &title_id),
        rpcs3::saves::list(&app, &title_id),
    )
}

#[tauri::command]
pub fn back_up_saves(
    app: AppHandle,
    title_id: String,
) -> Result<Option<rpcs3::saves::Backup>, String> {
    rpcs3::saves::back_up(&app, &title_id)
}

#[tauri::command]
pub fn restore_saves(app: AppHandle, title_id: String, made: u64) -> Result<(), String> {
    rpcs3::saves::restore(&app, &title_id, made)
}

#[tauri::command]
pub fn forget_backup(app: AppHandle, title_id: String, made: u64) -> Result<(), String> {
    rpcs3::saves::forget(&app, &title_id, made)
}

#[tauri::command]
pub fn get_account(app: AppHandle) -> rpcs3::account::Account {
    rpcs3::account::read(&app)
}

#[tauri::command]
pub fn list_regions() -> Vec<rpcs3::account::RegionChoice> {
    rpcs3::account::regions()
}

#[tauri::command]
pub fn set_username(app: AppHandle, name: String) -> Result<String, String> {
    rpcs3::account::set_username(&app, &name)
}

#[tauri::command]
pub fn set_region(app: AppHandle, id: String) -> Result<(), String> {
    rpcs3::account::set_region(&app, &id)
}

/// Whether the first-run questions still need asking.
#[tauri::command]
pub fn needs_setup(app: AppHandle) -> Result<bool, String> {
    Ok(!Settings::load(&settings_path(&app)?).set_up)
}

#[tauri::command]
pub fn finish_setup(app: AppHandle) -> Result<(), String> {
    let file = settings_path(&app)?;
    let mut settings = Settings::load(&file);
    settings.set_up = true;
    settings.save(&file)
}

#[derive(serde::Serialize)]
pub struct PendingUpdate {
    pub title_id: String,
    pub title: String,
    pub installed: String,
    pub newest: String,
}

/// Which games in the library have a newer version published.
///
/// Answered from the compatibility list we already keep, which carries the
/// newest version for every title. No request is made per game, so this is
/// instant and works with the network off.
#[tauri::command]
pub fn pending_updates(app: AppHandle) -> Result<(bool, Vec<PendingUpdate>), String> {
    let newest = rpcs3::compat::newest_versions(&app);
    let have_list = rpcs3::compat::have_list(&app);
    let library = Library::load(&library_path(&app)?);

    let mut waiting: Vec<PendingUpdate> = library
        .games()
        .iter()
        .filter_map(|game| {
            let installed = game.running_version()?;
            let published = newest.get(&game.title_id)?;
            rpcs3::compat::is_newer(published, installed).then(|| PendingUpdate {
                title_id: game.title_id.clone(),
                title: game.title.clone(),
                installed: installed.to_string(),
                newest: published.clone(),
            })
        })
        .collect();
    waiting.sort_by(|a, b| a.title.cmp(&b.title));

    Ok((have_list, waiting))
}

#[tauri::command]
pub fn installed_packages(app: AppHandle) -> Result<Vec<rpcs3::packages::Installed>, String> {
    rpcs3::packages::installed(&app)
}

/// Runs off the interface thread: RPCS3 is started to do the work and takes a
/// few seconds over it, and a frozen window during that is worse than the wait.
#[tauri::command]
pub async fn install_package(app: AppHandle, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        rpcs3::packages::install(&app, std::path::Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn remove_package(app: AppHandle, title_id: String) -> Result<(), String> {
    rpcs3::packages::remove(&app, &title_id)
}

/// What a path dropped on the window is, so the interface can send it the right
/// way without guessing from a file extension.
///
/// An archive is recognised by its first bytes rather than its name, the same
/// way importing one does, so a .zip renamed to .bin is still an archive and a
/// text file called game.zip is not.
#[tauri::command]
pub fn dropped_kind(path: String) -> &'static str {
    let path = Path::new(&path);
    if path.is_dir() {
        "folder"
    } else if archive::detect_kind(path).is_some() {
        "archive"
    } else {
        "unknown"
    }
}

/// Sizes the picture for this machine the first time it can, then never again.
///
/// RPCS3 writes its settings on its own first launch, so on a fresh install
/// this takes effect from the second game started. Until then `apply` reports
/// that there is nothing to change yet and this is tried again next time.
fn tune_picture(app: &AppHandle, backend: &dyn crate::backends::EmulatorBackend) {
    let Ok(file) = settings_path(app) else {
        return;
    };
    let mut settings = Settings::load(&file);
    if settings.tuned {
        return;
    }
    let hw = hardware::detect();
    let (Some(display), Some(gpu)) = (hw.display, hw.gpu) else {
        return;
    };
    if let Ok(scale) = backend.tune_picture(app, display.height, gpu.dedicated_memory_bytes) {
        settings.tuned = true;
        settings.tuned_scale = scale;
        let _ = settings.save(&file);
    }
}

#[derive(serde::Serialize)]
pub struct PlayerView {
    pub pad: crate::core::pad_layout::Pad,
    /// Whether that pad is plugged in right now.
    pub connected: bool,
    /// Every place on a pad and the input standing for it.
    pub buttons: std::collections::BTreeMap<String, String>,
}

/// What one console calls each place on a pad.
#[derive(serde::Serialize)]
pub struct ConsoleButtons {
    pub console: Console,
    pub name: &'static str,
    pub buttons: std::collections::BTreeMap<&'static str, &'static str>,
}

#[derive(serde::Serialize)]
pub struct ControllerView {
    /// Pads plugged in right now.
    pub connected: Vec<crate::core::pad_layout::Pad>,
    /// Players one to four, in order.
    pub players: Vec<PlayerView>,
    /// Every pad a player can be given.
    pub pads: Vec<crate::core::pad_layout::Pad>,
    /// Whether the layout has been kept. Until then the players shown are the
    /// ones pressing Play will set up.
    pub saved: bool,
    /// Whether this game has a layout of its own. For a game, false means it
    /// is using the layout for every game.
    pub own: bool,
    /// Every place a layout covers.
    pub inputs: Vec<&'static str>,
    pub consoles: Vec<ConsoleButtons>,
}

/// `title_id` empty is the layout for every game.
#[tauri::command]
pub fn controller_view(app: AppHandle, title_id: String) -> ControllerView {
    let connected = crate::pads::connected();
    let current = crate::controllers::current(&app, &title_id, &connected);
    // Every pad a player can be given: the four XInput slots, which can be
    // chosen before anything is in them, whatever else is plugged in, and any
    // pad a player already has that is not plugged in right now.
    let mut pads = crate::pads::xinput_slots();
    for pad in connected.iter().chain(current.players.iter().map(|p| &p.pad)) {
        if !pads.iter().any(|known| known.device == pad.device) {
            pads.push(pad.clone());
        }
    }
    ControllerView {
        players: current
            .players
            .iter()
            .map(|player| PlayerView {
                connected: connected.iter().any(|pad| pad.device == player.pad.device),
                pad: player.pad.clone(),
                buttons: player.all_buttons(),
            })
            .collect(),
        pads,
        saved: current.saved,
        own: current.own,
        connected,
        inputs: crate::core::pad_layout::INPUTS.to_vec(),
        consoles: crate::backends::all()
            .iter()
            .map(|backend| ConsoleButtons {
                console: backend.console(),
                name: backend.console().short(),
                buttons: backend.button_names().iter().copied().collect(),
            })
            .collect(),
    }
}

#[tauri::command]
pub fn set_up_controller(app: AppHandle, title_id: String) -> Result<(), String> {
    crate::controllers::restore_defaults(&app, &title_id)
}

/// `number` is the player, counted from 1.
#[tauri::command]
pub fn save_controller(
    app: AppHandle,
    title_id: String,
    number: usize,
    pad: crate::core::pad_layout::Pad,
    buttons: std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    crate::controllers::save_player(
        &app,
        &title_id,
        number,
        crate::core::pad_layout::Player::with_buttons(pad, buttons),
    )
}

#[tauri::command]
pub fn forget_controller(app: AppHandle, title_id: String) -> Result<(), String> {
    crate::controllers::forget(&app, &title_id)
}

#[tauri::command]
pub fn set_covers(app: AppHandle, on: bool) -> Result<(), String> {
    let file = settings_path(&app)?;
    let mut settings = Settings::load(&file);
    settings.covers = on;
    settings.save(&file)
}

#[tauri::command]
pub fn set_rawg_key(app: AppHandle, key: String) -> Result<(), String> {
    let file = settings_path(&app)?;
    let mut settings = Settings::load(&file);
    let key = key.trim().to_string();
    settings.rawg_key = (!key.is_empty()).then_some(key);
    settings.save(&file)
}

/// Looks up a cover for every game in the library. Each is asked about once,
/// found or not, so calling this again only asks about new games. The
/// library calls it whenever a game appears, so nothing is asked while
/// covers are off.
#[tauri::command]
pub async fn fetch_covers(app: AppHandle) -> Result<usize, String> {
    let settings = Settings::load(&settings_path(&app)?);
    if !settings.covers {
        return Ok(0);
    }
    let key = settings
        .rawg_key
        .filter(|key| !key.is_empty())
        .ok_or("Add a RAWG key first.")?;
    let covers = omoio_data_dir(&app)?.join("covers");
    let games: Vec<(String, String, crate::core::console::Console)> = Library::load(&library_path(&app)?)
        .games()
        .iter()
        .map(|game| (game.title_id.clone(), game.title.clone(), game.console))
        .collect();

    let client = reqwest::Client::new();
    let mut found = 0;
    for (title_id, title, console) in games {
        if crate::covers::fetch(&client, &key, &covers, &title_id, &title, console).await? {
            found += 1;
        }
    }
    Ok(found)
}

/// A cover for one catalogue game, fetched if it has not been asked about
/// before. Nothing when covers are off, there is no key, or RAWG had none.
#[tauri::command]
pub async fn catalogue_cover(
    app: AppHandle,
    key: String,
    name: String,
    console: crate::core::console::Console,
) -> Option<String> {
    let settings = Settings::load(&settings_path(&app).ok()?);
    if !settings.covers {
        return None;
    }
    let rawg_key = settings.rawg_key.filter(|k| !k.is_empty())?;
    let covers = omoio_data_dir(&app).ok()?.join("covers");
    let client = reqwest::Client::new();
    match crate::covers::fetch(&client, &rawg_key, &covers, &key, &name, console).await {
        Ok(true) => Some(
            crate::covers::cached_path(&covers, &key)
                .to_string_lossy()
                .into_owned(),
        ),
        _ => None,
    }
}

#[derive(serde::Serialize)]
pub struct EmulatorVersion {
    pub console: crate::core::console::Console,
    /// `None` when it is not installed.
    pub version: Option<String>,
}

/// What each emulator Omoio runs has installed, for the Emulators screen.
#[tauri::command]
pub fn emulator_versions(app: AppHandle) -> Vec<EmulatorVersion> {
    crate::backends::all()
        .iter()
        .map(|backend| EmulatorVersion {
            console: backend.console(),
            version: backend.detect_version(&app),
        })
        .collect()
}

#[derive(serde::Serialize)]
pub struct EmulatorUpdate {
    pub console: Console,
    pub name: &'static str,
    pub installed: String,
    pub newest: String,
}

/// Every installed emulator that has a newer official release. Asked once at
/// start. One whose release page could not be read is left out rather than
/// holding up the rest.
#[tauri::command]
pub async fn emulator_updates(app: AppHandle) -> Vec<EmulatorUpdate> {
    let mut behind = Vec::new();
    for backend in crate::backends::all() {
        let Some(installed) = backend.detect_version(&app) else {
            continue;
        };
        let Ok(newest) = backend.newest_version().await else {
            continue;
        };
        if crate::core::versions::is_newer_release(&newest, &installed) {
            behind.push(EmulatorUpdate {
                console: backend.console(),
                name: backend.name(),
                installed,
                newest,
            });
        }
    }
    behind
}

#[tauri::command]
pub async fn install_cemu(app: AppHandle, state: State<'_, InstallState>) -> Result<String, String> {
    refuse_while_playing(&app, Console::WiiU)?;
    let _installing = state.begin_install(Console::WiiU)?;
    state.cancel_cemu.store(false, Ordering::Relaxed);
    let cancel = state.cancel_cemu.clone();
    crate::backends::cemu::install(app, cancel).await
}

/// How many of the user's own keys Cemu has for disc images.
#[tauri::command]
pub fn cemu_keys(app: AppHandle) -> usize {
    crate::backends::cemu::keys::count(&app)
}

/// Adds the keys in a file the user picked to Cemu's. Resolves to how many
/// were new.
#[tauri::command]
pub fn add_cemu_keys(app: AppHandle, path: String) -> Result<usize, String> {
    crate::backends::cemu::keys::add(&app, Path::new(&path))
}

/// What the user's own Cemu, in the folder they picked, has to bring over to
/// Omoio's. Only reads that folder. A save that doesn't name its game is
/// named from the library when the game is there.
#[tauri::command]
pub fn look_at_own_cemu(app: AppHandle, path: String) -> Result<crate::backends::cemu::own_cemu::Found, String> {
    let mut found = crate::backends::cemu::own_cemu::look(&app, &path)?;
    let library = library_path(&app).map(|file| Library::load(&file)).unwrap_or_default();
    for save in found.saves.iter_mut().filter(|save| save.name.is_none()) {
        save.name = library
            .games()
            .iter()
            .find(|game| game.title_id.eq_ignore_ascii_case(&save.title_id))
            .map(|game| game.title.clone());
    }
    Ok(found)
}

/// Copies the user's keys, and the saves of games Omoio's Cemu has none for,
/// into Omoio's Cemu. Overwrites nothing.
#[tauri::command]
pub fn bring_own_cemu(app: AppHandle, path: String) -> Result<crate::backends::cemu::own_cemu::Brought, String> {
    refuse_while_playing(&app, Console::WiiU)?;
    crate::backends::cemu::own_cemu::bring_over(&app, &path)
}

/// Puts the user's save for one game in place of Omoio's, once they have
/// said so. Omoio's is moved aside first.
#[tauri::command]
pub fn replace_with_own_save(app: AppHandle, path: String, title_id: String) -> Result<(), String> {
    refuse_while_playing(&app, Console::WiiU)?;
    crate::backends::cemu::own_cemu::replace_save(&app, &path, &title_id)
}

#[tauri::command]
pub fn cancel_cemu_install(state: State<'_, InstallState>) {
    state.cancel_cemu.store(true, Ordering::Relaxed);
}

/// The emulator running the game right now, and its process.
fn running_emulator(app: &AppHandle) -> Result<(&'static dyn crate::backends::EmulatorBackend, u32), String> {
    let session = app.state::<Session>();
    let (Some(playing), Some(pid)) = (session.playing(), session.pid()) else {
        return Err("Start a game first.".to_string());
    };
    let backend = crate::backends::for_console(playing.console).ok_or("Start a game first.")?;
    Ok((backend, pid))
}

/// The figures on the running game's toy portal, by slot, empty where there
/// is none. Each of these takes a moment, since the emulator's own window does
/// the work, so they run off the interface thread. That window may take the
/// front from the menu, which takes it back after.
#[tauri::command]
pub async fn portal_figures(app: AppHandle) -> Result<Vec<String>, String> {
    let (backend, pid) = running_emulator(&app)?;
    let names = tauri::async_runtime::spawn_blocking(move || backend.portal_figures(pid))
        .await
        .map_err(|e| e.to_string())?;
    crate::portal_menu::take_front(&app);
    names
}

/// `slot` counts from 0. A figure that went on is remembered as used, so the
/// menu lists it first next time.
#[tauri::command]
pub async fn portal_load(app: AppHandle, slot: usize, figure: String) -> Result<Vec<String>, String> {
    let (backend, pid) = running_emulator(&app)?;
    let path = figure.clone();
    let names = tauri::async_runtime::spawn_blocking(move || backend.portal_load(pid, slot, Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?;
    crate::portal_menu::take_front(&app);
    let names = names?;
    crate::portal_menu::used(&app, &figure);
    Ok(names)
}

/// The user's figure files, the ones used lately first. With `playable`, only
/// those the running game reads, for the portal menu: a figure from a later
/// game does nothing in an earlier one, and Nintendo's SuperChargers figures
/// do nothing away from the Wii U. A file the user brought is kept, since
/// Omoio can't tell which character it is, except a Creation Crystal, which
/// it tells by the plain id in its first blocks.
#[tauri::command]
pub fn figures(app: AppHandle, playable: Option<bool>) -> Vec<crate::portal_menu::Figure> {
    use crate::core::figures::{game_from_title, reads};
    use crate::core::vehicles::plays_on;
    let playing = playable.unwrap_or(false).then(|| app.state::<Session>().playing()).flatten();
    let game = playing.as_ref().and_then(|playing| game_from_title(&playing.title));
    let console = playing.map(|playing| playing.console);
    crate::portal_menu::list(&app)
        .into_iter()
        .filter(|figure| {
            let character = figure.id.zip(figure.variant);
            game.is_none_or(|game| character.is_none_or(|(id, variant)| reads(game, id, variant)))
                && console.is_none_or(|console| figure.id.is_none_or(|id| plays_on(console, id)))
        })
        .collect()
}

/// Trap Team's villains: which the user has caught, and the trap holding each.
#[tauri::command]
pub fn villains(app: AppHandle) -> Vec<crate::portal_menu::VillainState> {
    crate::portal_menu::villains(&app)
}

/// Copies figure files the user picked into Omoio's figures folder.
#[tauri::command]
pub fn add_figures(app: AppHandle, paths: Vec<String>) -> Result<usize, String> {
    crate::portal_menu::add(&app, &paths)
}

#[tauri::command]
pub fn close_portal_menu(app: AppHandle) {
    crate::portal_menu::close(&app);
}

/// The kind of pad that opened the menu, to name its buttons as printed.
#[tauri::command]
pub fn portal_menu_family() -> String {
    crate::portal_menu::family()
}

/// The Skylanders game running now, so the menu lays itself out for it.
/// `None` when nothing runs or the title doesn't say which game it is.
#[tauri::command]
pub fn portal_game(app: AppHandle) -> Option<crate::core::figures::Game> {
    let playing = app.state::<Session>().playing()?;
    crate::core::figures::game_from_title(&playing.title)
}

/// Whether the running game takes the figures its emulator makes, so the
/// menu can say why before making one the game would turn away.
#[tauri::command]
pub fn portal_made_figures(app: AppHandle) -> crate::core::community::MadeFigures {
    let Some(playing) = app.state::<Session>().playing() else {
        return crate::core::community::MadeFigures::default();
    };
    crate::backends::for_console(playing.console)
        .map(|backend| backend.made_figures(&app, &playing.title))
        .unwrap_or_default()
}

/// Everything held on any pad, for a menu any player may use. Nothing while
/// another program is in front, so Omoio's menus never act on presses meant
/// for it. Asked of Windows rather than of the page's focus: the page loses
/// focus to the web view inside it the moment the window gains it (tao sends
/// `Focused(false)` on `WM_KILLFOCUS` when wry moves focus into WebView2),
/// so the page almost never believes it is in front. The running game's
/// windows count as Omoio's, since the one that keeps Cemu from hearing the
/// pad can be in front for a moment.
#[tauri::command]
pub fn pads_held(app: AppHandle) -> Vec<&'static str> {
    let mut ours = vec![std::process::id()];
    ours.extend(app.state::<Session>().pid());
    if !crate::backends::rpcs3::overlay::front_belongs_to(&ours) {
        return Vec::new();
    }
    crate::pads::held_anywhere()
}

/// Every character the running game's emulator can make a figure of that
/// the game reads on its console, each with its element and kind.
#[tauri::command]
pub async fn figure_characters(app: AppHandle) -> Result<Vec<crate::core::figures::Offer>, String> {
    let (backend, pid) = running_emulator(&app)?;
    let playing = app.state::<Session>().playing().ok_or("Start a game first.")?;
    let game = crate::core::figures::game_from_title(&playing.title);
    let handle = app.clone();
    let characters = tauri::async_runtime::spawn_blocking(move || {
        crate::portal_menu::characters(&handle, backend, playing.console, pid)
    })
    .await
    .map_err(|e| e.to_string())?;
    crate::portal_menu::take_front(&app);
    Ok(crate::core::figures::offers(characters?, game, playing.console))
}

/// A new figure on the portal: what the portal holds now, and the figure's
/// file.
#[derive(serde::Serialize)]
pub struct Made {
    names: Vec<String>,
    path: String,
}

/// Has the emulator make a new figure of a character, kept in the figures
/// folder, and put it on the portal in `slot`, counted from 0.
#[tauri::command]
pub async fn portal_create(
    app: AppHandle,
    slot: usize,
    character: crate::core::figures::Character,
) -> Result<Made, String> {
    let (backend, pid) = running_emulator(&app)?;
    let file = crate::portal_menu::new_figure(&app, &character.name)?;
    let path = file.clone();
    let made = character.clone();
    let names = tauri::async_runtime::spawn_blocking(move || backend.portal_create(pid, slot, &character, &path))
        .await
        .map_err(|e| e.to_string())?;
    crate::portal_menu::take_front(&app);
    // The emulator can write the figure and only then run into a problem, so
    // a file it wrote is recorded as its character either way.
    if file.is_file() {
        crate::portal_menu::made(&app, &file, &made);
    }
    let names = names?;
    let path = file.to_string_lossy().into_owned();
    crate::portal_menu::used(&app, &path);
    Ok(Made { names, path })
}

/// The pad button that opens the portal menu, as a place on the pad.
#[tauri::command]
pub fn portal_button(app: AppHandle) -> String {
    crate::portal_menu::button(&app)
}

#[tauri::command]
pub fn set_portal_button(app: AppHandle, button: String) -> Result<(), String> {
    crate::portal_menu::set_button(&app, &button)
}

#[tauri::command]
pub async fn portal_clear(app: AppHandle, slot: usize) -> Result<Vec<String>, String> {
    let (backend, pid) = running_emulator(&app)?;
    let names = tauri::async_runtime::spawn_blocking(move || backend.portal_clear(pid, slot))
        .await
        .map_err(|e| e.to_string())?;
    crate::portal_menu::take_front(&app);
    names
}

/// The figures' pictures a game has so far: the running game's when no
/// title id is given, which is how the portal menu asks.
#[tauri::command]
pub fn figure_pictures(app: AppHandle, title_id: Option<String>) -> Result<crate::figure_pictures::Pictures, String> {
    let title_id = match title_id {
        Some(id) => id,
        None => app.state::<Session>().playing().ok_or("Start a game first.")?.title_id,
    };
    crate::figure_pictures::pictures(&app, &title_id)
}

/// Reads the figures' pictures out of the user's own copy of the game.
/// Resolves to how many were kept, or, for a copy that can't be read as it
/// is, to what a temporary copy would take until `copy` says to make one.
#[tauri::command]
pub async fn get_figure_pictures(
    app: AppHandle,
    title_id: String,
    copy: Option<bool>,
) -> Result<crate::figure_pictures::Got, String> {
    let (backend, game) = game_and_emulator(&app, &title_id)?;
    crate::figure_pictures::get(app, backend, game, copy.unwrap_or(false)).await
}

#[tauri::command]
pub fn stop_figure_pictures() {
    crate::figure_pictures::stop();
}

/// What is held on a pad right now, for lighting the drawing and recording a
/// button. `None` when the pad does not answer, which is how it being
/// switched on or off shows.
#[tauri::command]
pub fn pad_input(device: String) -> Option<Vec<&'static str>> {
    crate::pads::held(&device)
}
