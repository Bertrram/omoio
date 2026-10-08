//! Whether a game about to be imported is worth a warning, and whether
//! another console Omoio runs has a version of it that plays well.
//!
//! Only what the compatibility lists already say is used, from the copies
//! Omoio keeps, so nothing here waits on the network, along with Omoio's own
//! short list of games it has been played with (`tested`). A game nobody has
//! rated gets no warning: there is nothing true to say about it.
//!
//! A Skylanders game the portal menu doesn't work in is warned about
//! whatever its rating: without figures on the portal it can't really be
//! played.

use crate::core::catalogue::{fold, same_game, tone_rank, Entry, Status};
use crate::core::console::Console;
use crate::core::figures::{self, has_portal_menu, portal_menu_elsewhere, versions};
use crate::core::library::Game;
use crate::core::tested::tested;
use serde::Serialize;

/// A game as far as the check needs to know it. An archive's game is known
/// this far before it is unpacked.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub console: Console,
    /// Empty when the game has no id its console's list could know it by.
    pub title_id: String,
    pub title: String,
}

impl Imported {
    pub fn of(game: &Game) -> Self {
        Self {
            console: game.console,
            title_id: game.title_id.clone(),
            title: game.title.clone(),
        }
    }
}

/// One console's emulator and every title its list rates, with each name
/// folded once rather than once per game checked, since a scan checks
/// hundreds against a list of thousands.
pub struct List {
    console: Console,
    emulator: &'static str,
    titles: Vec<(Entry, String, String)>,
}

impl List {
    pub fn new(console: Console, emulator: &'static str, entries: Vec<Entry>) -> Self {
        let titles = entries
            .into_iter()
            .filter(|entry| entry.named && !entry.status.tone.is_empty())
            .map(|entry| {
                let (folded, key) = (fold(&entry.name), same_game(&entry.name));
                (entry, folded, key)
            })
            .collect();
        Self { console, emulator, titles }
    }

    /// The best rated of the titles that pass `pick`, which is given each
    /// title with its folded name and its name as matched across consoles.
    fn best(&self, pick: impl Fn(&Entry, &str, &str) -> bool) -> Option<&Entry> {
        self.titles
            .iter()
            .filter(|(entry, folded, key)| pick(entry, folded, key))
            .map(|(entry, _, _)| entry)
            .min_by_key(|entry| tone_rank(entry.status.tone))
    }

    /// Where the list says the game came out. Only a list of games, one
    /// without title ids like the Cemu wiki's, says that. A list of releases
    /// like RPCS3's holds the ones somebody has tested, so a region missing
    /// from it says nothing about where the game was sold.
    fn sold_in(&self, key: &str) -> Vec<&'static str> {
        let mut regions: Vec<&'static str> = Vec::new();
        for (entry, _, other) in &self.titles {
            if other == key && entry.title_id.is_empty() {
                for region in &entry.regions {
                    if !regions.contains(region) {
                        regions.push(region);
                    }
                }
            }
        }
        regions
    }

    /// The game's own release, when the list knows its id.
    fn release(&self, game: &Imported) -> Option<&Entry> {
        if game.title_id.is_empty() {
            return None;
        }
        self.titles
            .iter()
            .map(|(entry, _, _)| entry)
            .find(|entry| entry.title_id == game.title_id)
    }

    /// How this list rates the game: its own release when the list has it,
    /// then the game by its name, then by the looser name used across
    /// consoles. Releases of one game are rated apart, and the catalogue
    /// shows the best of them, so this does too.
    fn result(&self, game: &Imported) -> Option<Status> {
        let folded = fold(&game.title);
        let key = same_game(&game.title);
        self.release(game)
            .or_else(|| self.best(|_, name, _| !folded.is_empty() && name == folded))
            .or_else(|| self.best(|_, _, other| !key.is_empty() && other == key))
            .map(|entry| entry.status)
    }
}

/// Whether a version sold in `sold` can be offered to someone whose copy was
/// sold in `regions`. One the list says came out in a single region, such as
/// Spyro's Adventure on the Wii U, sold in Japan only, is offered only to a
/// copy from there. A version the list gives no regions for is offered.
fn sold_alike(sold: &[&str], regions: &[&str]) -> bool {
    match sold {
        [only] => regions.contains(only),
        _ => true,
    }
}

fn portal_note(console: Console, title: &str) -> &'static str {
    if has_portal_menu(console, title) {
        ", and the portal menu works in it"
    } else {
        ""
    }
}

/// What to tell someone about to import a game that may not run well, or a
/// Skylanders game the portal menu doesn't work in.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Warning {
    pub title: String,
    pub console: Console,
    pub console_name: &'static str,
    /// "Omoio's portal menu doesn't work in Skylanders SuperChargers on the
    /// PS3, so you can't put figures on the portal." Said first. Empty for a
    /// game the menu works in, or one with no portal.
    pub portal: String,
    /// "RPCS3 rates it Ingame: it starts, but you may hit problems before the
    /// end." Empty when it plays well, has been played in Omoio, or nobody
    /// has rated it, which only a portal warning leaves.
    pub rating: String,
    /// "The Wii U version is rated Playable in Cemu.", or "The Wii U version
    /// runs well in Omoio." for one on the tested list. With a portal
    /// warning, where the menu works instead: "The portal menu works in the
    /// Wii U version." Empty when there is no such version.
    pub better: String,
}

/// The warning for a Skylanders game the portal menu doesn't work in, or
/// for a game whose own emulator rates it below playing well. `None` when
/// neither: it plays well, has been played in Omoio, or nobody has rated it.
pub fn warning(game: &Imported, lists: &[List]) -> Option<Warning> {
    let portal = portal_warning(game);
    let rated = rated(game, lists);
    if portal.is_none() && rated.is_none() {
        return None;
    }
    let (rating, better) = rated.unwrap_or_default();
    // Where the menu works is the version to point to. One the lists rate
    // well may have no menu at all.
    let (portal, better) = match portal {
        Some((portal, works)) => (portal, works),
        None => (String::new(), better),
    };
    Some(Warning {
        title: game.title.clone(),
        console: game.console,
        console_name: game.console.short(),
        portal,
        rating,
        better,
    })
}

/// Why a Skylanders game can't be played in Omoio on this console, and
/// where it can.
fn portal_warning(game: &Imported) -> Option<(String, String)> {
    let elsewhere = portal_menu_elsewhere(game.console, &game.title)?;
    let name = figures::game_from_title(&game.title).map_or("this game", figures::Game::name);
    Some(if elsewhere.is_empty() {
        (
            format!("Omoio's portal menu doesn't work in {name} on any console yet, so you can't put figures on the portal."),
            String::new(),
        )
    } else {
        (
            format!(
                "Omoio's portal menu doesn't work in {name} on the {}, so you can't put figures on the portal.",
                game.console.short()
            ),
            format!("The portal menu works in {}.", versions(&elsewhere)),
        )
    })
}

/// What the game's own list says, when it rates the game below playing
/// well, and the version elsewhere that plays well.
fn rated(game: &Imported, lists: &[List]) -> Option<(String, String)> {
    if tested(game.console, &game.title).is_some() {
        return None;
    }
    let own = lists.iter().find(|list| list.console == game.console)?;
    let status = own.result(game)?;
    if !matches!(status.tone, "warn" | "bad") {
        return None;
    }

    let rating = if status.caution.is_empty() {
        format!("{} rates it {}.", own.emulator, status.label)
    } else {
        format!("{} rates it {}: {}", own.emulator, status.label, status.caution)
    };

    // A list's own rating is named first, since anyone can look it up. The
    // tested list speaks for a version its list rates lower.
    let key = same_game(&game.title);
    let regions = own.release(game).map_or(&[][..], |entry| entry.regions.as_slice());
    let better = lists
        .iter()
        .filter(|list| list.console != game.console && !key.is_empty())
        .find_map(|list| {
            let rated = sold_alike(&list.sold_in(&key), regions)
                .then(|| list.best(|entry, _, other| other == key && entry.status.tone == "go"))
                .flatten()
                .map(|entry| {
                    format!(
                        "The {} version is rated {} in {}{}.",
                        list.console.short(),
                        entry.status.label,
                        list.emulator,
                        portal_note(list.console, &entry.name)
                    )
                });
            rated.or_else(|| {
                tested(list.console, &game.title).map(|played| {
                    format!(
                        "The {} version runs well in Omoio{}.",
                        list.console.short(),
                        portal_note(list.console, played.title)
                    )
                })
            })
        })
        .unwrap_or_default();

    Some((rating, better))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rated(label: &'static str) -> Status {
        let (tone, caution) = match label {
            "Playable" | "Perfect" => ("go", ""),
            "Ingame" => ("warn", "it starts, but you may hit problems before the end."),
            "Runs" => ("warn", "it gets into the game, but major glitches make it hard to finish."),
            "Nothing" => ("bad", "it doesn't start."),
            _ => ("", ""),
        };
        Status { label, tone, explanation: "", caution }
    }

    fn entry(console: Console, title_id: &str, name: &str, label: &'static str) -> Entry {
        Entry {
            console,
            key: if title_id.is_empty() { name.to_string() } else { title_id.to_string() },
            name: name.to_string(),
            named: true,
            title_id: title_id.to_string(),
            regions: Vec::new(),
            status: rated(label),
            kind: "",
        }
    }

    /// A PS3 release, sold where the third letter of its id says.
    fn ps3(title_id: &str, name: &str, label: &'static str) -> Entry {
        let region = match title_id.as_bytes()[2] {
            b'E' => "EU",
            b'U' => "US",
            _ => "JP",
        };
        Entry { regions: vec![region], ..entry(Console::Ps3, title_id, name, label) }
    }

    fn wii_u(name: &str, label: &'static str, regions: &[&'static str]) -> Entry {
        Entry { regions: regions.to_vec(), ..entry(Console::WiiU, "", name, label) }
    }

    /// What RPCS3's list and the Cemu wiki said about the Skylanders games,
    /// and one game the other way round, on 6 October 2026.
    fn lists() -> Vec<List> {
        let both = &["US", "EU"];
        vec![
            List::new(
                Console::Ps3,
                "RPCS3",
                vec![
                    ps3("BLES01272", "Skylanders Spyro's Adventure", "Ingame"),
                    ps3("BLJM61044", "Skylanders Spyro's Adventure", "Ingame"),
                    ps3("BLES01689", "Skylanders Giants", "Playable"),
                    ps3("BLES01860", "Skylanders SWAP Force", "Ingame"),
                    ps3("BLES02055", "Skylanders Trap Team", "Ingame"),
                    ps3("BLUS31545", "Skylanders SuperChargers", "Ingame"),
                    ps3("BLES02240", "Skylanders Imaginators", "Ingame"),
                    ps3("BLES01784", "Batman: Arkham Origins", "Playable"),
                    ps3("BLES00001", "Unrated Game", ""),
                ],
            ),
            List::new(
                Console::WiiU,
                "Cemu",
                vec![
                    wii_u("Skylanders: Spyro's Adventure", "Perfect", &["JP"]),
                    wii_u("Skylanders: Giants", "Playable", both),
                    wii_u("Skylanders: Swap Force", "Runs", both),
                    wii_u("Skylanders: Trap Team", "Playable", both),
                    wii_u("Skylanders: SuperChargers", "Perfect", both),
                    wii_u("Skylanders: Imaginators", "Perfect", both),
                    wii_u("Batman: Arkham Origins", "Runs", &["US", "EU", "JP"]),
                ],
            ),
        ]
    }

    fn wii_u_game(title: &str) -> Imported {
        Imported { console: Console::WiiU, title_id: "WUD87E51FD0F7F95".into(), title: title.into() }
    }

    fn ps3_game(title_id: &str, title: &str) -> Imported {
        Imported { console: Console::Ps3, title_id: title_id.into(), title: title.into() }
    }

    /// SuperChargers on the PS3 is the one people will try: RPCS3 rates it
    /// no worse than the others, but the portal menu doesn't work in it.
    #[test]
    fn a_skylanders_game_without_the_portal_menu_says_so_first() {
        let superchargers = warning(&ps3_game("BLUS31545", "Skylanders SuperChargers™"), &lists()).unwrap();
        assert_eq!(superchargers.title, "Skylanders SuperChargers™");
        assert_eq!(superchargers.console_name, "PS3");
        assert_eq!(
            superchargers.portal,
            "Omoio's portal menu doesn't work in Skylanders SuperChargers on the PS3, so you can't put figures on the portal."
        );
        assert_eq!(superchargers.rating, "RPCS3 rates it Ingame: it starts, but you may hit problems before the end.");
        assert_eq!(superchargers.better, "The portal menu works in the Wii U version.");
    }

    #[test]
    fn a_skylanders_game_without_the_portal_menu_is_warned_whatever_its_rating() {
        // Rated well, or not rated at all, it still can't take figures.
        let mut lists = lists();
        lists.push(List::new(
            Console::Wii,
            "Dolphin",
            vec![Entry { regions: vec!["EU"], ..entry(Console::Wii, "SKYP52", "Skylanders: Giants", "Perfect") }],
        ));
        let wii_giants = Imported { console: Console::Wii, title_id: "SKYP52".into(), title: "Skylanders Giants".into() };
        let giants = warning(&wii_giants, &lists).unwrap();
        assert_eq!(
            giants.portal,
            "Omoio's portal menu doesn't work in Skylanders Giants on the Wii, so you can't put figures on the portal."
        );
        assert_eq!(giants.rating, "", "Dolphin rates it Perfect");
        assert_eq!(giants.better, "The portal menu works in the PS3 version.");

        let imaginators = warning(&ps3_game("BLES02240", "Skylanders Imaginators"), &[]).unwrap();
        assert_eq!(imaginators.rating, "", "no lists yet");
        assert_eq!(imaginators.better, "The portal menu works in the Wii U version.");
    }

    #[test]
    fn every_version_with_the_portal_menu_is_named() {
        let wii = |title: &str| Imported { console: Console::Wii, title_id: "SVXP52".into(), title: title.into() };
        let swap_force = warning(&wii("Skylanders SWAP Force"), &[]).unwrap();
        assert_eq!(swap_force.better, "The portal menu works in the PS3 and Wii U versions.");
        let trap_team = warning(&wii("Skylanders Trap Team"), &[]).unwrap();
        assert_eq!(trap_team.better, "The portal menu works in the PS3 and Wii U versions.");

        // Spyro's Adventure came out on the Wii U in Japan; the menu works in
        // the PS3's and the Wii's, and Cemu's Perfect isn't offered instead.
        let spyro = warning(&wii_u_game("Skylanders: Spyro's Adventure"), &lists()).unwrap();
        assert_eq!(
            spyro.portal,
            "Omoio's portal menu doesn't work in Skylanders Spyro's Adventure on the Wii U, so you can't put figures on the portal."
        );
        assert_eq!(spyro.better, "The portal menu works in the PS3 and Wii versions.");
    }

    #[test]
    fn a_skylanders_game_omoio_cant_tell_has_the_menu_nowhere_yet() {
        let other = warning(&ps3_game("BLUS99998", "Skylanders Battlecast"), &[]).unwrap();
        assert_eq!(
            other.portal,
            "Omoio's portal menu doesn't work in this game on any console yet, so you can't put figures on the portal."
        );
        assert_eq!(other.better, "");
    }

    #[test]
    fn a_skylanders_game_with_the_portal_menu_gets_no_portal_warning() {
        assert_eq!(warning(&ps3_game("BLES02055", "Skylanders Trap Team"), &lists()), None);
        assert_eq!(warning(&wii_u_game("Skylanders SuperChargers"), &lists()), None);
        assert_eq!(warning(&wii_u_game("Skylanders Imaginators"), &[]), None);
        assert_eq!(warning(&ps3_game("BLES01689", "Skylanders Giants"), &[]), None);
        // Spyro's Adventure on the PS3 has the menu, and is warned of its
        // rating alone.
        let spyro = warning(&ps3_game("BLES01272", "Skylanders Spyro's Adventure"), &lists()).unwrap();
        assert_eq!(spyro.portal, "");
        assert!(spyro.rating.starts_with("RPCS3 rates it Ingame"));
    }

    #[test]
    fn no_better_version_is_offered_when_the_other_one_has_problems_too() {
        let lists = vec![
            List::new(Console::Ps3, "RPCS3", vec![ps3("BLES00002", "Some Game", "Ingame")]),
            List::new(Console::WiiU, "Cemu", vec![wii_u("Some Game", "Runs", &[])]),
        ];
        let warning = warning(&ps3_game("BLES00002", "Some Game"), &lists).unwrap();
        assert_eq!(warning.better, "", "Cemu rates the Wii U version Runs");
    }

    #[test]
    fn a_game_that_plays_well_or_has_no_rating_gets_no_warning() {
        assert_eq!(warning(&ps3_game("BLES01689", "Skylanders Giants"), &lists()), None);
        assert_eq!(warning(&ps3_game("BLES00001", "Unrated Game"), &lists()), None);
        assert_eq!(warning(&ps3_game("BLES99999", "A Game Nobody Listed"), &lists()), None);
        assert_eq!(warning(&ps3_game("BLES02055", "Skylanders Trap Team"), &[]), None, "no lists yet");
    }

    #[test]
    fn a_wii_u_game_is_found_by_its_name() {
        let warning = warning(&wii_u_game("Batman Arkham Origins"), &lists()).unwrap();
        assert_eq!(warning.console_name, "Wii U");
        assert_eq!(warning.portal, "", "not a Skylanders game");
        assert_eq!(
            warning.rating,
            "Cemu rates it Runs: it gets into the game, but major glitches make it hard to finish."
        );
        assert_eq!(warning.better, "The PS3 version is rated Playable in RPCS3.");
    }

    /// The Cemu wiki rates SWAP Force Runs, from before Cemu's Portal
    /// Stability Fix pack. It has been played in Omoio since.
    #[test]
    fn a_tested_game_gets_no_warning_on_either_console() {
        assert_eq!(warning(&wii_u_game("Skylanders - Swap Force"), &lists()), None);
        assert_eq!(warning(&wii_u_game("Skylanders - Trap Team"), &lists()), None);
        assert_eq!(warning(&ps3_game("BLES01860", "Skylanders SWAP Force"), &lists()), None);
        assert_eq!(warning(&ps3_game("BLES02055", "Skylanders Trap Team"), &lists()), None);
    }

    #[test]
    fn a_version_sold_in_one_region_is_offered_only_to_a_copy_from_there() {
        // Spyro's Adventure came out on the Wii U in Japan only.
        let european = warning(&ps3_game("BLES01272", "Skylanders Spyro's Adventure"), &lists()).unwrap();
        assert_eq!(european.better, "");
        let japanese = warning(&ps3_game("BLJM61044", "Skylanders Spyro's Adventure"), &lists()).unwrap();
        assert_eq!(japanese.better, "The Wii U version is rated Perfect in Cemu.");

        // A PS3 release found in one region doesn't mean the game came out
        // only there: RPCS3's list holds the releases somebody tested.
        let lists_one_release = vec![
            List::new(Console::Ps3, "RPCS3", vec![ps3("BLUS31147", "Batman: Arkham Origins", "Playable")]),
            List::new(Console::WiiU, "Cemu", vec![wii_u("Batman: Arkham Origins", "Runs", &["US", "EU"])]),
        ];
        let from_wii_u = warning(&wii_u_game("Batman: Arkham Origins"), &lists_one_release).unwrap();
        assert_eq!(from_wii_u.better, "The PS3 version is rated Playable in RPCS3.");

        // Where the list gives no regions, the version is offered.
        let lists = vec![
            List::new(Console::Ps3, "RPCS3", vec![ps3("BLES01272", "Skylanders Spyro's Adventure", "Ingame")]),
            List::new(Console::WiiU, "Cemu", vec![wii_u("Skylanders: Spyro's Adventure", "Perfect", &[])]),
        ];
        let unknown = warning(&ps3_game("BLES01272", "Skylanders Spyro's Adventure"), &lists).unwrap();
        assert_eq!(unknown.better, "The Wii U version is rated Perfect in Cemu.");
    }

    /// The Dolphin wiki lists a release per id, as RPCS3's list does. It
    /// rated Spyro's Adventure on the Wii five stars on 8 October 2026.
    #[test]
    fn a_wii_version_that_plays_well_in_dolphin_is_offered() {
        let mut lists = lists();
        let spyro = "Skylanders: Spyro's Adventure";
        lists.push(List::new(
            Console::Wii,
            "Dolphin",
            vec![
                Entry { regions: vec!["EU"], ..entry(Console::Wii, "SSPP52", spyro, "Perfect") },
                Entry { regions: vec!["US"], ..entry(Console::Wii, "SSPE52", spyro, "Perfect") },
            ],
        ));
        // The Wii U version came out in Japan only, so the Wii's is offered.
        let european = warning(&ps3_game("BLES01272", "Skylanders Spyro's Adventure"), &lists).unwrap();
        assert_eq!(
            european.better,
            "The Wii version is rated Perfect in Dolphin, and the portal menu works in it."
        );
    }

    #[test]
    fn a_release_the_list_lacks_is_rated_like_the_rest_of_its_game() {
        let warning = warning(&ps3_game("BLUS31600", "Skylanders Imaginators"), &lists()).unwrap();
        assert!(warning.rating.starts_with("RPCS3 rates it Ingame"));
        assert_eq!(warning.better, "The portal menu works in the Wii U version.");
    }

    #[test]
    fn the_portal_menu_is_only_mentioned_where_it_works() {
        // The Wii U's Giants plays, but the menu has only been played with
        // the PS3's.
        let lists = vec![
            List::new(Console::Ps3, "RPCS3", vec![entry(Console::Ps3, "BLES01689", "Skylanders Giants", "Nothing")]),
            List::new(Console::WiiU, "Cemu", vec![entry(Console::WiiU, "", "Skylanders: Giants", "Playable")]),
        ];
        let warning = warning(&ps3_game("BLES01689", "Skylanders Giants"), &lists).unwrap();
        assert_eq!(warning.rating, "RPCS3 rates it Nothing: it doesn't start.");
        assert_eq!(warning.better, "The Wii U version is rated Playable in Cemu.");
    }
}
