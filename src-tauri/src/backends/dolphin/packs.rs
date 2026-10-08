//! The patches and cheat codes Dolphin comes with, offered as a game's
//! community packs.
//!
//! Dolphin ships them in `Sys/GameSettings`, written by the people around
//! Dolphin and published with it under its licence, the GPL. Omoio reads
//! them from the Dolphin it installed and never copies them. A game's are
//! in the files named by the first letter of its id, its first three
//! letters, the whole id and the id with the disc's revision, read in that
//! order as one (`GetGameIniFilenames`, Core/ConfigLoaders/
//! GameConfigLoader.cpp; `SConfig::LoadDefaultGameIni`, Core/
//! ConfigManager.cpp; Dolphin 2609a, read 8 October 2026). Memory patches
//! are under `[OnFrame]`, Action Replay codes under `[ActionReplay]` and
//! Gecko codes under `[Gecko]`, each starting with a `$Name` line.
//!
//! Which are on is decided by name: `$Name` lines under `[OnFrame_Enabled]`
//! and `[OnFrame_Disabled]`, and the same for the others, first in
//! Dolphin's own files, then in the user's files of the same names in
//! `User/GameSettings` (`ReadEnabledAndDisabled`, Core/CheatCodes.h, called
//! by `LoadPatchSection` in Core/PatchEngine.cpp and `LoadCodes` in
//! Core/ActionReplay.cpp and Core/GeckoCodeConfig.cpp). Omoio writes what
//! Dolphin's own window writes when one is switched there, into the file
//! for the game's id, and leaves every other line of it alone.
//!
//! A patch Dolphin has on for a game stays on, and says so; every other
//! patch and code waits for the user. Action Replay and Gecko codes run only
//! with Dolphin's cheats on (`[Core] EnableCheats`, `Config::
//! AreCheatsEnabled`), so Omoio turns them on in the game's own file while
//! one of its codes is on, and nowhere else: with cheats on, Dolphin leaves
//! out a fix it makes for homebrew (`PatchFixedFunctions`, Core/HLE/HLE.cpp).
//!
//! The settings in the same files, such as `[Video_Hacks]`, Dolphin applies
//! by itself, so they aren't offered. The graphics mods Dolphin comes with
//! are offered after the patches and codes (mods.rs), and Dolphin's
//! graphics mods are turned on in the game's own file while one is on for
//! it, as its cheats are for codes.

use super::{ini, mods};
use crate::core::community::{Pack, PackChange, Packs};
use std::path::PathBuf;
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Patch,
    Gecko,
    ActionReplay,
}

impl Kind {
    /// In the order they are shown: patches mostly fix a game, codes change
    /// it.
    const ALL: [Kind; 3] = [Kind::Patch, Kind::Gecko, Kind::ActionReplay];

    /// The section a game's file keeps these in, which also names the lists
    /// of those on and off: `<section>_Enabled` and `<section>_Disabled`.
    fn section(self) -> &'static str {
        match self {
            Kind::Patch => "OnFrame",
            Kind::Gecko => "Gecko",
            Kind::ActionReplay => "ActionReplay",
        }
    }

    /// After the tabs Dolphin's own window shows them on.
    fn label(self) -> &'static str {
        match self {
            Kind::Patch => "Patches",
            Kind::Gecko => "Gecko codes",
            Kind::ActionReplay => "Action Replay codes",
        }
    }

    /// Codes run only with Dolphin's cheats on (`ActionReplay::ApplyCodes`,
    /// `Gecko::SetActiveCodes` in Core/GeckoCode.cpp); a patch always does
    /// (`ApplyPatches`, Core/PatchEngine.cpp).
    fn is_cheat(self) -> bool {
        self != Kind::Patch
    }

    /// Patches and Action Replay codes are read with comments taken out, so
    /// a `#` ends a name. Gecko codes only skip a line that starts with one.
    fn comments_out(self) -> bool {
        self != Kind::Gecko
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Code {
    kind: Kind,
    /// As Dolphin matches it in the lists of those on and off.
    name: String,
    /// Who wrote it, which a Gecko code gives in brackets after its name.
    creator: String,
    /// What a Gecko code does, from its `*` lines.
    notes: Vec<String>,
    /// Whether it came with Dolphin, rather than being one the user added in
    /// Dolphin's own window.
    shipped: bool,
    /// Whether Dolphin has it on by itself.
    default_on: bool,
    on: bool,
}

/// What decides a game's patches and codes: Dolphin's files and the user's
/// for the game, each in the order Dolphin reads them, and Dolphin's
/// settings for every game (Dolphin.ini).
#[derive(Debug, Clone)]
struct Files {
    shipped: Vec<String>,
    user: Vec<String>,
    /// Which of `user` is the file for the game's whole id, the one Omoio
    /// writes, as Dolphin's own window does.
    own: usize,
    settings: String,
    /// GFX.ini, Dolphin's graphics settings for every game.
    graphics: String,
}

const SOURCE: &str = "the Dolphin community";
const NOT_INSTALLED: &str = "Install Dolphin from the Emulators screen. Its patches and codes come with it.";
const ON_BY_DOLPHIN: &str = "On unless you turn it off, as Dolphin has it for this game.";
const GONE: &str = "That patch isn't in Dolphin's list for this game any more.";
const UNREADABLE: &str = "Couldn't read Dolphin's settings for this game, so Omoio left them as they are. Try again in a moment.";

/// Spaces and line breaks off both ends, as Dolphin's `StripWhitespace`
/// takes them.
fn strip(text: &str) -> &str {
    text.trim_matches([' ', '\t', '\r', '\n'])
}

/// A line as `IniFile::GetLines` hands it over: stripped, and with comments
/// taken out, cut at its first `#`. `None` for a line it drops.
fn tidy(line: &str, comments_out: bool) -> Option<&str> {
    let line = strip(line);
    if !comments_out {
        return Some(line);
    }
    match line.find('#') {
        Some(0) => None,
        Some(at) => Some(strip(&line[..at])),
        None => Some(line),
    }
}

/// A code from the line that starts it. A Gecko code's name ends at a `[`,
/// which opens its maker's name, and one written `+$Name` is on by itself:
/// the `+` and the character after it are skipped (`Gecko::LoadCodes`).
fn named(kind: Kind, line: &str, shipped: bool) -> Code {
    let on = kind == Kind::Gecko && line.starts_with('+');
    let rest = line.get(if on { 2 } else { 1 }..).unwrap_or_default();
    let (name, creator) = if kind != Kind::Gecko {
        (rest, "")
    } else {
        match rest.split_once('[') {
            Some((name, creator)) => (strip(name), creator.split(']').next().unwrap_or_default()),
            None => (strip(rest), ""),
        }
    };
    Code {
        kind,
        name: name.to_string(),
        creator: creator.to_string(),
        notes: Vec::new(),
        shipped,
        default_on: on,
        on,
    }
}

/// The codes in a section's lines, read as Dolphin reads them. One with no
/// name, or nothing under its name to apply, is left out.
fn read(kind: Kind, lines: &[String], shipped: bool) -> Vec<Code> {
    fn keep(found: &mut Vec<Code>, code: Option<Code>, body: bool) {
        if let Some(code) = code.filter(|code| body && !code.name.is_empty()) {
            found.push(code);
        }
    }
    let mut found = Vec::new();
    let mut current: Option<Code> = None;
    let mut body = false;
    for raw in lines {
        let Some(line) = tidy(raw, kind.comments_out()) else {
            continue;
        };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('$') || (kind == Kind::Gecko && line.starts_with('+')) {
            keep(&mut found, current.take(), body);
            body = false;
            current = Some(named(kind, line, shipped));
        } else if kind == Kind::Gecko && line.starts_with('*') {
            if let Some(code) = current.as_mut() {
                code.notes.push(strip(&line[1..]).to_string());
            }
        } else {
            body = true;
        }
    }
    keep(&mut found, current, body);
    found
}

/// Applies one set of files' lists of those on and off to every code read
/// so far, by exact name, the list of those on first.
fn apply_lists(codes: &mut [Code], on: &[String], off: &[String]) {
    for (lines, on) in [(on, true), (off, false)] {
        for line in lines {
            let Some(name) = strip(line).strip_prefix('$') else {
                continue;
            };
            for code in codes.iter_mut().filter(|code| code.name == name) {
                code.on = on;
            }
        }
    }
}

/// Every patch and code for a game, Dolphin's and the user's own, with
/// whether each is on as far as its lists go.
fn codes(files: &Files) -> Vec<Code> {
    let mut all = Vec::new();
    for kind in Kind::ALL {
        let mut found: Vec<Code> = Vec::new();
        for (texts, shipped) in [(&files.shipped, true), (&files.user, false)] {
            let section = |name: &str| -> Vec<String> { texts.iter().flat_map(|text| ini::lines(text, name)).collect() };
            found.extend(read(kind, &section(kind.section()), shipped));
            apply_lists(
                &mut found,
                &section(&format!("{}_Enabled", kind.section())),
                &section(&format!("{}_Disabled", kind.section())),
            );
            if shipped {
                for code in &mut found {
                    code.default_on = code.on;
                }
            }
        }
        all.extend(found);
    }
    all
}

/// Dolphin's reading of a yes or no (`TryParse`, Common/StringUtil.cpp).
fn is_true(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value.parse::<f32>().is_ok_and(|number| number == 1.0)
}

/// Whether one of Dolphin's switches is on for the game: `key` under
/// `section` in the game's files, and under `every_section` in `every_game`,
/// the settings for every game. The user's files for it come first, then
/// Dolphin's own, then the settings for every game, and the first that sets
/// it decides (`SEARCH_ORDER`, Common/Config/Enums.h); a value Dolphin can't
/// read counts as off (`GetUncached`, Common/Config/Config.h).
fn is_on(files: &Files, section: &str, every_game: &str, every_section: &str, key: &str) -> bool {
    files
        .user
        .iter()
        .rev()
        .chain(files.shipped.iter().rev())
        .find_map(|text| ini::get(text, section, key))
        .or_else(|| ini::get(every_game, every_section, key))
        .is_some_and(|value| is_true(&value))
}

/// Whether Dolphin's cheats are on for the game.
fn cheats_on(files: &Files) -> bool {
    is_on(files, "Core", &files.settings, "Core", "EnableCheats")
}

/// Whether Dolphin's graphics mods are on for the game: `[Settings]
/// EnableMods` in GFX.ini, which a game's file sets under `[Video_Settings]`
/// (`GetINIToSectionMap`, Core/ConfigLoaders/GameConfigLoader.cpp).
fn mods_on(files: &Files) -> bool {
    is_on(files, "Video_Settings", &files.graphics, "Settings", "EnableMods")
}

/// The game's own file with Dolphin's graphics mods on while any mod is on
/// for the game, and without the setting once none is, so Dolphin's own
/// choice for every game decides again.
fn mods_switched(files: &Files, any_on: bool) -> String {
    let own = &files.user[files.own];
    if any_on {
        ini::set(own, "Video_Settings", "EnableMods", "True")
    } else {
        ini::remove(own, "Video_Settings", "EnableMods")
    }
}

fn pack_id(code: &Code) -> String {
    format!("{}|{}", code.kind.section(), code.name)
}

/// The patches and codes that came with Dolphin, as packs. One the user
/// added in Dolphin's own window is theirs rather than the community's, and
/// is left out. A code shows as on only while it would run, which for one
/// that needs Dolphin's cheats means those are on too.
fn list(files: &Files) -> Vec<Pack> {
    let cheats = cheats_on(files);
    let mut seen = std::collections::HashSet::new();
    codes(files)
        .into_iter()
        .filter(|code| code.shipped && seen.insert(pack_id(code)))
        .map(|code| {
            let on = code.on && (cheats || !code.kind.is_cheat());
            let notes: Vec<&str> = code.notes.iter().map(|note| strip(note)).filter(|note| !note.is_empty()).collect();
            let creator = strip(&code.creator);
            Pack {
                id: pack_id(&code),
                kind: code.kind.label().to_string(),
                about: notes.join(" "),
                by: if creator.is_empty() { String::new() } else { format!("by {creator}") },
                on,
                applies: true,
                needs: None,
                on_because: (on && code.default_on).then(|| ON_BY_DOLPHIN.to_string()),
                choices: Vec::new(),
                name: code.name,
            }
        })
        .collect()
}

/// The game's own file with one patch or code switched, as Dolphin's window
/// would leave it: named in the list of those on or off only where that
/// differs from Dolphin's own choice (`SavePatchSection`,
/// `ActionReplay::SaveCodes`, `Gecko::SaveCodes`). Dolphin's cheats are on
/// in the file while any code that needs them is on, the user's own codes
/// included, and taken out of it once none is.
fn switched(files: &Files, id: &str, on: bool) -> Result<String, String> {
    let code = codes(files)
        .into_iter()
        .find(|code| code.shipped && pack_id(code) == id)
        .ok_or(GONE)?;
    let section = code.kind.section();
    let line = format!("${}", code.name);
    let (on_list, off_list) = (format!("{section}_Enabled"), format!("{section}_Disabled"));
    let mut text = ini::remove_line(&files.user[files.own], &on_list, &line);
    text = ini::remove_line(&text, &off_list, &line);
    if on != code.default_on {
        text = ini::add_line(&text, if on { &on_list } else { &off_list }, &line);
    }
    if code.kind.is_cheat() {
        let mut after = files.clone();
        after.user[files.own] = text.clone();
        let any = codes(&after).iter().any(|code| code.kind.is_cheat() && code.on);
        text = if any {
            ini::set(&text, "Core", "EnableCheats", "True")
        } else {
            ini::remove(&text, "Core", "EnableCheats")
        };
    }
    Ok(text)
}

/// The names of a game's files, in the order Dolphin reads them: the first
/// letter of its id, the first three, the whole id, then the id with the
/// disc's revision.
fn file_names(game_id: &str, revision: Option<u16>) -> Vec<String> {
    let mut names = Vec::new();
    if game_id.len() == 6 {
        names.extend([&game_id[..1], &game_id[..3]].map(|start| format!("{start}.ini")));
    }
    names.push(format!("{game_id}.ini"));
    if let Some(revision) = revision {
        names.push(format!("{game_id}r{revision}.ini"));
    }
    names
}

/// Where Dolphin keeps the files it comes with, beside itself.
fn shipped_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install::install_dir(app)?.join("Sys").join("GameSettings"))
}

fn user_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install::user_dir(app)?.join("GameSettings"))
}

fn load(app: &AppHandle, game_id: &str, revision: Option<u16>) -> Result<Files, String> {
    let (shipped, user) = (shipped_dir(app)?, user_dir(app)?);
    // Byte for byte, as Dolphin reads them (ini::read): one of Dolphin's
    // files has a stray byte outside UTF-8 in a comment. Only the game's own
    // file is ever written, so only it must be read whole; the others are
    // read as far as they can be.
    let read = |path: PathBuf| ini::read(&path).unwrap_or_default();
    let names = file_names(game_id, revision);
    let own_name = format!("{game_id}.ini");
    let own = names.iter().position(|name| *name == own_name).unwrap_or_default();
    let mut user_files: Vec<String> = names.iter().map(|name| read(user.join(name))).collect();
    user_files[own] = ini::read(&user.join(&own_name)).map_err(|_| UNREADABLE.to_string())?;
    let config = super::install::user_dir(app)?.join("Config");
    Ok(Files {
        shipped: names.iter().map(|name| read(shipped.join(name))).collect(),
        user: user_files,
        own,
        settings: read(config.join("Dolphin.ini")),
        graphics: read(config.join("GFX.ini")),
    })
}

/// The patches, codes and graphics mods that came with Dolphin and are for
/// a game, from Dolphin's own files, as packs.
fn everything(app: &AppHandle, game_id: &str, revision: Option<u16>) -> Result<Vec<Pack>, String> {
    let files = load(app, game_id, revision)?;
    let mut packs = list(&files);
    packs.extend(mods::list(&mods::load(app, game_id)?, mods_on(&files)));
    Ok(packs)
}

/// A game's patches, codes and graphics mods, for the Community packs list.
/// `title_id` is the library's, whose first six characters are the id
/// Dolphin files the game's settings under; `revision` is the disc's.
pub fn view(app: &AppHandle, title_id: &str, revision: Option<u16>) -> Packs {
    let have_list = shipped_dir(app).is_ok_and(|dir| dir.is_dir());
    let packs = match super::controllers::game_id(title_id) {
        Some(id) if have_list => everything(app, id, revision).unwrap_or_default(),
        _ => Vec::new(),
    };
    Packs {
        have_list,
        source: SOURCE.to_string(),
        waiting: (!have_list).then(|| NOT_INSTALLED.to_string()),
        with_emulator: true,
        packs,
    }
}

/// Switches one patch, code or graphics mod on or off for the game, in its
/// own files in Dolphin's user folder. Dolphin reads them as the game
/// starts.
pub fn set(app: &AppHandle, title_id: &str, revision: Option<u16>, change: &PackChange) -> Result<(), String> {
    let id = super::controllers::game_id(title_id).ok_or(GONE)?;
    let files = load(app, id, revision)?;
    let text = match change.id.strip_prefix(mods::ID) {
        Some(path) => {
            let found = mods::load(app, id)?;
            let (list, any_on) = mods::switched(&found, path, change.on)?;
            mods::save(app, &found, &list)?;
            mods_switched(&files, any_on)
        }
        None => switched(&files, &change.id, change.on)?,
    };
    if text == files.user[files.own] {
        return Ok(());
    }
    let failed = |_: std::io::Error| "Couldn't save that in Dolphin's settings for this game.".to_string();
    let path = user_dir(app)?.join(format!("{id}.ini"));
    if text.trim().is_empty() {
        return if path.exists() { std::fs::remove_file(&path).map_err(failed) } else { Ok(()) };
    }
    ini::write(&path, &text).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game's files in the shape Dolphin's own come in, made up for the
    /// test: three letters, the whole id, and the second revision.
    fn files(three: &str, whole: &str, revision: &str, user: &str, settings: &str) -> Files {
        Files {
            shipped: vec![String::new(), three.to_string(), whole.to_string(), revision.to_string()],
            user: vec![String::new(), String::new(), user.to_string(), String::new()],
            own: 2,
            settings: settings.to_string(),
            graphics: String::new(),
        }
    }

    const THREE: &str = "# GAME01, GAMP01 - A game\n\n[Core]\nCPUThread = False\n\n[OnFrame]\n# Add memory patches to be applied every frame here.\n$Fix startup hang\n0x801EF444:dword:0x480371ED\n$Widescreen\n0x80001234:dword:0x3FAAAAAB\n\n[Video_Hacks]\nEFBEmulateFormatChanges = True\n";

    const WHOLE: &str = "# GAME01 - A game\n\n[OnFrame_Enabled]\n$Fix startup hang\n\n[ActionReplay]\n# Add action replay cheats here.\n$Infinite health\n04001234 00000063\n$Max money # for the shop\n04005678 0000270F\n$Nothing under this one\n\n[Gecko]\n$60 FPS [Someone]\n*Runs the game at 60 frames a second.\n*Some cutscenes run fast.\n04001234 00000001\n$Debug menu #2\n04009999 00000001\n";

    const REVISION: &str = "[ActionReplay]\n$Only on revision 2\n04000000 00000001\n";

    fn names(packs: &[Pack]) -> Vec<&str> {
        packs.iter().map(|pack| pack.name.as_str()).collect()
    }

    fn pack<'a>(packs: &'a [Pack], name: &str) -> &'a Pack {
        packs.iter().find(|pack| pack.name == name).unwrap_or_else(|| panic!("{name} not offered"))
    }

    #[test]
    fn a_games_files_are_named_and_read_in_dolphins_order() {
        assert_eq!(file_names("SSPP52", Some(2)), ["S.ini", "SSP.ini", "SSPP52.ini", "SSPP52r2.ini"]);
        assert_eq!(file_names("SSPP52", None), ["S.ini", "SSP.ini", "SSPP52.ini"]);
    }

    #[test]
    fn the_patches_and_codes_come_from_every_file_and_nothing_else() {
        let packs = list(&files(THREE, WHOLE, REVISION, "", ""));
        assert_eq!(
            names(&packs),
            [
                "Fix startup hang",
                "Widescreen",
                "60 FPS",
                "Debug menu #2",
                "Infinite health",
                "Max money",
                "Only on revision 2"
            ],
            "patches, then Gecko codes, then Action Replay codes; a code with nothing under it and the video settings left out"
        );
        assert_eq!(pack(&packs, "Fix startup hang").kind, "Patches");
        assert_eq!(pack(&packs, "60 FPS").kind, "Gecko codes");
        assert_eq!(pack(&packs, "Infinite health").kind, "Action Replay codes");
        assert!(packs.iter().all(|pack| pack.applies && pack.choices.is_empty()));
    }

    #[test]
    fn a_gecko_code_names_its_maker_and_says_what_it_does() {
        let packs = list(&files(THREE, WHOLE, "", "", ""));
        let fps = pack(&packs, "60 FPS");
        assert_eq!(fps.by, "by Someone");
        assert_eq!(fps.about, "Runs the game at 60 frames a second. Some cutscenes run fast.");
        assert_eq!(fps.id, "Gecko|60 FPS");
        // A `#` ends an Action Replay code's name, as Dolphin reads it, and
        // not a Gecko code's.
        assert_eq!(pack(&packs, "Max money").id, "ActionReplay|Max money");
        assert_eq!(pack(&packs, "Debug menu #2").id, "Gecko|Debug menu #2");
    }

    #[test]
    fn a_patch_dolphin_has_on_is_on_and_says_why() {
        // Its patch is in one file and switched on in another, as Dolphin
        // reads them together.
        let packs = list(&files(THREE, WHOLE, "", "", ""));
        let fix = pack(&packs, "Fix startup hang");
        assert!(fix.on);
        assert_eq!(fix.on_because.as_deref(), Some(ON_BY_DOLPHIN));
        let wide = pack(&packs, "Widescreen");
        assert!(!wide.on, "every other patch waits for the user");
        assert_eq!(wide.on_because, None);
    }

    #[test]
    fn a_patch_dolphin_has_on_can_be_switched_off_and_back() {
        let user = "[Controls]\nWiimoteProfile1 = Omoio GAME01 1\n";
        let before = files(THREE, WHOLE, "", user, "");
        let off = switched(&before, "OnFrame|Fix startup hang", false).unwrap();
        assert_eq!(off, "[Controls]\nWiimoteProfile1 = Omoio GAME01 1\n[OnFrame_Disabled]\n$Fix startup hang\n");
        let mut after = before.clone();
        after.user[2] = off;
        let packs = list(&after);
        assert!(!pack(&packs, "Fix startup hang").on);
        assert_eq!(pack(&packs, "Fix startup hang").on_because, None);
        assert_eq!(switched(&after, "OnFrame|Fix startup hang", true).unwrap(), user, "back as it was");
    }

    #[test]
    fn a_code_switched_on_turns_on_dolphins_cheats_for_that_game_only() {
        let user = "[Controls]\nWiimoteSource0 = 1\n";
        let before = files(THREE, WHOLE, "", user, "[Core]\nEnableCheats = False\n");
        let on = switched(&before, "ActionReplay|Infinite health", true).unwrap();
        assert_eq!(
            on,
            "[Controls]\nWiimoteSource0 = 1\n[ActionReplay_Enabled]\n$Infinite health\n[Core]\nEnableCheats = True\n"
        );
        let mut after = before.clone();
        after.user[2] = on;
        assert!(pack(&list(&after), "Infinite health").on, "the game's own file wins over Dolphin's settings");

        let off = switched(&after, "ActionReplay|Infinite health", false).unwrap();
        assert_eq!(off, "[Controls]\nWiimoteSource0 = 1\n[Core]\n", "the controls stay, the code and cheats go");
    }

    #[test]
    fn cheats_stay_on_while_another_code_needs_them() {
        let user = "[Gecko]\n$My own code\n04000000 00000001\n[Gecko_Enabled]\n$My own code\n[Core]\nEnableCheats = True\n";
        let before = files(THREE, WHOLE, "", user, "");
        let packs = list(&before);
        assert!(!packs.iter().any(|pack| pack.name == "My own code"), "the user's own code isn't a pack");
        let on = switched(&before, "Gecko|60 FPS", true).unwrap();
        let mut after = before.clone();
        after.user[2] = on;
        let off = switched(&after, "Gecko|60 FPS", false).unwrap();
        assert_eq!(off, user, "the user's own code still needs the cheats");
    }

    #[test]
    fn a_code_is_on_only_while_dolphins_cheats_are() {
        let user = "[ActionReplay_Enabled]\n$Infinite health\n";
        assert!(!pack(&list(&files(THREE, WHOLE, "", user, "")), "Infinite health").on);
        let cheats = "[Core]\nEnableCheats = True\n";
        assert!(pack(&list(&files(THREE, WHOLE, "", user, cheats)), "Infinite health").on);
        // A patch doesn't need them.
        let user = "[OnFrame_Enabled]\n$Widescreen\n";
        assert!(pack(&list(&files(THREE, WHOLE, "", user, "")), "Widescreen").on);
    }

    #[test]
    fn graphics_mods_are_on_for_the_game_while_one_is() {
        let user = "[Controls]\nWiimoteSource0 = 1\n";
        let before = files(THREE, WHOLE, "", user, "");
        assert!(!mods_on(&before), "off unless something turns them on");
        let on = mods_switched(&before, true);
        assert_eq!(on, "[Controls]\nWiimoteSource0 = 1\n[Video_Settings]\nEnableMods = True\n");
        let mut after = before.clone();
        after.user[2] = on;
        assert!(mods_on(&after));
        assert!(!cheats_on(&after), "the other switch stays as it was");
        assert_eq!(mods_switched(&after, false), "[Controls]\nWiimoteSource0 = 1\n[Video_Settings]\n");
        // Dolphin's settings for every game count where no file for the
        // game says otherwise.
        let mut everywhere = before.clone();
        everywhere.graphics = "[Settings]\nInternalResolution = 4\nEnableMods = True\n".to_string();
        assert!(mods_on(&everywhere));
        everywhere.user[2] = "[Video_Settings]\nEnableMods = False\n".to_string();
        assert!(!mods_on(&everywhere), "the game's own file wins");
    }

    #[test]
    fn a_gecko_code_marked_with_a_plus_is_on_by_itself() {
        let whole = "[Gecko]\n+$Fix [Someone]\n04000000 00000001\n";
        let cheats = "[Core]\nEnableCheats = 1\n";
        let fix = list(&files("", whole, "", "", cheats)).remove(0);
        assert_eq!(fix.name, "Fix");
        assert!(fix.on);
        assert_eq!(fix.on_because.as_deref(), Some(ON_BY_DOLPHIN));
    }

    /// Reads the files the Dolphin Omoio installs comes with. Set
    /// OMOIO_DOLPHIN_SYS to its `Sys/GameSettings` folder to run this.
    #[test]
    #[ignore]
    fn reads_the_files_dolphin_comes_with() {
        let dir = PathBuf::from(std::env::var("OMOIO_DOLPHIN_SYS").expect("set OMOIO_DOLPHIN_SYS"));
        let read = |name: &String| {
            std::fs::read(dir.join(name)).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default()
        };
        let (mut games, mut offered, mut on) = (0, 0, 0);
        for entry in std::fs::read_dir(&dir).expect("the folder").flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = name.strip_suffix(".ini").filter(|id| id.len() == 6) else {
                continue;
            };
            let names = file_names(id, None);
            let files = Files {
                shipped: names.iter().map(read).collect(),
                user: vec![String::new(); names.len()],
                own: 2,
                settings: String::new(),
                graphics: String::new(),
            };
            let packs = list(&files);
            // Every patch Dolphin's file for the game switches on by name is
            // one read here, so the names are read as Dolphin reads them.
            for line in ini::lines(&files.shipped[2], "OnFrame_Enabled") {
                let Some(name) = strip(&line).strip_prefix('$') else {
                    continue;
                };
                let wanted = format!("OnFrame|{name}");
                assert!(packs.iter().any(|pack| pack.id == wanted && pack.on), "{id}: {wanted}");
            }
            games += 1;
            offered += packs.len();
            on += packs.iter().filter(|pack| pack.on).count();
        }
        println!("{games} games, {offered} patches and codes, {on} on by themselves");
        assert!(games > 300 && offered > 1000);
        // The Skylanders games on the Wii, and two others for comparison.
        for id in ["SSPP52", "SKYE52", "SVXE52", "SK8E52", "SKNE52", "GZLE01", "RMCE01"] {
            let names = file_names(id, None);
            let files = Files {
                shipped: names.iter().map(read).collect(),
                user: vec![String::new(); names.len()],
                own: 2,
                settings: String::new(),
                graphics: String::new(),
            };
            let packs = list(&files);
            let count = |kind: Kind| packs.iter().filter(|pack| pack.kind == kind.label()).count();
            println!(
                "{id}: {} patches, {} Gecko codes, {} Action Replay codes",
                count(Kind::Patch),
                count(Kind::Gecko),
                count(Kind::ActionReplay)
            );
        }
    }

    #[test]
    fn a_pack_no_longer_listed_is_refused() {
        let before = files(THREE, WHOLE, "", "", "");
        assert_eq!(switched(&before, "ActionReplay|Nothing under this one", true), Err(GONE.to_string()));
        assert_eq!(switched(&before, "Gecko|Infinite health", true), Err(GONE.to_string()));
    }
}
