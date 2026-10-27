//! Problems one game is known to have on one console, each with the public
//! report it comes from.
//!
//! A game here is warned about before it is imported, whatever its
//! emulator's list rates it and whether it has been played in Omoio: a
//! rating says how far a game gets, not what goes wrong on the way. Only a
//! problem with a public report goes here, with that report beside it.

use crate::core::catalogue::same_game;
use crate::core::console::Console;

#[derive(Debug)]
pub struct Problem {
    pub console: Console,
    /// Matched the way names are matched across consoles, as the tested
    /// list's are.
    pub title: &'static str,
    /// What the player meets, and the version that does better.
    pub says: &'static str,
    /// The public report, and what was seen.
    pub source: &'static str,
}

pub const PROBLEMS: &[Problem] = &[Problem {
    console: Console::Ps3,
    title: "Skylanders Spyro's Adventure",
    says: "In RPCS3 the picture can freeze at random while the sound plays on. The Wii version runs better.",
    source: "RPCS3 issue 13844, github.com/RPCS3/rpcs3/issues/13844, \"Freezing in Skylanders: \
             Spyro's Adventure\", open since 12 May 2023 and still open on 27 October 2026: the \
             game freezes at random while the music plays on, and none of the settings tried \
             there (SPU threads, driver wake-up delay, shader quality, frame limit, RSX FIFO \
             accuracy) stops it. A player saw it in five sessions in a row, after 1.5 to 14 \
             minutes, on RPCS3 0.0.43-20395 with its defaults and nothing wrong in the log \
             (reported on Discord, October 2026).",
}];

/// The problem this game is known to have on this console, if any.
pub fn known(console: Console, title: &str) -> Option<&'static Problem> {
    let key = same_game(title);
    PROBLEMS
        .iter()
        .find(|problem| problem.console == console && !key.is_empty() && same_game(problem.title) == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_problem_says_where_it_was_reported() {
        for problem in PROBLEMS {
            assert!(problem.says.ends_with('.'), "{}", problem.title);
            assert!(problem.source.contains("github.com/"), "{}", problem.title);
            assert!(known(problem.console, problem.title).is_some(), "{}", problem.title);
        }
    }

    #[test]
    fn a_problem_is_found_under_the_games_other_names_on_its_console_only() {
        assert!(known(Console::Ps3, "Skylanders: Spyro's Adventure").is_some());
        assert!(known(Console::Ps3, "Skylanders Spyro's Adventure™").is_some());
        assert!(known(Console::Wii, "Skylanders Spyro's Adventure").is_none());
        assert!(known(Console::Ps3, "Skylanders Giants").is_none());
        assert!(known(Console::Ps3, "").is_none());
    }
}
