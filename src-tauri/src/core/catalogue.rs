//! The catalogue: every title the emulators' compatibility lists know about,
//! put together so one game is one entry however many times it was released.
//!
//! Nothing here reads a file or the network. Each backend hands over what its
//! list says, and this decides what is shown and in what order.

use crate::core::console::{Console, Features};
use crate::core::figures::{has_portal_menu, is_skylanders, portal_menu_note};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The regions a release can be from, in the order releases are listed.
const REGIONS: [&str; 5] = ["EU", "US", "JP", "Asia", "KR"];

/// How many games a page shows when nobody has asked for more.
const PAGE: usize = 60;

/// How well a title runs, in the words of the list it came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Status {
    pub label: &'static str,
    /// "go", "warn" or "bad". Empty when nobody has reported on the title.
    pub tone: &'static str,
    pub explanation: &'static str,
    /// What the result means for someone about to import the game, to follow
    /// "RPCS3 rates it Ingame:". Empty for a result that needs no warning.
    pub caution: &'static str,
}

/// One title as a list reports it, before the releases of a game are put
/// together.
#[derive(Debug, Clone)]
pub struct Entry {
    pub console: Console,
    /// Unique within its console's list. Covers are cached under it.
    pub key: String,
    pub name: String,
    /// False when the list had no name and `name` is the title id.
    pub named: bool,
    /// Empty when the list has no title id for the title.
    pub title_id: String,
    pub regions: Vec<&'static str>,
    pub status: Status,
    /// "Virtual Console" for an older console's game sold again, else empty.
    pub kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Release {
    pub title_id: String,
    pub region: &'static str,
}

/// One game in the catalogue, with every release of it the list knows.
#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub console: Console,
    pub console_name: &'static str,
    pub key: String,
    pub name: String,
    pub named: bool,
    /// The best any release of it is reported to do.
    pub status: Status,
    pub kind: &'static str,
    pub regions: Vec<&'static str>,
    /// Every release with a title id, the chosen region's first.
    pub releases: Vec<Release>,
    pub demo: bool,
    /// Filled in by the caller, which knows the library.
    pub owned: bool,
    /// Filled in by the caller, which knows the backends.
    pub features: Features,
    /// For a Skylanders game, whether Omoio's portal menu works in it, which
    /// its tile says. `None` for any other game.
    pub portal_menu: Option<bool>,
    /// For a Skylanders game the menu doesn't work in, the line saying where
    /// it does, as a game's page says it.
    pub portal_note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub query: String,
    pub console: Option<Console>,
    /// One of the labels in `REGIONS`, or empty for every region.
    pub region: String,
    /// A tone, "go", "warn" or "bad", or empty for any result.
    pub runs: String,
    pub hide_demos: bool,
    /// "runs" puts what runs best first. Anything else sorts by name.
    pub sort: String,
    /// How many to return. Zero means one page.
    pub limit: usize,
}

/// A name with case and punctuation folded away, so "LittleBigPlanet™ 2" and
/// "LITTLEBIGPLANET 2" are the same game.
pub fn fold(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a bracketed note may be made of when it only says where a copy was
/// sold or which languages it has: "(Europe)", "(En,Fr,De)", "[USA]".
const REGION_NOTES: [&str; 36] = [
    "europe", "usa", "us", "eu", "uk", "japan", "jp", "world", "pal", "ntsc", "asia", "australia",
    "korea", "america", "north", "en", "fr", "de", "es", "it", "nl", "pt", "sv", "no", "da", "fi",
    "ru", "pl", "ja", "ko", "zh", "el", "tr", "cs", "hu", "ar",
];

/// Edition and platform words a port adds to the end of the same game's name:
/// "Minecraft: Wii U Edition", "Super Smash Bros. for Wii U", "Trine 2:
/// Director's Cut". Longest first, so "for wii u" goes before "wii u" does.
const PORT_WORDS: [&[&str]; 7] = [
    &["game", "of", "the", "year"],
    &["for", "wii", "u"],
    &["directors", "cut"],
    &["playstation", "3"],
    &["wii", "u"],
    &["goty"],
    &["ps3"],
];

fn is_region_note(inside: &str) -> bool {
    let words = fold(inside);
    !words.is_empty() && words.split(' ').all(|word| REGION_NOTES.contains(&word))
}

/// The name without its region and language notes. Any other bracketed note
/// stays: "(Multiplayer Beta)" is a different title from the game.
fn without_region_notes(name: &str) -> String {
    let mut kept = String::new();
    let mut rest = name;
    while let Some(open) = rest.find(['(', '[']) {
        let close = if rest[open..].starts_with('(') { ')' } else { ']' };
        let Some(length) = rest[open + 1..].find(close) else {
            break;
        };
        let end = open + 1 + length + 1;
        kept.push_str(&rest[..open]);
        if !is_region_note(&rest[open + 1..end - 1]) {
            kept.push_str(&rest[open..end]);
        }
        kept.push(' ');
        rest = &rest[end..];
    }
    kept.push_str(rest);
    kept
}

fn ends_with(words: &[String], phrase: &[&str]) -> bool {
    words.len() > phrase.len() && words[words.len() - phrase.len()..].iter().zip(phrase).all(|(a, b)| a == b)
}

/// Takes the port's own words off the end. A name is never cut to nothing.
fn trim_port_words(words: &mut Vec<String>) {
    loop {
        let before = words.len();
        if let Some(phrase) = PORT_WORDS.iter().find(|phrase| ends_with(words, phrase)) {
            words.truncate(words.len() - phrase.len());
        }
        if words.len() > 1 && words.last().is_some_and(|word| word == "edition") {
            words.pop();
            // "Armored Edition", "Special Edition": one word says which. A
            // number is left, since "2 Edition" would be a game of its own.
            match PORT_WORDS.iter().find(|phrase| ends_with(words, phrase)) {
                Some(phrase) => words.truncate(words.len() - phrase.len()),
                None if words.len() > 1 && words.last().is_some_and(|w| w.chars().all(char::is_alphabetic)) => {
                    words.pop();
                }
                None => {}
            }
        }
        // Wii U ports put a U on the end: "Need for Speed: Most Wanted U".
        if words.len() > 1 && words.last().is_some_and(|word| word == "u") {
            words.pop();
        }
        if words.len() == before {
            return;
        }
    }
}

/// A name brought down to what stays the same when one game is sold on two
/// consoles, so the PS3's "Skylanders SWAP Force™" and the Cemu wiki's
/// "Skylanders: Swap Force" come out alike.
///
/// Only for matching across consoles. On one console a special edition can
/// be a title of its own with its own result, which `fold` keeps apart.
pub fn same_game(name: &str) -> String {
    // An apostrophe joins rather than splits: "Spyro's", not "Spyro s".
    let name: String = name.chars().filter(|c| !matches!(c, '\'' | '’' | '‘' | '`')).collect();
    let name = without_region_notes(&name.replace('&', " and "));
    let mut words: Vec<String> = fold(&name).split_whitespace().map(str::to_string).collect();
    trim_port_words(&mut words);

    // Spaces go, so "SuperChargers" and "Super Chargers" meet, except
    // between two numbers, so "1.5" and "15" stay apart.
    let mut key = String::new();
    for word in &words {
        if key.ends_with(|c: char| c.is_ascii_digit()) && word.starts_with(|c: char| c.is_ascii_digit()) {
            key.push(' ');
        }
        key.push_str(word);
    }
    key
}

/// Demos, trials and betas are their own titles with their own results, and
/// someone who owns one should find it, so they are marked rather than dropped.
fn is_demo(name: &str) -> bool {
    name.contains("体験版")
        || fold(name)
            .split(' ')
            .any(|word| matches!(word, "demo" | "trial" | "beta"))
}

/// Best first: what plays well, then what runs with problems, then what
/// doesn't run, and last what nobody has rated.
pub fn tone_rank(tone: &str) -> u8 {
    match tone {
        "go" => 0,
        "warn" => 1,
        "bad" => 2,
        _ => 3,
    }
}

fn region_rank(region: &str) -> usize {
    REGIONS
        .iter()
        .position(|known| *known == region)
        .unwrap_or(REGIONS.len())
}

/// Puts the releases of each game together. The same game is sold once per
/// region and again on the store, so RPCS3's list holds four "Angry Birds
/// Trilogy" titles that are one game to anyone browsing.
pub fn group(entries: Vec<Entry>) -> Vec<Listing> {
    let mut index: HashMap<(Console, String), usize> = HashMap::new();
    let mut groups: Vec<Vec<Entry>> = Vec::new();
    for entry in entries {
        let folded = fold(&entry.name);
        // A title with no name is only ever itself.
        let same = if entry.named && !folded.is_empty() {
            folded
        } else {
            entry.key.clone()
        };
        match index.get(&(entry.console, same.clone())) {
            Some(&at) => groups[at].push(entry),
            None => {
                index.insert((entry.console, same), groups.len());
                groups.push(vec![entry]);
            }
        }
    }
    groups.into_iter().map(listing_of).collect()
}

fn listing_of(mut entries: Vec<Entry>) -> Listing {
    let rank = |entry: &Entry| {
        entry
            .regions
            .iter()
            .map(|region| region_rank(region))
            .min()
            .unwrap_or(REGIONS.len())
    };
    entries.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.title_id.cmp(&b.title_id)));

    let mut regions: Vec<&'static str> = entries
        .iter()
        .flat_map(|entry| entry.regions.iter().copied())
        .collect();
    regions.sort_by_key(|region| region_rank(region));
    regions.dedup();

    let releases = entries
        .iter()
        .filter(|entry| !entry.title_id.is_empty())
        .map(|entry| Release {
            title_id: entry.title_id.clone(),
            region: entry.regions.first().copied().unwrap_or(""),
        })
        .collect();

    // min_by_key keeps the first of equals, so a tie goes to the release
    // listed first.
    let status = entries
        .iter()
        .map(|entry| entry.status)
        .min_by_key(|status| tone_rank(status.tone))
        .unwrap_or_default();

    let first = &entries[0];
    Listing {
        console: first.console,
        console_name: first.console.short(),
        key: first.key.clone(),
        name: first.name.clone(),
        named: first.named,
        status,
        kind: first.kind,
        regions,
        releases,
        demo: is_demo(&first.name),
        owned: false,
        features: Features::default(),
        portal_menu: is_skylanders(&first.name).then(|| has_portal_menu(first.console, &first.name)),
        portal_note: portal_menu_note(first.console, &first.name),
    }
}

/// What the filter lets through, in the order asked for, and how many that
/// was before the page was cut.
pub fn pick(listings: Vec<Listing>, filter: &Filter) -> (usize, Vec<Listing>) {
    let needle = filter.query.trim().to_lowercase();
    let mut found: Vec<Listing> = listings
        .into_iter()
        .filter(|l| filter.console.map_or(true, |console| l.console == console))
        .filter(|l| filter.region.is_empty() || l.regions.iter().any(|r| *r == filter.region))
        .filter(|l| filter.runs.is_empty() || l.status.tone == filter.runs)
        .filter(|l| !(filter.hide_demos && l.demo))
        .filter(|l| {
            needle.is_empty()
                || l.name.to_lowercase().contains(&needle)
                || l.releases
                    .iter()
                    .any(|r| r.title_id.to_lowercase().contains(&needle))
        })
        .collect();

    // A name beginning with what was typed is what someone meant, and a title
    // with no name at all is the weakest match. Within that, the order asked
    // for.
    let best_first = filter.sort == "runs";
    found.sort_by_cached_key(|l| {
        let name = l.name.to_lowercase();
        let rank = if !l.named {
            3
        } else if name.starts_with(&needle) {
            0
        } else if name.split_whitespace().any(|word| word.starts_with(&needle)) {
            1
        } else {
            2
        };
        let runs = if best_first { tone_rank(l.status.tone) } else { 0 };
        (rank, runs, name)
    });

    let total = found.len();
    found.truncate(if filter.limit == 0 { PAGE } else { filter.limit });

    // The release a card adds is its first, so with a region chosen that is
    // the one from there. Stable, so the rest keep their order.
    if !filter.region.is_empty() {
        for listing in &mut found {
            listing.releases.sort_by_key(|r| r.region != filter.region);
        }
    }
    (total, found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titled(console: Console, id: &str, name: &str, regions: Vec<&'static str>, tone: &'static str) -> Entry {
        Entry {
            console,
            key: id.to_string(),
            name: name.to_string(),
            named: true,
            title_id: if console == Console::Ps3 { id.to_string() } else { String::new() },
            regions,
            status: Status {
                label: "Result",
                tone,
                explanation: "",
                caution: "",
            },
            kind: "",
        }
    }

    fn ps3(id: &str, name: &str, region: &'static str, tone: &'static str) -> Entry {
        titled(Console::Ps3, id, name, vec![region], tone)
    }

    fn names(listings: &[Listing]) -> Vec<&str> {
        listings.iter().map(|l| l.name.as_str()).collect()
    }

    #[test]
    fn a_skylanders_listing_says_whether_the_portal_menu_works_in_it() {
        let listings = group(vec![
            ps3("BLUS31545", "Skylanders SuperChargers", "US", "warn"),
            ps3("BLES02055", "Skylanders Trap Team", "EU", "warn"),
            ps3("BLES01784", "Batman: Arkham Origins", "EU", "go"),
            titled(Console::WiiU, "superchargers", "Skylanders: SuperChargers", vec!["US", "EU"], "go"),
        ]);
        let find = |console: Console, name: &str| {
            listings.iter().find(|l| l.console == console && l.name == name).unwrap()
        };
        let superchargers = find(Console::Ps3, "Skylanders SuperChargers");
        assert_eq!(superchargers.portal_menu, Some(false));
        assert_eq!(
            superchargers.portal_note.as_deref(),
            Some("The portal menu doesn't work in this version yet. It works in the Wii U version.")
        );
        let trap_team = find(Console::Ps3, "Skylanders Trap Team");
        assert_eq!((trap_team.portal_menu, trap_team.portal_note.as_deref()), (Some(true), None));
        let on_wii_u = find(Console::WiiU, "Skylanders: SuperChargers");
        assert_eq!((on_wii_u.portal_menu, on_wii_u.portal_note.as_deref()), (Some(true), None));
        let batman = find(Console::Ps3, "Batman: Arkham Origins");
        assert_eq!((batman.portal_menu, batman.portal_note.as_deref()), (None, None));
    }

    #[test]
    fn the_releases_of_one_game_become_one_listing() {
        let listings = group(vec![
            ps3("NPUB31054", "Angry Birds Trilogy", "US", "go"),
            ps3("BLES01732", "Angry Birds Trilogy", "EU", "go"),
            ps3("NPEB01235", "ANGRY BIRDS TRILOGY", "EU", "go"),
            ps3("BLUS31054", "Angry Birds Trilogy", "US", "go"),
        ]);
        assert_eq!(listings.len(), 1);
        let game = &listings[0];
        assert_eq!(game.regions, vec!["EU", "US"]);
        let ids: Vec<&str> = game.releases.iter().map(|r| r.title_id.as_str()).collect();
        assert_eq!(ids, ["BLES01732", "NPEB01235", "BLUS31054", "NPUB31054"]);
        assert_eq!(game.key, "BLES01732", "covers stay under the same release");
        assert_eq!(game.name, "Angry Birds Trilogy");
    }

    #[test]
    fn different_games_and_consoles_stay_apart() {
        let listings = group(vec![
            ps3("BLES01732", "Angry Birds Trilogy", "EU", "go"),
            ps3("BLES01943", "Angry Birds Star Wars", "EU", "go"),
            titled(Console::WiiU, "WU1", "Angry Birds Trilogy", vec!["EU", "US"], "go"),
        ]);
        assert_eq!(listings.len(), 3);
        let wii_u = listings.iter().find(|l| l.console == Console::WiiU).unwrap();
        assert!(wii_u.releases.is_empty(), "the wiki has no title ids");
        assert_eq!(wii_u.console_name, "Wii U");
    }

    #[test]
    fn the_best_result_among_releases_is_the_one_shown() {
        let listings = group(vec![
            ps3("BLES00001", "Some Game", "EU", "warn"),
            ps3("BLUS00001", "Some Game", "US", "go"),
        ]);
        assert_eq!(listings[0].status.tone, "go");
    }

    fn shelf() -> Vec<Listing> {
        group(vec![
            ps3("BLES01732", "Angry Birds Trilogy", "EU", "go"),
            ps3("NPUB30001", "Stacking Demo", "US", "go"),
            ps3("BLJM00001", "Rain", "JP", "bad"),
            ps3("BCES00797", "Heavy Rain", "EU", "warn"),
            titled(Console::WiiU, "WU2", "Mario Kart 8", vec!["EU", "US", "JP"], "go"),
        ])
    }

    #[test]
    fn each_filter_narrows_the_list() {
        let by = |filter: Filter| names(&pick(shelf(), &filter).1).join(", ");
        assert_eq!(by(Filter { region: "JP".into(), ..Default::default() }), "Mario Kart 8, Rain");
        assert_eq!(by(Filter { runs: "bad".into(), ..Default::default() }), "Rain");
        assert_eq!(by(Filter { console: Some(Console::WiiU), ..Default::default() }), "Mario Kart 8");
        assert!(!by(Filter { hide_demos: true, ..Default::default() }).contains("Demo"));
        assert!(by(Filter::default()).contains("Stacking Demo"));
    }

    #[test]
    fn a_name_starting_with_the_search_comes_first() {
        let (_, found) = pick(shelf(), &Filter { query: "rain".into(), ..Default::default() });
        assert_eq!(names(&found), ["Rain", "Heavy Rain"]);
    }

    #[test]
    fn runs_best_puts_what_plays_well_first() {
        let (_, found) = pick(shelf(), &Filter { sort: "runs".into(), ..Default::default() });
        assert_eq!(
            names(&found),
            ["Angry Birds Trilogy", "Mario Kart 8", "Stacking Demo", "Heavy Rain", "Rain"]
        );
    }

    #[test]
    fn a_page_is_cut_but_the_total_is_kept() {
        let (total, found) = pick(shelf(), &Filter { limit: 2, ..Default::default() });
        assert_eq!((total, found.len()), (5, 2));
        let (_, found) = pick(shelf(), &Filter::default());
        assert_eq!(found.len(), 5, "no limit means a whole page");
    }

    #[test]
    fn the_chosen_regions_release_comes_first() {
        let listings = group(vec![
            ps3("BLES01732", "Angry Birds Trilogy", "EU", "go"),
            ps3("BLUS31054", "Angry Birds Trilogy", "US", "go"),
        ]);
        let (_, found) = pick(listings, &Filter { region: "US".into(), ..Default::default() });
        assert_eq!(found[0].releases[0].title_id, "BLUS31054");
        assert_eq!(found[0].key, "BLES01732", "the cover does not change with the filter");
    }

    #[test]
    fn demos_are_told_by_whole_words() {
        for name in ["Stacking Demo", "1942: Joint Strike Trial", "LittleBigPlanet™ Beta", "ぼくのなつやすみ 体験版"] {
            assert!(is_demo(name), "{name}");
        }
        for name in ["Demon's Souls™", "Trials HD", "Alphabet"] {
            assert!(!is_demo(name), "{name}");
        }
    }

    #[test]
    fn folding_ignores_case_and_marks() {
        assert_eq!(fold("LittleBigPlanet™ 2"), "littlebigplanet 2");
        assert_eq!(fold("LITTLEBIGPLANET 2"), "littlebigplanet 2");
        assert_eq!(fold("  Mario   Kart 8 "), "mario kart 8");
    }

    fn assert_same(a: &str, b: &str) {
        assert_eq!(same_game(a), same_game(b), "{a:?} and {b:?} should be one game");
    }

    /// The names as RPCS3's list, the Cemu wiki, a PARAM.SFO, meta.xml and a
    /// disc image's file give them.
    #[test]
    fn each_skylanders_game_is_the_same_game_on_both_consoles() {
        for names in [
            &["Skylanders Spyro's Adventure", "Skylanders: Spyro's Adventure", "Skylanders Spyro’s Adventure®"][..],
            &["Skylanders Giants", "Skylanders: Giants", "Skylanders Giants™", "Skylanders - Giants (Europe) (En,Fr,De,Es,It,Nl,Sv,No,Da,Fi)"],
            &["Skylanders SWAP Force", "Skylanders: Swap Force", "Skylanders SWAP Force™", "Skylanders - Swap Force", "SKYLANDERS SWAP FORCE [EU]"],
            &["Skylanders Trap Team", "Skylanders: Trap Team", "Skylanders™ Trap Team", "Skylanders - Trap Team (USA)"],
            &["Skylanders SuperChargers", "Skylanders: SuperChargers", "Skylanders Superchargers", "Skylanders Super Chargers"],
            &["Skylanders Imaginators", "Skylanders: Imaginators", "Skylanders® Imaginators"],
        ] {
            for name in &names[1..] {
                assert_same(names[0], name);
            }
        }
    }

    #[test]
    fn no_two_skylanders_games_are_taken_for_each_other() {
        let games = ["Spyro's Adventure", "Giants", "SWAP Force", "Trap Team", "SuperChargers", "Imaginators"]
            .map(|game| same_game(&format!("Skylanders {game}")));
        for (i, a) in games.iter().enumerate() {
            for b in &games[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn a_port_is_the_same_game_under_its_edition_name() {
        for (ps3, wii_u) in [
            ("Batman: Arkham City", "Batman: Arkham City Armored Edition"),
            ("Mass Effect™ 3", "Mass Effect 3: Special Edition"),
            ("Minecraft: PlayStation®3 Edition", "Minecraft: Wii U Edition"),
            ("Need for Speed™ Most Wanted", "Need for Speed: Most Wanted U"),
            ("Trine 2", "Trine 2: Director's Cut"),
            ("Tekken Tag Tournament 2", "Tekken Tag Tournament 2 Wii U Edition"),
            ("Darksiders", "Darksiders Warmastered Edition"),
            ("Sonic and All-Stars Racing Transformed", "Sonic & All-Stars Racing Transformed"),
            ("Batman: Arkham Asylum Game of the Year Edition", "Batman: Arkham Asylum"),
        ] {
            assert_same(ps3, wii_u);
        }
    }

    #[test]
    fn different_games_stay_different() {
        for (a, b) in [
            ("Heavy Rain", "Rain"),
            ("Batman: Arkham Origins", "Batman: Arkham Origins Blackgate"),
            ("LEGO Batman 2: DC Super Heroes", "LEGO Batman 3: Beyond Gotham"),
            ("Disney Infinity 2.0", "Disney Infinity 3.0"),
            ("Call of Duty: Black Ops II", "Call of Duty: Black Ops"),
            ("Mario Kart 8", "Mario Kart 8 Deluxe"),
            ("Kingdom Hearts HD 1.5 ReMIX", "Kingdom Hearts HD 15 ReMIX"),
            // A demo or a beta is a title of its own, with its own result.
            ("God of War: Ascension (Multiplayer Beta)", "God of War: Ascension"),
            ("Stacking Demo", "Stacking"),
            ("FIFA 2 Edition", "FIFA"),
        ] {
            assert_ne!(same_game(a), same_game(b), "{a:?} and {b:?} are different games");
        }
    }

    #[test]
    fn a_name_is_never_cut_to_nothing() {
        assert_eq!(same_game("Edition"), "edition");
        assert_eq!(same_game("Special Edition"), "special");
        assert_eq!(same_game("U"), "u");
        assert_eq!(same_game("(Europe)"), "");
    }
}
