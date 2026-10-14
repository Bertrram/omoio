//! One button layout per player, kept by Omoio and shared by every emulator.
//!
//! Inputs are named by where they sit on a pad, in the names SDL gives them:
//! South is the bottom face button whatever is printed on it, A on an Xbox
//! pad, Cross on a PlayStation one, B on a Nintendo one. A layout says which
//! input on the player's own pad stands for each of those places, and each
//! emulator turns the places into its own console's buttons. A pad set up
//! once then feels the same in every game.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const PLAYERS: usize = 4;

/// Every place a layout covers.
pub const INPUTS: [&str; 25] = [
    "South", "East", "West", "North", "LB", "RB", "LT", "RT", "LS", "RS", "Up", "Down", "Left",
    "Right", "Back", "Start", "Guide", "LS Y+", "LS Y-", "LS X-", "LS X+", "RS Y+", "RS Y-",
    "RS X-", "RS X+",
];

fn generic() -> String {
    "generic".to_string()
}

/// A pad, as the emulators address it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pad {
    /// An Xbox-style pad by its XInput slot, "XInput Pad #1", or any other by
    /// its SDL name and a number, "DualSense Wireless Controller 0".
    pub device: String,
    pub name: String,
    /// How it is read: "XInput" or "SDL".
    pub handler: String,
    /// "xbox", "playstation", "nintendo" or "generic": which drawing it gets
    /// and what its buttons are called.
    #[serde(default = "generic")]
    pub family: String,
}

/// The handler of the pad a player has when someone chose no controller for
/// them.
pub const NO_PAD: &str = "None";

impl Pad {
    /// No controller at all, chosen for a player so no pad is ever given to
    /// them by itself (`fill`, `seat`, `give_lone_pad`). It is kept in the
    /// layout like a pad. Its device is empty, which no pad plugged in has
    /// (pads.rs names each by its name and a number), so it is never taken
    /// for one: RPCS3's side writes such a player with RPCS3's own handler for
    /// no pad, and Cemu's and Dolphin's leave the player's file without a
    /// device.
    pub fn none() -> Self {
        Pad {
            device: String::new(),
            name: "No controller".to_string(),
            handler: NO_PAD.to_string(),
            family: generic(),
        }
    }

    /// Whether this is `Pad::none`, no controller.
    pub fn is_none(&self) -> bool {
        self.handler == NO_PAD
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub pad: Pad,
    /// The places whose input is not the place itself. Nothing else is kept,
    /// so a layout nobody changed is empty.
    #[serde(default)]
    pub buttons: BTreeMap<String, String>,
}

impl Player {
    pub fn on(pad: Pad) -> Self {
        Self {
            pad,
            buttons: BTreeMap::new(),
        }
    }

    /// Keeps only places and inputs a layout knows, and only where they
    /// differ.
    pub fn with_buttons(pad: Pad, buttons: BTreeMap<String, String>) -> Self {
        let known = |name: &str| INPUTS.contains(&name);
        let buttons = buttons
            .into_iter()
            .filter(|(place, input)| known(place) && known(input) && place != input)
            .collect();
        Self { pad, buttons }
    }

    /// The input on this player's pad that stands for `place`.
    pub fn input<'a>(&'a self, place: &'a str) -> &'a str {
        self.buttons.get(place).map_or(place, String::as_str)
    }

    /// Every place with its input, for the Controller screen.
    pub fn all_buttons(&self) -> BTreeMap<String, String> {
        INPUTS
            .iter()
            .map(|place| (place.to_string(), self.input(place).to_string()))
            .collect()
    }
}

/// Everything kept: the layout for every game, and any game's own.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Layouts {
    #[serde(default)]
    pub every_game: Vec<Player>,
    #[serde(default)]
    pub games: BTreeMap<String, Vec<Player>>,
}

impl Layouts {
    /// `None` when nothing has been kept yet, or the file cannot be read.
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let temp = path.with_extension("json.writing");
        std::fs::write(&temp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    }

    /// A game's own players, or with an empty title id the ones for every
    /// game. `None` when there are none.
    pub fn get(&self, title_id: &str) -> Option<&Vec<Player>> {
        if title_id.is_empty() {
            Some(&self.every_game).filter(|players| !players.is_empty())
        } else {
            self.games.get(title_id)
        }
    }

    pub fn set(&mut self, title_id: &str, players: Vec<Player>) {
        if title_id.is_empty() {
            self.every_game = players;
        } else {
            self.games.insert(title_id.to_string(), players);
        }
    }
}

/// Who gets which pad when nothing has been chosen. Pads plugged in now take
/// the first players, in the order they are listed, and the `spare` pads
/// nobody has fill the rest and wait.
pub fn default_players(connected: &[Pad], spare: &[Pad]) -> Vec<Player> {
    let mut pads: Vec<Pad> = connected.to_vec();
    for pad in spare {
        if !pads.iter().any(|known| known.device == pad.device) {
            pads.push(pad.clone());
        }
    }
    pads.truncate(PLAYERS);
    pads.into_iter().map(Player::on).collect()
}

/// Four players from however many were found, each in their place. A player
/// missing gets a `spare` pad no other player has. A player found with no
/// controller (`Pad::none`) keeps none.
pub fn fill(found: Vec<Option<Player>>, spare: &[Pad]) -> Vec<Player> {
    let taken: Vec<String> = found.iter().flatten().map(|p| p.pad.device.clone()).collect();
    let mut free = spare.iter().filter(|pad| !taken.contains(&pad.device));
    let mut found = found.into_iter();
    (0..PLAYERS)
        .map(|at| {
            found
                .next()
                .flatten()
                .or_else(|| free.next().cloned().map(Player::on))
                .unwrap_or_else(|| Player::on(spare[at % spare.len().max(1)].clone()))
        })
        .collect()
}

/// Gives each pad plugged in that belongs to no player the place of the first
/// player whose own pad is not plugged in. Their buttons stay as they were.
/// Returns whether anyone moved.
pub fn seat(players: &mut [Player], connected: &[Pad]) -> bool {
    seat_from(players, connected, connected)
}

/// `seat`, handing out only the pads in `seatable`, those among the pads
/// `connected` that a player may be given without anyone choosing them
/// (pads.rs, `seatable`). A player on any pad plugged in keeps it, and a
/// player someone gave no controller (`Pad::none`) is not waiting for one.
pub fn seat_from(players: &mut [Player], connected: &[Pad], seatable: &[Pad]) -> bool {
    let mut moved = false;
    for pad in seatable {
        if players.iter().any(|p| p.pad.device == pad.device) {
            continue;
        }
        let waiting = players
            .iter()
            .position(|p| !p.pad.is_none() && !connected.iter().any(|c| c.device == p.pad.device));
        if let Some(at) = waiting {
            players[at].pad = pad.clone();
            moved = true;
        }
    }
    moved
}

/// Gives player 1 the only pad plugged in when it is one XInput doesn't read,
/// such as a DualSense or a Switch Pro Controller, and no player has it.
/// Players otherwise wait on the XInput slots, so a lone PlayStation pad
/// would sit unused until someone picked it. An Xbox-style pad is left
/// alone even when it shows up without its XInput slot, as one does for a
/// moment while it is switched on or off. A pad a player already has is
/// never moved, and player 1's own pad cannot be plugged in, since this one
/// is the only one. Their buttons stay as they were. Player 1 with no
/// controller chosen (`Pad::none`) keeps none. Returns whether player 1
/// changed.
pub fn give_lone_pad(players: &mut [Player], connected: &[Pad]) -> bool {
    let [pad] = connected else {
        return false;
    };
    let xbox = pad.handler == "XInput" || pad.family == "xbox";
    if xbox || players.iter().any(|p| p.pad.device == pad.device) {
        return false;
    }
    let Some(first) = players.first_mut().filter(|first| !first.pad.is_none()) else {
        return false;
    };
    first.pad = pad.clone();
    true
}

/// Gives player `at` this pad and layout. A pad another player had is swapped
/// over, so no pad ever drives two players. Any number of players can have
/// no controller (`Pad::none`), so that is never swapped.
pub fn give(players: &mut [Player], at: usize, player: Player) {
    let swapped = (!player.pad.is_none())
        .then(|| players.iter().position(|p| p.pad.device == player.pad.device))
        .flatten();
    if let Some(other) = swapped {
        if other != at {
            players[other].pad = players[at].pad.clone();
        }
    }
    players[at] = player;
}

/// A pad plugged in tells us more than a file does: its name as it reports
/// it, and so which kind it is. Players on it take those.
pub fn refresh(players: &mut [Player], connected: &[Pad]) {
    for player in players {
        if let Some(pad) = connected.iter().find(|c| c.device == player.pad.device) {
            player.pad = pad.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(device: &str) -> Pad {
        Pad {
            device: device.to_string(),
            name: device.to_string(),
            handler: if device.starts_with("XInput") { "XInput" } else { "SDL" }.to_string(),
            family: generic(),
        }
    }

    fn slots() -> Vec<Pad> {
        (1..=4).map(|n| pad(&format!("XInput Pad #{n}"))).collect()
    }

    fn devices(players: &[Player]) -> Vec<&str> {
        players.iter().map(|p| p.pad.device.as_str()).collect()
    }

    #[test]
    fn only_changed_known_buttons_are_kept() {
        let buttons = BTreeMap::from([
            ("South".to_string(), "East".to_string()),
            ("East".to_string(), "East".to_string()),
            ("Nonsense".to_string(), "South".to_string()),
            ("North".to_string(), "Nonsense".to_string()),
        ]);
        let player = Player::with_buttons(pad("XInput Pad #1"), buttons);
        assert_eq!(player.buttons.len(), 1);
        assert_eq!(player.input("South"), "East");
        assert_eq!(player.input("West"), "West", "untouched places are themselves");
        assert_eq!(player.all_buttons().len(), INPUTS.len());
    }

    #[test]
    fn with_nothing_plugged_in_four_players_wait_on_the_spare_pads() {
        assert_eq!(
            devices(&default_players(&[], &slots())),
            ["XInput Pad #1", "XInput Pad #2", "XInput Pad #3", "XInput Pad #4"]
        );
    }

    #[test]
    fn pads_plugged_in_take_the_first_players() {
        let players = default_players(&[pad("XInput Pad #2"), pad("DualSense Wireless Controller 0")], &slots());
        assert_eq!(
            devices(&players),
            ["XInput Pad #2", "DualSense Wireless Controller 0", "XInput Pad #1", "XInput Pad #3"]
        );
    }

    #[test]
    fn missing_players_are_filled_in_their_places() {
        let players = fill(vec![None, Some(Player::on(pad("XInput Pad #1")))], &slots());
        assert_eq!(
            devices(&players),
            ["XInput Pad #2", "XInput Pad #1", "XInput Pad #3", "XInput Pad #4"]
        );
    }

    #[test]
    fn a_pad_nobody_has_takes_the_place_of_one_not_plugged_in() {
        let mut players = default_players(&[], &slots());
        let connected = [pad("XInput Pad #1"), pad("DualSense Wireless Controller 0")];
        assert!(seat(&mut players, &connected));
        assert_eq!(
            devices(&players),
            ["XInput Pad #1", "DualSense Wireless Controller 0", "XInput Pad #3", "XInput Pad #4"]
        );
        assert!(!seat(&mut players, &connected), "nothing moves the second time");
    }

    #[test]
    fn a_lone_playstation_pad_is_player_one() {
        let mut players = default_players(&[], &slots());
        let lone = [pad("PS5 Controller 0")];
        assert!(give_lone_pad(&mut players, &lone));
        assert_eq!(
            devices(&players),
            ["PS5 Controller 0", "XInput Pad #2", "XInput Pad #3", "XInput Pad #4"]
        );
        assert!(!give_lone_pad(&mut players, &lone), "nothing moves the second time");
    }

    #[test]
    fn a_lone_pad_keeps_player_ones_buttons() {
        let buttons = BTreeMap::from([("South".to_string(), "East".to_string())]);
        let mut players = default_players(&[], &slots());
        players[0] = Player::with_buttons(players[0].pad.clone(), buttons);
        give_lone_pad(&mut players, &[pad("PS5 Controller 0")]);
        assert_eq!(players[0].input("South"), "East");
    }

    #[test]
    fn a_lone_pad_someone_chose_for_another_player_stays_there() {
        let mut players = default_players(&[], &slots());
        give(&mut players, 1, Player::on(pad("PS5 Controller 0")));
        assert!(!give_lone_pad(&mut players, &[pad("PS5 Controller 0")]));
        assert_eq!(devices(&players)[..2], ["XInput Pad #1", "PS5 Controller 0"]);
    }

    #[test]
    fn a_lone_xbox_pad_and_two_pads_are_left_as_they_are() {
        let mut players = default_players(&[], &slots());
        assert!(!give_lone_pad(&mut players, &[pad("XInput Pad #2")]));
        let mut powera = pad("PowerA Xbox Series X Controller 0");
        powera.family = "xbox".to_string();
        assert!(!give_lone_pad(&mut players, &[powera]), "an Xbox-style pad seen without its slot");
        assert!(!give_lone_pad(&mut players, &[pad("XInput Pad #1"), pad("PS5 Controller 0")]));
        assert!(!give_lone_pad(&mut players, &[]));
        assert_eq!(devices(&players), devices(&default_players(&[], &slots())));
    }

    #[test]
    fn giving_a_pad_another_player_has_swaps_them() {
        let mut players = default_players(&[], &slots());
        give(&mut players, 0, Player::on(pad("XInput Pad #3")));
        assert_eq!(
            devices(&players),
            ["XInput Pad #3", "XInput Pad #2", "XInput Pad #1", "XInput Pad #4"]
        );
    }

    #[test]
    fn no_controller_stays_chosen_when_players_are_filled_in() {
        let players = fill(vec![Some(Player::on(pad("XInput Pad #1"))), Some(Player::on(Pad::none())), None], &slots());
        assert_eq!(devices(&players), ["XInput Pad #1", "", "XInput Pad #2", "XInput Pad #3"]);
        assert!(players[1].pad.is_none());
    }

    #[test]
    fn a_player_with_no_controller_is_not_waiting_for_one() {
        let mut players = default_players(&[], &slots());
        players[0] = Player::on(Pad::none());
        let connected = [pad("vJoy Device 0"), pad("DualSense Wireless Controller 0")];
        assert!(seat(&mut players, &connected));
        assert_eq!(
            devices(&players),
            ["", "vJoy Device 0", "DualSense Wireless Controller 0", "XInput Pad #4"]
        );
    }

    #[test]
    fn only_seatable_pads_are_handed_out() {
        let mut players = default_players(&[], &slots());
        let connected = [pad("vJoy Device 0"), pad("PS4 Controller 0")];
        assert!(seat_from(&mut players, &connected, &connected[1..]));
        assert_eq!(
            devices(&players),
            ["PS4 Controller 0", "XInput Pad #2", "XInput Pad #3", "XInput Pad #4"]
        );
    }

    #[test]
    fn a_pad_picked_by_hand_is_kept_though_it_would_not_be_handed_out() {
        let mut players = default_players(&[], &slots());
        give(&mut players, 2, Player::on(pad("vJoy Device 0")));
        let connected = [pad("vJoy Device 0"), pad("PS4 Controller 0")];
        assert!(seat_from(&mut players, &connected, &connected[1..]));
        assert_eq!(
            devices(&players),
            ["PS4 Controller 0", "XInput Pad #2", "vJoy Device 0", "XInput Pad #4"]
        );
    }

    #[test]
    fn a_lone_pad_leaves_player_one_with_no_controller_alone() {
        let mut players = default_players(&[], &slots());
        players[0] = Player::on(Pad::none());
        assert!(!give_lone_pad(&mut players, &[pad("PS5 Controller 0")]));
        assert!(players[0].pad.is_none());
    }

    #[test]
    fn no_controller_is_never_swapped() {
        let mut players = default_players(&[], &slots());
        give(&mut players, 1, Player::on(Pad::none()));
        give(&mut players, 3, Player::on(Pad::none()));
        assert_eq!(devices(&players), ["XInput Pad #1", "", "XInput Pad #3", ""]);
        // A pad another player has still swaps, and that player takes no
        // controller in return.
        give(&mut players, 1, Player::on(pad("XInput Pad #1")));
        assert_eq!(devices(&players), ["", "XInput Pad #1", "XInput Pad #3", ""]);
    }

    #[test]
    fn no_controller_is_kept_with_the_layout() {
        let layouts = Layouts {
            every_game: vec![Player::on(Pad::none())],
            games: BTreeMap::new(),
        };
        let text = serde_json::to_string(&layouts).unwrap();
        let back: Layouts = serde_json::from_str(&text).unwrap();
        assert!(back.every_game[0].pad.is_none());
        assert_eq!(back.every_game[0].pad, Pad::none());
    }

    #[test]
    fn a_layout_kept_before_pads_had_a_kind_still_loads() {
        let text = r#"{"every_game":[{"pad":{"device":"XInput Pad #1","name":"Controller 1","handler":"XInput"}}]}"#;
        let layouts: Layouts = serde_json::from_str(text).unwrap();
        assert_eq!(layouts.every_game[0].pad.family, "generic");
        assert!(layouts.every_game[0].buttons.is_empty());
        assert!(layouts.get("BCES01663").is_none());
    }
}
