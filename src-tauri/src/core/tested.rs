//! The short list of games Omoio has been played with, each with its reason.
//!
//! A game here counts as playing well whatever its emulator's list says: it
//! gets no warning before it is imported, and it can be the version offered
//! for another console's copy. Only a game that has been played in Omoio goes
//! here, with what was seen written beside it, because a list rating can be
//! older than the fix that made the game run.

use crate::core::catalogue::same_game;
use crate::core::console::Console;

#[derive(Debug)]
pub struct Tested {
    pub console: Console,
    /// Matched the way names are matched across consoles, so "Skylanders -
    /// Swap Force" from a disc image's file name is this game too.
    pub title: &'static str,
    pub reason: &'static str,
}

pub const TESTED: &[Tested] = &[
    Tested {
        console: Console::WiiU,
        title: "Skylanders SWAP Force",
        reason: "Bertram plays it with the portal menu. The Cemu wiki's Runs is older than \
                 Cemu's Portal Stability Fix pack, which is on by default.",
    },
    Tested {
        console: Console::WiiU,
        title: "Skylanders Trap Team",
        reason: "Tested by Bertram on 6 October 2026, with the portal menu and its villains.",
    },
    Tested {
        console: Console::Ps3,
        title: "Skylanders SWAP Force",
        reason: "Tested by Bertram on 7 October 2026: swappers went on at once. RPCS3's list \
                 rates it Ingame from 2021.",
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
        reason: "Tested by Bertram on 8 October 2026 with the portal menu: a Sensei and new \
                 Creation Crystals went on with Cemu's Signature Patch, and an Imaginator made \
                 in one stayed on it.",
    },
];

/// The entry for this game on this console, if it has been played in Omoio.
pub fn tested(console: Console, title: &str) -> Option<&'static Tested> {
    let key = same_game(title);
    TESTED
        .iter()
        .find(|game| game.console == console && !key.is_empty() && same_game(game.title) == key)
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
