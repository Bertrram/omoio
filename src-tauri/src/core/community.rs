//! Community packs: what an emulator's community publishes for a game, such
//! as RPCS3's patches and Cemu's graphic packs, in one shape for the
//! interface. Each emulator says where its packs come from and fills these
//! in; nothing here reads a file.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    /// The emulator's own key for the pack, handed back unchanged to switch it.
    pub id: String,
    pub name: String,
    /// What kind of pack it is, such as Graphics, Mods or Fixes, for grouping.
    pub kind: String,
    /// What it does, in its makers' words.
    pub about: String,
    /// Who made it, and its version, in a line.
    pub by: String,
    pub on: bool,
    /// Whether it can be switched on for the copy of the game installed.
    pub applies: bool,
    /// Why it can't, when it can't.
    pub needs: Option<String>,
    /// Why it is on without being asked, when it is.
    pub on_because: Option<String>,
    /// Choices within the pack, such as a frame rate.
    pub choices: Vec<PackChoice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackChoice {
    /// The choice's own name, empty when the pack gives it none.
    pub name: String,
    pub options: Vec<String>,
    pub chosen: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Packs {
    /// False until the packs have been downloaded.
    pub have_list: bool,
    /// Who makes them, for the line at the top: "the RPCS3 community".
    pub source: String,
    /// Why there are none to show, when the emulator knows: a game that has
    /// to be played once first, for instance.
    pub waiting: Option<String>,
    pub packs: Vec<Pack>,
}

/// A pack switched on or off, with the choices made within it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackChange {
    pub id: String,
    pub on: bool,
    #[serde(default)]
    pub choices: BTreeMap<String, String>,
}

/// Whether a game takes the figures an emulator's figure maker makes for it.
/// Imaginators checks a factory signature on the figures released for it,
/// which a made figure doesn't carry, and a community pack can switch that
/// check off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MadeFigures {
    pub check: Check,
    /// The pack that switches the check off, as the Community packs list
    /// names it. Empty when the game checks nothing.
    pub pack: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    /// The game takes made figures as they are.
    #[default]
    None,
    /// The pack is on in the game running now.
    Passed,
    /// The pack is on, and counts from the game's next start.
    NextStart,
    /// The user switched the pack off.
    Off,
    /// The emulator's packs aren't downloaded, or this one isn't among them.
    NotDownloaded,
}
