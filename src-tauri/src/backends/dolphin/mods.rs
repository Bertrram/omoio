//! The graphics mods Dolphin comes with, offered as a game's community packs
//! beside its patches and codes (packs.rs).
//!
//! Dolphin ships them in `Sys/Load/GraphicMods`, a folder each, made by its
//! team and the people around it and published with it under its licence,
//! the GPL. Omoio reads them from the Dolphin it installed and never copies
//! them. A mod's `metadata.json` names things a game draws, such as its
//! bloom, as a group (`groups`), says what to do with a group of a name
//! (`features`), or both: Skylanders: Spyro's Adventure and Giants each have
//! a mod that names their "Bloom", and "Bloom Removal", "Bloom Blurred" and
//! "Native Resolution Bloom", made for every game, act on a group of that
//! name.
//!
//! A game's mods are, in this order, those in the game's list, then each
//! `metadata.json` in the user's folder of mods and then in Dolphin's, in a
//! folder named by the game's id or else its first three letters, or in one
//! holding, at any depth, a `.txt` file named by the id, its first three
//! letters or `all`; then they are put in order of weight
//! (`GraphicsModGroupConfig::Load`, VideoCommon/GraphicsModSystem/Config/
//! GraphicsModGroup.cpp; `GetTextureDirectoriesWithGameId`, VideoCommon/
//! HiresTextures.cpp; Dolphin 2609a, read 8 October 2026). Dolphin's window
//! lists only a mod that acts on a group one of them names
//! (`GraphicsModListWidget::RefreshModList`, DolphinQt/Config), so a mod
//! for every game shows only for a game it changes. Omoio offers those that
//! came with Dolphin the same way.
//!
//! A mod is switched on for one game alone: the game's list,
//! `User/Config/GraphicMods/<id>.json`, keeps each mod with `enabled`
//! (`GraphicsModConfig::SerializeToProfile`, Config/GraphicsMod.cpp), and
//! Dolphin does what a mod says only while it is enabled there
//! (`DecoratedAction`, Runtime/GraphicsModManager.cpp). A mod that only
//! names groups does nothing by itself, and its groups count whether or not
//! it is enabled. Mods run at all only with Dolphin's graphics mods on
//! (`[Settings] EnableMods` in GFX.ini, off by default, Core/Config/
//! GraphicsSettings.cpp), which Dolphin's window for one game's settings
//! writes as `[Video_Settings] EnableMods` in the game's own file
//! (`GetINILocationFromConfig`, Core/ConfigLoaders/GameConfigLoader.cpp).
//! Omoio writes a mod into the game's list as Dolphin's window does, leaving
//! the list's other entries as they are, and packs.rs turns the setting on
//! in the game's own file while any mod is on for the game.

use crate::core::community::Pack;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// How a graphics mod's pack id starts: after it comes where the mod's
/// `metadata.json` is in Dolphin's folder of mods.
pub const ID: &str = "GraphicMods|";

/// After the tab Dolphin's own window shows them on.
const KIND: &str = "Graphics mods";
const GONE: &str = "That graphics mod isn't in Dolphin's list for this game any more.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    /// `User/Load/GraphicMods`, where the user puts mods of their own.
    User,
    /// `Sys/Load/GraphicMods`, the ones Dolphin comes with.
    System,
}

impl Source {
    /// As the game's list names it. Anything but `system` there counts as
    /// the user's (`GraphicsModConfig::Create`).
    fn name(self) -> &'static str {
        match self {
            Source::User => "user",
            Source::System => "system",
        }
    }
}

/// One of the folders of mods: every file below it, by its path with `/`
/// between names, and the text of each `.json` file.
#[derive(Debug, Clone)]
struct Place {
    source: Source,
    files: BTreeMap<String, String>,
}

/// What decides a game's graphics mods.
#[derive(Debug, Clone)]
pub struct Found {
    game_id: String,
    /// The user's folder of mods, then Dolphin's.
    places: Vec<Place>,
    /// The game's list, when there is one.
    list: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct Mod {
    source: Source,
    /// Where its `metadata.json` is in its folder of mods, as the game's
    /// list names it.
    path: String,
    title: String,
    author: String,
    description: String,
    /// The groups it names, each with how many things it names in it.
    groups: Vec<(String, usize)>,
    /// The group each thing it does acts on.
    features: Vec<String>,
    enabled: bool,
    /// Where it comes in the order mods are applied, lowest first.
    weight: u16,
}

/// The kinds of things a group can name (`DeserializeTargetFromConfig`,
/// Config/GraphicsTarget.cpp).
const TARGET_TYPES: [&str; 6] = ["draw_started", "load_texture", "create_texture", "efb", "xfb", "projection"];

/// The items of an array Dolphin reads from `key`, none when it isn't an
/// array, and `None` when one of them isn't an object, which makes Dolphin
/// pass over the whole mod.
fn objects<'a>(value: &'a Value, key: &str) -> Option<Vec<&'a Map<String, Value>>> {
    match value.get(key) {
        Some(Value::Array(items)) => items.iter().map(Value::as_object).collect(),
        _ => Some(Vec::new()),
    }
}

/// A value Dolphin takes only as text, when it is there: `Err` when it is
/// something else.
fn text_at(object: &Map<String, Value>, key: &str) -> Result<String, ()> {
    match object.get(key) {
        None => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(_) => Err(()),
    }
}

/// A mod from its `metadata.json`, read as Dolphin reads one
/// (`GraphicsModConfig::DeserializeFromConfig` and the `DeserializeFromConfig`
/// of each part): the words about it where they are text, and a group, a
/// target, a feature or an asset of the wrong shape makes Dolphin pass over
/// the whole mod. A target is checked for its kind only; Dolphin also checks
/// what each kind needs, which the mods it comes with all have.
fn read_mod(source: Source, path: &str, text: &str) -> Option<Mod> {
    let root: Value = serde_json::from_str(text).ok()?;
    root.as_object()?;
    let words = |key: &str| {
        root.get("meta")
            .and_then(|meta| meta.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let mut groups = Vec::new();
    for group in objects(&root, "groups")? {
        let name = text_at(group, "name").ok()?;
        let targets = match group.get("targets") {
            None => 0,
            Some(Value::Array(targets)) => {
                let known = |target: &Value| {
                    target.get("type").and_then(Value::as_str).is_some_and(|kind| TARGET_TYPES.contains(&kind))
                };
                if !targets.iter().all(|target| target.is_object() && known(target)) {
                    return None;
                }
                targets.len()
            }
            Some(_) => return None,
        };
        groups.push((name, targets));
    }
    let mut features = Vec::new();
    for feature in objects(&root, "features")? {
        text_at(feature, "action").ok()?;
        features.push(text_at(feature, "group").ok()?);
    }
    objects(&root, "assets")?;
    Some(Mod {
        source,
        path: path.to_string(),
        title: words("title"),
        author: words("author"),
        description: words("description"),
        groups,
        features,
        enabled: false,
        weight: 0,
    })
}

/// A value of the game's list as text, as Dolphin takes `source` and
/// `path` there, whatever they are.
fn as_text(value: &Value) -> String {
    value.as_str().map_or_else(|| value.to_string(), str::to_string)
}

/// Which mod an entry of the game's list names, if it names one.
fn named(entry: &Map<String, Value>) -> Option<(Source, String)> {
    let source = if as_text(entry.get("source")?) == "system" { Source::System } else { Source::User };
    Some((source, as_text(entry.get("path")?).replace('\\', "/")))
}

/// Takes what the game's list says about a mod: whether it is on and its
/// weight. Dolphin reads neither when the list counts the mod's groups or
/// features differently from the mod (`DeserializeFromProfile`), as after
/// the mod has changed.
fn take_list(mod_: &mut Mod, entry: &Map<String, Value>) {
    let count = |key: &str| entry.get(key).and_then(Value::as_array).map(Vec::len);
    if count("groups").is_some_and(|n| n != mod_.groups.len()) || count("features").is_some_and(|n| n != mod_.features.len()) {
        return;
    }
    if let Some(on) = entry.get("enabled").and_then(Value::as_bool) {
        mod_.enabled = on;
    }
    if let Some(weight) = entry.get("weight").and_then(Value::as_f64) {
        mod_.weight = weight as u16;
    }
}

/// Whether a file is a mod's `metadata.json`: that name, the ending in any
/// case (`DoFileSearch`, Common/FileSearch.cpp).
fn is_metadata(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.len() == "metadata.json".len() && name.starts_with("metadata") && name[8..].eq_ignore_ascii_case(".json")
}

/// The folders in a folder of mods that are for the game: the one named by
/// its id, or else by its first three letters, and any holding, at any
/// depth, a `.txt` file named by either or `all` (`GetTextureDirectoriesWithGameId`).
/// A `.txt` file outside any folder counts for nothing.
fn folders(place: &Place, game_id: &str) -> BTreeSet<String> {
    let three = game_id.get(..3).unwrap_or(game_id);
    let tops = || place.files.keys().filter_map(|path| path.split_once('/').map(|(top, _)| top));
    let mut found = BTreeSet::new();
    let folder_named = |name: &str| tops().find(|top| top.eq_ignore_ascii_case(name)).map(str::to_string);
    if let Some(top) = folder_named(game_id).or_else(|| folder_named(three)) {
        found.insert(top);
    }
    for path in place.files.keys() {
        let Some((top, _)) = path.split_once('/') else {
            continue;
        };
        let name = path.rsplit('/').next().unwrap_or(path);
        let Some(stem) = name.len().checked_sub(4).filter(|&at| name.is_char_boundary(at)).map(|at| name.split_at(at)) else {
            continue;
        };
        if stem.1.eq_ignore_ascii_case(".txt") && [game_id, three, "all"].contains(&stem.0) {
            found.insert(top.to_string());
        }
    }
    found
}

/// The game's mods, as Dolphin finds them with `list` as the game's list.
/// A list Dolphin can't read leaves the game with no mods at all.
fn mods(found: &Found, list: Option<&str>) -> Vec<Mod> {
    let mut mods = Vec::new();
    let mut known = BTreeSet::new();
    if let Some(list) = list {
        let Ok(Value::Object(root)) = serde_json::from_str::<Value>(list) else {
            return Vec::new();
        };
        for entry in root.get("mods").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_object) {
            let Some((source, path)) = named(entry) else {
                continue;
            };
            let text = found.places.iter().find(|place| place.source == source).and_then(|place| place.files.get(&path));
            if let Some(mut mod_) = text.and_then(|text| read_mod(source, &path, text)) {
                take_list(&mut mod_, entry);
                known.insert((source, path));
                mods.push(mod_);
            }
        }
    }
    for place in &found.places {
        for folder in folders(place, &found.game_id) {
            let inside = format!("{folder}/");
            let files = place.files.range(inside.clone()..).take_while(|(path, _)| path.starts_with(&inside));
            for (path, text) in files.filter(|(path, _)| is_metadata(path)) {
                // Dolphin stops looking in a folder at a mod the list has
                // (a `return` in `try_add_mod`).
                if known.contains(&(place.source, path.clone())) {
                    break;
                }
                mods.extend(read_mod(place.source, path, text));
            }
        }
    }
    mods.sort_by_key(|mod_| mod_.weight);
    mods
}

/// The mods that came with Dolphin that its window lists for the game, once
/// each: those acting on a group one of the game's mods names.
fn offered(mods: &[Mod]) -> Vec<&Mod> {
    let groups: BTreeSet<&str> = mods.iter().flat_map(|mod_| mod_.groups.iter().map(|(name, _)| name.as_str())).collect();
    let mut seen = BTreeSet::new();
    mods.iter()
        .filter(|mod_| mod_.source == Source::System && mod_.features.iter().any(|group| groups.contains(group.as_str())))
        .filter(|mod_| seen.insert(mod_.path.as_str()))
        .collect()
}

/// The graphics mods for the game that came with Dolphin, as packs. One is
/// on only while it would run: on in the game's list, with Dolphin's
/// graphics mods on for the game (`mods_on`).
pub fn list(found: &Found, mods_on: bool) -> Vec<Pack> {
    let mods = mods(found, found.list.as_deref());
    offered(&mods)
        .into_iter()
        .map(|mod_| {
            let enabled = mods.iter().any(|each| each.source == mod_.source && each.path == mod_.path && each.enabled);
            let folder = mod_.path.split('/').next().unwrap_or_default();
            Pack {
                id: format!("{ID}{}", mod_.path),
                name: if mod_.title.is_empty() { folder.to_string() } else { mod_.title.clone() },
                kind: KIND.to_string(),
                about: mod_.description.clone(),
                by: if mod_.author.is_empty() { String::new() } else { format!("by {}", mod_.author) },
                on: mods_on && enabled,
                applies: true,
                needs: None,
                on_because: None,
                choices: Vec::new(),
            }
        })
        .collect()
}

/// A mod as Dolphin's window writes it into the game's list
/// (`GraphicsModConfig::SerializeToProfile`): an empty object for each
/// thing it does and for each thing its groups name, which is all those
/// write (`SerializeToProfile` in GraphicsModFeature.cpp and
/// GraphicsTargetGroup.cpp, `SerializeTargetToProfile` in
/// GraphicsTarget.cpp).
fn entry(mod_: &Mod) -> Value {
    let groups: Vec<Value> = mod_.groups.iter().map(|(_, targets)| json!({ "targets": vec![json!({}); *targets] })).collect();
    json!({
        "source": mod_.source.name(),
        "path": mod_.path,
        "groups": groups,
        "features": vec![json!({}); mod_.features.len()],
        "enabled": mod_.enabled,
        "weight": mod_.weight,
    })
}

/// The game's list with one of Dolphin's mods switched, as Dolphin's window
/// leaves it: the mod's entry written afresh where the list has it, or
/// added at the end, and every other entry as it was. Dolphin writes the
/// list as its JSON reader prints it, keys in order, two spaces deep
/// (`picojson::value::serialize`). With whether any of the game's mods is
/// on after it, the user's own included.
pub fn switched(found: &Found, path: &str, on: bool) -> Result<(String, bool), String> {
    let mods = mods(found, found.list.as_deref());
    let mut chosen = offered(&mods).into_iter().find(|mod_| mod_.path == path).cloned().ok_or(GONE)?;
    chosen.enabled = on;
    let fresh = entry(&chosen);
    // A list Dolphin can't read has no mods, so none is chosen from it.
    let mut root = match found.list.as_deref().map(serde_json::from_str::<Value>) {
        Some(Ok(Value::Object(root))) => root,
        _ => Map::new(),
    };
    let names_it = |each: &Value| each.as_object().and_then(named) == Some((chosen.source, chosen.path.clone()));
    match root.get_mut("mods") {
        Some(Value::Array(entries)) => {
            let mut written = false;
            for each in entries.iter_mut().filter(|each| names_it(each)) {
                *each = fresh.clone();
                written = true;
            }
            if !written {
                entries.push(fresh);
            }
        }
        _ => {
            root.insert("mods".to_string(), Value::Array(vec![fresh]));
        }
    }
    let mut text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    text.push('\n');
    let any = mods_after(found, &text);
    Ok((text, any))
}

fn mods_after(found: &Found, list: &str) -> bool {
    mods(found, Some(list)).iter().any(|mod_| mod_.enabled)
}

/// Every file below a folder of mods, with the text of each `.json` file.
fn place(source: Source, root: &Path) -> Place {
    let mut files = BTreeMap::new();
    let mut folders = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, above)) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("{above}{name}");
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => folders.push((entry.path(), format!("{path}/"))),
                Ok(_) => {
                    let is_json = name.len() > 5 && name.is_char_boundary(name.len() - 5) && name[name.len() - 5..].eq_ignore_ascii_case(".json");
                    let text = if is_json { std::fs::read_to_string(entry.path()).unwrap_or_default() } else { String::new() };
                    files.insert(path, text);
                }
                Err(_) => {}
            }
        }
    }
    Place { source, files }
}

/// Where Dolphin keeps a game's list of mods.
fn list_path(user: &Path, game_id: &str) -> PathBuf {
    user.join("Config").join("GraphicMods").join(format!("{game_id}.json"))
}

/// Reads what decides a game's graphics mods from the Dolphin Omoio
/// installed.
pub fn load(app: &AppHandle, game_id: &str) -> Result<Found, String> {
    let user = super::install::user_dir(app)?;
    let own = super::install::install_dir(app)?.join("Sys").join("Load").join("GraphicMods");
    Ok(Found {
        game_id: game_id.to_string(),
        places: vec![
            place(Source::User, &user.join("Load").join("GraphicMods")),
            place(Source::System, &own),
        ],
        list: read_list(&list_path(&user, game_id))?,
    })
}

/// The game's list of mods, `None` when it has none. A list that is there
/// but can't be read is an error rather than no list: switching a mod would
/// otherwise write a new list over it, the user's own mods gone from it.
fn read_list(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Couldn't read Dolphin's list of graphics mods for this game, so Omoio left it as it is.".to_string()),
    }
}

/// Writes the game's list, unless it says what it said before.
pub fn save(app: &AppHandle, found: &Found, list: &str) -> Result<(), String> {
    let before = found.list.as_deref().and_then(|text| serde_json::from_str::<Value>(text).ok());
    if before.is_some() && before == serde_json::from_str::<Value>(list).ok() {
        return Ok(());
    }
    let path = list_path(&super::install::user_dir(app)?, &found.game_id);
    let failed = |_: std::io::Error| "Couldn't save that in Dolphin's list of graphics mods for this game.".to_string();
    super::ini::replace(&path, list.as_bytes()).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mod that names a game's bloom, as the ones for a game come, made
    /// up for the test.
    const BLOOM_NAMED: &str = r#"{ "meta": { "title": "Bloom Texture Definitions", "author": "Someone" },
        "groups": [ { "name": "Bloom", "targets": [ { "type": "efb", "texture_filename": "efb1_n000008_80x57_6" } ] } ] }"#;

    /// Mods for every game, as Dolphin's come.
    const NO_BLOOM: &str = r#"{ "meta": { "title": "Bloom Removal", "author": "Dolphin Team", "description": "Skips drawing bloom effects." },
        "features": [ { "group": "Bloom", "action": "skip" } ] }"#;
    const SOFT_BLOOM: &str = r#"{ "meta": { "title": "Bloom Blurred", "author": "Dolphin Team" },
        "features": [ { "group": "Bloom", "action": "custom_pipeline", "action_data": { "material_asset": "blur" } } ],
        "assets": [ { "name": "blur", "data": { "metadata": "blur.rastermaterial" } } ] }"#;
    const NO_HUD: &str = r#"{ "meta": { "title": "Remove HUD", "author": "Dolphin Team" },
        "features": [ { "group": "HUD", "action": "skip" } ] }"#;

    fn files(list: &[(&str, &str)]) -> BTreeMap<String, String> {
        list.iter().map(|(path, text)| (path.to_string(), text.to_string())).collect()
    }

    /// Dolphin's folder of mods, made up: one game's bloom named in a
    /// folder with its three letters' `.txt`, three mods for every game,
    /// and another game's bloom.
    fn dolphins() -> Place {
        Place {
            source: Source::System,
            files: files(&[
                ("A Game/GAM.txt", ""),
                ("A Game/metadata.json", BLOOM_NAMED),
                ("All Games Bloom Removal/all.txt", ""),
                ("All Games Bloom Removal/metadata.json", NO_BLOOM),
                ("All Games Blurred Bloom/all.txt", ""),
                ("All Games Blurred Bloom/blur.rastermaterial", ""),
                ("All Games Blurred Bloom/metadata.json", SOFT_BLOOM),
                ("All Games HUD Removal/all.txt", ""),
                ("All Games HUD Removal/metadata.json", NO_HUD),
                ("Other Game/OTH.txt", ""),
                ("Other Game/metadata.json", BLOOM_NAMED),
            ]),
        }
    }

    fn found(user: Place, list: Option<&str>) -> Found {
        Found {
            game_id: "GAMP01".to_string(),
            places: vec![user, dolphins()],
            list: list.map(str::to_string),
        }
    }

    fn nobodys() -> Place {
        Place { source: Source::User, files: BTreeMap::new() }
    }

    fn names(packs: &[Pack]) -> Vec<&str> {
        packs.iter().map(|pack| pack.name.as_str()).collect()
    }

    #[test]
    fn a_games_folders_are_found_as_dolphin_finds_them() {
        let place = Place {
            source: Source::System,
            files: files(&[
                ("GAMP01/metadata.json", ""),
                ("GAM/metadata.json", ""),
                ("Deep/inside/ids/GAM.txt", ""),
                ("Everyone/all.txt", ""),
                ("Lower/gam.txt", ""),
                ("Other/OTH.txt", ""),
                ("GAMP01.txt", ""),
            ]),
        };
        assert_eq!(
            folders(&place, "GAMP01").into_iter().collect::<Vec<_>>(),
            ["Deep", "Everyone", "GAMP01"],
            "the id's own folder over its three letters', a .txt at any depth, names matched exactly"
        );
        let place = Place { source: Source::System, files: files(&[("gam/metadata.json", "")]) };
        assert_eq!(folders(&place, "GAMP01").into_iter().collect::<Vec<_>>(), ["gam"], "a folder's name in any case");
    }

    #[test]
    fn a_mod_for_every_game_is_offered_where_a_group_names_what_it_acts_on() {
        let packs = list(&found(nobodys(), None), true);
        assert_eq!(names(&packs), ["Bloom Removal", "Bloom Blurred"], "no HUD named for this game, and the names alone aren't a mod to switch");
        let removal = &packs[0];
        assert_eq!(removal.id, "GraphicMods|All Games Bloom Removal/metadata.json");
        assert_eq!(removal.kind, "Graphics mods");
        assert_eq!(removal.by, "by Dolphin Team");
        assert_eq!(removal.about, "Skips drawing bloom effects.");
        assert!(packs.iter().all(|pack| !pack.on && pack.applies && pack.on_because.is_none()));
        // A game nothing names a group for has none.
        let mut elsewhere = found(nobodys(), None);
        elsewhere.game_id = "XYZE01".to_string();
        assert!(list(&elsewhere, true).is_empty());
    }

    #[test]
    fn the_users_own_mods_are_not_offered_but_their_groups_count() {
        let own = Place {
            source: Source::User,
            files: files(&[
                ("My HUD/GAMP01.txt", ""),
                (
                    "My HUD/metadata.json",
                    r#"{ "groups": [ { "name": "HUD", "targets": [ { "type": "efb" } ] } ], "features": [ { "group": "HUD", "action": "skip" } ] }"#,
                ),
            ]),
        };
        let packs = list(&found(own, None), true);
        assert_eq!(names(&packs), ["Bloom Removal", "Bloom Blurred", "Remove HUD"]);
    }

    #[test]
    fn the_games_list_says_which_are_on_while_dolphins_mods_are() {
        let list_text = r#"{ "mods": [ { "source": "system", "path": "All Games Bloom Removal/metadata.json",
            "groups": [], "features": [ {} ], "enabled": true, "weight": 0 } ] }"#;
        let packs = super::list(&found(nobodys(), Some(list_text)), true);
        assert!(packs[0].on);
        assert!(!packs[1].on);
        assert!(!super::list(&found(nobodys(), Some(list_text)), false)[0].on, "not while Dolphin's mods are off for the game");
        // Dolphin reads nothing more of an entry that counts the mod's
        // features differently, so the mod stays off.
        let changed = list_text.replace(r#""features": [ {} ]"#, r#""features": [ {}, {} ]"#);
        assert!(!super::list(&found(nobodys(), Some(&changed)), true)[0].on);
    }

    #[test]
    fn mods_come_in_order_of_weight() {
        let list_text = r#"{ "mods": [ { "source": "system", "path": "All Games Bloom Removal/metadata.json", "weight": 5 } ] }"#;
        assert_eq!(names(&list(&found(nobodys(), Some(list_text)), true)), ["Bloom Blurred", "Bloom Removal"]);
    }

    #[test]
    fn a_mod_is_written_into_the_list_as_dolphins_window_writes_it() {
        let (text, any) = switched(&found(nobodys(), None), "All Games Bloom Removal/metadata.json", true).unwrap();
        assert_eq!(
            text,
            "{\n  \"mods\": [\n    {\n      \"enabled\": true,\n      \"features\": [\n        {}\n      ],\n      \"groups\": [],\n      \"path\": \"All Games Bloom Removal/metadata.json\",\n      \"source\": \"system\",\n      \"weight\": 0\n    }\n  ]\n}\n"
        );
        assert!(any);
        let after = found(nobodys(), Some(&text));
        assert!(list(&after, true)[0].on);

        let (off, any) = switched(&after, "All Games Bloom Removal/metadata.json", false).unwrap();
        assert!(off.contains("\"enabled\": false"), "{off}");
        assert!(!any, "nothing on, so Dolphin's mods can go off for the game");
    }

    #[test]
    fn the_lists_other_entries_stay_as_they_were() {
        let own = Place {
            source: Source::User,
            files: files(&[("Mine/GAM.txt", ""), ("Mine/metadata.json", r#"{ "features": [ { "group": "Bloom", "action": "skip" } ] }"#)]),
        };
        let list_text = r#"{ "mods": [
            { "source": "user", "path": "Mine/metadata.json", "enabled": true, "weight": 3, "note": "kept" },
            { "source": "system", "path": "Gone Now/metadata.json", "enabled": true },
            { "source": "system", "path": "All Games Bloom Removal/metadata.json", "features": [ {}, {} ], "enabled": true, "weight": 2 }
        ] }"#;
        let (text, any) = switched(&found(own.clone(), Some(list_text)), "All Games Bloom Removal/metadata.json", true).unwrap();
        let root: Value = serde_json::from_str(&text).unwrap();
        let entries = root["mods"].as_array().unwrap();
        assert_eq!(entries.len(), 3, "the mod's entry written where it was, nothing added");
        assert_eq!(entries[0], json!({ "source": "user", "path": "Mine/metadata.json", "enabled": true, "weight": 3, "note": "kept" }));
        assert_eq!(entries[1], json!({ "source": "system", "path": "Gone Now/metadata.json", "enabled": true }));
        assert_eq!(
            entries[2],
            json!({ "source": "system", "path": "All Games Bloom Removal/metadata.json", "groups": [], "features": [{}], "enabled": true, "weight": 0 }),
            "counted afresh, so Dolphin reads it again"
        );
        assert!(any);
        // The user's own mod keeps Dolphin's mods on after Dolphin's goes off.
        let (_, any) = switched(&found(own, Some(&text)), "All Games Bloom Removal/metadata.json", false).unwrap();
        assert!(any);
    }

    #[test]
    fn a_list_dolphin_cant_read_leaves_the_game_without_mods() {
        assert!(list(&found(nobodys(), Some("not a list")), true).is_empty());
        assert_eq!(
            switched(&found(nobodys(), Some("[]")), "All Games Bloom Removal/metadata.json", true),
            Err(GONE.to_string())
        );
    }

    #[test]
    fn a_mod_dolphin_cant_read_is_left_out() {
        assert!(read_mod(Source::System, "a/metadata.json", NO_BLOOM).is_some());
        assert!(read_mod(Source::System, "a/metadata.json", r#"{ "groups": [ "Bloom" ] }"#).is_none());
        assert!(read_mod(Source::System, "a/metadata.json", r#"{ "groups": [ { "name": 1 } ] }"#).is_none());
        assert!(read_mod(Source::System, "a/metadata.json", r#"{ "groups": [ { "targets": [ { "type": "tv" } ] } ] }"#).is_none());
        assert!(read_mod(Source::System, "a/metadata.json", r#"{ "features": [ { "action": 2 } ] }"#).is_none());
        assert!(read_mod(Source::System, "a/metadata.json", "[]").is_none());
        // Words that aren't text are only left blank.
        let odd = read_mod(Source::System, "a/metadata.json", r#"{ "meta": { "title": 5 } }"#).unwrap();
        assert_eq!(odd.title, "");
        assert!(is_metadata("A/b/metadata.JSON"));
        assert!(!is_metadata("A/Metadata.json"));
        assert!(!is_metadata("A/metadata.json.txt"));
    }

    #[test]
    fn a_mod_nobody_lists_any_more_is_refused() {
        assert_eq!(switched(&found(nobodys(), None), "All Games HUD Removal/metadata.json", true), Err(GONE.to_string()));
        assert_eq!(switched(&found(nobodys(), None), "A Game/metadata.json", true), Err(GONE.to_string()), "names alone");
    }

    #[test]
    fn a_list_that_cant_be_read_is_no_empty_list() {
        let dir = std::env::temp_dir().join(format!("omoio-dolphin-mod-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_list(&dir.join("SSPP52.json")), Ok(None), "none yet");
        std::fs::write(dir.join("SSPP52.json"), "{}\n").unwrap();
        assert_eq!(read_list(&dir.join("SSPP52.json")), Ok(Some("{}\n".to_string())));
        // A folder in its place can't be read, nor can a list that isn't UTF-8.
        std::fs::create_dir_all(dir.join("GALE01.json")).unwrap();
        assert!(read_list(&dir.join("GALE01.json")).is_err());
        std::fs::write(dir.join("RMCE01.json"), b"{\"mods\": \"\xF8\"}").unwrap();
        assert!(read_list(&dir.join("RMCE01.json")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reads the mods the Dolphin Omoio installs comes with. Set
    /// OMOIO_DOLPHIN_MODS to its `Sys/Load/GraphicMods` folder to run this.
    #[test]
    #[ignore]
    fn reads_the_mods_dolphin_comes_with() {
        let dir = PathBuf::from(std::env::var("OMOIO_DOLPHIN_MODS").expect("set OMOIO_DOLPHIN_MODS"));
        let dolphins = place(Source::System, &dir);
        let metadata: Vec<_> = dolphins.files.iter().filter(|(path, _)| is_metadata(path)).collect();
        for (path, text) in &metadata {
            assert!(read_mod(Source::System, path, text).is_some(), "{path} reads as Dolphin reads it");
        }
        println!("{} mods in {} files", metadata.len(), dolphins.files.len());
        for id in ["SSPP52", "SSPE52", "SKYE52", "SKYP52", "SVXE52", "SK8E52", "SKNE52", "RMCE01", "GZLE01", "SOUE01"] {
            let found = Found { game_id: id.to_string(), places: vec![nobodys(), dolphins.clone()], list: None };
            let packs = list(&found, true);
            println!("{id}: {:?}", names(&packs));
            if id.starts_with("SSP") || id.starts_with("SKY") {
                assert_eq!(names(&packs), ["Bloom Removal", "Bloom Blurred", "Native Resolution Bloom"], "{id}");
            }
        }
    }
}
