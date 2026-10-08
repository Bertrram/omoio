import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface CpuInfo {
  brand: string;
  physical_cores: number | null;
  logical_cores: number;
}

export interface MemoryInfo {
  total_bytes: number;
}

export interface GpuInfo {
  name: string;
  dedicated_memory_bytes: number;
}

export interface DisplayInfo {
  width: number;
  height: number;
  refresh_hz: number;
}

export interface HardwareInfo {
  cpu: CpuInfo;
  memory: MemoryInfo;
  gpu: GpuInfo | null;
  display: DisplayInfo | null;
}

export function getHardwareInfo(): Promise<HardwareInfo> {
  return invoke("get_hardware_info");
}

export interface InstallProgress {
  stage: "checking" | "downloading" | "verifying" | "extracting" | "done";
  bytes: number;
  total: number;
}

export function getRpcs3Version(): Promise<string | null> {
  return invoke("get_rpcs3_version");
}

export function installRpcs3(): Promise<string> {
  return invoke("install_rpcs3");
}

export function cancelRpcs3Install(): Promise<void> {
  return invoke("cancel_rpcs3_install");
}

export function onRpcs3InstallProgress(handler: (progress: InstallProgress) => void): Promise<UnlistenFn> {
  return listen<InstallProgress>("rpcs3-install-progress", (event) => handler(event.payload));
}

export interface Game {
  title_id: string;
  title: string;
  /// What the dump itself reports.
  version: string | null;
  /// The official update Omoio installed, if any. This is what actually runs.
  update_version: string | null;
  path: string;
  size_bytes: number;
  /// False when the folder isn't reachable right now, e.g. an external drive.
  available: boolean;
  /// Cached copy of the dump's own ICON0.PNG, or null if it had none.
  cover: string | null;
  /// False for a game noted from the catalogue that has no files yet. Not the
  /// same as `available`, which means the files exist but the drive is out.
  set_up: boolean;
  /// Where the cover came from: the dump's own icon, or RAWG.
  cover_source: "dump" | "rawg" | null;
  /// Which console the game is for, and so which emulator runs it.
  console: Console;
  /// What its emulator can do beyond starting it.
  features: {
    updates: boolean;
    /// Community packs: patches, graphic packs.
    packs: boolean;
    settings: boolean;
    saves: boolean;
    compatibility: boolean;
    /// The toy portal of a Skylanders game, filled from the portal menu.
    portal: boolean;
    /// The game stops reading the pad while Omoio is in front of it, so
    /// Big Picture's presses never reach a game waiting behind it.
    quiet_behind: boolean;
  };
  /// Whether the portal menu works in this game. Omoio's own rule, so the
  /// interface never guesses it from the title.
  portal_menu: boolean;
  /// For a Skylanders game the menu doesn't work in: "The portal menu
  /// doesn't work in this version yet. It works in the Wii U version."
  portal_note: string | null;
}

export function listGames(): Promise<Game[]> {
  return invoke("list_games");
}

export function importGame(path: string): Promise<Game> {
  return invoke("import_game", { path });
}

/// What to tell someone before a game that may not run well, or a Skylanders
/// game the portal menu doesn't work in, is imported.
export interface ImportWarning {
  title: string;
  console: Console;
  console_name: string;
  /// "Omoio's portal menu doesn't work in Skylanders SuperChargers on the
  /// PS3, so you can't put figures on the portal." Said first. Empty when
  /// the menu works or the game has no portal.
  portal: string;
  /// "RPCS3 rates it Ingame: it starts, but you may hit problems before the
  /// end." Empty when only the portal is warned about.
  rating: string;
  /// "The Wii U version is rated Playable in Cemu.", or with a portal
  /// warning "The portal menu works in the Wii U version." Empty when there
  /// is no such version.
  better: string;
}

export interface ImportCheck {
  /// False when the game can only be told once an archive is unpacked, so
  /// the check is made after the import instead.
  checked: boolean;
  warning: ImportWarning | null;
}

/// Answered from the compatibility lists Omoio already holds, so it never
/// waits on the network.
export function importCheck(path: string): Promise<ImportCheck> {
  return invoke("import_check", { path });
}

/// The same warning for a game already in the library.
export function gameWarning(titleId: string): Promise<ImportWarning | null> {
  return invoke("game_warning", { titleId });
}

export function removeGame(titleId: string): Promise<void> {
  return invoke("remove_game", { titleId });
}

export function launchGame(titleId: string): Promise<void> {
  return invoke("launch_game", { titleId });
}

/// Why the game would start with nobody answering player 1, asked before
/// Play. Null when all is well.
export function launchWarning(titleId: string): Promise<string | null> {
  return invoke("launch_warning", { titleId });
}

export interface Playing {
  title_id: string;
  title: string;
  console: Console;
}

export function stopGame(): Promise<void> {
  return invoke("stop_game");
}

export function playingGame(): Promise<Playing | null> {
  return invoke("playing_game");
}

export function setGameFullscreen(fullscreen: boolean): Promise<void> {
  return invoke("set_game_fullscreen", { fullscreen });
}

export function onGameStarted(handler: (playing: Playing) => void): Promise<UnlistenFn> {
  return listen<Playing>("game-started", (event) => handler(event.payload));
}

export function onGameStopped(handler: () => void): Promise<UnlistenFn> {
  return listen("game-stopped", () => handler());
}

export function onGameFullscreen(handler: (on: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("game-fullscreen", (event) => handler(event.payload));
}

/// Once a game has started, when its emulator couldn't find a player's
/// controller. The message is worded for the person playing.
export function onPadNotFound(handler: (message: string) => void): Promise<UnlistenFn> {
  return listen<string>("pad-not-found", (event) => handler(event.payload));
}

export interface Machine {
  rpcs3: string | null;
  cpu: string | null;
  os: string | null;
  gpu: string | null;
  renderer: string | null;
}

export interface PlaySession {
  /// Which console it ran on. Sessions kept before this was recorded were PS3.
  console?: Console;
  title_id: string;
  title: string;
  started: string;
  seconds: number;
  ending: "stopped" | "closed" | "crashed";
  machine: Machine;
  problems: string[];
  log_file: string;
}

export function listSessions(titleId?: string): Promise<PlaySession[]> {
  return invoke("list_sessions", { titleId: titleId ?? null });
}

export function readSessionLog(path: string): Promise<string> {
  return invoke("read_session_log", { path });
}

export function sessionPrompt(logFile: string): Promise<string> {
  return invoke("session_prompt", { logFile });
}

export interface Places {
  data: string;
  library: string;
  settings: string;
  logs: string;
  covers: string;
  figures: string;
  rpcs3: string;
  games_folder: string | null;
}

export function getPlaces(): Promise<Places> {
  return invoke("get_places");
}

export interface GameOption {
  /// The setting's full path through the emulator's config, joined with
  /// newlines. A section name can contain a slash, so nothing gentler is safe.
  key: string;
  group: string;
  name: string;
  label: string;
  hint: string;
  kind: "choice" | "number" | "switch" | "text";
  choices: string[];
  default: string;
  min: number;
  max: number;
  common: boolean;
}

/// path -> value. Anything absent is RPCS3's own default.
export type ChosenSettings = Record<string, string>;

/// Everything the settings sheet needs for one game.
export interface GameSettings {
  /// The emulator's name, as in "Cemu default".
  emulator: string;
  options: GameOption[];
  /// What this game is set to.
  chosen: ChosenSettings;
  /// Why Omoio set any of them itself, by key.
  reasons: Record<string, string>;
  /// The groups the Common tab shows, in order.
  common_groups: string[];
}

export function gameSettings(titleId: string): Promise<GameSettings> {
  return invoke("game_settings", { titleId });
}

export function setGameSettings(titleId: string, chosen: ChosenSettings): Promise<void> {
  return invoke("set_game_settings", { titleId, chosen });
}

export interface Settings {
  games_folder: string | null;
  start_fullscreen: boolean;
  /// Open Omoio in Big Picture, for a PC under a TV.
  start_in_big_picture: boolean;
  keep_sessions: number;
  /// The resolution scale Omoio set for this machine, once it has.
  tuned_scale: number | null;
  /// Real covers from RAWG in place of the generated tiles. Off until asked for.
  covers: boolean;
  /// The user's own RAWG key, kept on this machine only.
  rawg_key: string | null;
}

export function getSettings(): Promise<Settings> {
  return invoke("get_settings");
}

export function setStartFullscreen(on: boolean): Promise<void> {
  return invoke("set_start_fullscreen", { on });
}

export function setStartInBigPicture(on: boolean): Promise<void> {
  return invoke("set_start_in_big_picture", { on });
}

export interface BigPictureState {
  on: boolean;
  /// A game is running with its picture taken off the screen, waiting
  /// behind Big Picture.
  suspended: boolean;
}

export function bigPicture(): Promise<BigPictureState> {
  return invoke("big_picture");
}

export function setBigPicture(on: boolean): Promise<void> {
  return invoke("set_big_picture", { on });
}

/// Puts the game waiting behind Big Picture back on the screen.
export function resumeGame(): Promise<void> {
  return invoke("resume_game");
}

export function onBigPicture(handler: (state: BigPictureState) => void): Promise<UnlistenFn> {
  return listen<BigPictureState>("big-picture", (event) => handler(event.payload));
}

export function setKeepSessions(keep: number): Promise<void> {
  return invoke("set_keep_sessions", { keep });
}

export function revealFolder(path: string): Promise<void> {
  return invoke("reveal_folder", { path });
}

export function forgetAllGames(): Promise<void> {
  return invoke("forget_all_games");
}

export function clearSessionLogs(): Promise<void> {
  return invoke("clear_session_logs");
}

export function getGamesFolder(): Promise<string | null> {
  return invoke("get_games_folder");
}

export function setGamesFolder(path: string): Promise<void> {
  return invoke("set_games_folder", { path });
}

export function importArchive(path: string): Promise<Game> {
  return invoke("import_archive", { path });
}

export function cancelImport(): Promise<void> {
  return invoke("cancel_import");
}

export interface ImportProgress {
  stage: "unpacking" | "identifying";
  bytes: number;
  total: number;
}

export function onImportProgress(handler: (progress: ImportProgress) => void): Promise<UnlistenFn> {
  return listen<ImportProgress>("import-progress", (event) => handler(event.payload));
}

export function getFirmwareVersion(): Promise<string | null> {
  return invoke("get_firmware_version");
}

export function installFirmware(path: string): Promise<string> {
  return invoke("install_firmware", { path });
}

export interface Compatibility {
  known: boolean;
  label: string;
  tone: "go" | "warn" | "bad" | "mute";
  explanation: string;
  checked: string;
  stale: boolean;
  have_list: boolean;
}

export function gameCompatibility(titleId: string): Promise<Compatibility> {
  return invoke("game_compatibility", { titleId });
}

/// Every console's list, or only `console`'s.
export function refreshCompatibility(console?: Console): Promise<number> {
  return invoke("refresh_compatibility", { console: console ?? null });
}

export function cancelCompatibility(): Promise<void> {
  return invoke("cancel_compatibility");
}

export interface CompatProgress {
  stage: "names";
  bytes: number;
  total: number;
}

export function onCompatProgress(
  handler: (progress: CompatProgress) => void
): Promise<UnlistenFn> {
  return listen<CompatProgress>("compat-progress", (event) => handler(event.payload));
}

/// One thing an emulator's community publishes for a game: a patch, a
/// graphic pack.
export interface Pack {
  /// The emulator's own key for it, handed back unchanged to switch it.
  id: string;
  name: string;
  /// Graphics, Mods, Fixes and so on, for grouping.
  kind: string;
  about: string;
  /// Who made it, and its version.
  by: string;
  on: boolean;
  applies: boolean;
  /// Why it can't be switched on, when it can't.
  needs: string | null;
  /// Why it is on without being asked, when it is.
  on_because: string | null;
  choices: PackChoice[];
}

export interface PackChoice {
  /// Empty when the pack gives the choice no name.
  name: string;
  options: string[];
  chosen: string;
}

export interface Packs {
  /// False until the packs have been downloaded.
  have_list: boolean;
  /// Who makes them: "the Cemu community".
  source: string;
  /// Why none can be shown yet, when there is a reason.
  waiting: string | null;
  /// They come with the emulator, as Dolphin's do, so there is nothing to
  /// download or check for.
  with_emulator: boolean;
  packs: Pack[];
}

export interface PackChange {
  id: string;
  on: boolean;
  choices: Record<string, string>;
}

/// A game from the catalogue isn't in the library, so it names its console.
export function communityPacks(titleId: string, console?: Console): Promise<Packs> {
  return invoke("community_packs", { titleId, console: console ?? null });
}

export function setCommunityPack(titleId: string, change: PackChange): Promise<void> {
  return invoke("set_community_pack", { titleId, change });
}

/// Downloads the newest packs for a console's emulator. Returns how many
/// there are.
export function refreshCommunity(console: Console): Promise<number> {
  return invoke("refresh_community", { console });
}

export function cancelCommunity(): Promise<void> {
  return invoke("cancel_community");
}

export function onCommunityProgress(handler: (progress: InstallProgress) => void): Promise<UnlistenFn> {
  return listen<InstallProgress>("community-progress", (event) => handler(event.payload));
}

export interface ScanProgress {
  stage: "looking" | "reading";
  done: number;
  total: number;
  title: string;
}

export interface ScanResult {
  added: number;
  already_there: number;
  not_games: number;
  cancelled: boolean;
  /// The games added that may not run well.
  warnings: ImportWarning[];
}

export function scanFolder(path: string): Promise<ScanResult> {
  return invoke("scan_folder", { path });
}

export function onScanProgress(handler: (progress: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>("scan-progress", (event) => handler(event.payload));
}

export interface GameUpdate {
  version: string;
  size: number;
  sha1: string;
  url: string;
  firmware: string;
}

export function gameUpdates(titleId: string): Promise<GameUpdate[]> {
  return invoke("game_updates", { titleId });
}

export function installUpdate(titleId: string, update: GameUpdate): Promise<void> {
  return invoke("install_update", { titleId, update });
}

export function cancelUpdate(): Promise<void> {
  return invoke("cancel_update");
}

/// How far an update run has got, across every package in it.
export interface UpdateProgress {
  /// The version being downloaded or installed right now.
  version: string;
  /// Which package this is, counted from 1, and how many there are.
  step: number;
  steps: number;
  /// Bytes downloaded across the whole run, and the size of the whole run.
  bytes: number;
  total: number;
  /// True once this package is downloaded and is being installed.
  installing: boolean;
}

export function onUpdateProgress(handler: (progress: UpdateProgress) => void): Promise<UnlistenFn> {
  return listen<UpdateProgress>("update-progress", (event) => handler(event.payload));
}

export interface SaveBackup {
  /// Seconds since the epoch, like the session logs use.
  made: number;
  bytes: number;
  /// Not always one: a game can keep several save folders, or several
  /// files on a GameCube memory card.
  saves: number;
}

export function gameSaves(titleId: string): Promise<[boolean, SaveBackup[]]> {
  return invoke("game_saves", { titleId });
}

export function backUpSaves(titleId: string): Promise<SaveBackup | null> {
  return invoke("back_up_saves", { titleId });
}

export function restoreSaves(titleId: string, made: number): Promise<void> {
  return invoke("restore_saves", { titleId, made });
}

export function forgetBackup(titleId: string, made: number): Promise<void> {
  return invoke("forget_backup", { titleId, made });
}

export interface Account {
  username: string;
  /// Empty when RPCS3 is set to a combination no region of ours describes.
  region: string;
}

export interface RegionChoice {
  id: string;
  name: string;
  language: string;
}

export function getAccount(): Promise<Account> {
  return invoke("get_account");
}

export function listRegions(): Promise<RegionChoice[]> {
  return invoke("list_regions");
}

export function setUsername(name: string): Promise<string> {
  return invoke("set_username", { name });
}

export function setRegion(id: string): Promise<void> {
  return invoke("set_region", { id });
}

export function needsSetup(): Promise<boolean> {
  return invoke("needs_setup");
}

export function finishSetup(): Promise<void> {
  return invoke("finish_setup");
}

export interface PendingUpdate {
  title_id: string;
  title: string;
  installed: string;
  newest: string;
}

export function pendingUpdates(): Promise<[boolean, PendingUpdate[]]> {
  return invoke("pending_updates");
}

export type Console = "ps3" | "wiiu" | "wii" | "gamecube";

/// Each console as it is usually shortened, where room is short.
export const CONSOLE_SHORT: Record<Console, string> = { ps3: "PS3", wiiu: "Wii U", wii: "Wii", gamecube: "GameCube" };

/// The emulator that runs each console. Dolphin runs two.
export const EMULATOR_OF: Record<Console, string> = { ps3: "RPCS3", wiiu: "Cemu", wii: "Dolphin", gamecube: "Dolphin" };

export interface Release {
  title_id: string;
  region: string;
}

/// One game in the catalogue, however many times it was released.
export interface Listing {
  console: Console;
  console_name: string;
  /// Unique across the catalogue. Covers are cached under it.
  key: string;
  /// The game's name, or its title id when the list has none.
  name: string;
  named: boolean;
  /// The best any release of it is reported to do. Empty when nobody has.
  status: { label: string; tone: string; explanation: string };
  /// "Virtual Console" for an older console's game sold again, else empty.
  kind: string;
  regions: string[];
  /// Every release with a title id, the chosen region's first.
  releases: Release[];
  demo: boolean;
  owned: boolean;
  features: Game["features"];
  /// For a Skylanders game, whether Omoio's portal menu works in it. Null
  /// for any other game.
  portal_menu: boolean | null;
  /// For a Skylanders game the menu doesn't work in, where it does.
  portal_note: string | null;
}

export interface CatalogueFilter {
  query: string;
  console: Console | null;
  region: string;
  runs: string;
  hide_demos: boolean;
  sort: string;
  limit: number;
}

export interface CatalogueView {
  have_list: boolean;
  consoles: { console: Console; name: string }[];
  missing: Console[];
  total: number;
  shown: Listing[];
  sources: { label: string; url: string }[];
}

export function catalogue(filter: CatalogueFilter): Promise<CatalogueView> {
  return invoke("catalogue", { filter });
}

export function addToLibrary(console: Console, titleId: string, title: string): Promise<void> {
  return invoke("add_to_library", { console, titleId, title });
}

export interface InstalledPackage {
  title_id: string;
  title: string;
  version: string;
  size_bytes: number;
}

export function installedPackages(): Promise<InstalledPackage[]> {
  return invoke("installed_packages");
}

export function installPackage(path: string): Promise<void> {
  return invoke("install_package", { path });
}

export function removePackage(titleId: string): Promise<void> {
  return invoke("remove_package", { titleId });
}

export type DroppedKind = "folder" | "archive" | "unknown";

export function droppedKind(path: string): Promise<DroppedKind> {
  return invoke("dropped_kind", { path });
}

/// Which kind of pad, for its drawing and the names on its buttons.
export type PadFamily = "xbox" | "playstation" | "nintendo" | "generic";

export interface Pad {
  /// How the emulators address it, e.g. "XInput Pad #1".
  device: string;
  name: string;
  handler: string;
  family: PadFamily;
}

export interface PlayerSetup {
  pad: Pad;
  /// Whether that pad is plugged in right now.
  connected: boolean;
  /// Every place on a pad, by SDL name such as "South", and the input on this
  /// player's pad standing for it.
  buttons: Record<string, string>;
}

/// What one console calls each place on a pad.
export interface ConsoleButtons {
  console: Console;
  name: string;
  buttons: Record<string, string>;
}

export interface ControllerView {
  connected: Pad[];
  /// Players one to four, in order.
  players: PlayerSetup[];
  /// Every pad a player can be given, plugged in or not.
  pads: Pad[];
  /// False until the layout is kept. The players shown are then the ones
  /// pressing Play will set up.
  saved: boolean;
  own: boolean;
  /// Every place a layout covers.
  inputs: string[];
  consoles: ConsoleButtons[];
}

export function controllerView(titleId: string): Promise<ControllerView> {
  return invoke("controller_view", { titleId });
}

export function setUpController(titleId: string): Promise<void> {
  return invoke("set_up_controller", { titleId });
}

/// `number` is the player, counted from 1.
export function saveController(
  titleId: string,
  number: number,
  pad: Pad,
  buttons: Record<string, string>
): Promise<void> {
  return invoke("save_controller", { titleId, number, pad, buttons });
}

export function forgetController(titleId: string): Promise<void> {
  return invoke("forget_controller", { titleId });
}

/// What is held on a pad right now, by SDL name, or null when it does not
/// answer: switched off, or not a pad Omoio reads directly, which today is
/// anything but an Xbox pad.
export function padInput(device: string): Promise<string[] | null> {
  return invoke("pad_input", { device });
}

/// Everything held on any pad plugged in.
export function padsHeld(): Promise<string[]> {
  return invoke("pads_held");
}

/// The pads plugged in right now.
export function padsConnected(): Promise<Pad[]> {
  return invoke("pads_connected");
}

/// One of the user's figure files, kept in Omoio's figures folder.
/// Kaos is an element of his own in Imaginators, held by the Kaos Sensei only.
export type FigureElement = "air" | "earth" | "fire" | "water" | "life" | "undead" | "magic" | "tech" | "light" | "dark" | "kaos";

export type FigureKind = "character" | "item" | "trap" | "adventure" | "vehicle" | "trophy" | "crystal";

/// How a Swap Force swapper gets about, which its bottom half decides.
export type Movement = "bounce" | "climb" | "dig" | "rocket" | "sneak" | "speed" | "spin" | "teleport";

/// Where a SuperChargers vehicle goes, which is also what a trophy is for.
export type Terrain = "land" | "sea" | "sky";

/// An Imaginators Sensei's battle class, which it teaches the Imaginators of
/// its class. Kaos is a class of his own.
export type BattleClass =
  | "knight"
  | "bowslinger"
  | "quickshot"
  | "ninja"
  | "brawler"
  | "smasher"
  | "sorcerer"
  | "swashbuckler"
  | "sentinel"
  | "bazooker"
  | "kaos";

/// The design of an Imaginators Creation Crystal's casing, by the names
/// collectors give them; Activision named none.
export type Casing = "angel" | "pyramid" | "lantern" | "rune" | "reactor" | "acorn" | "armor" | "fanged" | "claw" | "rocket";

export interface Figure {
  name: string;
  path: string;
  /// Known for figures Omoio had the emulator make, null for the user's own files.
  id: number | null;
  variant: number | null;
  element: FigureElement | null;
  kind: FigureKind | null;
  /// Set where the figure's name doesn't say its series, as with SWAP
  /// Force's new poses of older Skylanders (series 3).
  series: number | null;
  movement: Movement | null;
  class: FigureClass | null;
  /// A vehicle's terrain, or the races a trophy is for.
  terrain: Terrain | null;
  /// A vehicle's own SuperCharger, or a SuperCharger's own vehicle, by id.
  partner: number | null;
  /// An Imaginators Sensei's battle class.
  battle_class: BattleClass | null;
  /// A Creation Crystal's casing.
  casing: Casing | null;
  /// The villain a Trap Team trap holds, read from the trap's own data.
  holds: Trapped | null;
}

export interface Trapped {
  /// The villain's number in the game's own files, 1001 to 1046.
  villain: number;
  /// Its variant form, such as Outlaw Brawl and Chain.
  variant: boolean;
  evolved: boolean;
}

/// The user's figure files. With `playable`, only those the running game reads.
export function figures(playable = false): Promise<Figure[]> {
  return invoke("figures", { playable });
}

/// A Trap Team villain: caught or not, and the saved trap that holds it now.
export interface Villain {
  id: number;
  name: string;
  /// null for Kaos, who only fits his own trap.
  element: FigureElement | null;
  caught: boolean;
  trap: {
    name: string;
    path: string;
    id: number;
    variant: number;
    /// The trap's own element, null for the Kaos trap.
    element: FigureElement | null;
    variant_form: boolean;
    evolved: boolean;
  } | null;
}

export function villains(): Promise<Villain[]> {
  return invoke("villains");
}

/// Resolves to how many were added.
export function addFigures(paths: string[]): Promise<number> {
  return invoke("add_figures", { paths });
}

/// Moves one of the user's saved figures to the Recycle Bin.
export function deleteFigure(path: string): Promise<void> {
  return invoke("delete_figure", { path });
}

/// The figures on the running game's portal, by slot, empty where none is.
export function portalFigures(): Promise<string[]> {
  return invoke("portal_figures");
}

/// `slot` counts from 0. Resolves to what the portal holds afterwards.
export function portalLoad(slot: number, figure: string): Promise<string[]> {
  return invoke("portal_load", { slot, figure });
}

export function portalClear(slot: number): Promise<string[]> {
  return invoke("portal_clear", { slot });
}

export function closePortalMenu(): Promise<void> {
  return invoke("close_portal_menu");
}

/// The kind of pad that opened the portal menu.
export function portalMenuFamily(): Promise<string> {
  return invoke("portal_menu_family");
}

/// A character the running game's emulator can make a figure of.
export interface Character {
  name: string;
  id: number;
  variant: number;
}

/// A character as the portal menu lists it: what the game reads, with the
/// element and kind that decide its tab.
export interface Offer extends Character {
  element: FigureElement | null;
  kind: FigureKind;
  /// A Swap Force swapper is two figures, a top and a bottom.
  half: "top" | "bottom" | null;
  series: number | null;
  /// Set on a bottom half.
  movement: Movement | null;
  class: FigureClass | null;
  /// A vehicle's terrain, or the races a trophy is for.
  terrain: Terrain | null;
  /// A vehicle's own SuperCharger, or a SuperCharger's own vehicle, by id,
  /// whatever the variant.
  partner: number | null;
  /// What a SuperChargers trophy unlocks: the villains to race as and the
  /// tracks it opens. Left out for any other figure.
  unlocks?: { villains: string[]; tracks: string[] };
  /// An Imaginators Sensei's battle class.
  battle_class: BattleClass | null;
  /// A Creation Crystal's casing.
  casing: Casing | null;
}

/// The kinds of Skylander the games' checklists mark apart: the Giants,
/// Trap Team's Trap Masters, the Minis, SuperChargers' own Skylanders, and
/// Imaginators' Senseis, the villains among them marked apart.
export type FigureClass = "giant" | "trap_master" | "mini" | "supercharger" | "sensei" | "villain_sensei";

export function figureCharacters(): Promise<Offer[]> {
  return invoke("figure_characters");
}

export type SkylandersGame = "spyro" | "giants" | "swapforce" | "trapteam" | "superchargers" | "imaginators";

/// The Skylanders game running now, which decides how the portal menu is
/// laid out. `null` for one Omoio can't tell.
export function portalGame(): Promise<SkylandersGame | null> {
  return invoke("portal_game");
}

/// Whether the running game takes the figures its emulator makes. Imaginators
/// checks a factory signature on its own figures, which a made one can't
/// carry, and a community pack, named in `pack`, takes that check away.
export interface MadeFigures {
  check: "none" | "passed" | "next_start" | "off" | "not_downloaded" | "missing";
  pack: string;
}

export function portalMadeFigures(): Promise<MadeFigures> {
  return invoke("portal_made_figures");
}

/// Makes a new figure of `character` and puts it on the portal in `slot`,
/// counted from 0. Resolves to what the portal holds afterwards and the new
/// figure's file.
export function portalCreate(slot: number, character: Character): Promise<{ names: string[]; path: string }> {
  return invoke("portal_create", { slot, character });
}

/// The pad button that opens the portal menu while a Skylanders game runs,
/// as a place such as "Guide".
export function portalButton(): Promise<string> {
  return invoke("portal_button");
}

export function setPortalButton(button: string): Promise<void> {
  return invoke("set_portal_button", { button });
}

export function onPortalMenu(handler: (state: { open: boolean; family: string }) => void): Promise<UnlistenFn> {
  return listen<{ open: boolean; family: string }>("portal-menu", (event) => handler(event.payload));
}

/// The figures' pictures a game has so far: where they are, and each one's
/// name without `.png`, `<id>-<variant>` with the variant as four hex digits.
/// The game's element symbols are `element-<element>`, its Swap Zone badges
/// `movement-<movement>` and a kind of figure's badge `class-<kind>`.
export interface FigurePictures {
  folder: string;
  names: string[];
  /// Badges this game lacks that another of the user's games had, by name,
  /// with the file.
  elsewhere: Record<string, string>;
}

/// Without a title id, the running game's, which is how the portal menu asks.
export function figurePictures(titleId?: string): Promise<FigurePictures> {
  return invoke("figure_pictures", { titleId: titleId ?? null });
}

/// What asking for the pictures came to: how many were kept, or, for a game
/// whose own files can't be read, the room in bytes that a temporary copy
/// of it needs, and the room free, to ask the user about first.
export type GotPictures = { pictures: number } | { copy: { need: number; free: number } };

/// Reads the figures' pictures out of the user's own copy of the game. With
/// `copy`, the user has agreed to a temporary copy being made first.
export function getFigurePictures(titleId: string, copy = false): Promise<GotPictures> {
  return invoke("get_figure_pictures", { titleId, copy });
}

export function stopFigurePictures(): Promise<void> {
  return invoke("stop_figure_pictures");
}

/// How far getting the pictures is: making the copy, then reading.
export interface PicturesProgress {
  title_id: string;
  step: "copy" | "read";
  done: number;
  of: number;
}

export function onFigurePictures(handler: (progress: PicturesProgress) => void): Promise<UnlistenFn> {
  return listen<PicturesProgress>("figure-pictures", (event) => handler(event.payload));
}

export function setCovers(on: boolean): Promise<void> {
  return invoke("set_covers", { on });
}

export function setRawgKey(key: string): Promise<void> {
  return invoke("set_rawg_key", { key });
}

/// Looks up covers for the library. Resolves to how many games have one.
export function fetchCovers(): Promise<number> {
  return invoke("fetch_covers");
}

/// The RAWG cover for a catalogue game, or null when there is none or covers
/// are off.
export function catalogueCover(key: string, name: string, console: Console): Promise<string | null> {
  return invoke("catalogue_cover", { key, name, console });
}

export interface EmulatorVersion {
  console: Console;
  /// Null when it is not installed.
  version: string | null;
}

export function emulatorVersions(): Promise<EmulatorVersion[]> {
  return invoke("emulator_versions");
}

export function installCemu(): Promise<string> {
  return invoke("install_cemu");
}

export function cancelCemuInstall(): Promise<void> {
  return invoke("cancel_cemu_install");
}

/// How many of the user's own keys Cemu has for Wii U disc images.
export function cemuKeys(): Promise<number> {
  return invoke("cemu_keys");
}

/// Resolves to how many of the file's keys were new.
export function addCemuKeys(path: string): Promise<number> {
  return invoke("add_cemu_keys", { path });
}

/// A game's save in the user's own Cemu, and whether Omoio's Cemu has one:
/// none yet, the same files, or a save of its own.
export interface OwnCemuSave {
  title_id: string;
  name: string | null;
  state: "new" | "same" | "differs";
}

/// What the user's own Cemu has to bring over to Omoio's.
export interface OwnCemu {
  /// The folder its keys and saves were found in.
  folder: string;
  /// Keys Omoio's Cemu doesn't have yet.
  keys: number;
  keys_problem: string | null;
  saves: OwnCemuSave[];
}

/// Only reads the folder picked.
export function lookAtOwnCemu(path: string): Promise<OwnCemu> {
  return invoke("look_at_own_cemu", { path });
}

/// Copies the keys, and the saves of games Omoio's Cemu has none for.
/// Overwrites nothing. Resolves to how many of each were copied.
export function bringOwnCemu(path: string): Promise<{ keys: number; saves: number }> {
  return invoke("bring_own_cemu", { path });
}

/// Puts the user's save for one game in place of Omoio's, which is moved
/// aside first.
export function replaceWithOwnSave(path: string, titleId: string): Promise<void> {
  return invoke("replace_with_own_save", { path, titleId });
}

export function onCemuInstallProgress(handler: (progress: InstallProgress) => void): Promise<UnlistenFn> {
  return listen<InstallProgress>("cemu-install-progress", (event) => handler(event.payload));
}

/// Dolphin runs Wii and GameCube games from the same install.
export function installDolphin(): Promise<string> {
  return invoke("install_dolphin");
}

export function cancelDolphinInstall(): Promise<void> {
  return invoke("cancel_dolphin_install");
}

export function onDolphinInstallProgress(handler: (progress: InstallProgress) => void): Promise<UnlistenFn> {
  return listen<InstallProgress>("dolphin-install-progress", (event) => handler(event.payload));
}

/// An installed emulator with a newer official release.
export interface EmulatorUpdate {
  console: Console;
  name: string;
  installed: string;
  newest: string;
}

export function emulatorUpdates(): Promise<EmulatorUpdate[]> {
  return invoke("emulator_updates");
}
