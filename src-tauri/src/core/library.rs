//! Our own record of what the user has imported. Games are referenced where
//! they already live rather than copied, so a dump on an external drive is
//! normal and an entry whose path is missing is expected, not an error.

use crate::core::console::Console;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    /// Which console the game is for, and so which emulator runs it. A library
    /// written before this existed holds PS3 games only, so missing means PS3.
    #[serde(default)]
    pub console: Console,
    pub title_id: String,
    pub title: String,
    /// The version of the dump itself, out of its PARAM.SFO.
    pub version: Option<String>,
    /// The official update Omoio installed, if any.
    ///
    /// An update lives in the emulator's own storage rather than in the dump,
    /// so the dump keeps saying what it always said. RPCS3's game-data folder
    /// does hold a version, but it is written for reasons of its own and did
    /// not match either the disc or anything we installed, so it is not
    /// something to read a version out of. What we put there ourselves is.
    #[serde(default)]
    pub update_version: Option<String>,
    pub path: PathBuf,
    pub size_bytes: u64,
}

impl Game {
    /// The version that actually runs, which is the update when there is one.
    pub fn running_version(&self) -> Option<&str> {
        self.update_version
            .as_deref()
            .or(self.version.as_deref())
    }

    /// Whether this entry has files behind it.
    ///
    /// A game added from the catalogue has none yet: it is a note that the user
    /// owns it, waiting for them to import it. That is a different thing from a
    /// game whose drive is unplugged, and the interface has to say so
    /// differently or the two look like the same fault.
    pub fn is_set_up(&self) -> bool {
        !self.path.as_os_str().is_empty()
    }

    /// An entry for a game the user says they own but has not imported.
    pub fn not_set_up(console: Console, title_id: String, title: String) -> Self {
        Self {
            console,
            title_id,
            title,
            version: None,
            update_version: None,
            path: PathBuf::new(),
            size_bytes: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct Library {
    games: Vec<Game>,
    /// Entries this build can't read, such as a game for a console a newer
    /// Omoio knows. They are written back as they came, so saving the list
    /// here never takes them from the build that can show them. On 8 October
    /// 2026 a build that didn't know Wii games yet emptied a whole library
    /// over one of these.
    unknown: Vec<Value>,
    /// Fields of a readable entry that this build doesn't know, such as one
    /// a newer Omoio added, by title id. Written back with the entry.
    fields: HashMap<String, Map<String, Value>>,
    /// Whatever the file holds beside the list of games, written back as it
    /// came, so a newer Omoio's file keeps what it added.
    rest: Map<String, Value>,
    /// The file was there but couldn't be opened, as while something else
    /// has it locked. Saving would put a short list over the real one.
    unopened: bool,
}

/// Why the list differs from the file as it was, so a list that comes back
/// shorter, or empty, is never a mystery to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// The file was gone and the copy set aside last reads now, so the list
    /// came back from it.
    Restored,
    /// Some entries couldn't be read. The rest loaded and a copy of the file
    /// was kept.
    Partly,
    /// Nothing in the file could be read. It was set aside and the list
    /// starts empty.
    Whole,
    /// The file was there but couldn't be opened, so the list is empty for
    /// now and nothing is saved over the file.
    Unopened,
}

/// What the last load that kept something did, until the library screen
/// asks. Many places load the library and only that one tells the user.
static NOTICE: Mutex<Option<Kept>> = Mutex::new(None);

/// Loads run on several threads at once. Without this two of them could
/// each set aside the same file.
static LOADING: Mutex<()> = Mutex::new(());

/// One change at a time, from loading the library to saving it (`edit`).
static EDITING: Mutex<()> = Mutex::new(());

/// Each write's own temporary file, so two writes at once never rename each
/// other's half-written one.
static WRITES: AtomicU32 = AtomicU32::new(0);

/// The library as written, with the unknown entries after the rest.
#[derive(Serialize)]
struct Written<'a> {
    games: Vec<Entry<'a>>,
    #[serde(flatten)]
    rest: &'a Map<String, Value>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Entry<'a> {
    Known(Known<'a>),
    Unknown(&'a Value),
}

#[derive(Serialize)]
struct Known<'a> {
    #[serde(flatten)]
    game: &'a Game,
    #[serde(flatten)]
    fields: Option<&'a Map<String, Value>>,
}

impl Library {
    pub fn games(&self) -> &[Game] {
        &self.games
    }

    /// Re-importing a game the user already has replaces the entry instead of
    /// adding a second copy of it.
    pub fn upsert(&mut self, game: Game) {
        match self.games.iter_mut().find(|g| g.title_id == game.title_id) {
            Some(existing) => *existing = game,
            None => self.games.push(game),
        }
    }

    pub fn get_mut(&mut self, title_id: &str) -> Option<&mut Game> {
        self.games.iter_mut().find(|g| g.title_id == title_id)
    }

    pub fn remove(&mut self, title_id: &str) -> bool {
        let before = self.games.len();
        self.games.retain(|g| g.title_id != title_id);
        self.fields.remove(title_id);
        self.games.len() != before
    }

    /// Forgets every game, those this build can't read as well.
    pub fn forget_all(&mut self) {
        self.games.clear();
        self.unknown.clear();
        self.fields.clear();
    }

    /// Loads the library at `path`, lets `change` change it and saves it,
    /// with no other change of Omoio's in between. With each change loading
    /// and saving on its own, a folder scan that kept its copy for a whole
    /// drive saved over every import, removal and version learned meanwhile.
    pub fn edit<T>(path: &Path, change: impl FnOnce(&mut Library) -> Result<T, String>) -> Result<T, String> {
        let _turn = EDITING.lock().unwrap_or_else(PoisonError::into_inner);
        let mut library = Self::load(path);
        let done = change(&mut library)?;
        library.save(path)?;
        Ok(done)
    }

    /// A library we cannot read is kept aside rather than overwritten, what
    /// it holds that can be read still loads, and the library screen is told.
    pub fn load(path: &Path) -> Self {
        let (library, kept) = Self::load_keeping(path);
        if kept.is_some() {
            *NOTICE.lock().unwrap_or_else(PoisonError::into_inner) = kept;
        }
        library
    }

    /// What the last load kept, once.
    pub fn take_notice() -> Option<Kept> {
        NOTICE.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    fn load_keeping(path: &Path) -> (Self, Option<Kept>) {
        let _turn = LOADING.lock().unwrap_or_else(PoisonError::into_inner);
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return restore(path),
            Err(_) => return (Self { unopened: true, ..Self::default() }, Some(Kept::Unopened)),
        };
        match parse(&bytes) {
            Some(library) if library.unknown.is_empty() => (library, None),
            Some(library) => {
                // Kept the first time only. Every load after it meets the
                // same entries, and the next save carries them on anyway.
                if copies(path).iter().any(|copy| holds(copy, &library.unknown)) {
                    return (library, None);
                }
                let _ = std::fs::copy(path, aside(path));
                (library, Some(Kept::Partly))
            }
            // Left in place, the next save would write over it.
            None => match std::fs::rename(path, aside(path)) {
                Ok(()) => (Self::default(), Some(Kept::Whole)),
                Err(_) => (Self { unopened: true, ..Self::default() }, Some(Kept::Unopened)),
            },
        }
    }

    /// Written to a temporary file and renamed over the original, so losing
    /// power mid-write leaves the previous library intact rather than a
    /// half-written one.
    fn save(&self, path: &Path) -> Result<(), String> {
        if self.unopened {
            return Err("Omoio couldn't open your game list, so it left it as it was. Try again in a moment.".to_string());
        }
        let known = self.games.iter().map(|game| {
            Entry::Known(Known {
                game,
                fields: self.fields.get(&game.title_id),
            })
        });
        let games = known.chain(self.unknown.iter().map(Entry::Unknown)).collect();
        let text = serde_json::to_string_pretty(&Written { games, rest: &self.rest }).map_err(|e| e.to_string())?;
        write(path, text.as_bytes())
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let number = WRITES.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("json.writing-{}-{number}", std::process::id()));
    let written = std::fs::write(&temp, bytes).and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written.map_err(|e| e.to_string())
}

/// Read entry by entry, so an entry this build doesn't know costs only that
/// entry. `None` when the file isn't a library at all.
fn parse(bytes: &[u8]) -> Option<Library> {
    let Value::Object(mut rest) = serde_json::from_slice(bytes).ok()? else {
        return None;
    };
    let entries = match rest.remove("games") {
        None => Vec::new(),
        Some(Value::Array(entries)) => entries,
        Some(_) => return None,
    };
    let mut library = Library { rest, ..Library::default() };
    for entry in entries {
        match serde_json::from_value::<Game>(entry.clone()) {
            Ok(mut game) => {
                // A Wii U Skylanders game imported under a name that doesn't
                // say so, before Omoio knew it by its title id, takes its
                // English name, so it gets its portal without being imported
                // again (`figures::english_name`).
                if let Some(name) = crate::core::figures::english_name(game.console, &game.title_id, &game.title) {
                    game.title = name.to_string();
                }
                let fields = fields_unknown(&game, entry);
                if !fields.is_empty() {
                    library.fields.insert(game.title_id.clone(), fields);
                }
                library.games.push(game);
            }
            Err(_) => library.unknown.push(entry),
        }
    }
    Some(library)
}

/// The fields of `entry` that `game`, read from it, doesn't hold.
fn fields_unknown(game: &Game, entry: Value) -> Map<String, Value> {
    let (Value::Object(mut fields), Ok(Value::Object(known))) = (entry, serde_json::to_value(game)) else {
        return Map::new();
    };
    fields.retain(|name, _| !known.contains_key(name));
    fields
}

/// With the file gone, the copy set aside last is taken back if it reads now,
/// as when the build that couldn't read it has been replaced by one that can.
/// Once only: the copy is then named `.restored` rather than `.unreadable`,
/// with what it holds unchanged, so a list deleted on purpose after that
/// stays deleted.
fn restore(path: &Path) -> (Library, Option<Kept>) {
    let newest = copies(path)
        .into_iter()
        .max_by_key(|copy| std::fs::metadata(copy).and_then(|m| m.modified()).ok());
    let Some((library, bytes, copy)) = newest.and_then(|copy| {
        let bytes = std::fs::read(&copy).ok()?;
        parse(&bytes).map(|library| (library, bytes, copy))
    }) else {
        return (Library::default(), None);
    };
    if library.games.is_empty() && library.unknown.is_empty() {
        return (Library::default(), None);
    }
    if write(path, &bytes).is_ok() {
        if let Some(name) = copy.file_name().map(|n| n.to_string_lossy().replacen(".unreadable", ".restored", 1)) {
            let _ = std::fs::rename(&copy, copy.with_file_name(name));
        }
    }
    (library, Some(Kept::Restored))
}

/// Every copy set aside beside `path`: `library.json.unreadable` from before
/// copies were dated, and the dated ones since.
fn copies(path: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Vec::new();
    };
    let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
    let prefix = format!("{}.unreadable", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        .collect()
}

/// Whether a copy already holds every one of these entries.
fn holds(copy: &Path, unknown: &[Value]) -> bool {
    let Some(kept) = std::fs::read(copy).ok().and_then(|bytes| parse(&bytes)) else {
        return false;
    };
    unknown.iter().all(|entry| kept.unknown.contains(entry))
}

/// A name no earlier copy has, dated so the copies can be told apart:
/// `library.json.unreadable-2026-10-08`, then `-2` and on for another copy
/// the same day.
fn aside(path: &Path) -> PathBuf {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!(
        "{}.unreadable-{}",
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        date(seconds)
    );
    let mut candidate = path.with_file_name(&name);
    let mut count = 2;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{name}-{count}"));
        count += 1;
    }
    candidate
}

/// The UTC date of a Unix time, by Howard Hinnant's `civil_from_days`
/// (howardhinnant.github.io/date_algorithms.html), so naming a copy needs no
/// date crate.
fn date(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64 + 719_468;
    let era = days / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(title_id: &str, title: &str) -> Game {
        Game {
            console: Console::Ps3,
            title_id: title_id.to_string(),
            title: title.to_string(),
            version: Some("01.00".to_string()),
            update_version: None,
            path: PathBuf::from(format!("D:\\games\\{title_id}")),
            size_bytes: 8_200_000_000,
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omoio-lib-{}-{name}.json", std::process::id()))
    }

    #[test]
    fn reimporting_replaces_rather_than_duplicates() {
        let mut library = Library::default();
        library.upsert(game("BCES00141", "LittleBigPlanet"));
        library.upsert(game("BCES00850", "LittleBigPlanet 2"));

        let mut updated = game("BCES00141", "LittleBigPlanet");
        updated.version = Some("02.00".to_string());
        library.upsert(updated);

        assert_eq!(library.games().len(), 2);
        let entry = library.games().iter().find(|g| g.title_id == "BCES00141").unwrap();
        assert_eq!(entry.version.as_deref(), Some("02.00"));
    }

    #[test]
    fn an_installed_update_is_the_version_that_runs() {
        let mut entry = game("BCES00850", "LittleBigPlanet 2");
        assert_eq!(entry.running_version(), Some("01.00"));

        // The dump still says what it always said; the update is what runs.
        entry.update_version = Some("01.33".to_string());
        assert_eq!(entry.running_version(), Some("01.33"));
        assert_eq!(entry.version.as_deref(), Some("01.00"));
    }

    #[test]
    fn removes_by_title_id() {
        let mut library = Library::default();
        library.upsert(game("BCES00141", "LittleBigPlanet"));
        assert!(library.remove("BCES00141"));
        assert!(!library.remove("BCES00141"));
        assert!(library.games().is_empty());
    }

    #[test]
    fn survives_a_round_trip_to_disk() {
        let path = temp_path("roundtrip");
        let mut library = Library::default();
        library.upsert(game("BCES00141", "LittleBigPlanet"));
        library.save(&path).unwrap();

        let loaded = Library::load(&path);
        assert_eq!(loaded.games(), library.games());
        let _ = std::fs::remove_file(&path);
    }

    /// Written before consoles were recorded. It must load, and load as what
    /// it was: PS3 games.
    #[test]
    fn a_library_from_before_consoles_loads_as_ps3() {
        let path = temp_path("before-consoles");
        std::fs::write(
            &path,
            r#"{"games":[{"title_id":"BCES00850","title":"LittleBigPlanet 2","version":"01.00","path":"D:\\games\\lbp2","size_bytes":1}]}"#,
        )
        .unwrap();

        let loaded = Library::load(&path);
        assert_eq!(loaded.games().len(), 1);
        assert_eq!(loaded.games()[0].console, Console::Ps3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_library_is_an_empty_one() {
        assert!(Library::load(Path::new("no-such-library.json")).games().is_empty());
    }

    /// A folder of its own, as `%APPDATA%\Omoio` is, so the copies set aside
    /// beside the library can be counted.
    fn temp_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("omoio-lib-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn copies_in(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("library.json.unreadable"))
            .collect();
        names.sort();
        names
    }

    const TWO_GAMES: &str = r#"{"games":[
        {"console":"ps3","title_id":"BCES00850","title":"LittleBigPlanet 2","version":"01.00","path":"D:\\games\\lbp2","size_bytes":1},
        {"console":"wii","title_id":"SSPE52","title":"Skylanders: Spyro's Adventure","version":null,"path":"D:\\games\\ssa.wbfs","size_bytes":2}
    ]}"#;

    #[test]
    fn an_unreadable_library_is_set_aside_not_overwritten() {
        let folder = temp_folder("corrupt");
        let path = folder.join("library.json");
        std::fs::write(&path, "{ this is not json").unwrap();

        let (library, kept) = Library::load_keeping(&path);
        assert!(library.games().is_empty());
        assert_eq!(kept, Some(Kept::Whole));
        assert!(!path.exists());
        let copies = copies_in(&folder);
        assert_eq!(copies.len(), 1, "the unreadable file should be kept for recovery");
        assert_eq!(std::fs::read_to_string(folder.join(&copies[0])).unwrap(), "{ this is not json");

        let _ = std::fs::remove_dir_all(&folder);
    }

    /// What happened on 8 October 2026: the copy a build couldn't read is
    /// fine for the build after it, and the library is gone.
    #[test]
    fn a_copy_that_reads_now_comes_back() {
        let folder = temp_folder("restore");
        let path = folder.join("library.json");
        let copy = folder.join("library.json.unreadable");
        std::fs::write(&copy, TWO_GAMES).unwrap();

        let (library, kept) = Library::load_keeping(&path);
        assert_eq!(kept, Some(Kept::Restored));
        assert_eq!(library.games().len(), 2);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), TWO_GAMES);
        let restored = folder.join("library.json.restored");
        assert_eq!(std::fs::read_to_string(&restored).unwrap(), TWO_GAMES, "the copy stays, renamed");
        assert!(copies_in(&folder).is_empty());

        // Back in place, it is an ordinary library again.
        let (again, kept) = Library::load_keeping(&path);
        assert_eq!(kept, None);
        assert_eq!(again.games(), library.games());

        // Deleted on purpose after that, it stays deleted.
        std::fs::remove_file(&path).unwrap();
        let (gone, kept) = Library::load_keeping(&path);
        assert_eq!(kept, None);
        assert!(gone.games().is_empty());
        assert!(!path.exists());

        let _ = std::fs::remove_dir_all(&folder);
    }

    /// What a newer Omoio adds, to an entry or beside the list, is written
    /// back as it came by a build that doesn't know it.
    #[test]
    fn what_a_newer_omoio_added_is_kept() {
        let folder = temp_folder("newer");
        let path = folder.join("library.json");
        std::fs::write(
            &path,
            r#"{"games":[
                {"console":"ps3","title_id":"BCES00850","title":"LittleBigPlanet 2","version":"01.00","path":"D:\\games\\lbp2","size_bytes":1,"played_minutes":90},
                {"console":"ps3","title_id":"BCES00141","title":"LittleBigPlanet","version":"01.00","path":"D:\\games\\lbp","size_bytes":2,"favourite":true}
            ],"collections":[{"name":"Mine","games":["BCES00850"]}]}"#,
        )
        .unwrap();

        Library::edit(&path, |library| {
            library.upsert(game("BCES00850", "LittleBigPlanet 2"));
            library.remove("BCES00141");
            library.upsert(game("BCUS98148", "LittleBigPlanet 3"));
            Ok(())
        })
        .unwrap();

        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["collections"], serde_json::json!([{"name":"Mine","games":["BCES00850"]}]));
        let games = saved["games"].as_array().unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0]["title_id"], "BCES00850");
        assert_eq!(games[0]["played_minutes"], 90, "kept through a re-import");
        assert_eq!(games[1]["title_id"], "BCUS98148");
        assert!(games[1].get("favourite").is_none(), "gone with the game it was on");
        assert_eq!(Library::load_keeping(&path).1, None);

        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Two changes at once both land, rather than the later save putting
    /// back the list from before the other.
    #[test]
    fn changes_at_once_both_stay() {
        let folder = temp_folder("at-once");
        let path = folder.join("library.json");
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let path = path.clone();
                std::thread::spawn(move || {
                    Library::edit(&path, |library| {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        library.upsert(game(&format!("BCES0000{n}"), "A game"));
                        Ok(())
                    })
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
        assert_eq!(Library::load(&path).games().len(), 8);
        let left: Vec<_> = std::fs::read_dir(&folder).unwrap().flatten().map(|entry| entry.file_name()).collect();
        assert_eq!(left, ["library.json"], "no temporary file left behind");

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_copy_that_still_cannot_be_read_stays_a_copy() {
        let folder = temp_folder("no-restore");
        let path = folder.join("library.json");
        std::fs::write(folder.join("library.json.unreadable"), "{ still not json").unwrap();

        let (library, kept) = Library::load_keeping(&path);
        assert!(library.games().is_empty());
        assert_eq!(kept, None);
        assert!(!path.exists());

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_second_failure_keeps_both_copies() {
        let folder = temp_folder("twice");
        let path = folder.join("library.json");
        std::fs::write(folder.join("library.json.unreadable"), "first, from before copies were dated").unwrap();

        std::fs::write(&path, "second").unwrap();
        assert_eq!(Library::load_keeping(&path).1, Some(Kept::Whole));
        std::fs::write(&path, "third").unwrap();
        assert_eq!(Library::load_keeping(&path).1, Some(Kept::Whole));

        let copies = copies_in(&folder);
        assert_eq!(copies.len(), 3);
        let mut texts: Vec<String> = copies
            .iter()
            .map(|name| std::fs::read_to_string(folder.join(name)).unwrap())
            .collect();
        texts.sort();
        assert_eq!(texts, ["first, from before copies were dated", "second", "third"]);

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_entry_this_build_cannot_read_costs_only_that_entry() {
        let folder = temp_folder("partly");
        let path = folder.join("library.json");
        let text = r#"{"games":[
            {"console":"ps3","title_id":"BCES00850","title":"LittleBigPlanet 2","version":"01.00","path":"D:\\games\\lbp2","size_bytes":1},
            {"console":"switch","title_id":"0100000000010000","title":"A console Omoio doesn't know","version":null,"path":"D:\\games\\x","size_bytes":3},
            {"console":"wiiu","title_id":"0005000010101A00","title":"No path or size"},
            {"console":"gamecube","title_id":"GALE01","title":"Super Smash Bros. Melee","version":null,"path":"D:\\games\\melee.iso","size_bytes":4}
        ]}"#;
        std::fs::write(&path, text).unwrap();

        let (library, kept) = Library::load_keeping(&path);
        assert_eq!(kept, Some(Kept::Partly));
        let ids: Vec<&str> = library.games().iter().map(|g| g.title_id.as_str()).collect();
        assert_eq!(ids, ["BCES00850", "GALE01"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "loading changes nothing");
        let copies = copies_in(&folder);
        assert_eq!(copies.len(), 1);
        assert_eq!(std::fs::read_to_string(folder.join(&copies[0])).unwrap(), text);

        // Said once: the copy already holds these entries.
        assert_eq!(Library::load_keeping(&path).1, None);

        // Saving carries the unknown entries on, so nothing is lost even
        // without the copy, and that still makes no second copy.
        let mut library = library;
        library.remove("GALE01");
        library.save(&path).unwrap();
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let saved_ids: Vec<&str> = saved["games"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["title_id"].as_str().unwrap())
            .collect();
        assert_eq!(saved_ids, ["BCES00850", "0100000000010000", "0005000010101A00"]);
        assert_eq!(saved["games"][2], serde_json::json!({"console":"wiiu","title_id":"0005000010101A00","title":"No path or size"}));
        let (reloaded, kept) = Library::load_keeping(&path);
        assert_eq!(kept, None);
        assert_eq!(reloaded.games().len(), 1);
        assert_eq!(copies_in(&folder).len(), 1);

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_good_library_is_left_alone() {
        let folder = temp_folder("good");
        let path = folder.join("library.json");
        std::fs::write(&path, TWO_GAMES).unwrap();

        let (library, kept) = Library::load_keeping(&path);
        assert_eq!(kept, None);
        assert_eq!(library.games().len(), 2);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), TWO_GAMES);
        assert!(copies_in(&folder).is_empty());

        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Something holding the file open must not lead to a short list being
    /// saved over it. A folder in the file's place can't be read either.
    #[test]
    fn a_library_that_cannot_be_opened_is_never_saved_over() {
        let folder = temp_folder("unopened");
        let path = folder.join("library.json");
        std::fs::create_dir_all(&path).unwrap();

        let (mut library, kept) = Library::load_keeping(&path);
        assert_eq!(kept, Some(Kept::Unopened));
        library.upsert(game("BCES00141", "LittleBigPlanet"));
        assert!(library.save(&path).is_err());
        let edited = Library::edit(&path, |library| {
            library.upsert(game("BCES00141", "LittleBigPlanet"));
            Ok(())
        });
        assert!(edited.is_err());
        assert!(path.is_dir());
        assert!(copies_in(&folder).is_empty());

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn copies_are_named_by_the_date() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_709_164_800), "2024-02-29");
        assert_eq!(date(1_791_417_600 + 86_399), "2026-10-08");
    }
}
