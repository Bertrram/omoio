//! What the Dolphin wiki says about how well each Wii and GameCube game runs.
//!
//! Dolphin's compatibility list, dolphin-emu.org/compat, is made from its
//! wiki. Each game's page shows a rating of up to five stars, which files the
//! page under a category such as "5 stars (Rating)", and each id a disc names
//! itself by, such as SSPP52, is a redirect to the game's page. The wiki's
//! MediaWiki API hands over a console's games with both in one go: 10
//! requests for the Wii's 1654 games and 5 for the GameCube's 724 when this
//! was written.
//!
//! The wiki's text is under Creative Commons Attribution-ShareAlike 3.0, as
//! its footer and its API's rights information say. What is kept is a game's
//! name, its rating and its ids, the words for each rating are Omoio's own,
//! and the catalogue credits the wiki and names the licence.
//!
//! The wiki sits behind Anubis, which asks a client that calls itself a
//! browser to prove it is one and lets one that gives its own name through.
//! Omoio gives its own name, as it does to every site it asks.

use crate::core::catalogue::{Entry, Status};
use crate::core::console::Console;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};

const API: &str = "https://wiki.dolphin-emu.org/api.php";

/// The categories a game's rating files its page under. Zero stars is the
/// wiki saying nobody has rated the game.
const RATINGS: [&str; 6] = [
    "Category:0 stars (Rating)",
    "Category:1 stars (Rating)",
    "Category:2 stars (Rating)",
    "Category:3 stars (Rating)",
    "Category:4 stars (Rating)",
    "Category:5 stars (Rating)",
];

/// Bumped when a field is added, so an older copy is fetched again.
const CACHE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    fetched: u64,
    games: Vec<Game>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Game {
    page_id: u64,
    title: String,
    /// One to five, or zero when nobody has rated the game.
    stars: u8,
    /// The ids the game's discs name themselves by, one per release.
    ids: Vec<String>,
}

/// The wiki's category of each console's disc games. WiiWare and the
/// Virtual Console have categories of their own, and their games are
/// channels, which Omoio doesn't install.
fn category(console: Console) -> &'static str {
    match console {
        Console::GameCube => "Category:GameCube games",
        _ => "Category:Wii games",
    }
}

/// The credit under the catalogue, naming the licence the wiki's text is
/// under, and the wiki's own list of the console's games.
pub fn source(console: Console) -> (&'static str, &'static str) {
    match console {
        Console::GameCube => (
            "GameCube results from the Dolphin wiki (CC BY-SA 3.0)",
            "https://wiki.dolphin-emu.org/index.php?title=Category:GameCube_games",
        ),
        _ => (
            "Wii results from the Dolphin wiki (CC BY-SA 3.0)",
            "https://wiki.dolphin-emu.org/index.php?title=Category:Wii_games",
        ),
    }
}

/// What the wiki means by each number of stars, from its own table of them
/// (Template:Compatibility), in fewer words. The tones follow the colours
/// Dolphin's compatibility page gives each.
pub fn describe(stars: u8) -> Option<(&'static str, &'static str, &'static str)> {
    match stars {
        5 => Some(("Perfect", "go", "Runs with no problems at all.")),
        4 => Some((
            "Playable",
            "go",
            "Plays to the end, with minor graphics or sound glitches at most.",
        )),
        3 => Some((
            "Starts",
            "warn",
            "Starts and may play well, but crashes or has major graphics or sound glitches.",
        )),
        2 => Some((
            "Intro/Menu",
            "bad",
            "Hangs or crashes before the game itself starts.",
        )),
        1 => Some(("Broken", "bad", "Crashes as it boots.")),
        _ => None,
    }
}

/// What a rating means for someone about to import the game, said after
/// "Dolphin rates it Starts:". Empty for Perfect and Playable, which need no
/// warning.
pub fn caution(stars: u8) -> &'static str {
    match stars {
        3 => "it starts, but it may crash or have major glitches.",
        2 => "it hangs or crashes before the game starts.",
        1 => "it crashes as it boots.",
        _ => "",
    }
}

/// The page's title without the note the wiki adds when a game has a page
/// for each console, as in "Metroid Prime (GC)", since the catalogue names
/// the console itself.
fn shown_name(title: &str, console: Console) -> &str {
    let note = match console {
        Console::GameCube => " (GC)",
        _ => " (Wii)",
    };
    title.strip_suffix(note).unwrap_or(title)
}

/// Where a release was sold, from the fourth character of its id, read the
/// way Dolphin reads it (`CountryCodeToCountry`, DiscIO/Enums.cpp). The PAL
/// countries, Australia among them, count as Europe, as on the other lists.
fn region_of(id: &str, console: Console) -> Vec<&'static str> {
    match id.as_bytes().get(3) {
        // E is also a GameCube game in English sold in Korea on a few later
        // discs, which only the disc's revision tells apart.
        Some(b'E' | b'B' | b'N') => vec!["US"],
        // X, Y and Z are extra language versions, nearly all European; the
        // id alone can't tell the rare American one.
        Some(b'P' | b'D' | b'F' | b'I' | b'H' | b'R' | b'S' | b'U' | b'V' | b'L' | b'M' | b'X' | b'Y' | b'Z') => {
            vec!["EU"]
        }
        Some(b'J') => vec!["JP"],
        Some(b'K' | b'Q' | b'T') => vec!["KR"],
        // A GameCube game in English sold in Korea, or a Wii game in
        // Chinese sold in Taiwan.
        Some(b'W') if console == Console::GameCube => vec!["KR"],
        Some(b'W') => vec!["Asia"],
        // Sold everywhere.
        Some(b'A') => vec!["EU", "US", "JP"],
        _ => Vec::new(),
    }
}

/// One entry per release the wiki has an id for, so the catalogue lists each
/// and can add one to the library by it, as with RPCS3's list. A game the
/// wiki has no id for is one entry the catalogue can only offer to import.
fn entries_of(game: &Game, console: Console) -> Vec<Entry> {
    let status = describe(game.stars)
        .map(|(label, tone, explanation)| Status {
            label,
            tone,
            explanation,
            caution: caution(game.stars),
        })
        .unwrap_or_default();
    let name = shown_name(&game.title, console).to_string();
    if game.ids.is_empty() {
        return vec![Entry {
            console,
            key: format!("DW{}", game.page_id),
            name,
            named: true,
            title_id: String::new(),
            regions: Vec::new(),
            status,
            kind: "",
        }];
    }
    game.ids
        .iter()
        .map(|id| Entry {
            console,
            key: id.clone(),
            name: name.clone(),
            named: true,
            title_id: id.clone(),
            regions: region_of(id, console),
            status,
            kind: "",
        })
        .collect()
}

fn cache_path(app: &AppHandle, console: Console) -> Result<PathBuf, String> {
    let data = app.path().data_dir().map_err(|e| e.to_string())?;
    let name = match console {
        Console::GameCube => "dolphin-gamecube-compatibility.json",
        _ => "dolphin-wii-compatibility.json",
    };
    Ok(data.join("Omoio").join(name))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_cache(app: &AppHandle, console: Console) -> Option<Cache> {
    let text = std::fs::read_to_string(cache_path(app, console).ok()?).ok()?;
    let cache: Cache = serde_json::from_str(&text).ok()?;
    (cache.version == CACHE_VERSION).then_some(cache)
}

/// Every game in the console's list, as the catalogue takes them. `None`
/// until the list has been downloaded.
pub fn entries(app: &AppHandle, console: Console) -> Option<Vec<Entry>> {
    let cache = read_cache(app, console)?;
    Some(cache.games.iter().flat_map(|game| entries_of(game, console)).collect())
}

/// A page as the API's answers build it up. One page's categories and
/// redirects can come over several answers, so they are gathered by page.
#[derive(Debug, Default)]
struct Page {
    title: String,
    categories: Vec<String>,
    redirects: Vec<String>,
}

fn titles(page: &serde_json::Value, key: &str) -> Vec<String> {
    page.get(key)
        .and_then(|list| list.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("title")?.as_str().map(String::from))
        .collect()
}

/// Adds what one answer says about its pages to what is known of them.
fn gather(body: &serde_json::Value, pages: &mut BTreeMap<u64, Page>) {
    for page in body
        .pointer("/query/pages")
        .and_then(|pages| pages.as_array())
        .into_iter()
        .flatten()
    {
        let Some(page_id) = page.get("pageid").and_then(|v| v.as_u64()) else {
            continue;
        };
        let known = pages.entry(page_id).or_default();
        if let Some(title) = page.get("title").and_then(|v| v.as_str()) {
            known.title = title.to_string();
        }
        known.categories.extend(titles(page, "categories"));
        known.redirects.extend(titles(page, "redirects"));
    }
}

/// What to send back for the rest, as the answer's `continue` gives it.
/// Empty once the wiki has said everything.
fn continuation(body: &serde_json::Value) -> Vec<(String, String)> {
    body.get("continue")
        .and_then(|fields| fields.as_object())
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
        .collect()
}

/// The rating a page is filed under, or zero for none.
fn stars(categories: &[String]) -> u8 {
    categories
        .iter()
        .find_map(|category| {
            category
                .strip_prefix("Category:")?
                .strip_suffix(" stars (Rating)")?
                .parse::<u8>()
                .ok()
        })
        .filter(|stars| *stars <= 5)
        .unwrap_or(0)
}

/// The game's ids among the redirects to its page: six capitals and digits,
/// as a disc names itself. The others are other names for the game. The
/// wiki's own infobox picks its ids out the same way, by their six
/// characters (Template:Infobox VG).
fn game_ids(redirects: &[String]) -> Vec<String> {
    let mut ids: Vec<String> = redirects
        .iter()
        .filter(|name| name.len() == 6 && name.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
        .cloned()
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn game_of(page_id: u64, page: Page) -> Game {
    Game {
        page_id,
        stars: stars(&page.categories),
        ids: game_ids(&page.redirects),
        title: page.title,
    }
}

fn progress(app: &AppHandle, console: Console, done: u64, total: u64) {
    let _ = app.emit(
        "compat-progress",
        crate::core::types::Progress {
            stage: console.short().to_ascii_lowercase(),
            bytes: done.min(total),
            total,
        },
    );
}

async fn get(client: &reqwest::Client, params: &[(&str, &str)]) -> Result<serde_json::Value, String> {
    let url = reqwest::Url::parse_with_params(API, params).map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .header("User-Agent", "Omoio")
        .send()
        .await
        .map_err(|_| "Couldn't reach the Dolphin wiki.".to_string())?;
    if !response.status().is_success() {
        return Err("The Dolphin wiki isn't answering right now.".into());
    }
    // A page asking for proof of a browser is HTML, so it ends here rather
    // than being read as a list with no games.
    response
        .json()
        .await
        .map_err(|_| "The Dolphin wiki answered in a form Omoio doesn't understand.".to_string())
}

/// How many games the category holds, for the progress bar. Without it the
/// list is fetched all the same.
async fn category_size(client: &reqwest::Client, category: &str) -> Option<u64> {
    let body = get(
        client,
        &[
            ("action", "query"),
            ("titles", category),
            ("prop", "categoryinfo"),
            ("format", "json"),
            ("formatversion", "2"),
        ],
    )
    .await
    .ok()?;
    body.pointer("/query/pages/0/categoryinfo/pages")?.as_u64()
}

/// Every game in the console's category, with its rating and ids. `report`
/// is told how many games have come in and how many the category holds.
async fn fetch(console: Console, cancel: &AtomicBool, report: impl Fn(u64, u64)) -> Result<Vec<Game>, String> {
    // The Wii's list took 10 requests when this was written. The loop stops
    // when the wiki says there is no more; this is here so a change at their
    // end cannot spin forever.
    const MOST_REQUESTS: usize = 100;

    let client = reqwest::Client::new();
    let category = category(console);
    let total = category_size(&client, category).await.unwrap_or(0);
    let ratings = RATINGS.join("|");

    // Each answer covers up to 500 of the category's pages, each with its
    // rating and the redirects to it, and says how to ask for the rest.
    let mut pages = BTreeMap::new();
    let mut next: Vec<(String, String)> = Vec::new();
    for _ in 0..MOST_REQUESTS {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let mut params = vec![
            ("action", "query"),
            ("generator", "categorymembers"),
            ("gcmtitle", category),
            ("gcmnamespace", "0"),
            ("gcmlimit", "500"),
            ("prop", "categories|redirects"),
            ("clcategories", ratings.as_str()),
            ("cllimit", "max"),
            ("rdprop", "title"),
            ("rdnamespace", "0"),
            ("rdlimit", "max"),
            ("format", "json"),
            ("formatversion", "2"),
        ];
        params.extend(next.iter().map(|(key, value)| (key.as_str(), value.as_str())));
        let body = get(&client, &params).await?;
        gather(&body, &mut pages);
        report(pages.len() as u64, total);
        next = continuation(&body);
        if next.is_empty() {
            break;
        }
    }
    if pages.is_empty() {
        return Err("The Dolphin wiki listed no games.".into());
    }
    report(total, total);

    let mut games: Vec<Game> = pages.into_iter().map(|(page_id, page)| game_of(page_id, page)).collect();
    games.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(games)
}

/// Downloads the console's list and keeps what the catalogue shows. Returns
/// how many games it now knows about.
pub async fn refresh(app: &AppHandle, console: Console, cancel: &AtomicBool) -> Result<usize, String> {
    let games = fetch(console, cancel, |done, total| progress(app, console, done, total)).await?;
    let count = games.len();
    let path = cache_path(app, console)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let cache = Cache {
        version: CACHE_VERSION,
        fetched: now(),
        games,
    };
    std::fs::write(&path, serde_json::to_string(&cache).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::catalogue::group;
    use serde_json::json;

    /// Two answers in the shape the API gives them: one page in both, with
    /// its rating in the first and the rest of its redirects in the second,
    /// and no `continue` in the second, as in the last answer.
    fn answers() -> [serde_json::Value; 2] {
        [
            json!({
                "continue": { "rdcontinue": "7294|123", "continue": "gcmcontinue||" },
                "query": { "pages": [
                    {
                        "pageid": 7294,
                        "ns": 0,
                        "title": "Skylanders: Spyro's Adventure",
                        "categories": [{ "ns": 14, "title": "Category:5 stars (Rating)" }],
                        "redirects": [
                            { "pageid": 1, "ns": 0, "title": "SSPE52" },
                            { "pageid": 2, "ns": 0, "title": "SSPP52" }
                        ]
                    },
                    { "pageid": 9, "ns": 0, "title": "Metroid Prime (Wii)" }
                ]}
            }),
            json!({
                "batchcomplete": true,
                "query": { "pages": [
                    {
                        "pageid": 7294,
                        "ns": 0,
                        "title": "Skylanders: Spyro's Adventure",
                        "redirects": [
                            { "pageid": 3, "ns": 0, "title": "SSPJGD" },
                            { "pageid": 4, "ns": 0, "title": "Skylanders spyro's adventure" }
                        ]
                    },
                    {
                        "pageid": 9,
                        "ns": 0,
                        "title": "Metroid Prime (Wii)",
                        "categories": [{ "ns": 14, "title": "Category:4 stars (Rating)" }],
                        "redirects": [{ "pageid": 5, "ns": 0, "title": "R3IE01" }]
                    }
                ]}
            }),
        ]
    }

    fn gathered() -> Vec<Game> {
        let mut pages = BTreeMap::new();
        for answer in answers() {
            gather(&answer, &mut pages);
        }
        pages.into_iter().map(|(id, page)| game_of(id, page)).collect()
    }

    #[test]
    fn a_page_split_over_answers_is_put_back_together() {
        let games = gathered();
        assert_eq!(games.len(), 2);
        let spyro = games.iter().find(|game| game.page_id == 7294).unwrap();
        assert_eq!(spyro.stars, 5);
        assert_eq!(spyro.ids, ["SSPE52", "SSPJGD", "SSPP52"], "ids only, sorted");
        let metroid = games.iter().find(|game| game.page_id == 9).unwrap();
        assert_eq!(metroid.stars, 4);
        assert_eq!(metroid.ids, ["R3IE01"]);
    }

    #[test]
    fn the_rest_is_asked_for_with_what_the_wiki_sent_back() {
        let [first, last] = answers();
        let mut next = continuation(&first);
        next.sort();
        assert_eq!(
            next,
            [
                ("continue".to_string(), "gcmcontinue||".to_string()),
                ("rdcontinue".to_string(), "7294|123".to_string())
            ]
        );
        assert!(continuation(&last).is_empty(), "the last answer has no continue");
    }

    #[test]
    fn a_page_with_no_rating_or_zero_stars_has_none() {
        let filed = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(stars(&filed(&[])), 0);
        assert_eq!(stars(&filed(&["Category:0 stars (Rating)"])), 0);
        assert_eq!(stars(&filed(&["Category:3 stars (Rating)"])), 3);
        assert_eq!(stars(&filed(&["Category:9 stars (Rating)"])), 0, "not a rating the wiki gives");
        assert!(describe(0).is_none());
    }

    #[test]
    fn only_redirects_named_like_a_disc_are_ids() {
        let redirects: Vec<String> = [
            "SSPP52",
            "Skylanders giants",
            "Jumper",
            "NDDemo",
            "ボンバーマン",
            "SSPP52",
            "G2MEAB",
            "SSPP5",
        ]
        .iter()
        .map(|n| n.to_string())
        .collect();
        assert_eq!(game_ids(&redirects), ["G2MEAB", "SSPP52"]);
    }

    #[test]
    fn the_wikis_console_note_is_left_off_the_name() {
        assert_eq!(shown_name("Metroid Prime (GC)", Console::GameCube), "Metroid Prime");
        assert_eq!(shown_name("Metroid Prime (Wii)", Console::Wii), "Metroid Prime");
        assert_eq!(shown_name("Metroid Prime (GC)", Console::Wii), "Metroid Prime (GC)");
        assert_eq!(
            shown_name("Metroid Prime (Metroid Prime: Trilogy)", Console::Wii),
            "Metroid Prime (Metroid Prime: Trilogy)",
            "a version of its own keeps its note"
        );
    }

    #[test]
    fn an_id_says_where_its_release_was_sold() {
        for (id, console, want) in [
            ("SSPE52", Console::Wii, vec!["US"]),
            ("SSPP52", Console::Wii, vec!["EU"]),
            ("SSPX52", Console::Wii, vec!["EU"]),
            ("SK8D52", Console::Wii, vec!["EU"]),
            ("SSPJGD", Console::Wii, vec!["JP"]),
            ("RMGK01", Console::Wii, vec!["KR"]),
            ("RSBW01", Console::Wii, vec!["Asia"]),
            ("GZLW01", Console::GameCube, vec!["KR"]),
            ("GALE01", Console::GameCube, vec!["US"]),
            ("RXXA01", Console::Wii, vec!["EU", "US", "JP"]),
            ("GZLO01", Console::GameCube, vec![]),
            ("GZL", Console::GameCube, vec![]),
        ] {
            assert_eq!(region_of(id, console), want, "{id}");
        }
    }

    #[test]
    fn each_release_is_listed_and_one_game_shows_once() {
        let entries: Vec<Entry> = gathered().iter().flat_map(|game| entries_of(game, Console::Wii)).collect();
        assert_eq!(entries.len(), 4, "three Spyro releases and one Metroid");
        let listings = group(entries);
        assert_eq!(listings.len(), 2);
        let spyro = listings.iter().find(|l| l.name == "Skylanders: Spyro's Adventure").unwrap();
        assert_eq!(spyro.regions, ["EU", "US", "JP"]);
        let ids: Vec<&str> = spyro.releases.iter().map(|r| r.title_id.as_str()).collect();
        assert_eq!(ids, ["SSPP52", "SSPE52", "SSPJGD"], "Europe's first, as on the other lists");
        assert_eq!(spyro.key, "SSPP52");
        assert_eq!((spyro.status.label, spyro.status.tone), ("Perfect", "go"));
        assert_eq!(spyro.console_name, "Wii");
        assert!(listings.iter().any(|l| l.name == "Metroid Prime"));
    }

    #[test]
    fn a_game_with_no_id_is_one_entry_by_its_page() {
        let game = Game {
            page_id: 31,
            title: "Phantasy Star Online Episode I & II Plus".into(),
            stars: 0,
            ids: Vec::new(),
        };
        let entries = entries_of(&game, Console::GameCube);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "DW31");
        assert_eq!(entries[0].title_id, "");
        assert!(entries[0].regions.is_empty());
        assert_eq!(entries[0].status.tone, "", "nobody has rated it");
    }

    #[test]
    fn every_rating_the_wiki_gives_has_words_of_our_own() {
        for stars in 1..=5 {
            let (label, tone, explanation) = describe(stars).unwrap();
            assert!(!label.is_empty());
            assert!(matches!(tone, "go" | "warn" | "bad"), "{stars} stars has tone {tone}");
            assert!(explanation.ends_with('.'), "{stars} stars should read as a sentence");
            let caution = caution(stars);
            if tone == "go" {
                assert_eq!(caution, "", "{stars} stars");
            } else {
                assert!(caution.starts_with("it ") && caution.ends_with('.'), "{stars} stars: {caution:?}");
            }
        }
    }

    /// Asks the real wiki for the Wii's list, the only check that the
    /// requests and the reading work against the wiki itself:
    ///   cargo test reaches_the_dolphin_wiki -- --ignored --nocapture
    #[test]
    #[ignore = "asks the Dolphin wiki for the Wii's list"]
    fn reaches_the_dolphin_wiki_and_reads_the_wii_list() {
        let cancel = AtomicBool::new(false);
        let reports = std::sync::Mutex::new(Vec::new());
        let games = tauri::async_runtime::block_on(fetch(Console::Wii, &cancel, |done, total| {
            reports.lock().unwrap().push((done, total));
        }))
        .unwrap();
        let rated = games.iter().filter(|game| game.stars > 0).count();
        let ids: usize = games.iter().map(|game| game.ids.len()).sum();
        println!("{} games, {rated} rated, {ids} ids, progress {:?}", games.len(), reports.lock().unwrap());
        assert!(games.len() > 1500 && rated > 1400 && ids > 3000);
        let spyro = games.iter().find(|game| game.title == "Skylanders: Spyro's Adventure").unwrap();
        assert!(spyro.stars >= 4, "rated {}", spyro.stars);
        for id in ["SSPE52", "SSPP52"] {
            assert!(spyro.ids.iter().any(|known| known == id), "{id} in {:?}", spyro.ids);
        }
        let last = *reports.lock().unwrap().last().unwrap();
        assert!(last.0 == last.1 && last.1 > 1500, "the bar ends full: {last:?}");
    }

    #[test]
    fn each_console_has_its_own_list_and_credit() {
        assert_eq!(category(Console::Wii), "Category:Wii games");
        assert_eq!(category(Console::GameCube), "Category:GameCube games");
        let (wii, gamecube) = (source(Console::Wii), source(Console::GameCube));
        assert_ne!(wii.0, gamecube.0, "both credits show at once");
        for (label, url) in [wii, gamecube] {
            assert!(label.contains("Dolphin wiki") && label.contains("CC BY-SA 3.0"), "{label}");
            assert!(url.starts_with("https://wiki.dolphin-emu.org/"), "{url}");
        }
    }
}
