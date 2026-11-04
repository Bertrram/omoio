//! The short list of games Omoio has been played with, each with its reason.
//!
//! A game here counts as playing well whatever its emulator's list says: it
//! gets no warning before it is imported, it can be the version offered for
//! another console's copy, and its catalogue card, its pages and Big Picture
//! say it was played in Omoio and why, beside the list's rating where one is
//! shown (`played`). Only a game that has been played in Omoio goes
//! here, with what was seen written beside it, because a list rating can be
//! older than the fix that made the game run.
//!
//! Bertram played each game here. The interface names nobody, so a reason
//! says what was seen and when, and his name stays in this note.

use crate::core::catalogue::same_game;
use crate::core::console::Console;
use serde::Serialize;

#[derive(Debug)]
pub struct Tested {
    pub console: Console,
    /// Matched the way names are matched across consoles, so "Skylanders -
    /// Swap Force" from a disc image's file name is this game too.
    pub title: &'static str,
    /// Shown as it is after "Played in Omoio", beside the list's rating, so
    /// it is written for the interface.
    pub reason: &'static str,
}

pub const TESTED: &[Tested] = &[
    Tested {
        console: Console::WiiU,
        title: "Skylanders SWAP Force",
        reason: "Plays well with the portal menu. The Cemu wiki rated it Runs before Cemu's \
                 Portal Stability Fix pack, which is on by default.",
    },
    Tested {
        console: Console::WiiU,
        title: "Skylanders Trap Team",
        reason: "Tested on 6 October 2026 with the portal menu and its villains.",
    },
    Tested {
        console: Console::Ps3,
        title: "Skylanders SWAP Force",
        reason: "Tested on 7 October 2026: swappers went on the portal at once. RPCS3's \
                 Ingame rating is from 2021.",
    },
    // Trap Team on the PS3 was here from 7 October 2026, when its figures and
    // traps went on at once. It came off on 27 October: it is very unstable in
    // RPCS3, where a player had drops and lag on a Ryzen 7 5800X and an RTX
    // 4060 with a clean log and RPCS3's defaults (reported on Discord), while
    // the Wii U version runs without problems (Bertram). So it is warned about
    // as RPCS3's Ingame says, and the warning names the Wii U version.
    Tested {
        console: Console::WiiU,
        title: "Skylanders Imaginators",
        reason: "Tested on 8 October 2026 with the portal menu: a Sensei and new Creation \
                 Crystals went on with Cemu's Signature Patch, and an Imaginator made in one \
                 stayed on it.",
    },
];

/// The entry for this game on this console, if it has been played in Omoio.
pub fn tested(console: Console, title: &str) -> Option<&'static Tested> {
    let key = same_game(title);
    TESTED
        .iter()
        .find(|game| game.console == console && !key.is_empty() && same_game(game.title) == key)
}

/// What a game's pages and its catalogue card say beside the list's rating,
/// already in words, so the library, the catalogue and Big Picture say it
/// alike.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Played {
    pub label: &'static str,
    pub reason: &'static str,
}

/// The line for a game played in Omoio on this console. The list's rating
/// is still shown, since anyone can look it up; this says why Omoio counts
/// the game as playing well all the same.
pub fn played(console: Console, title: &str) -> Option<Played> {
    tested(console, title).map(|game| Played {
        label: "Played in Omoio",
        reason: game.reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tested_game_says_why() {
        for game in TESTED {
            assert!(game.reason.ends_with('.'), "{}", game.title);
            assert!(tested(game.console, game.title).is_some(), "{}", game.title);
        }
    }

    /// A reason is shown as it is, so it keeps to the interface's own rules:
    /// no dashes or exclamation marks, and nobody named.
    #[test]
    fn every_reason_reads_as_interface_text() {
        for game in TESTED {
            for unwanted in ['\u{2014}', '\u{2013}', '!'] {
                assert!(!game.reason.contains(unwanted), "{}: {unwanted}", game.title);
            }
            assert!(!game.reason.contains("Bertram"), "{}", game.title);
        }
    }

    #[test]
    fn a_game_played_in_omoio_gets_its_line_and_another_gets_none() {
        let swap_force = played(Console::Ps3, "Skylanders SWAP Force\u{2122}").unwrap();
        assert_eq!(swap_force.label, "Played in Omoio");
        assert!(swap_force.reason.contains("7 October 2026"), "{}", swap_force.reason);
        assert!(played(Console::WiiU, "Skylanders - Swap Force (Europe)").is_some());
        assert_eq!(played(Console::Ps3, "Skylanders Trap Team"), None, "taken off on 27 October");
        assert_eq!(played(Console::WiiU, "Mario Kart 8"), None);
    }

    #[test]
    fn a_tested_game_is_found_under_its_other_names_on_its_own_console() {
        assert!(tested(Console::WiiU, "Skylanders - Swap Force").is_some_and(|game| game.console == Console::WiiU));
        assert!(tested(Console::WiiU, "Skylanders: Trap Team").is_some());
        assert!(tested(Console::Ps3, "Skylanders SWAP Force™").is_some_and(|game| game.console == Console::Ps3));
        assert!(tested(Console::Ps3, "Skylanders Giants").is_none());
        assert!(tested(Console::WiiU, "Skylanders: SuperChargers").is_none());
        assert!(tested(Console::WiiU, "").is_none());
    }
}
