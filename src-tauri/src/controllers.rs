//! Each player's pad and layout, kept by Omoio in controllers.json and handed
//! to every emulator in its own form. What a layout is lives in
//! core/pad_layout.rs; this is keeping it and passing it on.

use crate::backends::EmulatorBackend;
use crate::core::pad_layout::{self, Layouts, Pad, Player, PLAYERS};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app.path().data_dir().map_err(|e| e.to_string())?;
    Ok(data.join("Omoio").join("controllers.json"))
}

/// What is kept. Until something is, what the emulators already had is read
/// in, so a layout set up before Omoio kept its own is not lost.
fn load(app: &AppHandle) -> Layouts {
    if let Some(kept) = path(app).ok().and_then(|file| Layouts::load(&file)) {
        return kept;
    }
    let mut layouts = Layouts::default();
    for backend in crate::backends::all() {
        for (title_id, players) in backend.existing_layouts(app) {
            if layouts.get(&title_id).is_none() {
                layouts.set(&title_id, players);
            }
        }
    }
    layouts
}

fn store(app: &AppHandle, layouts: &Layouts) -> Result<(), String> {
    layouts
        .save(&path(app)?)
        .map_err(|_| "Couldn't save the controller settings.".to_string())
}

/// Hands a layout to every emulator. One failing does not stop the others;
/// the first failure is what is reported.
fn hand_over(app: &AppHandle, title_id: &str, players: &[Player]) -> Result<(), String> {
    let mut failed = None;
    for backend in crate::backends::all() {
        if let Err(err) = backend.write_layout(app, title_id, players) {
            failed.get_or_insert(err);
        }
    }
    failed.map_or(Ok(()), Err)
}

pub struct Current {
    pub players: Vec<Player>,
    /// Whether this game has a layout of its own.
    pub own: bool,
    /// Whether the players shown have been kept, rather than being who
    /// pressing Play would set up.
    pub saved: bool,
}

/// The four players a game plays with: its own layout, else the one for
/// every game, else who pressing Play would set up.
pub fn current(app: &AppHandle, title_id: &str, connected: &[Pad]) -> Current {
    let mut layouts = load(app);
    let own = !title_id.is_empty() && layouts.games.contains_key(title_id);
    let scope = if own { title_id } else { "" };
    let found = layouts.get(scope).cloned();
    let saved = found.is_some();
    let spare = crate::pads::xinput_slots();
    let mut players = match found {
        Some(players) => pad_layout::fill(players.into_iter().map(Some).collect(), &spare),
        None => pad_layout::default_players(connected, &spare),
    };
    pad_layout::refresh(&mut players, connected);
    // Kept as soon as it happens, so the Controller screen shows the pad as
    // player 1 rather than leaving it to be picked, and Play agrees with it.
    if pad_layout::give_lone_pad(&mut players, connected) {
        layouts.set(scope, players.clone());
        let _ = store(app, &layouts);
    }
    Current { players, own, saved }
}

/// Gives player `number`, counted from 1, this pad and layout. A game's first
/// change starts from the layout for every game, which is what it was
/// playing with until then.
pub fn save_player(app: &AppHandle, title_id: &str, number: usize, player: Player) -> Result<(), String> {
    let at = number
        .checked_sub(1)
        .filter(|at| *at < PLAYERS)
        .ok_or("Couldn't save the controller settings.")?;
    let mut players = current(app, title_id, &crate::pads::connected()).players;
    pad_layout::give(&mut players, at, player);
    let mut layouts = load(app);
    layouts.set(title_id, players.clone());
    store(app, &layouts)?;
    hand_over(app, title_id, &players)
}

/// Gives all four players their pad's own layout, pads plugged in first.
pub fn restore_defaults(app: &AppHandle, title_id: &str) -> Result<(), String> {
    let players = pad_layout::default_players(&crate::pads::connected(), &crate::pads::xinput_slots());
    let mut layouts = load(app);
    layouts.set(title_id, players.clone());
    store(app, &layouts)?;
    hand_over(app, title_id, &players)
}

/// Takes a game's own layout away, so it goes back to the one for every game.
pub fn forget(app: &AppHandle, title_id: &str) -> Result<(), String> {
    if title_id.is_empty() {
        return Err("Couldn't remove that.".to_string());
    }
    let mut layouts = load(app);
    layouts.games.remove(title_id);
    store(app, &layouts)?;
    for backend in crate::backends::all() {
        backend.forget_layout(app, title_id)?;
    }
    Ok(())
}

/// Why the game would start with nobody to play player 1, set up as
/// `before_launch` would set the players up, or `None`. A game with its own
/// layout is not asked about: the emulator may keep that layout apart.
pub fn launch_warning(app: &AppHandle, backend: &dyn EmulatorBackend, title_id: &str) -> Option<String> {
    let connected = crate::pads::connected();
    warning(current(app, title_id, &connected), &connected, |players, connected| {
        backend.missing_first_player(app, players, connected)
    })
}

/// `launch_warning` once the players are known: `ask` is the emulator's
/// question about the players as they would start.
fn warning(
    current: Current,
    connected: &[Pad],
    ask: impl FnOnce(&[Player], &[Pad]) -> Option<String>,
) -> Option<String> {
    if current.own {
        return None;
    }
    let mut players = current.players;
    pad_layout::seat(&mut players, connected);
    ask(&players, connected)
}

/// Called before a game starts, so plugging in and pressing Play is enough.
/// A pad plugged in that no player has takes the place of one whose pad is
/// not there, buttons someone chose never change, and the layout goes to the
/// game's emulator.
pub fn before_launch(app: &AppHandle, backend: &dyn EmulatorBackend, title_id: &str) {
    let connected = crate::pads::connected();
    let Current { mut players, own, saved } = current(app, title_id, &connected);
    let moved = pad_layout::seat(&mut players, &connected);
    let scope = if own { title_id } else { "" };
    if moved || !saved {
        let mut layouts = load(app);
        layouts.set(scope, players.clone());
        let _ = store(app, &layouts);
    }
    let _ = backend.write_layout(app, scope, &players);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(device: &str, handler: &str) -> Pad {
        Pad {
            device: device.to_string(),
            name: device.to_string(),
            handler: handler.to_string(),
            family: "generic".to_string(),
        }
    }

    fn on_a_ps5_pad(own: bool) -> Current {
        Current {
            players: vec![Player::on(pad("PS5 Controller 0", "SDL"))],
            own,
            saved: true,
        }
    }

    #[test]
    fn a_game_with_its_own_layout_is_not_asked_about() {
        let asked = warning(on_a_ps5_pad(true), &[], |_, _| Some("no pad".to_string()));
        assert_eq!(asked, None);
    }

    #[test]
    fn the_emulator_is_asked_about_the_players_as_they_would_start() {
        // A pad nobody has takes the place of player 1, whose pad is not
        // plugged in, as `before_launch` would seat it.
        let plugged = [pad("XInput Pad #2", "XInput")];
        let asked = warning(on_a_ps5_pad(false), &plugged, |players, connected| {
            assert_eq!(connected, &plugged[..]);
            Some(players[0].pad.device.clone())
        });
        assert_eq!(asked.as_deref(), Some("XInput Pad #2"));
    }
}
