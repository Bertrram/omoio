//! The community patches RPCS3 publishes, per game.
//!
//! Two files, both RPCS3's, both in its install:
//!
//! - `patches/patch.yml` is the catalogue. It is real YAML, with anchors and
//!   aliases and the same key repeated at the top level, which is why this is
//!   the one place Omoio uses a YAML parser rather than reading lines.
//! - `config/patch_config.yml` is what is switched on. RPCS3 writes only the
//!   patches that are enabled, so switching one off means removing its entry,
//!   not writing `false`.
//!
//! Both shapes were read out of RPCS3's bin_patch.cpp rather than guessed, and
//! a test loads the real published file to check this still reads it.
//!
//! Only patches on Omoio's own list of fixes (`fixes.rs`) are switched on
//! without being asked, once, with the reason shown. Every other patch waits
//! for the user.

use crate::core::community::Pack;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;
use yaml_rust2::{yaml::Hash, Yaml, YamlEmitter, YamlLoader};

/// From RPCS3's patch_manager_dialog.cpp. The version is the patch engine's,
/// declared in bin_patch.h, and the server returns a file for that version.
const PATCH_ENGINE_VERSION: &str = "1.2";

fn patch_url() -> String {
    format!("https://rpcs3.net/compatibility?patch&api=v1&v={PATCH_ENGINE_VERSION}")
}

/// Keys as RPCS3 spells them, from patch_key in bin_patch.h.
const K_GAMES: &str = "Games";
const K_AUTHOR: &str = "Author";
const K_NOTES: &str = "Notes";
const K_PATCH_VERSION: &str = "Patch Version";
const K_ENABLED: &str = "Enabled";
const K_ALL: &str = "All";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    /// RPCS3 keys everything on the hash of the executable, so it travels with
    /// these values and is not ours to shorten.
    pub hash: String,
    pub name: String,
    pub game: String,
    /// The serial the patch list files it under for this game: the game's own
    /// title id, or "All" for a patch written for every game. RPCS3 reads the
    /// switch from under the same key, so it has to be written there.
    #[serde(default)]
    pub serial: String,
    pub author: String,
    pub notes: String,
    pub version: String,
    /// The game versions this patch was written for.
    pub versions: Vec<String>,
    /// Whether it covers the copy of the game actually installed.
    pub applies: bool,
    pub enabled: bool,
    /// Why Omoio switches it on, when it is one of Omoio's own fixes.
    #[serde(default)]
    pub fix: Option<String>,
}

/// How a patch is known to the interface: its serial, the executable's hash
/// and its name, which together pick out one patch.
pub fn pack_id(patch: &Patch) -> String {
    format!("{}|{}|{}", patch.serial, patch.hash, patch.name)
}

/// A patch in the shape every emulator's community packs share.
pub fn as_pack(patch: Patch) -> Pack {
    let by = [
        (!patch.author.is_empty()).then(|| format!("by {}", patch.author)),
        (!patch.version.is_empty()).then(|| format!("v{}", patch.version)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    Pack {
        id: pack_id(&patch),
        kind: "Patches".to_string(),
        about: patch.notes.clone(),
        by,
        on: patch.enabled,
        applies: patch.applies,
        // A patch written for another release of the game would not be
        // applied, so it says which one it wants.
        needs: (!patch.applies).then(|| format!("Written for version {}.", patch.versions.join(", "))),
        on_because: patch.fix.as_ref().map(|why| format!("Omoio turns this on for this game. {why}")),
        choices: Vec::new(),
        name: patch.name,
    }
}

pub fn catalogue_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?.join("patches").join("patch.yml"))
}

/// `config/patch_config.yml`, next to RPCS3's other settings. On Windows
/// RPCS3 asks for its config subdirectory here, which its own File.cpp
/// resolves to `config/` beside the executable.
pub fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::install_dir(app)?.join("config").join("patch_config.yml"))
}

pub fn have_catalogue(app: &AppHandle) -> bool {
    catalogue_path(app).is_ok_and(|p| p.exists())
}

/// A scalar as it was written. Version numbers like `01.33` would come back
/// from a number-aware reader as 1.33, and RPCS3 matches these as text, so the
/// original spelling is the only safe thing to keep.
fn scalar(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Looks a key up the way yaml-cpp resolves it: the last one written wins.
///
/// Keys repeated inside a mapping have been padded apart so the document parses
/// (see `make_keys_unique`), and one patch in the published file really does
/// name `Notes` twice. Reading the last match keeps us agreeing with RPCS3
/// about what the file says.
fn last<'a>(map: &'a Hash, key: &str) -> Option<&'a Yaml> {
    map.iter()
        .filter(|(k, _)| k.as_str().is_some_and(|k| k.trim_end() == key))
        .next_back()
        .map(|(_, value)| value)
}

fn text(map: &Hash, key: &str) -> String {
    last(map, key).and_then(scalar).unwrap_or_default()
}

/// The published file names the same key twice in the same mapping, in two
/// places: 166 times at the top level, once for every `Anchors` block and for
/// ten real entries, and inside patches too, where one names `Notes` twice.
/// yaml-cpp, which RPCS3 reads it with, keeps the last and carries on. Our
/// parser refuses the document outright.
///
/// Dropping the earlier ones is not the same thing, because yaml-cpp still
/// reads them and every anchor they define stays defined. So the repeats are
/// padded apart with trailing spaces instead. Everything parses, every anchor
/// survives, and `last` then reads the one RPCS3 would have used.
///
/// This is safe to do by line because the file has no block scalars, so no
/// indented text can be mistaken for a key.
fn make_keys_unique(text: &str) -> String {
    // One counter per nesting level, dropped as soon as that level closes.
    let mut levels: Vec<(usize, std::collections::HashMap<String, usize>)> = Vec::new();
    let mut out = String::with_capacity(text.len() + 8192);

    for line in text.lines() {
        let Some((indent, key, rest)) = split_key(line) else {
            out.push_str(line);
            out.push('\n');
            continue;
        };

        while levels.last().is_some_and(|(at, _)| *at > indent) {
            levels.pop();
        }
        if levels.last().map(|(at, _)| *at) != Some(indent) {
            levels.push((indent, std::collections::HashMap::new()));
        }

        let seen = &mut levels.last_mut().expect("just pushed").1;
        let count = seen.entry(key.to_string()).or_insert(0);
        *count += 1;

        out.push_str(&" ".repeat(indent));
        if *count > 1 {
            out.push_str(&format!("\"{}{}\"", key, " ".repeat(*count - 1)));
        } else {
            out.push_str(&format!("\"{key}\""));
        }
        out.push_str(rest);
        out.push('\n');
    }
    out
}

/// Splits a mapping line into its indent, the key's text, and everything from
/// the colon onwards. `None` for anything that is not a key: list items,
/// comments, and blank lines.
fn split_key(line: &str) -> Option<(usize, &str, &str)> {
    let indent = line.len() - line.trim_start().len();
    let body = &line[indent..];
    if body.is_empty() || body.starts_with('#') || body.starts_with('-') {
        return None;
    }

    // A key may be quoted, and one of them really is "32:9", so the colon
    // inside it must not be mistaken for the separator.
    if let Some(unquoted) = body.strip_prefix('"') {
        let end = unquoted.find('"')?;
        let rest = &unquoted[end + 1..];
        if !rest.starts_with(':') {
            return None;
        }
        return Some((indent, &unquoted[..end], rest));
    }

    let colon = body.find(':')?;
    let key = &body[..colon];
    if key.is_empty() || key.contains('\'') {
        return None;
    }
    Some((indent, key, &body[colon..]))
}

fn load(path: &PathBuf) -> Result<Yaml, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&raw)
}

fn parse(raw: &str) -> Result<Yaml, String> {
    let mut docs = YamlLoader::load_from_str(&make_keys_unique(raw))
        .map_err(|_| "Couldn't read the patch list.".to_string())?;
    if docs.is_empty() {
        return Err("The patch list is empty.".into());
    }
    Ok(docs.remove(0))
}

/// Every patch published for this game, whether or not it fits the copy
/// installed. One that needs another version is still worth showing, because
/// otherwise it looks like no patch exists.
pub fn for_title(app: &AppHandle, title_id: &str, app_version: &str) -> Vec<Patch> {
    let Ok(root) = catalogue_path(app).and_then(|p| load(&p)) else {
        return Vec::new();
    };
    let Some(top) = root.as_hash() else {
        return Vec::new();
    };
    match_title(top, title_id, app_version, &read_enabled(app))
}

/// The patches in the list for one game.
///
/// A patch names its games by serial, or with "All" for every game, and RPCS3
/// looks for the game's own serial first and falls back to "All". An "All"
/// patch is only listed when it is on Omoio's list of fixes for this game:
/// the list holds hundreds of them, and most do nothing for a given game.
///
/// A patch shows as on only where RPCS3 applies it to the version it runs:
/// its switch sits under the keys `entry_keys` gives for that version. A
/// switch left under another version is that version's alone, so it shows
/// as off, and switching the patch on writes the running version's.
fn match_title(
    top: &Hash,
    title_id: &str,
    app_version: &str,
    switched_on: &[[String; 5]],
) -> Vec<Patch> {
    let mut found = Vec::new();

    for (hash, entries) in patch_entries(top) {
        for (name, info) in entries_last_wins(entries) {
            let Some(info) = info.as_hash() else { continue };
            let Some(games) = last(info, K_GAMES).and_then(Yaml::as_hash) else {
                continue;
            };
            let fix = super::fixes::patch_reason(title_id, &hash, &name);

            for (game, serials) in entries_last_wins(games) {
                let Some(serials) = serials.as_hash() else { continue };
                let (serial, versions) = match last(serials, title_id).and_then(Yaml::as_vec) {
                    Some(versions) => (title_id, versions),
                    None => match (fix, last(serials, K_ALL).and_then(Yaml::as_vec)) {
                        (Some(_), Some(versions)) => (K_ALL, versions),
                        _ => continue,
                    },
                };
                let versions: Vec<String> = versions.iter().filter_map(scalar).collect();
                let applies = versions.iter().any(|v| v == K_ALL || v == app_version);

                let mut patch = Patch {
                    applies,
                    enabled: false,
                    hash: hash.clone(),
                    name: name.clone(),
                    game: game.clone(),
                    serial: serial.to_string(),
                    author: text(info, K_AUTHOR),
                    notes: text(info, K_NOTES),
                    version: text(info, K_PATCH_VERSION),
                    versions,
                    fix: fix.map(str::to_string),
                };
                patch.enabled = switched_on.contains(&entry_keys(&patch, title_id, app_version));
                found.push(patch);
            }
        }
    }

    // Ones that fit first, then by name, so the list does not reshuffle.
    found.sort_by(|a, b| b.applies.cmp(&a.applies).then_with(|| a.name.cmp(&b.name)));
    found
}

/// The top-level entries that actually hold patches, keyed by hash with the
/// padding stripped back off.
///
/// Which prefixes RPCS3 uses is not worth guessing at: the file has PPU, SPU,
/// OVL and PRX today and could gain another. An entry is a patch entry when it
/// contains something that names the games it is for, which is what makes it
/// usable at all. Repeats keep the last, as yaml-cpp does.
fn patch_entries(top: &Hash) -> Vec<(String, &Hash)> {
    entries_last_wins(top)
        .into_iter()
        .filter_map(|(hash, value)| Some((hash, value.as_hash()?)))
        .filter(|(_, entries)| {
            entries
                .values()
                .any(|info| info.as_hash().is_some_and(|i| last(i, K_GAMES).is_some()))
        })
        .collect()
}

/// A mapping's entries with repeats collapsed to the last one written, and the
/// padding that kept them apart stripped back off.
fn entries_last_wins(map: &Hash) -> Vec<(String, &Yaml)> {
    let mut out: Vec<(String, &Yaml)> = Vec::new();
    for (key, value) in map {
        let Some(key) = key.as_str() else { continue };
        let key = key.trim_end().to_string();
        match out.iter_mut().find(|(seen, _)| *seen == key) {
            Some(slot) => slot.1 = value,
            None => out.push((key, value)),
        }
    }
    out
}

/// The switches in RPCS3's patch_config.yml that are on (`switched_on`).
fn read_enabled(app: &AppHandle) -> Vec<[String; 5]> {
    let Ok(root) = config_path(app).and_then(|p| load(&p)) else {
        return Vec::new();
    };
    root.as_hash().map(switched_on).unwrap_or_default()
}

/// Every switch that is on, by where it sits: hash, name, game, serial and
/// version, the keys RPCS3 reads it by (`entry_keys`). RPCS3 keeps a patch
/// switched off only when its own values were changed, as an entry without
/// `Enabled`, which it reads as off, and an entry in RPCS3's older form is
/// the switch alone (Utilities/bin_patch.cpp, `save_config` and
/// `load_config`, RPCS3/rpcs3 b2e52d6, read 4 November 2026).
fn switched_on(top: &Hash) -> Vec<[String; 5]> {
    fn maps(map: &Hash) -> impl Iterator<Item = (String, &Hash)> {
        entries_last_wins(map).into_iter().filter_map(|(key, value)| Some((key, value.as_hash()?)))
    }
    let mut on = Vec::new();
    for (hash, names) in maps(top) {
        for (name, games) in maps(names) {
            for (game, serials) in maps(games) {
                for (serial, versions) in maps(serials) {
                    for (version, values) in entries_last_wins(versions) {
                        let enabled = match values {
                            Yaml::Hash(values) => last(values, K_ENABLED) == Some(&Yaml::Boolean(true)),
                            legacy => *legacy == Yaml::Boolean(true),
                        };
                        if enabled {
                            on.push([hash.clone(), name.clone(), game.clone(), serial.clone(), version]);
                        }
                    }
                }
            }
        }
    }
    on
}

fn nested<'a>(map: &'a mut Hash, key: &str) -> &'a mut Hash {
    let key = Yaml::String(key.into());
    if !matches!(map.get(&key), Some(Yaml::Hash(_))) {
        map.insert(key.clone(), Yaml::Hash(Hash::new()));
    }
    match map.get_mut(&key) {
        Some(Yaml::Hash(inner)) => inner,
        _ => unreachable!("just inserted a map"),
    }
}

/// Where RPCS3 looks for a patch's switch: hash, name, game, serial, version.
///
/// RPCS3's `patch_engine::apply` walks the patch's own games, serials and
/// versions, trying the game's serial and version first and "All" after, so
/// the switch has to sit under the same keys the patch list uses.
fn entry_keys(patch: &Patch, title_id: &str, app_version: &str) -> [String; 5] {
    let serial = if patch.serial.is_empty() { title_id } else { patch.serial.as_str() };
    // A patch written for "All" versions has no version of its own, so the
    // game's is only used when the patch names it.
    let version = if patch.versions.iter().any(|v| v == K_ALL)
        && !patch.versions.iter().any(|v| v == app_version)
    {
        K_ALL
    } else {
        app_version
    };
    [
        patch.hash.clone(),
        patch.name.clone(),
        patch.game.clone(),
        serial.to_string(),
        version.to_string(),
    ]
}

/// Switches one patch on or off for one game, leaving every other entry in
/// RPCS3's file alone.
pub fn set_enabled(
    app: &AppHandle,
    patch: &Patch,
    title_id: &str,
    app_version: &str,
    on: bool,
) -> Result<(), String> {
    let path = config_path(app)?;
    let mut root = match load(&path) {
        Ok(Yaml::Hash(hash)) => hash,
        _ => Hash::new(),
    };

    switch(&mut root, &entry_keys(patch, title_id, app_version), on);

    if root.is_empty() {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut out = String::new();
    YamlEmitter::new(&mut out)
        .dump(&Yaml::Hash(root))
        .map_err(|e| e.to_string())?;
    std::fs::write(&path, out).map_err(|e| e.to_string())
}

/// Puts one patch's switch in under `keys` (`entry_keys`), or takes it out.
fn switch(root: &mut Hash, keys: &[String; 5], on: bool) {
    let [hash, name, game, serial, version] = keys;
    if on {
        let leaf = nested(nested(nested(nested(nested(root, hash), name), game), serial), version);
        leaf.insert(Yaml::String(K_ENABLED.into()), Yaml::Boolean(true));
    } else {
        remove(root, &[hash.as_str(), name.as_str(), game.as_str(), serial.as_str(), version.as_str()]);
    }
}

/// Takes one entry out and tidies up behind it, so switching a patch off does
/// not leave a trail of empty maps RPCS3 has to wade through.
fn remove(map: &mut Hash, path: &[&str]) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    let key = Yaml::String((*head).into());
    if rest.is_empty() {
        map.remove(&key);
        return;
    }
    if let Some(Yaml::Hash(inner)) = map.get_mut(&key) {
        remove(inner, rest);
        if inner.is_empty() {
            map.remove(&key);
        }
    }
}

/// Downloads the catalogue into RPCS3's own patches folder, where RPCS3 reads
/// it from. Returns how many patch entries it holds.
pub async fn refresh(app: &AppHandle) -> Result<usize, String> {
    let response = reqwest::Client::new()
        .get(patch_url())
        .header("User-Agent", super::USER_AGENT)
        .send()
        .await
        .map_err(|_| "Couldn't reach the patch list.".to_string())?;

    if !response.status().is_success() {
        return Err("The patch list isn't available right now.".into());
    }

    // The file arrives wrapped in JSON, with its own checksum beside it.
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "The patch list came back in a form we don't understand.".to_string())?;

    if body.get("return_code").and_then(|c| c.as_i64()).unwrap_or(-255) != 0 {
        return Err("The patch list isn't available right now.".into());
    }
    let yaml = body
        .get("patch")
        .and_then(|p| p.as_str())
        .ok_or("The patch list came back empty.")?;

    if let Some(expected) = body.get("sha256").and_then(|s| s.as_str()) {
        if !matches_checksum(yaml, expected) {
            return Err("The patch list arrived damaged. Try again.".into());
        }
    }

    let path = catalogue_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, yaml).map_err(|e| e.to_string())?;

    Ok(count_entries(yaml))
}

fn matches_checksum(text: &str, expected: &str) -> bool {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    got.eq_ignore_ascii_case(expected)
}

fn count_entries(yaml: &str) -> usize {
    yaml.lines()
        .filter(|line| line.starts_with("PPU") || line.starts_with("SPU"))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_version_spelled_the_way_it_was_written() {
        // Read as a number, 01.33 comes back 1.33 and stops matching.
        let docs = YamlLoader::load_from_str("v: [ 01.33, 1.00, All ]").unwrap();
        let list = docs[0]["v"].as_vec().unwrap();
        let versions: Vec<String> = list.iter().filter_map(scalar).collect();
        assert_eq!(versions, vec!["01.33", "1.00", "All"]);
    }

    #[test]
    fn resolves_the_aliases_the_patch_file_is_built_from() {
        // Every real entry names its games through an anchor like this one.
        let text = "\
Anchors:
  lbp: &lbp
    \"LittleBigPlanet 2\":
      BCES00850: [ 01.33 ]
PPU-abc:
  \"Unlock FPS\":
    Games: *lbp
    Author: \"someone\"
";
        let docs = YamlLoader::load_from_str(text).unwrap();
        let games = &docs[0]["PPU-abc"]["Unlock FPS"][K_GAMES];
        assert!(games.as_hash().is_some(), "alias was not resolved");
        assert!(games["LittleBigPlanet 2"]["BCES00850"].as_vec().is_some());
    }

    #[test]
    fn switching_a_patch_off_leaves_nothing_behind() {
        let mut root = Hash::new();
        {
            let a = nested(&mut root, "PPU-abc");
            let b = nested(a, "Unlock FPS");
            let c = nested(b, "LittleBigPlanet 2");
            let d = nested(c, "BCES00850");
            let leaf = nested(d, "01.33");
            leaf.insert(Yaml::String(K_ENABLED.into()), Yaml::Boolean(true));
        }
        remove(
            &mut root,
            &["PPU-abc", "Unlock FPS", "LittleBigPlanet 2", "BCES00850", "01.33"],
        );
        assert!(root.is_empty(), "empty maps were left behind");
    }

    #[test]
    fn switching_one_off_leaves_the_others_alone() {
        let mut root = Hash::new();
        {
            let leaf = nested(
                nested(nested(nested(nested(&mut root, "PPU-abc"), "One"), "Game"), "SER"),
                "01.00",
            );
            leaf.insert(Yaml::String(K_ENABLED.into()), Yaml::Boolean(true));
            let other = nested(
                nested(nested(nested(nested(&mut root, "PPU-abc"), "Two"), "Game"), "SER"),
                "01.00",
            );
            other.insert(Yaml::String(K_ENABLED.into()), Yaml::Boolean(true));
        }
        remove(&mut root, &["PPU-abc", "One", "Game", "SER", "01.00"]);
        assert!(root.get(&Yaml::String("PPU-abc".into())).is_some());
        let kept = root[&Yaml::String("PPU-abc".into())].as_hash().unwrap();
        assert!(kept.get(&Yaml::String("Two".into())).is_some());
        assert!(kept.get(&Yaml::String("One".into())).is_none());
    }

    #[test]
    fn a_damaged_download_is_refused() {
        // sha256 of "hello"
        let good = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert!(matches_checksum("hello", good));
        assert!(!matches_checksum("hello world", good));
    }

    /// Reads the file RPCS3 actually publishes. Set OMOIO_PATCH_YML to a copy
    /// of it to run this.
    #[test]
    #[ignore]
    fn reads_the_published_patch_file() {
        let path = std::env::var("OMOIO_PATCH_YML").expect("set OMOIO_PATCH_YML");
        let root = load(&PathBuf::from(path)).expect("parsed");
        let top = root.as_hash().expect("a map at the top");
        let entries = patch_entries(top);
        assert!(entries.len() > 100, "only found {} patch entries", entries.len());

        // A repeated hash must appear once, not twice, or a patch RPCS3 never
        // applies would be offered as though it did.
        let mut names: Vec<&String> = entries.iter().map(|(hash, _)| hash).collect();
        let before = names.len();
        names.sort();
        names.dedup();
        assert_eq!(before, names.len(), "the same hash was kept twice");

        // No key should still be carrying the padding used to make it unique.
        assert!(!names.iter().any(|hash| hash.ends_with(' ')));

        // Nearly every patch names its games through an alias, so this fails
        // loudly if alias support ever goes away.
        let with_games: usize = entries
            .iter()
            .map(|(_, entry)| {
                entry
                    .values()
                    .filter(|info| {
                        info.as_hash()
                            .and_then(|i| i.get(&Yaml::String(K_GAMES.into())))
                            .and_then(Yaml::as_hash)
                            .is_some()
                    })
                    .count()
            })
            .sum();
        assert!(with_games > 100, "only {with_games} patches named their games");
    }

    #[test]
    fn a_repeated_key_parses_and_the_last_one_wins() {
        // Both halves of what the published file does: repeated Anchors blocks
        // whose anchors must all survive, and a repeated entry.
        let text = "\
Anchors:
  first: &first
    \"Game One\":
      SER00001: [ 01.00 ]
Anchors:
  second: &second
    \"Game Two\":
      SER00002: [ 01.00 ]
PPU-abc:
  \"Old\":
    Games: *first
PPU-abc:
  \"New\":
    Games: *second
";
        let root = parse(text).expect("a repeated key should not stop the file being read");
        let entries = patch_entries(root.as_hash().unwrap());

        assert_eq!(entries.len(), 1, "the repeated hash should appear once");
        let (hash, patches) = &entries[0];
        assert_eq!(hash, "PPU-abc", "the padding should be gone");
        // yaml-cpp keeps the last, so RPCS3 only ever applies "New".
        assert!(patches.contains_key(&Yaml::String("New".into())));
        assert!(!patches.contains_key(&Yaml::String("Old".into())));
        // The anchor from the first block still resolved, which is why the
        // blocks are renamed rather than dropped.
        assert!(root["Anchors"]["first"].as_hash().is_some());
    }

    /// One patch for one game by serial, the real MLAA fix written for every
    /// game, and another "All" patch that is not one of Omoio's fixes.
    const LIST: &str = "\
PPU-abc:
  \"Unlock FPS\":
    Games:
      \"LittleBigPlanet 3\":
        BCES01663: [ 01.26 ]
SPU-702d0205a89d445d15dc0f96548546c4e2e7a59f:
  \"Disable SPU MLAA - LittleBigPlanet 2, LittleBigPlanet 3, LittleBigPlanet Hub\":
    Games:
      All:
        All: [ All ]
SPU-def:
  \"Some other patch for every game\":
    Games:
      All:
        All: [ All ]
";

    #[test]
    fn an_all_patch_on_the_fix_list_is_offered_and_switched_on_under_all() {
        let root = parse(LIST).unwrap();
        let found = match_title(root.as_hash().unwrap(), "BCES01663", "01.26", &[]);
        let mlaa = found
            .iter()
            .find(|p| p.name.starts_with("Disable SPU MLAA"))
            .expect("the MLAA fix is offered for LittleBigPlanet 3");
        assert_eq!(mlaa.serial, "All");
        assert!(mlaa.applies);
        assert!(mlaa.fix.is_some(), "the reason travels with it");
        // Where RPCS3 looks for it. The game's own serial here is what made it
        // look switched on while RPCS3 never applied it.
        let keys = entry_keys(mlaa, "BCES01663", "01.26");
        assert_eq!(keys[2..], ["All".to_string(), "All".to_string(), "All".to_string()]);
    }

    #[test]
    fn an_all_patch_off_the_fix_list_is_not_offered() {
        let root = parse(LIST).unwrap();
        let found = match_title(root.as_hash().unwrap(), "BCES01663", "01.26", &[]);
        assert!(!found.iter().any(|p| p.name.starts_with("Some other patch")));
        let lbp1 = match_title(root.as_hash().unwrap(), "BCES00141", "01.30", &[]);
        assert!(lbp1.is_empty(), "nothing in this list is for LittleBigPlanet 1");
    }

    /// A patch written for two versions of one game, as an update leaves a
    /// game with a choice of two.
    const TWO_VERSIONS: &str = "\
PPU-sf:
  \"60 FPS\":
    Games:
      \"Skylanders SWAP Force\":
        BLES01860: [ 01.00, 01.01 ]
";

    #[test]
    fn a_patch_shows_as_on_only_for_the_version_its_switch_was_written_for() {
        let list = parse(TWO_VERSIONS).unwrap();
        let shown = |config: &Hash, version: &str| {
            let found = match_title(list.as_hash().unwrap(), "BLES01860", version, &switched_on(config));
            found.into_iter().find(|p| p.name == "60 FPS").unwrap()
        };
        let mut config = Hash::new();
        // Switched on while the disc's 01.00 ran.
        let on_disc = shown(&config, "01.00");
        switch(&mut config, &entry_keys(&on_disc, "BLES01860", "01.00"), true);
        assert!(shown(&config, "01.00").enabled);
        // Updated to 01.01, which RPCS3 reads its own switch for.
        let updated = shown(&config, "01.01");
        assert!(updated.applies && !updated.enabled, "01.00's switch isn't 01.01's");
        // On, and off again, for the version that runs.
        switch(&mut config, &entry_keys(&updated, "BLES01860", "01.01"), true);
        assert!(shown(&config, "01.01").enabled);
        switch(&mut config, &entry_keys(&updated, "BLES01860", "01.01"), false);
        assert!(!shown(&config, "01.01").enabled, "not stuck on");
        assert!(shown(&config, "01.00").enabled, "01.00's own is left as it was");
    }

    #[test]
    fn a_switch_for_every_version_holds_whatever_version_runs() {
        let list = parse(LIST).unwrap();
        let mlaa = |config: &Hash, version: &str| {
            match_title(list.as_hash().unwrap(), "BCES01663", version, &switched_on(config))
                .into_iter()
                .find(|p| p.name.starts_with("Disable SPU MLAA"))
                .unwrap()
        };
        let mut config = Hash::new();
        let keys = entry_keys(&mlaa(&config, "01.26"), "BCES01663", "01.26");
        switch(&mut config, &keys, true);
        // Written out and read back, as set_enabled and for_title do.
        let mut out = String::new();
        YamlEmitter::new(&mut out).dump(&Yaml::Hash(config)).unwrap();
        let config = parse(&out).unwrap();
        let config = config.as_hash().unwrap();
        assert!(mlaa(config, "01.26").enabled);
        assert!(mlaa(config, "01.30").enabled, "an update keeps it on");
    }

    #[test]
    fn reads_the_switches_rpcs3_writes() {
        // Switched on; switched off with a value of its own changed, which
        // RPCS3 keeps without Enabled; and switched on in RPCS3's older form.
        let text = "\
PPU-abc:
  \"Unlock FPS\":
    \"LittleBigPlanet 3\":
      BCES01663:
        01.26:
          Enabled: true
SPU-def:
  \"Changed, then switched off\":
    All:
      All:
        All:
          Configurable Values:
            Speed: 2
PPU-old:
  \"From before values\":
    \"LittleBigPlanet 2\":
      BCES00850:
        01.33: true
";
        let root = parse(text).unwrap();
        let keys = |keys: [&str; 5]| keys.map(str::to_string);
        assert_eq!(
            switched_on(root.as_hash().unwrap()),
            [
                keys(["PPU-abc", "Unlock FPS", "LittleBigPlanet 3", "BCES01663", "01.26"]),
                keys(["PPU-old", "From before values", "LittleBigPlanet 2", "BCES00850", "01.33"]),
            ]
        );
    }

    #[test]
    fn a_patch_for_one_game_keeps_that_games_serial_and_version() {
        let root = parse(LIST).unwrap();
        let found = match_title(root.as_hash().unwrap(), "BCES01663", "01.26", &[]);
        let fps = found.iter().find(|p| p.name == "Unlock FPS").unwrap();
        assert_eq!(fps.serial, "BCES01663");
        assert!(fps.fix.is_none(), "not one of Omoio's fixes, so it waits for the user");
        let keys = entry_keys(fps, "BCES01663", "01.26");
        assert_eq!(
            keys[2..],
            ["LittleBigPlanet 3".to_string(), "BCES01663".to_string(), "01.26".to_string()]
        );
    }
}
