//! Cemu's community graphic packs: resolution, frame rate and other changes
//! made per game by the people around Cemu, published under CC0 at
//! github.com/cemu-project/cemu_graphic_packs.
//!
//! Fetched the way Cemu's own "Download latest community graphic packs" does
//! (`DownloadGraphicPacksWindow.cpp`, v2.6): the latest release's zip,
//! unpacked into `graphicPacks/downloadedGraphicPacks` in Cemu's data folder,
//! with the release's name in `version.txt` there, so Cemu sees them as its
//! own. Omoio also checks the zip against the SHA-256 GitHub publishes.
//!
//! A pack is a folder with a `rules.txt`, read as Cemu reads it
//! (`GraphicPack2.cpp`, `IniParser.cpp`). Which packs are on is in Cemu's
//! settings.xml under `GraphicPack`: an `Entry` per pack, named by the path
//! of its rules.txt, with a `Preset` for each choice made in it. A pack with
//! no entry is off, unless its makers wrote `default = 1`; Cemu then has it
//! on until an entry says `disabled`. Omoio keeps the packs the user changed
//! in its own file as well and writes them in again before every game,
//! since Cemu writes its settings back and would undo a change made while
//! it ran.
//!
//! Omoio switches on by itself only the packs on its own short list
//! (`OWN_PACKS`), each tied to a named game with its reason written down.
//! Every other pack waits for the user, and one of Omoio's own that the user
//! switches off in Omoio stays off.

use crate::core::community::{Check, MadeFigures, Pack, PackChange, PackChoice, Packs};
use crate::core::types::Progress;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

const RELEASES_API: &str = "https://api.github.com/repos/cemu-project/cemu_graphic_packs/releases/latest";

/// Where the downloaded packs sit in Cemu's data folder. settings.xml names
/// each pack by its path from that folder, which starts with this.
const DOWNLOADED: [&str; 2] = ["graphicPacks", "downloadedGraphicPacks"];

/// Cemu reads no pack older than this (`GP_LEGACY_VERSION` in GraphicPack2.cpp).
const OLDEST_VERSION: i32 = 3;

/// From version 5 a pack's presets are grouped into choices, may depend on
/// one another, and one of each is always picked.
const GROUPED_PRESETS: i32 = 5;

pub const SOURCE: &str = "the Cemu community";

#[derive(Deserialize)]
struct Release {
    name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?.join("portable"))
}

fn folder(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join(DOWNLOADED[0]).join(DOWNLOADED[1]))
}

/// The release the downloaded packs came from, `None` before any were.
pub fn installed(app: &AppHandle) -> Option<String> {
    let text = std::fs::read_to_string(folder(app).ok()?.join("version.txt")).ok()?;
    Some(text.trim().to_string()).filter(|name| !name.is_empty())
}

fn emit(app: &AppHandle, stage: &str, bytes: u64, total: u64) {
    let _ = app.emit(
        "community-progress",
        Progress {
            stage: stage.to_string(),
            bytes,
            total,
        },
    );
}

/// Downloads the newest packs and puts them where Cemu keeps them. Returns
/// how many packs there are.
pub async fn download(app: &AppHandle, cancel: &AtomicBool) -> Result<usize, String> {
    let into = folder(app)?;
    download_into(&into, cancel, |stage, bytes, total| emit(app, stage, bytes, total)).await
}

async fn download_into(into: &Path, cancel: &AtomicBool, emit: impl Fn(&str, u64, u64)) -> Result<usize, String> {
    let client = reqwest::Client::new();
    emit("checking", 0, 0);
    let release: Release = client
        .get(RELEASES_API)
        .header("User-Agent", super::USER_AGENT)
        .send()
        .await
        .map_err(|_| "Couldn't reach GitHub. Check your internet connection and try again.".to_string())?
        .json()
        .await
        .map_err(|_| "GitHub answered in a form Omoio doesn't understand.".to_string())?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(".zip"))
        .ok_or("The newest graphic packs have no download.")?;
    let expected = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .ok_or("GitHub gave no checksum for the graphic packs, so Omoio won't use them.")?
        .to_ascii_lowercase();

    let response = client
        .get(&asset.browser_download_url)
        .header("User-Agent", super::USER_AGENT)
        .send()
        .await
        .map_err(|_| "Couldn't download the graphic packs. Check your internet connection and try again.".to_string())?;
    let total = response.content_length().unwrap_or(0);
    let mut zip = Vec::with_capacity(total as usize);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_string());
        }
        let chunk = chunk.map_err(|_| "The download stopped part way. Try again.".to_string())?;
        zip.extend_from_slice(&chunk);
        emit("downloading", zip.len() as u64, total);
    }

    emit("verifying", 0, 0);
    let got: String = Sha256::digest(&zip).iter().map(|byte| format!("{byte:02x}")).collect();
    if got != expected {
        return Err("The graphic packs didn't download whole. Try again.".to_string());
    }

    emit("extracting", 0, 0);
    let (into, name) = (into.to_path_buf(), release.name.clone());
    let count = tauri::async_runtime::spawn_blocking(move || put_in_place(&zip, &into, &name))
        .await
        .map_err(|e| e.to_string())??;
    emit("done", 1, 1);
    Ok(count)
}

/// Unpacks beside the old packs and swaps them over, so a failure part way
/// leaves the old ones as they were.
fn put_in_place(zip: &[u8], into: &Path, release: &str) -> Result<usize, String> {
    let fresh = into.with_extension("new");
    let _ = std::fs::remove_dir_all(&fresh);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))
        .map_err(|_| "The graphic packs download is damaged. Try again.".to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name already refuses anything that climbs out of the folder.
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let target = fresh.join(name);
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
    std::fs::write(fresh.join("version.txt"), release).map_err(|e| e.to_string())?;
    if into.exists() {
        std::fs::remove_dir_all(into).map_err(|_| "Couldn't replace the old graphic packs. Close Cemu and try again.".to_string())?;
    }
    std::fs::rename(&fresh, into).map_err(|e| e.to_string())?;
    let mut found = Vec::new();
    walk(into, &mut found);
    Ok(found.len())
}

// ---- reading a pack ----

#[derive(Debug, Clone, PartialEq)]
struct Rules {
    name: String,
    /// The pack's place in Cemu's list, "Game/Mods/FPS", which says its kind.
    path: String,
    description: String,
    version: i32,
    default_on: bool,
    title_ids: Vec<String>,
    defaults: Vec<(String, f64)>,
    presets: Vec<Preset>,
}

#[derive(Debug, Clone, PartialEq)]
struct Preset {
    name: String,
    category: String,
    condition: Option<String>,
    is_default: bool,
    vars: Vec<(String, f64)>,
}

/// Cuts a comment off: from `#` or `;`, unless inside quotes.
fn without_comment(line: &str) -> &str {
    let mut quoted = false;
    for (at, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' | ';' if !quoted => return &line[..at],
            _ => {}
        }
    }
    line
}

/// The sections of an ini file in order, each with its options in order.
fn sections(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut found: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for raw in text.lines() {
        let line = without_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            found.push((name.trim().to_string(), Vec::new()));
            continue;
        }
        let (Some((key, value)), Some(section)) = (line.split_once('='), found.last_mut()) else {
            continue;
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .unwrap_or(value);
        section.1.push((key.trim().to_string(), value.to_string()));
    }
    found
}

fn option<'a>(options: &'a [(String, String)], name: &str) -> Option<&'a str> {
    options
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn is_on(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value.trim().parse::<i64>().is_ok_and(|n| n != 0)
}

/// A section's `$name = value` lines, each worked out with the ones before
/// it, as `ParsePresetVars` does. A type after a colon, `$fps:int`, is not
/// part of the name.
fn vars(options: &[(String, String)]) -> Vec<(String, f64)> {
    let mut known: HashMap<String, f64> = HashMap::new();
    let mut found = Vec::new();
    for (key, value) in options {
        if !key.starts_with('$') {
            continue;
        }
        let name = key.split(':').next().unwrap_or(key).trim().to_string();
        if let Some(number) = evaluate(value, &known, false) {
            known.insert(name.clone(), number);
            found.push((name, number));
        }
    }
    found
}

fn parse_rules(text: &str) -> Option<Rules> {
    let all = sections(text);
    let (first, definition) = all.first()?;
    if !first.eq_ignore_ascii_case("Definition") {
        return None;
    }
    let version = option(definition, "version")?.trim().parse::<i32>().ok()?;
    if version < OLDEST_VERSION {
        return None;
    }
    let title_ids: Vec<String> = option(definition, "titleIds")?
        .split(',')
        .map(|id| id.trim().to_ascii_lowercase())
        .filter(|id| !id.is_empty())
        .collect();
    if title_ids.is_empty() {
        return None;
    }
    let mut rules = Rules {
        name: option(definition, "name").unwrap_or_default().to_string(),
        path: option(definition, "path")?.to_string(),
        description: option(definition, "description").unwrap_or_default().to_string(),
        version,
        default_on: option(definition, "default").is_some_and(is_on),
        title_ids,
        defaults: Vec::new(),
        presets: Vec::new(),
    };
    for (name, options) in &all[1..] {
        if name.eq_ignore_ascii_case("Default") {
            rules.defaults = vars(options);
        } else if name.eq_ignore_ascii_case("Preset") {
            let Some(preset) = option(options, "name") else {
                continue;
            };
            rules.presets.push(Preset {
                name: preset.to_string(),
                category: option(options, "category").unwrap_or_default().to_string(),
                condition: option(options, "condition").map(str::to_string),
                is_default: option(options, "default").is_some_and(is_on),
                vars: vars(options),
            });
        }
    }
    Some(rules)
}

/// Every folder under `dir` with a rules.txt. A pack's own folders are not
/// looked inside, as in Cemu's `LoadAll`.
fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
    if dir.join("rules.txt").is_file() {
        found.push(dir.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut inside: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
    inside.sort();
    for sub in inside {
        walk(&sub, found);
    }
}

/// The name settings.xml gives a pack: the path of its rules.txt from Cemu's
/// data folder, with Windows' separators, as Cemu writes it.
fn key_for(pack: &Path, packs: &Path) -> Option<String> {
    let inside = pack.strip_prefix(packs).ok()?;
    let mut parts: Vec<String> = DOWNLOADED.iter().map(|part| part.to_string()).collect();
    parts.extend(inside.components().map(|part| part.as_os_str().to_string_lossy().into_owned()));
    parts.push("rules.txt".to_string());
    Some(parts.join("\\"))
}

/// Every pack in the download, by its settings.xml name, read once per
/// release: reading them all takes over half a second, and the game panel
/// asks each time a game is picked.
static READ: Mutex<Option<(String, Vec<(String, Rules)>)>> = Mutex::new(None);

fn all_packs(app: &AppHandle) -> Vec<(String, Rules)> {
    let (Some(release), Ok(packs)) = (installed(app), folder(app)) else {
        return Vec::new();
    };
    let mut read = READ.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((seen, all)) = read.as_ref() {
        if *seen == release {
            return all.clone();
        }
    }
    let mut found = Vec::new();
    walk(&packs, &mut found);
    let all: Vec<(String, Rules)> = found
        .iter()
        .filter_map(|dir| {
            let rules = parse_rules(&std::fs::read_to_string(dir.join("rules.txt")).ok()?)?;
            Some((key_for(dir, &packs)?, rules))
        })
        .collect();
    *read = Some((release, all.clone()));
    all
}

/// The packs written for this title, by their settings.xml name.
fn packs_for(app: &AppHandle, title: &str) -> Vec<(String, Rules)> {
    all_packs(app)
        .into_iter()
        .filter(|(_, rules)| rules.title_ids.iter().any(|id| id == title))
        .collect()
}

// ---- conditions and choices ----

/// Works out one of Cemu's expressions: numbers, `$names`, `+ - * / % ^`
/// and the comparisons, which give 1 or 0, with the precedence of Cemu's
/// `ExpressionParser`. `whole` works in whole numbers, as a preset's
/// condition is worked out (`TExpressionParser<int>`). `None` for anything
/// it can't read, which Cemu also treats as false.
fn evaluate(text: &str, constants: &HashMap<String, f64>, whole: bool) -> Option<f64> {
    let mut parser = Expression {
        text: text.as_bytes(),
        at: 0,
        constants,
        whole,
    };
    let value = parser.comparison()?;
    parser.space();
    (parser.at == parser.text.len()).then_some(value)
}

struct Expression<'a> {
    text: &'a [u8],
    at: usize,
    constants: &'a HashMap<String, f64>,
    whole: bool,
}

impl Expression<'_> {
    fn space(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn take(&mut self, symbol: &str) -> bool {
        self.space();
        if self.text[self.at..].starts_with(symbol.as_bytes()) {
            self.at += symbol.len();
            true
        } else {
            false
        }
    }

    fn fit(&self, value: f64) -> f64 {
        if self.whole {
            value.trunc()
        } else {
            value
        }
    }

    fn comparison(&mut self) -> Option<f64> {
        let mut left = self.sum()?;
        loop {
            let op = ["==", "!=", "<=", ">=", "<", ">"].into_iter().find(|op| self.take(op));
            let Some(op) = op else {
                return Some(left);
            };
            let right = self.sum()?;
            let holds = match op {
                "==" => left == right,
                "!=" => left != right,
                "<=" => left <= right,
                ">=" => left >= right,
                "<" => left < right,
                _ => left > right,
            };
            left = if holds { 1.0 } else { 0.0 };
        }
    }

    fn sum(&mut self) -> Option<f64> {
        let mut left = self.product()?;
        loop {
            if self.take("+") {
                let right = self.product()?;
                left = self.fit(left + right);
            } else if self.take("-") {
                let right = self.product()?;
                left = self.fit(left - right);
            } else {
                return Some(left);
            }
        }
    }

    fn product(&mut self) -> Option<f64> {
        let mut left = self.power()?;
        loop {
            if self.take("*") {
                let right = self.power()?;
                left = self.fit(left * right);
            } else if self.take("/") {
                let right = self.power()?;
                if right == 0.0 {
                    return None;
                }
                left = self.fit(left / right);
            } else if self.take("%") {
                let right = self.power()?;
                if right == 0.0 {
                    return None;
                }
                left = self.fit(left % right);
            } else {
                return Some(left);
            }
        }
    }

    /// `^` groups from the right, as in Cemu.
    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.take("^") {
            let exponent = self.power()?;
            return Some(self.fit(base.powf(exponent)));
        }
        Some(base)
    }

    fn unary(&mut self) -> Option<f64> {
        if self.take("-") {
            return Some(-self.unary()?);
        }
        if self.take("+") {
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> Option<f64> {
        if self.take("(") {
            let value = self.comparison()?;
            return self.take(")").then_some(value);
        }
        self.space();
        let start = self.at;
        let first = *self.text.get(start)?;
        if first.is_ascii_digit() || first == b'.' {
            while self.text.get(self.at).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'.') {
                self.at += 1;
            }
            let word = std::str::from_utf8(&self.text[start..self.at]).ok()?;
            let number = match word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
                Some(hex) => i64::from_str_radix(hex, 16).ok()? as f64,
                None => word.parse::<f64>().ok()?,
            };
            return Some(self.fit(number));
        }
        if first == b'$' || first.is_ascii_alphabetic() || first == b'_' {
            self.at += 1;
            while self.text.get(self.at).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
                self.at += 1;
            }
            let name = std::str::from_utf8(&self.text[start..self.at]).ok()?;
            return self.constants.get(name).map(|value| self.fit(*value));
        }
        None
    }
}

/// What Cemu ends up using for each choice in a pack and which of its
/// presets it shows, worked out as `UpdatePresetVisibility` and
/// `ValidatePresetSelections` do: a preset whose condition fails is hidden,
/// and a choice with nothing picked, or with a hidden preset picked, takes
/// the one marked default or else the first shown.
fn settle(rules: &Rules, asked: &BTreeMap<String, String>) -> (BTreeMap<String, String>, Vec<bool>) {
    let presets = &rules.presets;
    let mut active: Vec<bool> = presets.iter().map(|p| asked.get(&p.category) == Some(&p.name)).collect();
    let mut visible = vec![true; presets.len()];
    let mut categories: Vec<&str> = Vec::new();
    for preset in presets {
        if !categories.contains(&preset.category.as_str()) {
            categories.push(&preset.category);
        }
    }
    // Showing a preset can change what the others depend on, so it is worked
    // out again until nothing moves, as Cemu does after each change.
    for _ in 0..8 {
        if rules.version >= GROUPED_PRESETS {
            let constants = constants(rules, &active, &visible);
            visible = presets
                .iter()
                .map(|p| {
                    p.condition
                        .as_deref()
                        .is_none_or(|condition| evaluate(condition, &constants, true).is_some_and(|v| v != 0.0))
                })
                .collect();
        }
        let mut moved = false;
        for category in &categories {
            let mine: Vec<usize> = (0..presets.len()).filter(|&i| presets[i].category == *category).collect();
            let shown: Vec<usize> = mine.iter().copied().filter(|&i| visible[i]).collect();
            let Some(&first) = shown.first() else {
                continue;
            };
            let fallback = shown.iter().copied().find(|&i| presets[i].is_default).unwrap_or(first);
            match mine.iter().copied().find(|&i| active[i]) {
                None => {
                    active[fallback] = true;
                    moved = true;
                }
                Some(picked) if !visible[picked] => {
                    active[picked] = false;
                    active[fallback] = true;
                    moved = true;
                }
                Some(_) => {}
            }
        }
        if !moved {
            break;
        }
    }
    let chosen = (0..presets.len())
        .filter(|&i| active[i])
        .map(|i| (presets[i].category.clone(), presets[i].name.clone()))
        .collect();
    (chosen, visible)
}

/// The values a condition can see: the picked presets' own, those shown
/// first, then the pack's defaults (`FillPresetConstants`).
fn constants(rules: &Rules, active: &[bool], visible: &[bool]) -> HashMap<String, f64> {
    let mut known = HashMap::new();
    for shown in [true, false] {
        for (i, preset) in rules.presets.iter().enumerate() {
            if active[i] && visible[i] == shown {
                for (name, value) in &preset.vars {
                    known.entry(name.clone()).or_insert(*value);
                }
            }
        }
    }
    for (name, value) in &rules.defaults {
        known.entry(name.clone()).or_insert(*value);
    }
    known
}

/// The choices as the interface shows them, in the pack's own order, each
/// with the presets that are shown.
fn choices(rules: &Rules, chosen: &BTreeMap<String, String>, visible: &[bool]) -> Vec<PackChoice> {
    let mut found: Vec<PackChoice> = Vec::new();
    for (i, preset) in rules.presets.iter().enumerate() {
        if !visible[i] {
            continue;
        }
        match found.iter_mut().find(|choice| choice.name == preset.category) {
            Some(choice) => choice.options.push(preset.name.clone()),
            None => found.push(PackChoice {
                name: preset.category.clone(),
                options: vec![preset.name.clone()],
                chosen: chosen.get(&preset.category).cloned().unwrap_or_default(),
            }),
        }
    }
    found
}

/// The kind of pack, from the second part of its place in Cemu's list.
fn kind(path: &str) -> &'static str {
    match path.split('/').nth(1).map(str::trim) {
        Some("Graphics") => "Graphics",
        Some("Enhancements") => "Enhancements",
        Some("Workarounds" | "Workaround") => "Fixes",
        Some("Cheats") => "Cheats",
        Some("Debug") => "Debug",
        _ => "Mods",
    }
}

/// What a pack does, and who made it. Cemu shows `|` as a line break, and
/// the makers' names follow a blank line.
fn about(description: &str) -> (String, String) {
    let mut parts = description.split("||");
    let what = parts.next().unwrap_or_default().replace('|', " ").trim().to_string();
    let who = parts.collect::<Vec<_>>().join(" ").replace('|', " ").trim().to_string();
    (what, who)
}

// ---- Omoio's own ----

/// A pack Omoio switches on by itself, found by the `path` its rules.txt
/// gives: the name its makers give it and the place Cemu's own list shows
/// it. Its folder, which settings.xml names it by, is only where the
/// download keeps it today, and a reshuffle of the repository's folders
/// would lose a match on that while the pack stayed the same.
struct OwnPack {
    path: &'static str,
    /// Shown while it is on, so nobody has to wonder why.
    reason: &'static str,
    /// Shown once the user has switched it off: what that costs.
    when_off: &'static str,
}

/// Skylanders Imaginators checks a factory signature on the figures
/// released for it, Senseis and Creation Crystals, and won't take one
/// without. A figure Cemu's figure maker makes carries none, and a UID of
/// its own, so the portal menu's Senseis and crystals need this pack, by
/// Winner Nombre with MusicDisc, MoltenLavaCore and NefariousTechSupport. It
/// removes the check in game versions 1.0.0 and 1.1.0 and the demo, and its
/// makers leave it off (rules.txt and patch_sigpatch.asm in the graphic
/// packs' v987, read 7 October 2026). Figures from the older games carry no
/// such signature, so no other game needs it.
const SIGNATURE_PATCH: &str = "Skylanders Imaginators/Mods/Signature Patch";

const OWN_PACKS: &[OwnPack] = &[OwnPack {
    path: SIGNATURE_PATCH,
    reason: "On so Imaginators takes the Senseis and Creation Crystals the portal menu makes. The game checks a factory signature those figures can't carry.",
    when_off: "Off, so Imaginators won't take the Senseis and Creation Crystals the portal menu makes.",
}];

/// Omoio's own entry for a pack, `None` for one that waits for the user.
fn own(rules: &Rules) -> Option<&'static OwnPack> {
    OWN_PACKS.iter().find(|own| rules.path.eq_ignore_ascii_case(own.path))
}

// ---- which are on ----

/// A pack as the user left it in Omoio.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Kept {
    on: bool,
    #[serde(default)]
    choices: BTreeMap<String, String>,
}

fn kept_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().data_dir().map_err(|e| e.to_string())?.join("Omoio").join("cemu-packs.json"))
}

fn load_kept(app: &AppHandle) -> BTreeMap<String, Kept> {
    kept_path(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_kept(app: &AppHandle, kept: &BTreeMap<String, Kept>) -> Result<(), String> {
    let text = serde_json::to_string_pretty(kept).map_err(|e| e.to_string())?;
    std::fs::write(kept_path(app)?, text).map_err(|_| "Couldn't save that choice.".to_string())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn unescape(text: &str) -> String {
    quick_xml::escape::unescape(text).map(|t| t.into_owned()).unwrap_or_else(|_| text.to_string())
}

/// One pack's entry in settings.xml, where it sits and what it says.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    start: usize,
    end: usize,
    filename: String,
    disabled: bool,
    choices: BTreeMap<String, String>,
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let length = tag[at..].find('"')?;
    Some(unescape(&tag[at..at + length]))
}

fn inner(text: &str, name: &str) -> Option<String> {
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    let from = text.find(&open)? + open.len();
    let length = text[from..].find(&close)?;
    Some(unescape(text[from..from + length].trim()))
}

/// The entries under `GraphicPack`, as Cemu writes them (`CemuConfig.cpp`).
fn entries(text: &str) -> Vec<Entry> {
    let Ok((start, end)) = super::section(text, &["GraphicPack"]) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut at = start;
    while let Some(offset) = text[at..end].find("<Entry") {
        let open = at + offset;
        let Some(tag_length) = text[open..end].find('>') else {
            break;
        };
        let tag = &text[open..open + tag_length + 1];
        let (body, close) = if tag.ends_with("/>") {
            ("", open + tag.len())
        } else {
            let from = open + tag.len();
            let Some(length) = text[from..end].find("</Entry>") else {
                break;
            };
            (&text[from..from + length], from + length + "</Entry>".len())
        };
        let mut choices = BTreeMap::new();
        for preset in body.split("<Preset>").skip(1) {
            let preset = preset.split("</Preset>").next().unwrap_or_default();
            if let Some(name) = inner(preset, "preset") {
                choices.insert(inner(preset, "category").unwrap_or_default(), name);
            }
        }
        found.push(Entry {
            start: open,
            end: close,
            filename: attribute(tag, "filename").unwrap_or_default(),
            disabled: attribute(tag, "disabled").is_some_and(|value| is_on(&value)),
            choices,
        });
        at = close;
    }
    found
}

fn same_file(a: &str, b: &str) -> bool {
    a.replace('/', "\\").eq_ignore_ascii_case(&b.replace('/', "\\"))
}

/// settings.xml with one pack's entry as kept: on with its choices, off with
/// `disabled` for a pack Cemu would otherwise have on, or no entry at all for
/// one it wouldn't. Everything else in the file is left as it is.
fn with_entry(text: &str, key: &str, default_on: bool, kept: &Kept) -> String {
    let presets: String = kept
        .choices
        .iter()
        .map(|(category, preset)| {
            let category = if category.is_empty() {
                String::new()
            } else {
                format!("<category>{}</category>", escape(category))
            };
            format!("<Preset>{category}<preset>{}</preset></Preset>", escape(preset))
        })
        .collect();
    let entry = if kept.on {
        format!("<Entry filename=\"{}\">{presets}</Entry>", escape(key))
    } else if default_on {
        format!("<Entry filename=\"{}\" disabled=\"true\"/>", escape(key))
    } else {
        String::new()
    };
    if let Some(old) = entries(text).into_iter().find(|old| same_file(&old.filename, key)) {
        return format!("{}{entry}{}", &text[..old.start], &text[old.end..]);
    }
    if entry.is_empty() {
        return text.to_string();
    }
    match super::section(text, &["GraphicPack"]) {
        Ok((_, end)) => format!("{}{entry}\n{}", &text[..end], &text[end..]),
        Err(_) => {
            if let Some(at) = text.find("<GraphicPack/>") {
                return format!("{}<GraphicPack>{entry}</GraphicPack>{}", &text[..at], &text[at + "<GraphicPack/>".len()..]);
            }
            match text.rfind("</content>") {
                Some(at) => format!("{}<GraphicPack>{entry}</GraphicPack>\n{}", &text[..at], &text[at..]),
                None => text.to_string(),
            }
        }
    }
}

/// Whether Cemu has a pack on: as its entry says, or as its makers set it
/// when it has none.
fn cemu_has_on(rules: &Rules, entry: Option<&Entry>) -> bool {
    entry.map_or(rules.default_on, |entry| !entry.disabled)
}

/// How Omoio has a pack written into settings.xml: as the user left it in
/// Omoio, or, for one of Omoio's own the user hasn't touched there, on with
/// its default choices. `None` leaves the pack to Cemu, as it does one of
/// Omoio's own that Cemu already has on, so that entry isn't rewritten
/// before every game.
fn wanted(rules: &Rules, kept: Option<&Kept>, entry: Option<&Entry>) -> Option<Kept> {
    if let Some(kept) = kept {
        return Some(kept.clone());
    }
    if own(rules).is_none() || cemu_has_on(rules, entry) {
        return None;
    }
    Some(Kept {
        on: true,
        choices: settle(rules, &BTreeMap::new()).0,
    })
}

/// The user's choice for a pack, kept under its settings.xml name; for one
/// of Omoio's own also under its path, so switching it off outlasts its
/// folder moving in a later download.
fn kept_for<'a>(kept: &'a BTreeMap<String, Kept>, key: &str, rules: &Rules) -> Option<&'a Kept> {
    kept.get(key).or_else(|| own(rules).and_then(|own| kept.get(own.path)))
}

/// settings.xml with every pack Omoio has a say in written in. A kept pack
/// no longer in the download is left alone, as is everything else in the
/// file.
fn settings_with(text: &str, packs: &[(String, Rules)], kept: &BTreeMap<String, Kept>) -> String {
    let before = entries(text);
    let mut text = text.to_string();
    for (key, rules) in packs {
        let entry = before.iter().find(|entry| same_file(&entry.filename, key));
        if let Some(wanted) = wanted(rules, kept_for(kept, key, rules), entry) {
            text = with_entry(&text, key, rules.default_on, &wanted);
        }
    }
    text
}

/// settings.xml as the latest game started with it. Cemu reads its packs
/// only as a game starts, so a pack switched on or downloaded while the game
/// runs is in the file for the next start but not in this one.
static STARTED_WITH: Mutex<Option<String>> = Mutex::new(None);

/// Writes the packs Omoio has a say in into Cemu's settings.xml as a game
/// is about to start, and keeps what that game starts with. Called only
/// then; a change made in Omoio is written by `set`.
pub fn apply(app: &AppHandle) {
    let started_with = write(app);
    *STARTED_WITH.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = started_with;
}

/// Writes the packs the user changed in Omoio and Omoio's own into Cemu's
/// settings.xml. Returns the file as it now is, `None` when there is none.
fn write(app: &AppHandle) -> Option<String> {
    let settings = data_dir(app).ok()?.join("settings.xml");
    let before = std::fs::read_to_string(&settings).ok()?;
    let text = settings_with(&before, &all_packs(app), &load_kept(app));
    if text == before {
        return Some(before);
    }
    match std::fs::write(&settings, &text) {
        Ok(()) => Some(text),
        Err(_) => Some(before),
    }
}

/// Whether a pack is on, with the choices made in it, and whether anyone
/// changed it: in Omoio, as kept there, or else in Cemu, as its entry says.
/// One of Omoio's own the user hasn't touched in Omoio is on, since it is
/// written on before the next game.
fn state(rules: &Rules, kept: Option<&Kept>, entry: Option<&Entry>) -> (bool, BTreeMap<String, String>, bool) {
    if let Some(kept) = kept {
        return (kept.on, kept.choices.clone(), true);
    }
    if let Some(wanted) = wanted(rules, None, entry) {
        return (wanted.on, wanted.choices, false);
    }
    match entry {
        Some(entry) => (!entry.disabled, entry.choices.clone(), true),
        None => (rules.default_on, BTreeMap::new(), false),
    }
}

/// Why a pack is on without being asked. One of Omoio's own says why while
/// it is on, and what that costs once the user has switched it off.
fn on_because(rules: &Rules, on: bool, changed: bool) -> Option<String> {
    if let Some(own) = own(rules) {
        return Some(if on { own.reason } else { own.when_off }.to_string());
    }
    (on && rules.default_on && !changed).then(|| "On unless you turn it off, as its makers advise for this game.".to_string())
}

/// The packs for a game whose Cemu title id is `title`, `None` when it is
/// not known until the game has been played once.
pub fn view(app: &AppHandle, title: Option<&str>) -> Packs {
    let have_list = installed(app).is_some();
    let mut answer = Packs {
        have_list,
        source: SOURCE.to_string(),
        waiting: None,
        packs: Vec::new(),
    };
    if !have_list {
        return answer;
    }
    let Some(title) = title else {
        answer.waiting = Some("Play this game once, and its community packs can be chosen here.".to_string());
        return answer;
    };
    let kept = load_kept(app);
    let in_settings = data_dir(app)
        .ok()
        .and_then(|data| std::fs::read_to_string(data.join("settings.xml")).ok())
        .map(|text| entries(&text))
        .unwrap_or_default();
    for (key, rules) in packs_for(app, title) {
        let entry = in_settings.iter().find(|entry| same_file(&entry.filename, &key));
        let (on, asked, changed) = state(&rules, kept_for(&kept, &key, &rules), entry);
        let (chosen, visible) = settle(&rules, &asked);
        let (what, who) = about(&rules.description);
        answer.packs.push(Pack {
            id: key,
            name: rules.name.clone(),
            kind: kind(&rules.path).to_string(),
            about: what,
            by: who,
            on,
            applies: true,
            needs: None,
            on_because: on_because(&rules, on, changed),
            choices: choices(&rules, &chosen, &visible),
        });
    }
    const ORDER: [&str; 6] = ["Fixes", "Graphics", "Enhancements", "Mods", "Cheats", "Debug"];
    answer.packs.sort_by_key(|pack| (ORDER.iter().position(|kind| *kind == pack.kind), pack.name.to_lowercase()));
    answer
}

/// Switches a pack on or off with the choices made in it, keeps that, and
/// writes it into Cemu's settings.
pub fn set(app: &AppHandle, title: Option<&str>, change: &PackChange) -> Result<(), String> {
    let title = title.ok_or("Play this game once, and its community packs can be chosen here.")?;
    let (key, rules) = packs_for(app, title)
        .into_iter()
        .find(|(key, _)| *key == change.id)
        .ok_or("That pack isn't in the download any more. Download the packs again.")?;
    // Every choice is written out, picked or not, so Cemu uses exactly what
    // Omoio shows rather than working out its own.
    let (choices, _) = settle(&rules, &change.choices);
    let mut kept = load_kept(app);
    let chosen = Kept {
        on: change.on,
        choices: if change.on { choices } else { BTreeMap::new() },
    };
    if let Some(own) = own(&rules) {
        kept.insert(own.path.to_string(), chosen.clone());
    }
    kept.insert(key, chosen);
    save_kept(app, &kept)?;
    write(app);
    Ok(())
}

/// Whether Skylanders Imaginators takes the figures Cemu's figure maker
/// makes, which needs the Signature Patch on in the game that runs.
pub fn signature_patch(app: &AppHandle) -> MadeFigures {
    let started_with = STARTED_WITH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        // Omoio started after the game did, so the file is the best guide.
        .or_else(|| data_dir(app).ok().and_then(|data| std::fs::read_to_string(data.join("settings.xml")).ok()))
        .unwrap_or_default();
    made_figures(&all_packs(app), &load_kept(app), &started_with)
}

/// What `signature_patch` answers, from every pack in the download (none
/// before there is one), the packs as the user left them in Omoio, and the
/// settings.xml the game started with.
fn made_figures(packs: &[(String, Rules)], kept: &BTreeMap<String, Kept>, started_with: &str) -> MadeFigures {
    let found = packs.iter().find(|(_, rules)| rules.path.eq_ignore_ascii_case(SIGNATURE_PATCH));
    let check = match found {
        None if packs.is_empty() => Check::NotDownloaded,
        None => Check::Missing,
        Some((key, rules)) if kept_for(kept, key, rules).is_some_and(|kept| !kept.on) => Check::Off,
        Some((key, rules)) => {
            let entry = entries(started_with).into_iter().find(|entry| same_file(&entry.filename, key));
            if cemu_has_on(rules, entry.as_ref()) {
                Check::Passed
            } else {
                Check::NextStart
            }
        }
    };
    // Without the download, the last part of its path, which is also the
    // name its rules.txt gives.
    let pack = found.map_or_else(
        || SIGNATURE_PATCH.rsplit('/').next().unwrap_or(SIGNATURE_PATCH).to_string(),
        |(_, rules)| rules.name.clone(),
    );
    MadeFigures { check, pack }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: &str = r#"[Definition]
titleIds = 0005000010139200,0005000010140400
name = FPS
path = "Skylanders Swap Force/Mods/FPS"
description = Changes the game's FPS limit. Might have bugs.||Made by Mew00; added by TheSkyDude134.
version = 6

[Default]
$targetFPS:int = 60

# FPS Limit
[Preset]
name = 60 FPS
category = FPS Limit
$targetFPS:int = 60

[Preset]
name = 30 FPS (Default)
category = FPS Limit
default = 1
$targetFPS:int = 30
"#;

    #[test]
    fn a_pack_is_read_as_cemu_reads_it() {
        let rules = parse_rules(FPS).unwrap();
        assert_eq!(rules.name, "FPS");
        assert_eq!(rules.path, "Skylanders Swap Force/Mods/FPS");
        assert_eq!(rules.title_ids, ["0005000010139200", "0005000010140400"]);
        assert_eq!(rules.version, 6);
        assert!(!rules.default_on);
        assert_eq!(rules.defaults, [("$targetFPS".to_string(), 60.0)]);
        assert_eq!(rules.presets.len(), 2);
        assert!(rules.presets[1].is_default);
        assert_eq!(kind(&rules.path), "Mods");
        // A `;` inside the quoted description would be a comment outside it;
        // this one isn't quoted, so the line ends there, as in Cemu.
        assert_eq!(about(&rules.description).0, "Changes the game's FPS limit. Might have bugs.");
    }

    #[test]
    fn an_old_pack_or_one_without_titles_is_not_read() {
        assert!(parse_rules("[Definition]\ntitleIds = 1\npath = a/b\nversion = 2\n").is_none());
        assert!(parse_rules("[Definition]\npath = a/b\nversion = 6\n").is_none());
        assert!(parse_rules("[Preset]\nname = x\n[Definition]\ntitleIds = 1\npath = a\nversion = 6\n").is_none());
    }

    #[test]
    fn nothing_picked_takes_the_preset_marked_default() {
        let rules = parse_rules(FPS).unwrap();
        let (chosen, _) = settle(&rules, &BTreeMap::new());
        assert_eq!(chosen["FPS Limit"], "30 FPS (Default)");
        let asked = BTreeMap::from([("FPS Limit".to_string(), "60 FPS".to_string())]);
        assert_eq!(settle(&rules, &asked).0["FPS Limit"], "60 FPS");
        // A name the pack doesn't have falls back the same way.
        let asked = BTreeMap::from([("FPS Limit".to_string(), "999 FPS".to_string())]);
        assert_eq!(settle(&rules, &asked).0["FPS Limit"], "30 FPS (Default)");
    }

    #[test]
    fn presets_hide_and_show_with_what_else_is_picked() {
        let rules = parse_rules(
            r#"[Definition]
titleIds = 0005000010101c00
path = "Game/Graphics"
version = 6
[Preset]
name = 16:9
category = Aspect Ratio
$aspectRatioWidth = 16
$aspectRatioHeight = 9
[Preset]
name = 21:9
category = Aspect Ratio
$aspectRatioWidth = 21
$aspectRatioHeight = 9
[Preset]
name = 1920x1080
category = Resolution
condition = ((($aspectRatioWidth - 16) == 0) + (($aspectRatioHeight - 9) == 0)) == 2
[Preset]
name = 2560x1080
category = Resolution
condition = ((($aspectRatioWidth - 21) == 0) + (($aspectRatioHeight - 9) == 0)) == 2
"#,
        )
        .unwrap();
        let (chosen, visible) = settle(&rules, &BTreeMap::new());
        assert_eq!(chosen["Aspect Ratio"], "16:9");
        assert_eq!(chosen["Resolution"], "1920x1080");
        assert_eq!(visible, [true, true, true, false]);

        // Picking the wider screen hides the resolution picked before and
        // moves to the one that fits.
        let asked = BTreeMap::from([
            ("Aspect Ratio".to_string(), "21:9".to_string()),
            ("Resolution".to_string(), "1920x1080".to_string()),
        ]);
        let (chosen, visible) = settle(&rules, &asked);
        assert_eq!(chosen["Resolution"], "2560x1080");
        assert_eq!(visible, [true, true, false, true]);
        let shown = choices(&rules, &chosen, &visible);
        assert_eq!(shown[1].options, ["2560x1080"]);
    }

    #[test]
    fn expressions_follow_cemus_rules() {
        let constants = HashMap::from([("$a".to_string(), 7.0), ("$b".to_string(), 2.0)]);
        let whole = |text: &str| evaluate(text, &constants, true);
        let real = |text: &str| evaluate(text, &constants, false);
        assert_eq!(whole("$a / $b"), Some(3.0));
        assert_eq!(real("$a / $b"), Some(3.5));
        assert_eq!(whole("1 + 2 * 3"), Some(7.0));
        assert_eq!(whole("2 ^ 3 ^ 2"), Some(512.0));
        assert_eq!(whole("$a % $b == 1"), Some(1.0));
        assert_eq!(whole("-$b <= 0"), Some(1.0));
        assert_eq!(whole("$optionsshown <= 1"), None);
        assert_eq!(whole("(1 + 2"), None);
        assert_eq!(whole("1 / 0"), None);
        assert_eq!(real("0x10 + 0.5"), Some(16.5));
    }

    #[test]
    fn a_vars_value_can_use_the_ones_before_it() {
        let found = vars(&[
            ("$width".to_string(), "1280".to_string()),
            ("$height:int".to_string(), "$width * 9 / 16".to_string()),
        ]);
        assert_eq!(found, [("$width".to_string(), 1280.0), ("$height".to_string(), 720.0)]);
    }

    const SETTINGS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<content>\n\t<GraphicPack>\n\t\t<Entry filename=\"graphicPacks\\downloadedGraphicPacks\\A\\rules.txt\">\n\t\t\t<Preset>\n\t\t\t\t<category>FPS Limit</category>\n\t\t\t\t<preset>60 FPS</preset>\n\t\t\t</Preset>\n\t\t</Entry>\n\t\t<Entry filename=\"graphicPacks\\downloadedGraphicPacks\\B\\rules.txt\" disabled=\"true\"/>\n\t</GraphicPack>\n</content>\n";

    #[test]
    fn entries_are_read_as_cemu_writes_them() {
        let found = entries(SETTINGS);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].filename, r"graphicPacks\downloadedGraphicPacks\A\rules.txt");
        assert!(!found[0].disabled);
        assert_eq!(found[0].choices["FPS Limit"], "60 FPS");
        assert!(found[1].disabled);
        assert!(found[1].choices.is_empty());
    }

    #[test]
    fn an_entry_is_replaced_added_or_taken_away() {
        let on = Kept {
            on: true,
            choices: BTreeMap::from([("FPS Limit".to_string(), "30 FPS (Default)".to_string())]),
        };
        let a = r"graphicPacks\downloadedGraphicPacks\A\rules.txt";
        let b = r"graphicPacks\downloadedGraphicPacks\B\rules.txt";
        let text = with_entry(SETTINGS, a, false, &on);
        assert_eq!(entries(&text)[0].choices["FPS Limit"], "30 FPS (Default)");
        assert_eq!(entries(&text).len(), 2);

        // Off, a pack that is off without an entry loses it; one Cemu has on
        // by default is marked disabled.
        let off = Kept::default();
        let text = with_entry(SETTINGS, a, false, &off);
        assert_eq!(entries(&text).len(), 1);
        let text = with_entry(SETTINGS, a, true, &off);
        assert!(entries(&text).iter().any(|e| same_file(&e.filename, a) && e.disabled));

        // Turning the disabled one on.
        let text = with_entry(SETTINGS, b, true, &Kept { on: true, choices: BTreeMap::new() });
        assert!(entries(&text).iter().all(|e| !e.disabled));

        // Added to an empty list, and to a file without one.
        let empty = "<content>\n\t<GraphicPack/>\n</content>\n";
        assert_eq!(entries(&with_entry(empty, a, false, &on)).len(), 1);
        let none = "<content>\n\t<check_update>false</check_update>\n</content>\n";
        let text = with_entry(none, a, false, &on);
        assert_eq!(entries(&text).len(), 1);
        assert!(text.contains("<check_update>false</check_update>"));
    }

    #[test]
    fn a_packs_settings_name_is_its_path_from_cemus_data_folder() {
        let packs = Path::new("C:/x/portable/graphicPacks/downloadedGraphicPacks");
        assert_eq!(
            key_for(&packs.join("SkylandersSwapForce").join("Mods").join("FPS"), packs).unwrap(),
            r"graphicPacks\downloadedGraphicPacks\SkylandersSwapForce\Mods\FPS\rules.txt"
        );
    }

    /// The Signature Patch's rules.txt as the graphic packs' v987 has it,
    /// read 7 October 2026.
    const SIGNATURE: &str = "[Definition]\ntitleIds =  00050000101F4D00,00050000101FB100,0005000010205E00\nname = Signature Patch\npath = \"Skylanders Imaginators/Mods/Signature Patch\"\ndescription = This patch removes the check for a factory's signature on dumps of characters released for Skylanders Imaginators, allowing for unique UIDs and unreleased characters to be generated.||Made by Winner Nombre with help from MusicDisc, MoltenLavaCore, and NefariousTechSupport.\nversion = 7";

    const SIGNATURE_KEY: &str = r"graphicPacks\downloadedGraphicPacks\SkylandersImaginators\Mods\SignaturePatch\rules.txt";
    const A: &str = r"graphicPacks\downloadedGraphicPacks\A\rules.txt";
    const B: &str = r"graphicPacks\downloadedGraphicPacks\B\rules.txt";

    /// The two packs SETTINGS names, A the FPS pack and B one its makers
    /// have on, beside the Signature Patch.
    fn three_packs() -> Vec<(String, Rules)> {
        let mut on_by_default = parse_rules(FPS).unwrap();
        on_by_default.default_on = true;
        vec![
            (A.to_string(), parse_rules(FPS).unwrap()),
            (B.to_string(), on_by_default),
            (SIGNATURE_KEY.to_string(), parse_rules(SIGNATURE).unwrap()),
        ]
    }

    fn has_on(text: &str, key: &str) -> bool {
        entries(text).iter().any(|entry| same_file(&entry.filename, key) && !entry.disabled)
    }

    #[test]
    fn the_signature_patch_is_omoios_own_and_the_rest_are_not() {
        let signature = parse_rules(SIGNATURE).unwrap();
        assert!(own(&signature).is_some());
        assert!(!signature.default_on, "its makers leave it off");
        assert_eq!(kind(&signature.path), "Mods");
        assert!(own(&parse_rules(FPS).unwrap()).is_none());
        // Its path is matched whatever its case.
        let lower = SIGNATURE.replace("Skylanders Imaginators/Mods", "skylanders imaginators/mods");
        assert!(own(&parse_rules(&lower).unwrap()).is_some());
    }

    #[test]
    fn omoios_own_pack_is_written_on_and_the_others_are_left_as_they_were() {
        let text = settings_with(SETTINGS, &three_packs(), &BTreeMap::new());
        let found = entries(&text);
        assert_eq!(found.len(), 3);
        assert!(has_on(&text, SIGNATURE_KEY));
        assert_eq!(found[0].choices["FPS Limit"], "60 FPS");
        assert!(found[1].disabled);
        assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<content>\n"));

        // Written once: the next game finds nothing to change, nor in the
        // shape Cemu writes the entry back in.
        assert_eq!(settings_with(&text, &three_packs(), &BTreeMap::new()), text);
        let cemu = SETTINGS.replace("</GraphicPack>", &format!("<Entry filename=\"{SIGNATURE_KEY}\"/>\n\t</GraphicPack>"));
        assert_eq!(settings_with(&cemu, &three_packs(), &BTreeMap::new()), cemu);

        // Off in Cemu alone, Omoio writes it on again.
        let disabled = SETTINGS.replace("</GraphicPack>", &format!("<Entry filename=\"{SIGNATURE_KEY}\" disabled=\"true\"/>\n\t</GraphicPack>"));
        assert!(has_on(&settings_with(&disabled, &three_packs(), &BTreeMap::new()), SIGNATURE_KEY));

        // Added to a file with no packs on yet.
        let empty = "<content>\n\t<GraphicPack/>\n</content>\n";
        let text = settings_with(empty, &three_packs(), &BTreeMap::new());
        assert_eq!(entries(&text).len(), 1);
        assert!(has_on(&text, SIGNATURE_KEY));
    }

    #[test]
    fn omoios_own_pack_switched_off_in_omoio_stays_off() {
        let off = BTreeMap::from([(SIGNATURE_KEY.to_string(), Kept::default())]);
        assert_eq!(settings_with(SETTINGS, &three_packs(), &off), SETTINGS, "never written");
        let written = settings_with(SETTINGS, &three_packs(), &BTreeMap::new());
        let text = settings_with(&written, &three_packs(), &off);
        assert!(!entries(&text).iter().any(|entry| same_file(&entry.filename, SIGNATURE_KEY)));
        assert_eq!(entries(&text).len(), 2);

        // Switched on again, it is written as kept.
        let on = BTreeMap::from([(SIGNATURE_KEY.to_string(), Kept { on: true, choices: BTreeMap::new() })]);
        assert!(has_on(&settings_with(&text, &three_packs(), &on), SIGNATURE_KEY));

        // Off is kept under the pack's path too, so it stays off when a later
        // download moves its folder.
        let moved = r"graphicPacks\downloadedGraphicPacks\Imaginators\SignaturePatch\rules.txt";
        let packs = vec![(moved.to_string(), parse_rules(SIGNATURE).unwrap())];
        let off_by_path = BTreeMap::from([(SIGNATURE_PATCH.to_string(), Kept::default())]);
        assert_eq!(settings_with(SETTINGS, &packs, &off_by_path), SETTINGS, "never written");
        assert_eq!(made_figures(&packs, &off_by_path, SETTINGS).check, Check::Off);
        assert!(has_on(&settings_with(SETTINGS, &packs, &BTreeMap::new()), moved));
    }

    #[test]
    fn packs_the_user_changed_are_still_written_as_they_left_them() {
        let thirty = BTreeMap::from([("FPS Limit".to_string(), "30 FPS (Default)".to_string())]);
        let kept = BTreeMap::from([
            (A.to_string(), Kept { on: true, choices: thirty }),
            (B.to_string(), Kept { on: true, choices: BTreeMap::new() }),
            (r"graphicPacks\downloadedGraphicPacks\Gone\rules.txt".to_string(), Kept { on: true, choices: BTreeMap::new() }),
        ]);
        let text = settings_with(SETTINGS, &three_packs(), &kept);
        let found = entries(&text);
        assert_eq!(found[0].choices["FPS Limit"], "30 FPS (Default)");
        assert!(has_on(&text, B));
        assert!(has_on(&text, SIGNATURE_KEY));
        // A pack no longer in the download is left alone.
        assert!(!found.iter().any(|entry| entry.filename.contains("Gone")));
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn the_list_says_why_omoios_own_pack_is_on_and_what_off_costs() {
        let signature = parse_rules(SIGNATURE).unwrap();
        let (on, chosen, changed) = state(&signature, None, None);
        assert!(on && chosen.is_empty() && !changed);
        assert_eq!(on_because(&signature, on, changed).as_deref(), Some(OWN_PACKS[0].reason));

        let (on, _, changed) = state(&signature, Some(&Kept::default()), None);
        assert!(!on);
        assert_eq!(on_because(&signature, on, changed).as_deref(), Some(OWN_PACKS[0].when_off));

        // Off in Cemu alone, the list shows it on, as it will be next game.
        let disabled = Entry {
            start: 0,
            end: 0,
            filename: SIGNATURE_KEY.to_string(),
            disabled: true,
            choices: BTreeMap::new(),
        };
        assert!(state(&signature, None, Some(&disabled)).0);

        // A pack that waits for the user says nothing, and one its makers
        // have on says so until someone changes it.
        let fps = parse_rules(FPS).unwrap();
        assert_eq!(state(&fps, None, None), (false, BTreeMap::new(), false));
        assert_eq!(on_because(&fps, false, false), None);
        let mut advised = fps.clone();
        advised.default_on = true;
        assert!(on_because(&advised, true, false).is_some());
        assert!(on_because(&advised, true, true).is_none());
    }

    #[test]
    fn omoios_own_packs_say_why_in_plain_sentences() {
        const BANNED: [&str; 9] = ["seamless", "effortless", "powerful", "robust", "elevate", "unlock", "simply", "just", "leverage"];
        for own in OWN_PACKS {
            for line in [own.reason, own.when_off] {
                assert!(line.ends_with('.'), "{line}");
                assert!(!line.contains('\u{2014}') && !line.contains('!'), "{line}");
                let lower = line.to_lowercase();
                let mut words = lower.split(|c: char| !c.is_alphabetic());
                assert!(!words.any(|word| BANNED.contains(&word)), "{line}");
            }
        }
    }

    #[test]
    fn imaginators_takes_made_figures_only_with_the_patch_in_the_game_that_runs() {
        let none = BTreeMap::new();
        let written = settings_with(SETTINGS, &three_packs(), &none);
        let check = |packs: &[(String, Rules)], kept: &BTreeMap<String, Kept>, started_with: &str| {
            made_figures(packs, kept, started_with).check
        };

        // Not downloaded, or not in the download.
        assert_eq!(check(&[], &none, &written), Check::NotDownloaded);
        assert_eq!(check(&three_packs()[..2], &none, &written), Check::Missing);
        assert_eq!(made_figures(&[], &none, "").pack, "Signature Patch");

        assert_eq!(check(&three_packs(), &none, &written), Check::Passed);
        assert_eq!(made_figures(&three_packs(), &none, &written).pack, "Signature Patch");

        // On, but the game started before it was written in, as when the
        // packs were downloaded or it was switched on while the game ran.
        assert_eq!(check(&three_packs(), &none, SETTINGS), Check::NextStart);
        let on = BTreeMap::from([(SIGNATURE_KEY.to_string(), Kept { on: true, choices: BTreeMap::new() })]);
        assert_eq!(check(&three_packs(), &on, SETTINGS), Check::NextStart);
        assert_eq!(check(&three_packs(), &on, &written), Check::Passed);

        // Switched off in Omoio, whatever the game started with.
        let off = BTreeMap::from([(SIGNATURE_KEY.to_string(), Kept::default())]);
        assert_eq!(check(&three_packs(), &off, &written), Check::Off);
        assert_eq!(check(&three_packs(), &off, SETTINGS), Check::Off);
    }

    /// The Signature Patch in the download Omoio's Cemu has on this machine,
    /// found by its path. Skipped where the packs aren't downloaded.
    #[test]
    fn the_downloaded_signature_patch_is_found_by_its_path() {
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let packs = Path::new(&local).join("Omoio").join("cemu").join("portable").join(DOWNLOADED[0]).join(DOWNLOADED[1]);
        if !packs.is_dir() {
            return;
        }
        let mut found = Vec::new();
        walk(&packs, &mut found);
        let all: Vec<(String, Rules)> = found
            .iter()
            .filter_map(|dir| Some((key_for(dir, &packs)?, parse_rules(&std::fs::read_to_string(dir.join("rules.txt")).ok()?)?)))
            .collect();
        let ours: Vec<&(String, Rules)> = all.iter().filter(|(_, rules)| own(rules).is_some()).collect();
        assert_eq!(ours.len(), 1, "one pack is Omoio's own");
        let (key, rules) = ours[0];
        assert_eq!(rules.name, "Signature Patch");
        assert!(!rules.default_on);
        assert!(rules.title_ids.iter().any(|id| id == "00050000101f4d00"));
        println!("{key}");

        let none = BTreeMap::new();
        assert_eq!(
            made_figures(&all, &none, ""),
            MadeFigures {
                check: Check::NextStart,
                pack: "Signature Patch".to_string()
            }
        );
        // Written into settings with no packs on, it is the only entry.
        let text = settings_with("<content>\n\t<GraphicPack/>\n</content>\n", &all, &none);
        assert_eq!(entries(&text).len(), 1);
        assert_eq!(made_figures(&all, &none, &text).check, Check::Passed);
    }

    /// Reads every pack in a real download of the graphic packs, unpacked
    /// into a folder:
    ///   OMOIO_GRAPHIC_PACKS=<folder> cargo test real_graphic_packs -- --ignored --nocapture
    #[test]
    #[ignore = "needs OMOIO_GRAPHIC_PACKS pointing at unpacked graphic packs"]
    fn reads_every_pack_in_real_graphic_packs() {
        let root = std::env::var("OMOIO_GRAPHIC_PACKS").expect("set OMOIO_GRAPHIC_PACKS");
        let mut found = Vec::new();
        walk(Path::new(&root), &mut found);
        let mut unread = Vec::new();
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        for dir in &found {
            let text = std::fs::read_to_string(dir.join("rules.txt")).unwrap();
            match parse_rules(&text) {
                Some(rules) => {
                    let (chosen, visible) = settle(&rules, &BTreeMap::new());
                    let shown = choices(&rules, &chosen, &visible);
                    // Every choice shown has its pick among what is shown.
                    for choice in &shown {
                        assert!(choice.options.contains(&choice.chosen), "{}: {} has {:?}", dir.display(), choice.name, choice.chosen);
                    }
                    *kinds.entry(kind(&rules.path)).or_default() += 1;
                    if dir.ends_with("SkylandersSwapForce/Mods/FPS") || dir.ends_with("SkylandersSwapForce/Workarounds/PortalStabilityFix") {
                        println!("{}: on by default {}, choices {:?}", rules.name, rules.default_on, shown);
                    }
                }
                None => unread.push(dir.display().to_string()),
            }
        }
        println!("{} packs, {} not read: {:?}", found.len(), unread.len(), unread);
        println!("{kinds:?}");
        assert!(found.len() > 300);
    }

    /// Downloads the real packs from GitHub into a scratch folder, checked
    /// against GitHub's checksum, and reads them back:
    ///   cargo test real_download -- --ignored --nocapture
    #[test]
    #[ignore = "downloads the graphic packs from GitHub"]
    fn a_real_download_is_checked_and_put_in_place() {
        let into = std::env::temp_dir().join("omoio-graphic-packs-test").join("downloadedGraphicPacks");
        let _ = std::fs::remove_dir_all(into.parent().unwrap());
        let cancel = AtomicBool::new(false);
        let stages = std::sync::Mutex::new(Vec::new());
        let count = tauri::async_runtime::block_on(download_into(&into, &cancel, |stage, _, _| {
            let mut seen = stages.lock().unwrap();
            if seen.last() != Some(&stage.to_string()) {
                seen.push(stage.to_string());
            }
        }))
        .unwrap();
        println!("{count} packs, release {:?}, stages {:?}", std::fs::read_to_string(into.join("version.txt")).unwrap(), stages.lock().unwrap());
        assert!(count > 300);
        assert!(into.join("SkylandersSwapForce").join("Mods").join("FPS").join("rules.txt").is_file());
        // A second download replaces the first rather than mixing with it.
        let again = tauri::async_runtime::block_on(download_into(&into, &cancel, |_, _, _| {})).unwrap();
        assert_eq!(again, count);
        assert!(!into.with_extension("new").exists());
    }
}
