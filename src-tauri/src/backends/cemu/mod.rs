//! Cemu as the Wii U's emulator.
//!
//! Every name here was read out of Cemu's own source at tag v2.6 or measured
//! on its release. What was found, and how, is in docs/what-we-verified.md.

pub mod compat;
pub mod controllers;
pub mod convert;
pub mod game_profile;
pub mod keys;
pub mod own_cemu;
pub mod packs;
pub mod portal;
pub mod release;
pub mod sdl;
pub mod wua;

use crate::core::console::{Console, Features};
use crate::core::import_warning::Imported;
use crate::core::library::Game;
use crate::core::types::Progress;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

const USER_AGENT: &str = "Omoio";

/// Cemu prints nothing for `--version` when started without a console, which
/// is how Omoio starts it (tested), so the release tag is kept here instead.
const VERSION_FILE: &str = "omoio-version.txt";

pub fn install_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let local_data = app.path().local_data_dir().map_err(|e| e.to_string())?;
    Ok(local_data.join("Omoio").join("cemu"))
}

fn exe_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(install_dir(app)?.join("Cemu.exe"))
}

pub fn detect_version(app: &AppHandle) -> Option<String> {
    let dir = install_dir(app).ok()?;
    if !dir.join("Cemu.exe").is_file() {
        return None;
    }
    let tag = std::fs::read_to_string(dir.join(VERSION_FILE)).ok()?;
    let tag = tag.trim();
    Some(tag.strip_prefix('v').unwrap_or(tag).to_string())
}

fn emit(app: &AppHandle, stage: &str, bytes: u64, total: u64) {
    let _ = app.emit(
        "cemu-install-progress",
        Progress {
            stage: stage.to_string(),
            bytes,
            total,
        },
    );
}

/// The release Omoio installs and updates to, as `detect_version` reports
/// it: the newest one checked (release.rs), never Cemu's latest.
pub async fn newest_version() -> Result<String, String> {
    Ok(release::version().to_string())
}

/// Downloads the release Omoio was checked against from Cemu's own GitHub
/// page, checks it is the file that was checked, and unpacks it into Omoio's
/// folder. Returns the version installed.
pub async fn install(app: AppHandle, cancel: Arc<AtomicBool>) -> Result<String, String> {
    let client = reqwest::Client::new();

    emit(&app, "checking", 0, 0);
    let dir = install_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let zip_path = dir.join(release::FILE);

    let download = client
        .get(release::DOWNLOAD)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|_| "Couldn't download Cemu. Check your internet connection and try again.".to_string())?;
    if !download.status().is_success() {
        return Err("GitHub isn't handing out Cemu right now. Try again in a while.".to_string());
    }
    let total = download.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&zip_path).await.map_err(|e| e.to_string())?;
    let mut stream = download.bytes_stream();
    let mut done: u64 = 0;
    let mut hasher = Sha256::new();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&zip_path);
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
    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != release::SHA256 {
        let _ = std::fs::remove_file(&zip_path);
        return Err("The Cemu download wasn't the file Omoio expects. Try again in a while.".to_string());
    }

    emit(&app, "extracting", 0, 1);
    let (from, into) = (zip_path.clone(), dir.clone());
    tauri::async_runtime::spawn_blocking(move || unpack(&from, &into))
        .await
        .map_err(|e| e.to_string())??;
    let _ = std::fs::remove_file(&zip_path);

    // A folder named portable beside Cemu.exe keeps its settings, saves and
    // cache inside Omoio's folder instead of %APPDATA%\Cemu, where the user
    // may already have a Cemu of their own.
    std::fs::create_dir_all(dir.join("portable")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(VERSION_FILE), release::TAG).map_err(|e| e.to_string())?;

    emit(&app, "done", 1, 1);
    detect_version(&app).ok_or_else(|| "Cemu unpacked, but Cemu.exe isn't where it should be.".to_string())
}

/// The release zip holds one folder, such as `Cemu_2.6/`, around everything.
/// It is dropped, so Cemu.exe sits in Omoio's folder whatever the version.
fn unpack(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| "The Cemu download is damaged. Try again.".to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name already refuses anything that climbs out of the folder.
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(inner) = without_top_folder(&name) else {
            continue;
        };
        let target = dest.join(inner);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn without_top_folder(path: &Path) -> Option<PathBuf> {
    let mut parts = path.components();
    parts.next()?;
    let rest: PathBuf = parts.collect();
    (!rest.as_os_str().is_empty()).then_some(rest)
}

/// A file's own name, lower case, wherever it sits.
fn file_name(name: &str) -> String {
    name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase()
}

/// A Wii U disc image, which Cemu reads with the user's own keys.
fn is_disc_image(name: &str) -> bool {
    let file = file_name(name);
    file.ends_with(".wud") || file.ends_with(".wux")
}

/// A download in NUS form, which carries a `title.tmd` beside its encrypted
/// content and needs its ticket as well as a key (what-we-verified.md,
/// "Formats"). Omoio does not take those.
fn is_download(name: &str) -> bool {
    file_name(name) == "title.tmd"
}

/// Why a dump with these file names cannot be taken, if it cannot. A
/// readable game in there wins, so a `title.tmd` or an image left beside an
/// unpacked copy or a .wua never turns the copy away.
fn refusal(names: &[String], have_keys: bool) -> Option<String> {
    if names.iter().any(|name| readable_meta(name) || is_archive(name)) {
        return None;
    }
    if names.iter().any(|name| is_download(name)) {
        return Some(
            "This Wii U download is encrypted and needs its ticket, which Omoio doesn't take. \
             Omoio takes Wii U games unpacked into code, content and meta folders, or disc images."
                .to_string(),
        );
    }
    if !have_keys && names.iter().any(|name| is_disc_image(name)) {
        return Some(
            "This Wii U disc image needs your keys to read. Add them under Emulators, Cemu, then import it again."
                .to_string(),
        );
    }
    None
}

/// The one disc image picked, or the one inside the folder picked, at the top
/// or one level down, which is where an archive unpacks it. Two or more is
/// not clear, so none.
fn disc_image(picked: &Path) -> Option<PathBuf> {
    one_file(picked, is_disc_image)
}

/// The one .wua picked, or found where `disc_image` looks.
fn wua_file(picked: &Path) -> Option<PathBuf> {
    one_file(picked, is_archive)
}

fn one_file(picked: &Path, kind: fn(&str) -> bool) -> Option<PathBuf> {
    let is_image = |path: &Path| path.is_file() && kind(&path.to_string_lossy());
    if is_image(picked) {
        return Some(picked.to_path_buf());
    }
    if !picked.is_dir() {
        return None;
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(picked).ok()?.flatten() {
        let path = entry.path();
        if is_image(&path) {
            found.push(path);
        } else if path.is_dir() {
            if let Ok(inner) = std::fs::read_dir(&path) {
                found.extend(inner.flatten().map(|e| e.path()).filter(|p| is_image(p)));
            }
        }
    }
    (found.len() == 1).then(|| found.remove(0))
}

/// A game's name from its image's file name, without the region and language
/// notes such names carry: "Game (Europe) (En,Fr)" is "Game".
fn title_from_file(stem: &str) -> String {
    let mut title = String::new();
    let mut depth = 0;
    for c in stem.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth as i32 - 1).max(0) as usize,
            _ if depth == 0 => title.push(c),
            _ => {}
        }
    }
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        stem.to_string()
    } else {
        title
    }
}

/// An id for a disc image, which cannot be read for its real one without
/// decrypting it. Made from the file's name so it is the same every time.
fn disc_id(file: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in file.to_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("WUD{:013X}", hash >> 12)
}

fn identify_disc(image: &Path) -> Game {
    let stem = image.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let file = image.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Game {
        console: Console::WiiU,
        title_id: disc_id(&file),
        title: title_from_file(&stem),
        version: None,
        update_version: None,
        size_bytes: std::fs::metadata(image).map(|m| m.len()).unwrap_or(0),
        path: image.to_path_buf(),
    }
}

/// The title id Cemu writes to its log as a game loads, "TitleId:
/// 00050000-10140400", as the sixteen lower-case digits its game profiles
/// are named by.
fn title_id_in_log(log: &str) -> Option<String> {
    log.lines()
        .find_map(|line| line.split_once("TitleId: ").map(|(_, id)| id.trim().replace('-', "").to_ascii_lowercase()))
        .filter(|id| id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit()))
}

const NOT_PLAYED_YET: &str = "Play this game once, and its settings can be changed here.";

/// The title id Cemu files a game's settings under. An unpacked title is
/// imported under it already. A disc image is encrypted, so its id is known
/// only once Cemu has run it: it is read from the log Omoio keeps of that
/// play, and remembered, since old logs are cleared away.
fn title_id_for(app: &AppHandle, game: &Game) -> Option<String> {
    let own = game.title_id.to_ascii_lowercase();
    if own.len() == 16 && own.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(own);
    }
    let data = app.path().data_dir().ok()?.join("Omoio");
    let known_file = data.join("cemu-title-ids.json");
    let mut known: std::collections::BTreeMap<String, String> = std::fs::read_to_string(&known_file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    if let Some(id) = known.get(&game.title_id) {
        return Some(id.clone());
    }
    let prefix = format!("{}-", game.title_id);
    let mut logs: Vec<PathBuf> = std::fs::read_dir(data.join("logs"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".log"))
        })
        .collect();
    logs.sort();
    let id = logs
        .iter()
        .rev()
        .find_map(|log| std::fs::read_to_string(log).ok().and_then(|text| title_id_in_log(&text)))?;
    known.insert(game.title_id.clone(), id.clone());
    if let Ok(text) = serde_json::to_string_pretty(&known) {
        let _ = std::fs::write(&known_file, text);
    }
    Some(id)
}

/// The title id Cemu knows a game by: the library game's, or a catalogue
/// entry's own id, which is already Cemu's.
fn cemu_title(app: &AppHandle, title_id: &str, game: Option<&Game>) -> Option<String> {
    match game {
        Some(game) => title_id_for(app, game),
        None => {
            let id = title_id.to_ascii_lowercase();
            (id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit())).then_some(id)
        }
    }
}

/// Cemu's own compressed archive of a title (.wua), which Cemu writes with
/// the files decrypted.
fn is_archive(name: &str) -> bool {
    file_name(name).ends_with(".wua")
}

/// A decrypted copy of the game whose files Omoio can read: the game itself
/// when it is unpacked or a .wua, or else a .wua Cemu made of it, looked for
/// beside the game and one folder up and known by its title id. A disc
/// image's own files stay out of reach: they are encrypted, and Omoio never
/// decrypts.
fn readable_copy(app: &AppHandle, game: &Game, title_of: &dyn Fn(&Path) -> Option<String>) -> Option<PathBuf> {
    if game.path.is_dir() || is_wua(&game.path) {
        return Some(game.path.clone());
    }
    let wanted = title_id_for(app, game)?;
    let beside = game.path.parent()?;
    [Some(beside), beside.parent()]
        .into_iter()
        .flatten()
        .filter_map(|folder| std::fs::read_dir(folder).ok())
        .flat_map(|entries| entries.flatten().map(|entry| entry.path()))
        .filter(|path| is_archive(&path.to_string_lossy()))
        .find(|path| title_of(path).as_deref() == Some(wanted.as_str()))
}

/// The title version Cemu writes to its log as a game loads, "TitleVersion:
/// v16", in the same numbers meta.xml uses. A disc image's meta.xml is
/// encrypted, so this is where Omoio learns its version: after the first play.
fn title_version(log: &str) -> Option<String> {
    log.lines()
        .find_map(|line| line.split_once("TitleVersion: v").map(|(_, version)| version.trim()))
        .filter(|version| !version.is_empty() && version.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

/// The meta.xml of an unpacked Wii U title, which Omoio can read.
fn readable_meta(name: &str) -> bool {
    name.replace('\\', "/").to_ascii_lowercase().ends_with("meta/meta.xml")
}

/// An unpacked Wii U title: the three folders Cemu itself looks for.
fn is_game_folder(dir: &Path) -> bool {
    ["code", "content", "meta"].iter().all(|part| dir.join(part).is_dir())
}

/// Archives usually unpack into a folder of their own name, so the game may
/// sit one level below what was picked. Only when there is exactly one.
fn find_root(picked: &Path) -> Option<PathBuf> {
    if is_game_folder(picked) {
        return Some(picked.to_path_buf());
    }
    let mut found = None;
    for entry in std::fs::read_dir(picked).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() && is_game_folder(&path) {
            if found.is_some() {
                return None;
            }
            found = Some(path);
        }
    }
    found
}

#[derive(Debug, Default, PartialEq)]
struct Meta {
    title_id: String,
    version: String,
    long_name: String,
    short_name: String,
}

/// One element's text out of meta.xml. The file is a flat list of elements
/// written by Nintendo's tools, the same shape Cemu's ParsedMetaXml.h reads,
/// so each value is found by its element name rather than walked as a tree.
fn element(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(at) = xml[from..].find(&open) {
        let after = from + at + open.len();
        // "<title_id" must not match "<title_idx".
        if xml[after..].starts_with(|c: char| c == '>' || c.is_whitespace()) {
            let start = after + xml[after..].find('>')? + 1;
            let end = start + xml[start..].find(&format!("</{name}>"))?;
            let raw = &xml[start..end];
            return Some(
                quick_xml::escape::unescape(raw)
                    .map(|text| text.into_owned())
                    .unwrap_or_else(|_| raw.to_string()),
            );
        }
        from = after;
    }
    None
}

fn parse_meta(xml: &str) -> Meta {
    let get = |name: &str| element(xml, name).unwrap_or_default();
    Meta {
        title_id: get("title_id").trim().to_string(),
        version: get("title_version").trim().to_string(),
        long_name: get("longname_en"),
        short_name: get("shortname_en"),
    }
}

fn tga_as_png(file: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(file).ok()?;
    crate::core::tga::to_png(&crate::core::tga::decode(&data)?)
}

/// The game's own picture as a PNG: the one the Wii U shows on the TV as the
/// game starts, which is wide like the library's tiles, or else its menu
/// icon. An unpacked game's and a .wua's can be read. A disc image's are
/// encrypted.
fn own_picture(root: &Path) -> Option<Vec<u8>> {
    if is_wua(root) {
        return wua_picture(root);
    }
    if !root.is_dir() {
        return None;
    }
    PICTURES
        .into_iter()
        .find_map(|name| tga_as_png(&root.join("meta").join(name)))
}

const PICTURES: [&str; 2] = ["bootTvTex.tga", "iconTex.tga"];

fn wua_picture(file: &Path) -> Option<Vec<u8>> {
    let mut archive = wua::Archive::open(file).ok()?;
    let game = game_in_wua(&archive.folders()).ok()?.to_string();
    PICTURES.into_iter().find_map(|name| {
        let data = archive.read(&format!("{game}/meta/{name}"))?;
        crate::core::tga::to_png(&crate::core::tga::decode(&data)?)
    })
}

/// Where Cemu keeps the Wii U's own storage, saves included: the folder
/// named in its settings, or `mlc01` in its portable folder when none is
/// (`ActiveSettings.cpp`, v2.6). Omoio never starts Cemu with `--mlc`.
fn mlc_folder(portable: &Path) -> PathBuf {
    std::fs::read_to_string(portable.join("settings.xml"))
        .ok()
        .and_then(|text| setting(&text, &[], "mlc_path"))
        .map(|raw| {
            quick_xml::escape::unescape(&raw)
                .map(|text| text.into_owned())
                .unwrap_or_else(|_| raw.clone())
        })
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| portable.join("mlc01"))
}

/// The icon a game keeps beside its save, as the Wii U lays its storage out:
/// usr/save, then the title id's two halves, then meta.
fn save_icon(mlc: &Path, title_id: &str) -> Option<PathBuf> {
    let id = title_id.to_ascii_lowercase();
    if id.len() != 16 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(mlc.join("usr").join("save").join(&id[..8]).join(&id[8..]).join("meta").join("iconTex.tga"))
}

/// A game's save, as the Wii U lays its storage out (see `save_icon`): one
/// folder, usr/save/<high half>/<low half>, holding the game's `user` saves
/// and its `meta`. It is copied whole, as bringing saves over from the
/// user's own Cemu copies it (own_cemu.rs), and kept under the high half.
fn save_folders_in(mlc: &Path, title_id: &str) -> Vec<super::SaveFolder> {
    let id = title_id.to_ascii_lowercase();
    if id.len() != 16 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Vec::new();
    }
    let high = mlc.join("usr").join("save").join(&id[..8]);
    let save = high.join(&id[8..]);
    if !own_cemu::has_files(&save) {
        return Vec::new();
    }
    vec![super::SaveFolder { kept_as: id[..8].to_string(), path: high, saves: vec![save] }]
}

/// The folder of the high half `kept_as` that a backup's save goes back
/// into.
fn save_folder_in(mlc: &Path, kept_as: &str) -> Option<PathBuf> {
    let is_half = kept_as.len() == 8 && kept_as.chars().all(|c| c.is_ascii_hexdigit());
    is_half.then(|| mlc.join("usr").join("save").join(kept_as))
}

/// A game whose dump can't be read, such as a disc image, still gets its
/// icon once it has been played and saved: Cemu writes it beside the save,
/// decrypted, as the console does. Omoio only reads what Cemu wrote.
fn played_picture(app: &AppHandle, game: &Game) -> Option<Vec<u8>> {
    let title_id = title_id_for(app, game)?;
    let mlc = mlc_folder(&install_dir(app).ok()?.join("portable"));
    tga_as_png(&save_icon(&mlc, &title_id)?)
}

/// A title's type is the low byte of the high half of its id, per Cemu's
/// TitleId.h: 00 a game, 02 a demo, 0C add-on content, 0E an update.
fn title_type(title_id: &str) -> Option<u8> {
    u8::from_str_radix(title_id.get(6..8)?, 16).ok()
}

fn identify(picked: &Path) -> Result<Game, String> {
    let root = find_root(picked).ok_or("This doesn't look like an unpacked Wii U game.")?;
    let xml = std::fs::read_to_string(root.join("meta").join("meta.xml"))
        .map_err(|_| "Couldn't read meta.xml. This folder doesn't look like a Wii U game.".to_string())?;
    let (title_id, title, version) = read_meta(&xml)?;

    Ok(Game {
        console: Console::WiiU,
        title_id,
        title,
        version,
        update_version: None,
        size_bytes: crate::import::directory_size(&root),
        path: root,
    })
}

/// Which game an archive holds, before anything is unpacked: from its
/// meta.xml, or from a disc image's file name. The places `find_root` and
/// `disc_image` look, the top or one folder down, and only when there is one.
/// A .wua in there is told only once it is out: its meta.xml is inside it.
fn identify_packed(names: &[String], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<Imported> {
    let depth = |name: &str| name.split(['/', '\\']).filter(|part| !part.is_empty()).count();
    let metas: Vec<&String> = names.iter().filter(|name| readable_meta(name) && depth(name) <= 3).collect();
    if let [meta] = metas.as_slice() {
        let (title_id, title, _) = read_meta(&String::from_utf8_lossy(&read(meta)?)).ok()?;
        return Some(Imported { console: Console::WiiU, title_id, title });
    }
    if !metas.is_empty() || names.iter().any(|name| is_archive(name) && depth(name) <= 2) {
        return None;
    }
    let images: Vec<&String> = names.iter().filter(|name| is_disc_image(name) && depth(name) <= 2).collect();
    let [image] = images.as_slice() else {
        return None;
    };
    let file = image.rsplit(['/', '\\']).next().unwrap_or(image.as_str());
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    Some(Imported {
        console: Console::WiiU,
        title_id: disc_id(file),
        title: title_from_file(stem),
    })
}

fn is_wua(path: &Path) -> bool {
    path.is_file() && is_archive(&path.to_string_lossy())
}

/// A title's folder in a .wua, which Cemu names `<title id>_v<version>`
/// (`TitleInfo::ParseWuaTitleFolderName`, v2.6), as the title id.
fn wua_title(folder: &str) -> Option<&str> {
    let (id, version) = folder.split_once("_v")?;
    let id_ok = id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit());
    let version_ok = version.parse::<u16>().is_ok() && (version == "0" || !version.starts_with('0'));
    (id_ok && version_ok).then_some(id)
}

/// Which of a .wua's titles is the game: the first game or demo, the one
/// Cemu starts when it is given the file (`TitleInfo::DetectFormat`, v2.6).
/// An update or add-on beside it belongs to the game and is not a game.
fn game_in_wua(folders: &[String]) -> Result<&str, String> {
    let titles: Vec<(&str, u8)> = folders
        .iter()
        .filter_map(|folder| Some((folder.as_str(), title_type(wua_title(folder)?)?)))
        .collect();
    if let Some((folder, _)) = titles.iter().find(|(_, kind)| matches!(kind, 0x00 | 0x02)) {
        return Ok(folder);
    }
    Err(match titles.first().map(|(_, kind)| kind) {
        Some(0x0E) => "This .wua holds a game update, not the game. Import the game itself first.",
        Some(0x0C) => "This .wua holds add-on content, not the game. Import the game itself first.",
        _ => "Couldn't find a Wii U game inside this .wua file.",
    }
    .to_string())
}

fn identify_wua(file: &Path) -> Result<Game, String> {
    let mut archive = wua::Archive::open(file)?;
    let game = game_in_wua(&archive.folders())?.to_string();
    let xml = archive
        .read(&format!("{game}/meta/meta.xml"))
        .ok_or("Couldn't read meta.xml inside this .wua file. Have Cemu make it again.")?;
    let (title_id, title, version) = read_meta(&String::from_utf8_lossy(&xml))?;
    Ok(Game {
        console: Console::WiiU,
        title_id,
        title,
        version,
        update_version: None,
        size_bytes: std::fs::metadata(file).map(|m| m.len()).unwrap_or(0),
        path: file.to_path_buf(),
    })
}

/// The title id, name and version meta.xml gives, or why it can't be taken.
fn read_meta(xml: &str) -> Result<(String, String, Option<String>), String> {
    let meta = parse_meta(xml);

    let title_id = meta.title_id.to_ascii_uppercase();
    if title_id.len() != 16 || !title_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("This game's meta.xml has no title ID, so it can't be identified.".to_string());
    }
    // An update or add-on has the same three folders as a game. Letting one
    // into the library would put a second copy of the name beside the game.
    match title_type(&title_id) {
        Some(0x0E) => return Err("This is a game update, not a game. Import the game itself first.".to_string()),
        Some(0x0C) => return Err("This is add-on content, not a game. Import the game itself first.".to_string()),
        _ => {}
    }

    // Nintendo breaks long names over lines for the console's menu.
    let title = [meta.long_name, meta.short_name]
        .into_iter()
        .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|name| !name.is_empty())
        .unwrap_or_else(|| title_id.clone());

    Ok((title_id, title, (!meta.version.is_empty()).then_some(meta.version)))
}

/// Cemu shows its Getting started window whenever `settings.xml` is missing
/// (`CemuApp.cpp`), and it would stand in front of the first game. This is
/// the file Omoio writes instead. The graphics API is in it because a file
/// that leaves it out is read as OpenGL (`CemuConfig.cpp`), while a Cemu that
/// went through its own first start uses Vulkan. Cemu's own update check is
/// off because Omoio keeps it up to date, and a prompt from Cemu would land
/// on top of the game. Everything else is Cemu's default.
const FIRST_SETTINGS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<content>\n\
\x20   <check_update>false</check_update>\n\
\x20   <Graphic>\n\
\x20       <api>1</api>\n\
\x20   </Graphic>\n\
</content>\n";

/// The Wii U's Skylanders games, by name: each needs the portal, and nothing
/// else uses it.
fn is_skylanders(title: &str) -> bool {
    title.to_lowercase().contains("skylanders")
}

/// Where the inside of the element nested as `parents` sits in settings.xml,
/// found one level at a time from the top. `Err` gives where the first
/// missing parent would go and how many of the parents were found.
fn section(text: &str, parents: &[&str]) -> Result<(usize, usize), (usize, usize)> {
    let (Some(top), Some(bottom)) = (text.find("<content>"), text.rfind("</content>")) else {
        return Err((text.len(), usize::MAX));
    };
    let (mut start, mut end) = (top + "<content>".len(), bottom);
    for (depth, parent) in parents.iter().enumerate() {
        let (open, close) = (format!("<{parent}>"), format!("</{parent}>"));
        let Some(at) = text[start..end].find(&open) else {
            return Err((end, depth));
        };
        let from = start + at + open.len();
        let Some(length) = text[from..end].find(&close) else {
            return Err((text.len(), usize::MAX));
        };
        (start, end) = (from, from + length);
    }
    Ok((start, end))
}

/// The text of one setting, `None` when it isn't there.
fn setting(text: &str, parents: &[&str], name: &str) -> Option<String> {
    let (start, end) = section(text, parents).ok()?;
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    let from = start + text[start..end].find(&open)? + open.len();
    let length = text[from..end].find(&close)?;
    Some(text[from..from + length].to_string())
}

/// Sets one setting, adding it and any missing parents. Everything else in
/// the file is left as it is.
fn set_setting(text: &str, parents: &[&str], name: &str, value: &str) -> String {
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    match section(text, parents) {
        Ok((start, end)) => {
            if let Some(at) = text[start..end].find(&open) {
                let from = start + at + open.len();
                if let Some(length) = text[from..end].find(&close) {
                    return format!("{}{value}{}", &text[..from], &text[from + length..]);
                }
            }
            let empty = format!("<{name}/>");
            if let Some(at) = text[start..end].find(&empty) {
                let from = start + at;
                return format!("{}{open}{value}{close}{}", &text[..from], &text[from + empty.len()..]);
            }
            format!("{}{open}{value}{close}\n{}", &text[..end], &text[end..])
        }
        Err((_, usize::MAX)) => text.to_string(),
        Err((at, found)) => {
            let missing = &parents[found..];
            let opening: String = missing.iter().map(|p| format!("<{p}>")).collect();
            let closing: String = missing.iter().rev().map(|p| format!("</{p}>")).collect();
            format!("{}{opening}{open}{value}{close}{closing}\n{}", &text[..at], &text[at..])
        }
    }
}

/// Where Cemu's window starts: off every screen, and short of -32000, which
/// Windows uses for a minimised window.
const OFF_SCREEN: &str = "-30000";

/// What Omoio sets in Cemu's settings.xml before each game. Cemu v2.6 reads
/// all of these at start (`CemuConfig.cpp`), and a user's own sound device is
/// never replaced.
///
/// - The Skylanders portal is plugged in for a Skylanders game only. Cemu
///   leaves it out, and a game finds only a portal that is there.
/// - Cemu starts windowed. Omoio places the picture itself, and Cemu keeps
///   its own fullscreen from the last time F11 was pressed in it.
/// - Cemu opens its window far off the screen. It puts the window where
///   `window_position` says without checking (`MainWindow.cpp`), so nothing
///   of Cemu shows before Omoio has taken the picture into its own window.
/// - Cemu's notices over the picture are off: the controller profile of
///   every player, shaders being compiled, and the friend service.
/// - An empty TV sound device is Cemu's "no sound", and it starts empty.
///   With DirectSound, its first sound system, "default" is the system's
///   own device (`DirectSoundAPI.cpp`).
/// - Cemu starts the game's volume at 20 of 100, which through a TV, or a
///   cloud PC's stream, is next to silent: Swap Force measured 0.010 of full
///   level at 20 and 0.353 at 100. Only that starting value is raised, so a
///   volume someone set in Cemu stays.
fn tune_settings(settings: &Path, skylanders: bool) -> std::io::Result<()> {
    let before = std::fs::read_to_string(settings)?;
    let portal = if skylanders { "true" } else { "false" };
    let mut text = set_setting(&before, &["EmulatedUsbDevices"], "EmulateSkylanderPortal", portal);
    text = set_setting(&text, &[], "fullscreen", "false");
    text = set_setting(&text, &[], "window_maximized", "false");
    text = set_setting(&text, &["window_position"], "x", OFF_SCREEN);
    text = set_setting(&text, &["window_position"], "y", OFF_SCREEN);
    for notice in ["ControllerProfiles", "ShaderCompiling", "FriendService"] {
        text = set_setting(&text, &["Graphic", "Notification"], notice, "false");
    }
    let direct_sound = matches!(setting(&text, &["Audio"], "api").as_deref().map(str::trim), None | Some("0"));
    let no_device = setting(&text, &["Audio"], "TVDevice").is_none_or(|device| device.trim().is_empty());
    if direct_sound && no_device {
        text = set_setting(&text, &["Audio"], "TVDevice", "default");
    }
    if setting(&text, &["Audio"], "TVVolume").is_none_or(|volume| matches!(volume.trim(), "" | "20")) {
        text = set_setting(&text, &["Audio"], "TVVolume", "100");
    }
    if text != before {
        std::fs::write(settings, text)?;
    }
    Ok(())
}

fn write_first_settings(portable: &Path) -> std::io::Result<()> {
    let path = portable.join("settings.xml");
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(portable)?;
    std::fs::write(path, FIRST_SETTINGS)
}

/// Cemu is a GUI program; without this every start of it from Omoio would
/// flash a console window over whatever the user is looking at.
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

pub struct Cemu;

impl super::EmulatorBackend for Cemu {
    fn console(&self) -> Console {
        Console::WiiU
    }

    fn name(&self) -> &'static str {
        "Cemu"
    }

    fn newest_version(&self) -> futures_util::future::BoxFuture<'static, Result<String, String>> {
        Box::pin(newest_version())
    }

    fn features(&self) -> Features {
        // Starting games, the Skylanders portal through Cemu's own window,
        // Cemu's settings for a game, its community's graphic packs and
        // backups of a game's save, which sits where bringing saves over
        // from the user's own Cemu found them. Nothing else is offered until
        // it has been checked against Cemu the way RPCS3's was. Cemu reads the
        // pad whatever is in front, so Omoio keeps its input settings window
        // open while a menu is over the game, which stops that (`hush`).
        Features {
            portal: true,
            settings: true,
            packs: true,
            saves: true,
            quiet_behind: true,
            ..Features::default()
        }
    }

    fn recognises(&self, path: &Path) -> bool {
        find_root(path).is_some() || wua_file(path).is_some() || disc_image(path).is_some()
    }

    fn is_game_file(&self, name: &str) -> bool {
        is_archive(name)
    }

    /// A readable copy first: an unpacked folder, then a .wua, and a disc
    /// image only when there is neither.
    fn identify(&self, path: &Path) -> Result<Game, String> {
        if find_root(path).is_some() {
            return identify(path);
        }
        if let Some(file) = wua_file(path) {
            return identify_wua(&file);
        }
        match disc_image(path) {
            Some(image) => Ok(identify_disc(&image)),
            None => identify(path),
        }
    }

    fn identify_packed(&self, names: &[String], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<Imported> {
        identify_packed(names, read)
    }

    fn refuses(&self, app: &AppHandle, names: &[String]) -> Option<String> {
        refusal(names, keys::count(app) > 0)
    }

    fn icon(&self, app: &AppHandle, game: &Game) -> Option<Vec<u8>> {
        own_picture(&game.path).or_else(|| played_picture(app, game))
    }

    fn prepare(&self, app: &AppHandle, game: &Game) -> Result<(), String> {
        let Ok(dir) = install_dir(app) else {
            return Ok(());
        };
        if !dir.join("Cemu.exe").is_file() {
            return Ok(());
        }
        let portable = dir.join("portable");
        let _ = write_first_settings(&portable);
        let _ = tune_settings(&portable.join("settings.xml"), is_skylanders(&game.title));
        packs::apply(app);
        Ok(())
    }

    fn tidy_window(&self, pid: u32, _game: isize) {
        portal::tidy(pid);
    }

    fn hush(&self, pid: u32, hushed: bool) -> Result<(), String> {
        portal::hush(pid, hushed)
    }

    fn readable_copy(&self, app: &AppHandle, game: &Game, title_of: &dyn Fn(&Path) -> Option<String>) -> Option<PathBuf> {
        readable_copy(app, game, title_of)
    }

    fn copy_size(&self, game: &Game) -> Option<u64> {
        is_disc_image(&game.path.to_string_lossy())
            .then(|| std::fs::metadata(&game.path).ok())
            .flatten()
            .map(|file| file.len())
    }

    fn make_copy(
        &self,
        app: &AppHandle,
        game: &Game,
        into: &Path,
        progress: &dyn Fn(u32),
        cancel: &AtomicBool,
    ) -> Result<PathBuf, String> {
        let exe = exe_path(app)?;
        if !exe.is_file() {
            return Err("Install Cemu from the Emulators screen first.".to_string());
        }
        let portable = install_dir(app)?.join("portable");
        let _ = write_first_settings(&portable);
        let wua = into.join("game.wua");
        convert::make_wua(&exe, &portable.join("settings.xml"), &game.path, &game.title, &wua, progress, cancel)?;
        Ok(wua)
    }

    fn game_settings(&self, app: &AppHandle, game: &Game) -> Result<crate::core::game_settings::GameSettings, String> {
        let title_id = title_id_for(app, game).ok_or(NOT_PLAYED_YET)?;
        Ok(crate::core::game_settings::GameSettings {
            emulator: "Cemu".to_string(),
            options: game_profile::options(),
            chosen: game_profile::read(&install_dir(app)?, &title_id),
            reasons: std::collections::BTreeMap::new(),
            common_groups: ["Graphics", "CPU", "General"].map(String::from).to_vec(),
        })
    }

    fn set_game_settings(
        &self,
        app: &AppHandle,
        game: &Game,
        chosen: &crate::core::game_settings::Chosen,
    ) -> Result<(), String> {
        let title_id = title_id_for(app, game).ok_or(NOT_PLAYED_YET)?;
        game_profile::write(&install_dir(app)?, &title_id, &game.title, chosen)
    }

    /// A disc image's title id is known only once Cemu has run it, and a
    /// game never run has saved nothing.
    fn save_folders(&self, app: &AppHandle, game: &Game) -> Vec<super::SaveFolder> {
        let (Some(title_id), Ok(install)) = (title_id_for(app, game), install_dir(app)) else {
            return Vec::new();
        };
        save_folders_in(&mlc_folder(&install.join("portable")), &title_id)
    }

    fn save_folder(&self, app: &AppHandle, _game: &Game, kept_as: &str) -> Option<PathBuf> {
        save_folder_in(&mlc_folder(&install_dir(app).ok()?.join("portable")), kept_as)
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

    fn ready_portal(&self, pid: u32) {
        portal::ready(pid)
    }

    /// Imaginators turns away the Senseis and Creation Crystals Cemu makes
    /// unless the Signature Patch pack is on. The figures of every other game
    /// carry no signature.
    fn made_figures(&self, app: &AppHandle, title: &str) -> crate::core::community::MadeFigures {
        use crate::core::figures::{game_from_title, Game};
        if game_from_title(title) == Some(Game::Imaginators) {
            packs::signature_patch(app)
        } else {
            crate::core::community::MadeFigures::default()
        }
    }

    fn button_names(&self) -> &'static [(&'static str, &'static str)] {
        &controllers::WII_U
    }

    fn missing_first_player(
        &self,
        app: &AppHandle,
        players: &[crate::core::pad_layout::Player],
        connected: &[crate::core::pad_layout::Pad],
    ) -> Option<String> {
        controllers::missing_first_player(app, players, connected)
    }

    fn write_layout(
        &self,
        app: &AppHandle,
        title_id: &str,
        players: &[crate::core::pad_layout::Player],
    ) -> Result<(), String> {
        controllers::write(app, title_id, players)
    }

    /// Cemu's picture is left as Cemu sets it.
    fn sizes_picture(&self) -> bool {
        false
    }

    fn tune_picture(
        &self,
        _app: &AppHandle,
        _display_width: u32,
        _display_height: u32,
        _graphics_memory: u64,
    ) -> Result<Option<u32>, String> {
        // Never asked (`sizes_picture`). An error rather than "nothing to
        // do", so Cemu would never be noted as sized if it were.
        Err("Omoio does not size Cemu's picture.".to_string())
    }

    fn launch(&self, app: &AppHandle, game: &Game) -> Result<u32, String> {
        let exe = exe_path(app)?;
        if !exe.is_file() {
            return Err("Install Cemu from the Emulators screen first, then you can play.".to_string());
        }
        if !game.path.exists() {
            return Err("This game isn't where it was. Reconnect the drive it's on.".to_string());
        }
        // Known by name as well as by id: a disc image's id is known only once
        // Cemu has run it, and a game's first start is when it needs this.
        let pro = crate::core::figures::game_from_title(&game.title) == Some(crate::core::figures::Game::TrapTeam)
            || title_id_for(app, game).is_some_and(|id| controllers::PRO_FIRST.contains(&id.as_str()));
        let _ = controllers::first_player(app, pro);
        // The game's own folder or .wua, so Cemu reads its meta and starts it
        // as a proper title rather than in the standalone mode it keeps for
        // loose programs. Given a .wua, Cemu starts the title `game_in_wua`
        // picks (checked with Trap Team, 6 October 2026). Not `-f`: Omoio places the picture itself, in its window
        // or across the screen, the same as it does for RPCS3.
        let child = command(&exe)
            .arg("-g")
            .arg(&game.path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(child.id())
    }

    fn detect_version(&self, app: &AppHandle) -> Option<String> {
        detect_version(app)
    }

    fn log_file(&self, app: &AppHandle) -> Option<PathBuf> {
        install_dir(app).ok().map(|dir| dir.join("portable").join("log.txt"))
    }

    fn version_from_log(&self, log: &str) -> Option<String> {
        title_version(log)
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

    fn community_packs(&self, app: &AppHandle, title_id: &str, game: Option<&Game>) -> crate::core::community::Packs {
        packs::view(app, cemu_title(app, title_id, game).as_deref())
    }

    fn set_community_pack(
        &self,
        app: &AppHandle,
        game: &Game,
        change: &crate::core::community::PackChange,
    ) -> Result<(), String> {
        packs::set(app, title_id_for(app, game).as_deref(), change)
    }

    fn refresh_community<'a>(
        &'a self,
        app: &'a AppHandle,
        cancel: &'a AtomicBool,
    ) -> futures_util::future::BoxFuture<'a, Result<usize, String>> {
        Box::pin(packs::download(app, cancel))
    }

    fn catalogue_source(&self) -> Option<(&'static str, &'static str)> {
        Some(("Wii U results from the Cemu wiki", "https://wiki.cemu.info/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made up, in the shape Cemu's parser reads: a menu element holding each
    /// value as text, with the escaping and line breaks real names carry.
    #[test]
    fn a_disc_images_version_comes_from_cemus_log() {
        let log = "[10:41:02.118] ------- Loaded title -------\n\
                   [10:41:02.118] TitleId: 00050000-10101e00\n\
                   [10:41:02.118] TitleVersion: v16\n\
                   [10:41:02.118] TitleRegion: EU\n";
        assert_eq!(title_version(log).as_deref(), Some("16"));
        assert_eq!(title_id_in_log(log).as_deref(), Some("0005000010101e00"));
        assert_eq!(title_version("[10:41:02.118] Mounting title 0005000010101e00"), None);
    }

    const META: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<menu type="complex" access="777">
  <version type="unsignedInt" length="4">33</version>
  <product_code type="string" length="32">WUP-P-ABCD</product_code>
  <title_version type="unsignedInt" length="4">16</title_version>
  <title_id type="hexBinary" length="8">0005000010abcd00</title_id>
  <longname_en type="string" length="512">Example Game &amp; Friends
Deluxe</longname_en>
  <shortname_en type="string" length="256">Example Game</shortname_en>
</menu>"#;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omoio-cemu-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn game_folder(at: &Path, meta: &str) {
        for part in ["code", "content", "meta"] {
            std::fs::create_dir_all(at.join(part)).unwrap();
        }
        std::fs::write(at.join("meta").join("meta.xml"), meta).unwrap();
    }

    #[test]
    fn reads_the_fields_omoio_shows() {
        let meta = parse_meta(META);
        assert_eq!(meta.title_id, "0005000010abcd00");
        assert_eq!(meta.version, "16");
        assert_eq!(meta.long_name, "Example Game & Friends
Deluxe");
        assert_eq!(meta.short_name, "Example Game");
    }

    #[test]
    fn an_element_is_not_matched_by_a_longer_one_starting_the_same_way() {
        let xml = "<menu><title_idx>no</title_idx><title_id>yes</title_id></menu>";
        assert_eq!(element(xml, "title_id").as_deref(), Some("yes"));
        assert_eq!(element(xml, "version"), None);
    }

    #[test]
    fn a_game_is_identified_by_its_meta() {
        let dir = scratch("identify");
        game_folder(&dir, META);
        let game = identify(&dir).unwrap();
        assert_eq!(game.console, Console::WiiU);
        assert_eq!(game.title_id, "0005000010ABCD00");
        assert_eq!(game.title, "Example Game & Friends Deluxe");
        assert_eq!(game.version.as_deref(), Some("16"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An uncompressed 24 bit TGA of one grey.
    fn tga_file(width: u16, height: u16) -> Vec<u8> {
        let mut file = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        file.extend_from_slice(&width.to_le_bytes());
        file.extend_from_slice(&height.to_le_bytes());
        file.extend_from_slice(&[24, 0]);
        file.resize(file.len() + usize::from(width) * usize::from(height) * 3, 0x40);
        file
    }

    fn png_size(file: &[u8]) -> (u32, u32) {
        let reader = png::Decoder::new(file).read_info().unwrap();
        (reader.info().width, reader.info().height)
    }

    #[test]
    fn the_games_picture_is_its_boot_picture_or_else_its_icon() {
        let dir = scratch("picture");
        game_folder(&dir, META);
        let meta = dir.join("meta");
        assert_eq!(own_picture(&dir), None, "no picture, so the drawn tile");

        std::fs::write(meta.join("iconTex.tga"), tga_file(4, 4)).unwrap();
        assert_eq!(png_size(&own_picture(&dir).unwrap()), (4, 4));
        std::fs::write(meta.join("bootTvTex.tga"), tga_file(16, 9)).unwrap();
        assert_eq!(png_size(&own_picture(&dir).unwrap()), (16, 9));
        std::fs::write(meta.join("bootTvTex.tga"), b"not a picture").unwrap();
        assert_eq!(png_size(&own_picture(&dir).unwrap()), (4, 4), "a broken one is passed over");

        let image = dir.join("Game (Europe).wux");
        std::fs::write(&image, b"x").unwrap();
        assert_eq!(own_picture(&image), None, "a disc image's pictures are encrypted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_saves_are_where_cemus_settings_say_or_in_its_portable_folder() {
        let portable = scratch("mlc");
        assert_eq!(mlc_folder(&portable), portable.join("mlc01"), "no settings yet");

        write_first_settings(&portable).unwrap();
        assert_eq!(mlc_folder(&portable), portable.join("mlc01"), "no mlc_path in them");

        let settings = portable.join("settings.xml");
        std::fs::write(&settings, CEMU_SETTINGS.replace("<content>", "<content><mlc_path></mlc_path>")).unwrap();
        assert_eq!(mlc_folder(&portable), portable.join("mlc01"), "an empty mlc_path is Cemu's default");

        std::fs::write(
            &settings,
            CEMU_SETTINGS.replace("<content>", "<content><mlc_path>D:\\Wii U &amp; more\\mlc</mlc_path>"),
        )
        .unwrap();
        assert_eq!(mlc_folder(&portable), PathBuf::from("D:\\Wii U & more\\mlc"));
        let _ = std::fs::remove_dir_all(&portable);
    }

    #[test]
    fn a_save_icon_is_found_under_the_title_ids_two_halves() {
        let mlc = Path::new("mlc01");
        assert_eq!(
            save_icon(mlc, "0005000010101E00"),
            Some(mlc.join("usr").join("save").join("00050000").join("10101e00").join("meta").join("iconTex.tga"))
        );
        assert_eq!(save_icon(mlc, "WUD87E51FD0F7F95"), None, "a disc image's own id is no title id");
        assert_eq!(save_icon(mlc, "00050000"), None);

        let portable = scratch("save-icon");
        let meta = portable.join("mlc01/usr/save/00050000/10140400/meta");
        std::fs::create_dir_all(&meta).unwrap();
        let found = || tga_as_png(&save_icon(&mlc_folder(&portable), "0005000010140400")?);
        assert_eq!(found(), None, "played, but not saved yet");
        std::fs::write(meta.join("iconTex.tga"), tga_file(8, 8)).unwrap();
        assert_eq!(png_size(&found().unwrap()), (8, 8));
        let _ = std::fs::remove_dir_all(&portable);
    }

    #[test]
    fn a_games_save_is_its_folder_under_the_title_ids_two_halves() {
        let mlc = scratch("save-folder");
        let high = mlc.join("usr/save/00050000");
        let save = high.join("10140400");
        std::fs::create_dir_all(save.join("user/80000001")).unwrap();
        assert!(save_folders_in(&mlc, "0005000010140400").is_empty(), "no file in it yet");

        std::fs::write(save.join("user/80000001/slot0.dat"), b"save").unwrap();
        std::fs::create_dir_all(high.join("10101e00/user")).unwrap();
        std::fs::write(high.join("10101e00/user/other.dat"), b"another game's").unwrap();

        assert_eq!(
            save_folders_in(&mlc, "0005000010140400"),
            [super::super::SaveFolder { kept_as: "00050000".to_string(), path: high.clone(), saves: vec![save] }]
        );
        assert!(save_folders_in(&mlc, "WUD87E51FD0F7F95").is_empty(), "a disc image's own id is no title id");
        assert_eq!(save_folder_in(&mlc, "00050000"), Some(high));
        assert_eq!(save_folder_in(&mlc, "EUR"), None);
        let _ = std::fs::remove_dir_all(&mlc);
    }

    #[test]
    fn an_archives_game_is_told_before_it_is_unpacked() {
        let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect::<Vec<_>>();
        let meta = |name: &str| (name == "Game (EU)/meta/meta.xml").then(|| META.as_bytes().to_vec());
        let game = identify_packed(&names(&["Game (EU)/code/app.xml", "Game (EU)/meta/meta.xml"]), &meta).unwrap();
        assert_eq!((game.console, game.title_id.as_str()), (Console::WiiU, "0005000010ABCD00"));
        assert_eq!(game.title, "Example Game & Friends Deluxe");

        let nothing = |_: &str| -> Option<Vec<u8>> { None };
        let image = identify_packed(&names(&["Skylanders - Trap Team (Europe) (En,Fr)\\Skylanders - Trap Team (Europe) (En,Fr).wux"]), &nothing).unwrap();
        assert_eq!(image.title, "Skylanders - Trap Team");

        let update = META.replace("0005000010abcd00", "0005000e10abcd00");
        let update = |_: &str| Some(update.as_bytes().to_vec());
        assert_eq!(identify_packed(&names(&["meta/meta.xml"]), &update), None, "an update");
        assert_eq!(identify_packed(&names(&["a/meta/meta.xml", "b/meta/meta.xml"]), &meta), None, "two games");
        assert_eq!(identify_packed(&names(&["a.wud", "b.wux"]), &nothing), None, "two images");
    }

    #[test]
    fn updates_and_add_ons_are_not_games() {
        assert_eq!(title_type("0005000010ABCD00"), Some(0x00));
        assert_eq!(title_type("0005000E10ABCD00"), Some(0x0E));
        assert_eq!(title_type("0005000C10ABCD00"), Some(0x0C));

        let dir = scratch("update");
        game_folder(&dir, &META.replace("0005000010abcd00", "0005000e10abcd00"));
        assert!(identify(&dir).unwrap_err().contains("update"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_game_can_sit_one_level_down_but_not_among_several() {
        let dir = scratch("nested");
        game_folder(&dir.join("Example Game"), META);
        assert_eq!(find_root(&dir), Some(dir.join("Example Game")));

        game_folder(&dir.join("Another Game"), META);
        assert_eq!(find_root(&dir), None, "two games inside is not clear");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_without_all_three_parts_is_not_a_game() {
        let dir = scratch("partial");
        std::fs::create_dir_all(dir.join("code")).unwrap();
        std::fs::create_dir_all(dir.join("meta")).unwrap();
        assert!(find_root(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disc_images_and_downloads_are_told_apart() {
        assert!(is_disc_image("Swap Force/game.wux"));
        assert!(is_disc_image("C:\\Games\\GAME.WUD"));
        assert!(is_download("0005000010abcd00/title.tmd"));
        assert!(!is_disc_image("Example Game/meta/meta.xml"));
        assert!(!is_download("notes about title.tmd.txt"));
    }

    #[test]
    fn what_is_refused_depends_on_the_form_and_the_keys() {
        let names = |list: &[&str]| list.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        let unpacked = names(&["Game/code/app.xml", "Game\\meta\\meta.xml", "Game/title.tmd"]);
        assert!(refusal(&unpacked, false).is_none(), "a readable copy always gets through");
        let disc = names(&["Game/game.wux"]);
        assert!(refusal(&disc, false).unwrap().contains("Add them under Emulators"));
        assert!(refusal(&disc, true).is_none(), "with keys, Cemu reads it");
        let download = names(&["Game/title.tmd", "Game/00000000.app"]);
        assert!(refusal(&download, true).is_some(), "a download needs its ticket too");
    }

    #[test]
    fn a_disc_image_is_named_after_its_file() {
        assert_eq!(title_from_file("Skylanders SWAP Force (Europe) (En,Fr,De)"), "Skylanders SWAP Force");
        assert_eq!(title_from_file("Mario Kart 8 [USA]"), "Mario Kart 8");
        assert_eq!(title_from_file("(only notes)"), "(only notes)");
        assert_eq!(disc_id("Game.wux"), disc_id("GAME.WUX"), "the same file, the same id");
        assert_ne!(disc_id("Game.wux"), disc_id("Other.wux"));
        assert_eq!(disc_id("Game.wux").len(), 16);
    }

    #[test]
    fn an_image_is_found_on_its_own_or_one_level_down() {
        let dir = scratch("disc");
        std::fs::create_dir_all(dir.join("Unpacked")).unwrap();
        std::fs::write(dir.join("Unpacked").join("Game (Europe).wux"), b"x").unwrap();
        let image = disc_image(&dir).unwrap();
        assert!(image.ends_with("Game (Europe).wux"));
        assert_eq!(disc_image(&image), Some(image.clone()));
        let game = identify_disc(&image);
        assert_eq!(game.title, "Game");
        assert_eq!(game.console, Console::WiiU);

        std::fs::write(dir.join("Second.wud"), b"x").unwrap();
        assert!(disc_image(&dir).is_none(), "two images is not clear");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn folders(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_wua_titles_folder_is_named_by_its_id_and_version() {
        assert_eq!(wua_title("0005000010181f00_v1"), Some("0005000010181f00"));
        assert_eq!(wua_title("0005000E10181F00_v80"), Some("0005000E10181F00"));
        assert_eq!(wua_title("0005000010181f00_v0"), Some("0005000010181f00"));
        assert_eq!(wua_title("0005000010181f00_v01"), None, "Cemu allows no leading zero");
        assert_eq!(wua_title("0005000010181f00_v70000"), None, "past a 16-bit version");
        assert_eq!(wua_title("0005000010181f00"), None);
        assert_eq!(wua_title("code"), None);
    }

    #[test]
    fn the_game_in_a_wua_is_its_first_game_whatever_comes_with_it() {
        let game = "0005000010abcd00_v16";
        let with = folders(&["0005000e10abcd00_v80", "0005000c10abcd00_v0", game]);
        assert_eq!(game_in_wua(&with), Ok(game));
        assert_eq!(game_in_wua(&folders(&["notes", "0005000210abcd00_v0"])), Ok("0005000210abcd00_v0"), "a demo");
        assert!(game_in_wua(&folders(&["0005000e10abcd00_v80"])).unwrap_err().contains("update"));
        assert!(game_in_wua(&folders(&["0005000c10abcd00_v0"])).unwrap_err().contains("add-on"));
        assert!(game_in_wua(&folders(&[])).is_err());
    }

    /// A .wua the way Cemu makes one of a game with its update and DLC.
    fn wua_of(at: &Path, with_picture: bool) {
        let update = META.replace("0005000010abcd00", "0005000e10abcd00").replace(">16<", ">80<");
        let picture = tga_file(16, 9);
        let mut files: Vec<(&str, &[u8])> = vec![
            ("0005000e10abcd00_v80/meta/meta.xml", update.as_bytes()),
            ("0005000010abcd00_v16/code/app.xml", b"<app/>"),
            ("0005000010abcd00_v16/meta/meta.xml", META.as_bytes()),
            ("0005000c10abcd00_v0/meta/meta.xml", b"dlc"),
        ];
        if with_picture {
            files.push(("0005000010abcd00_v16/meta/bootTvTex.tga", &picture));
        }
        std::fs::write(at, wua::build::archive(&files)).unwrap();
    }

    #[test]
    fn a_wua_is_imported_as_its_game_picked_or_in_a_folder() {
        use crate::backends::EmulatorBackend;
        let dir = scratch("wua");
        let file = dir.join("Example (EU).wua");
        wua_of(&file, true);

        for picked in [file.clone(), dir.clone()] {
            assert!(Cemu.recognises(&picked));
            let game = Cemu.identify(&picked).unwrap();
            assert_eq!(game.console, Console::WiiU);
            assert_eq!(game.title_id, "0005000010ABCD00", "the game, not its update");
            assert_eq!(game.title, "Example Game & Friends Deluxe");
            assert_eq!(game.version.as_deref(), Some("16"));
            assert_eq!(game.path, file);
            assert_eq!(game.size_bytes, std::fs::metadata(&file).unwrap().len());
        }
        assert_eq!(png_size(&own_picture(&file).unwrap()), (16, 9), "the tile from inside the .wua");

        let nested = scratch("wua-nested");
        std::fs::create_dir_all(nested.join("Unpacked")).unwrap();
        wua_of(&nested.join("Unpacked").join("Example.wua"), false);
        assert!(Cemu.identify(&nested).unwrap().path.ends_with("Example.wua"));
        assert_eq!(own_picture(&nested.join("Unpacked").join("Example.wua")), None, "no picture, so the drawn tile");

        std::fs::write(dir.join("Example (EU).wux"), b"x").unwrap();
        assert!(Cemu.identify(&dir).unwrap().path.ends_with("Example (EU).wua"), "the readable copy wins");
        assert!(refusal(&crate::backends::names_in(&dir), false).is_none(), "and needs no keys");

        std::fs::write(dir.join("broken.wua"), b"not an archive").unwrap();
        assert!(Cemu.identify(&dir.join("broken.wua")).unwrap_err().contains(".wua"));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&nested);
    }

    #[test]
    fn a_wua_inside_an_archive_is_told_once_it_is_out() {
        let names = folders(&["Game/Game.wua", "Game/Game.wux"]);
        let nothing = |_: &str| -> Option<Vec<u8>> { None };
        assert_eq!(identify_packed(&names, &nothing), None, "not by the image's name");
        assert!(refusal(&names, false).is_none());
    }

    /// Checks a real .wua, which is the only thing that proves the reader.
    /// Game files are not committed, so point this at one to run it:
    ///   set OMOIO_WUA=D:\games\some-game.wua
    ///   cargo test real_wua -- --ignored --nocapture
    #[test]
    #[ignore = "needs OMOIO_WUA pointing at a real .wua"]
    fn identifies_a_real_wua() {
        use crate::backends::EmulatorBackend;
        let picked = PathBuf::from(std::env::var("OMOIO_WUA").expect("set OMOIO_WUA"));
        let archive = wua::Archive::open(&picked).unwrap();
        println!("titles: {:?}", archive.folders());
        let game = Cemu.identify(&picked).expect("should identify the .wua");
        println!("game:   {} {} v{}", game.title_id, game.title, game.version.as_deref().unwrap_or("-"));
        let picture = own_picture(&picked).expect("its own picture");
        println!("tile:   {:?}", png_size(&picture));
        assert_eq!(game.title_id.len(), 16);
    }

    #[test]
    fn settings_are_switched_in_place_or_added() {
        let dir = scratch("portal");
        let file = dir.join("settings.xml");

        write_first_settings(&dir).unwrap();
        tune_settings(&file, true).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("<EmulateSkylanderPortal>true</EmulateSkylanderPortal>"), "{text}");
        assert!(text.contains("<api>1</api>"), "the rest is left alone");
        assert!(text.contains("<fullscreen>false</fullscreen>"), "{text}");
        assert!(text.contains("<ShaderCompiling>false</ShaderCompiling>"), "{text}");
        assert!(text.contains("<TVDevice>default</TVDevice>"), "{text}");
        assert!(text.contains("<window_position><x>-30000</x>"), "{text}");
        assert!(text.contains("<TVVolume>100</TVVolume>"), "{text}");
        assert!(text.contains("<y>-30000</y>"), "{text}");

        tune_settings(&file, false).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("<EmulateSkylanderPortal>false</EmulateSkylanderPortal>"));
        assert_eq!(text.matches("<EmulatedUsbDevices>").count(), 1, "switched, not added twice");
        assert_eq!(text.matches("<Notification>").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The shape Cemu 2.6 writes, cut down.
    const CEMU_SETTINGS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<content>\n\
        \x20   <fullscreen_menubar>false</fullscreen_menubar>\n\
        \x20   <fullscreen>true</fullscreen>\n\
        \x20   <Graphic>\n        <api>1</api>\n        <Notification>\n\
        \x20           <ControllerProfiles>true</ControllerProfiles>\n\
        \x20           <ControllerBattery>true</ControllerBattery>\n\
        \x20       </Notification>\n    </Graphic>\n\
        \x20   <Audio>\n        <api>0</api>\n        <TVDevice></TVDevice>\n    </Audio>\n</content>\n";

    #[test]
    fn only_the_named_setting_changes() {
        let text = set_setting(CEMU_SETTINGS, &[], "fullscreen", "false");
        assert!(text.contains("<fullscreen_menubar>false</fullscreen_menubar>"));
        assert!(text.contains("<fullscreen>false</fullscreen>"));
        let text = set_setting(&text, &["Graphic", "Notification"], "ControllerProfiles", "false");
        assert!(text.contains("<ControllerProfiles>false</ControllerProfiles>"));
        assert!(text.contains("<ControllerBattery>true</ControllerBattery>"), "battery warnings stay");
        assert_eq!(setting(&text, &["Audio"], "api").as_deref(), Some("0"), "the sound api, not the graphics one");
        assert_eq!(setting(&text, &["Audio"], "TVDevice").as_deref(), Some(""));
    }

    #[test]
    fn a_sound_device_the_user_chose_is_kept() {
        let dir = scratch("sound");
        let file = dir.join("settings.xml");
        let chosen = CEMU_SETTINGS.replace(
            "<TVDevice></TVDevice>",
            "<TVDevice>{0.0.0.00000000}.{abc}</TVDevice><TVVolume>55</TVVolume>",
        );
        std::fs::write(&file, &chosen).unwrap();
        tune_settings(&file, true).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("<TVDevice>{0.0.0.00000000}.{abc}</TVDevice>"), "{text}");
        assert!(text.contains("<TVVolume>55</TVVolume>"), "a volume someone chose stays: {text}");

        std::fs::write(&file, CEMU_SETTINGS).unwrap();
        tune_settings(&file, true).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("<TVDevice>default</TVDevice>"), "an empty one is filled in");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_skylanders_games_get_the_portal() {
        assert!(is_skylanders("Skylanders SWAP Force"));
        assert!(is_skylanders("Skylanders Imaginators"));
        assert!(!is_skylanders("Mario Kart 8"));
    }

    #[test]
    fn first_settings_ask_for_vulkan_and_are_written_once() {
        let dir = scratch("settings");
        write_first_settings(&dir).unwrap();
        let text = std::fs::read_to_string(dir.join("settings.xml")).unwrap();
        assert!(text.contains("<Graphic>") && text.contains("<api>1</api>"), "{text}");
        assert!(text.contains("<check_update>false</check_update>"), "Omoio does the updating");

        std::fs::write(dir.join("settings.xml"), "mine").unwrap();
        write_first_settings(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("settings.xml")).unwrap(), "mine");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_release_folder_is_dropped_when_unpacking() {
        assert_eq!(without_top_folder(Path::new("Cemu_2.6/Cemu.exe")), Some(PathBuf::from("Cemu.exe")));
        assert_eq!(
            without_top_folder(Path::new("Cemu_2.6/resources/de/cemu.mo")),
            Some(PathBuf::from("resources/de/cemu.mo"))
        );
        assert_eq!(without_top_folder(Path::new("Cemu_2.6")), None);
    }
}
