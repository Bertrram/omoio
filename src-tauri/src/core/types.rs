use serde::Serialize;

#[derive(Serialize)]
pub struct HardwareInfo {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub gpu: Option<GpuInfo>,
    pub display: Option<DisplayInfo>,
}

#[derive(Serialize)]
pub struct CpuInfo {
    pub brand: String,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
}

#[derive(Serialize)]
pub struct MemoryInfo {
    pub total_bytes: u64,
}

#[derive(Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub dedicated_memory_bytes: u64,
}

#[derive(Serialize)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
}

/// A library entry as the interface needs it.
///
/// Two different absences, told apart on purpose. `set_up` is false for a game
/// added from the catalogue that has no files yet. `available` is false for a
/// game that does have files, on a drive that is not plugged in right now,
/// which is normal rather than broken.
#[derive(Serialize)]
pub struct GameEntry {
    #[serde(flatten)]
    pub game: crate::core::library::Game,
    /// The version the emulator runs, read from its own files where it keeps
    /// updates of its own (`EmulatorBackend::running_version`), so the pages
    /// show the one the patches are matched against.
    pub running_version: Option<String>,
    pub available: bool,
    pub set_up: bool,
    /// The dump's own tile art, copied somewhere we control so it still shows
    /// when the drive holding the game is unplugged.
    pub cover: Option<String>,
    /// "dump" for the game's own icon, "rawg" for a RAWG cover.
    pub cover_source: Option<&'static str>,
    /// What this game's emulator can do, so the interface shows only what
    /// applies.
    pub features: crate::core::console::Features,
    /// Whether the portal menu works in this game. The emulator offering a
    /// portal is not enough: the menu has been played through with three
    /// games only, so the interface asks this rather than the title.
    pub portal_menu: bool,
    /// For a Skylanders game the menu doesn't work in, the line its page
    /// shows: "The portal menu doesn't work in this version yet. It works in
    /// the Wii U version."
    pub portal_note: Option<String>,
    /// For Imaginators, what works of the figures an emulator makes
    /// (`figures::made_figures_note`).
    pub figures_note: Option<&'static str>,
    /// The toy the game is played with, which picks the menu over it.
    /// `None` for a game played without one.
    pub toy: Option<crate::core::toys::Toy>,
    /// Whether the toy's menu works in this game, the portal menu among
    /// them (`toys::offers_menu`), for what opens it: the Portal button and
    /// Big Picture's button setting. The same as `portal_menu` for a
    /// Skylanders game, which keeps its own Skylanders things on that.
    pub toy_menu: bool,
    /// For a Disney Infinity or LEGO Dimensions game the menu doesn't work
    /// in, the line its page shows: "The base menu doesn't work in this
    /// version yet. It works in the PS3 version." (`toys::page_note`). A
    /// Skylanders game's line is `portal_note`.
    pub toy_note: Option<String>,
    /// Whether the game's page offers to get its figures' pictures
    /// (`figure_pictures::offered`): a Skylanders game's where `portal_menu`
    /// is, and a Disney Infinity or LEGO Dimensions game's where its menu
    /// works and the picture reader knows the game.
    pub figure_pictures: bool,
    /// For a game played in Omoio, what its pages say beside the list's
    /// rating, with or without one shown (`tested::played`).
    pub tested: Option<crate::core::tested::Played>,
}

#[derive(Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub bytes: u64,
    pub total: u64,
}
