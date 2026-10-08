import "./styles/tokens.css";
import "./styles/portal.css";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  cancelCommunity,
  closePortalMenu,
  figureCharacters,
  figurePictures,
  figures as listFigures,
  onCommunityProgress,
  onPortalMenu,
  padsHeld,
  portalClear,
  portalCreate,
  portalFigures,
  portalLoad,
  portalGame,
  portalMadeFigures,
  portalMenuFamily,
  refreshCommunity,
  villains as listVillains,
  type BattleClass,
  type Casing,
  type Figure,
  type FigureClass,
  type FigureElement,
  type FigureKind,
  type MadeFigures,
  type Movement,
  type Offer,
  type PadFamily,
  type SkylandersGame,
  type Terrain,
  type Villain,
} from "./api";
import { nameOf } from "./components/padNames";
import adventureIcon from "./icons/adventures.svg";
import itemIcon from "./icons/items.svg";
import swapperIcon from "./icons/swappers.svg";

/// The Skylanders menu, drawn by Omoio over the running game and used with
/// the pad alone. The shoulder buttons go through the tabs, in the order the
/// game wants them: Saved, traps and the villains they hold, then one tab to
/// each element, then swappers, items such as the treasure chest and the
/// swords, and adventure packs. SuperChargers puts its garage of vehicles
/// next to Saved instead, and the older pieces last. Imaginators puts its
/// Creation Crystals and its Senseis there, the Senseis by battle class. The
/// d-pad or left stick moves, the bottom face button puts a figure on the
/// portal, the left one takes it off, the right one closes, and in
/// SuperChargers the top one puts a SuperCharger and its vehicle on
/// together. Mouse and keyboard work as well.
///
/// A character is made by the emulator's own figure maker the first time it
/// is chosen, and saved. After that the saved figure goes on, so it keeps
/// what it has earned, and a vehicle the mods bought for it.

/// How many tiles sit side by side, which is also how far up or down moves.
const COLUMNS = 5;

/// A direction held down keeps moving after a pause, as on a console.
const REPEAT_AFTER = 380;
const REPEAT_EVERY = 140;

type Move = "up" | "down" | "left" | "right";

const MOVES: Record<string, Move> = {
  Up: "up",
  "LS Y+": "up",
  Down: "down",
  "LS Y-": "down",
  Left: "left",
  "LS X-": "left",
  Right: "right",
  "LS X+": "right",
};

const ELEMENTS: [FigureElement, string][] = [
  ["air", "Air"],
  ["earth", "Earth"],
  ["fire", "Fire"],
  ["water", "Water"],
  ["life", "Life"],
  ["undead", "Undead"],
  ["magic", "Magic"],
  ["tech", "Tech"],
  ["light", "Light"],
  ["dark", "Dark"],
];

/// Kaos is an element of his own in Imaginators, held by the Kaos Sensei
/// alone, so he has no tab of it: he is under Senseis.
const ELEMENT_NAMES = new Map<FigureElement, string>([...ELEMENTS, ["kaos", "Kaos"]]);

const KINDS: [FigureKind, string][] = [
  ["item", "Items"],
  ["trap", "Traps"],
  ["adventure", "Adventure packs"],
  ["vehicle", "Vehicles"],
  ["trophy", "Trophies"],
];

/// What a figure without an element is called under its name.
const KIND_NAMES: Partial<Record<FigureKind, string>> = {
  item: "Item",
  trap: "Trap",
  adventure: "Adventure pack",
  vehicle: "Vehicle",
  trophy: "Trophy",
  crystal: "Creation Crystal",
};

/// Omoio's own icons for the tabs without an element and for the figures of
/// those kinds, drawn in the style of the game's element symbols: shapes,
/// painted in the kind's colour as the symbols are in the element's.
const ICONS = { item: itemIcon, adventure: adventureIcon, swapper: swapperIcon };

type Icon = keyof typeof ICONS;

/// SuperChargers' three terrains, Land first, since the game's own path
/// needs only a Land vehicle (Game Informer and the Skylanders wiki's
/// "Vehicles", read 7 October 2026). A vehicle goes on one, and a trophy is
/// for its races.
const TERRAINS: [Terrain, string][] = [
  ["land", "Land"],
  ["sea", "Sea"],
  ["sky", "Sky"],
];

const TERRAIN_NAMES = new Map(TERRAINS);

const MOVEMENT_NAMES: Record<Movement, string> = {
  bounce: "Bounce",
  climb: "Climb",
  dig: "Dig",
  rocket: "Rocket",
  sneak: "Sneak",
  speed: "Speed",
  spin: "Spin",
  teleport: "Teleport",
};

/// Omoio's own drawing for each element, for the kinds that have no icon,
/// and for SuperChargers' terrains: a tyre, waves and a plane. Used until
/// Omoio has read the game's own symbols out of the game, and for an element
/// that game doesn't have. A child who can't read yet goes by the shape and
/// colour, which the games use too.
const MARKS: Record<string, string> = {
  air: `<path d="M3 8h11a3 3 0 1 0-3-3M3 12h15a3 3 0 1 1-3 3M3 16h8" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>`,
  earth: `<path d="M2 20 9 8l4 6 3-4 6 10z" fill="currentColor"/>`,
  fire: `<path d="M12 2c1 4 6 6.5 6 12a6 6 0 0 1-12 0c0-2.6 1.3-4 2.5-5 0 2 .8 3.3 2 3.8C10 9 10.8 5 12 2z" fill="currentColor"/>`,
  water: `<path d="M12 2.5c3.5 5 6.5 8.4 6.5 12a6.5 6.5 0 0 1-13 0c0-3.6 3-7 6.5-12z" fill="currentColor"/>`,
  life: `<path d="M4 20C4 11 9 4 20 4c0 11-7 16-16 16zM4 20l9-9" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/>`,
  undead: `<path fill-rule="evenodd" d="M12 2.5a8 8 0 0 0-8 8c0 2.8 1.3 4.6 3.2 5.7V20a1 1 0 0 0 1 1h7.6a1 1 0 0 0 1-1v-3.8c1.9-1.1 3.2-2.9 3.2-5.7a8 8 0 0 0-8-8zM9 9.5a1.8 1.8 0 1 0 0 3.6 1.8 1.8 0 0 0 0-3.6zm6 0a1.8 1.8 0 1 0 0 3.6 1.8 1.8 0 0 0 0-3.6z" fill="currentColor"/>`,
  magic: `<path d="m12 2 2.6 6.6 7.1.5-5.5 4.6 1.8 6.9L12 16.8l-6 3.8 1.8-6.9-5.5-4.6 7.1-.5z" fill="currentColor"/>`,
  tech: `<circle cx="12" cy="12" r="4.5" fill="none" stroke="currentColor" stroke-width="2.5"/><path d="M12 2v4M12 18v4M2 12h4M18 12h4M4.9 4.9l2.8 2.8M16.3 16.3l2.8 2.8M4.9 19.1l2.8-2.8M16.3 7.7l2.8-2.8" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/>`,
  light: `<path d="M12 6.2a5.8 5.8 0 1 1 0 11.6a5.8 5.8 0 1 1 0-11.6ZM10.05 5.69L12.00 0.70L13.95 5.69ZM15.08 6.16L19.99 4.01L17.84 8.92ZM18.31 10.05L23.30 12.00L18.31 13.95ZM17.84 15.08L19.99 19.99L15.08 17.84ZM13.95 18.31L12.00 23.30L10.05 18.31ZM8.92 17.84L4.01 19.99L6.16 15.08ZM5.69 13.95L0.70 12.00L5.69 10.05ZM6.16 8.92L4.01 4.01L8.92 6.16Z" fill="currentColor"/>`,
  dark: `<path d="M14.6 2.4A10 10 0 1 0 21.8 16.6A8.3 8.3 0 0 1 14.6 2.4Z" fill="currentColor"/>`,
  trap: `<path d="M12 2.5 17.5 9 12 17 6.5 9z" fill="currentColor"/><path d="M8 19.5h8M12 17v2.5" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>`,
  villain: `<path d="M12 2.5 20.5 7.3v9.4L12 21.5 3.5 16.7V7.3z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"/>`,
  vehicle: `<circle cx="12" cy="12" r="8.6" fill="none" stroke="currentColor" stroke-width="2.4"/><circle cx="12" cy="12" r="2.4" fill="currentColor"/><path d="m3.8 10.8 5.9.8m10.5-.8-5.9.8M12 14.4v6" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/>`,
  land: `<circle cx="12" cy="12" r="7.64" fill="none" stroke="currentColor" stroke-width="4.4" stroke-dasharray="3.1 0.9"/><circle cx="12" cy="12" r="2.2" fill="currentColor"/>`,
  sea: `<path d="M2.5 9.5q2.4-3 4.8 0t4.7 0 4.8 0 4.7 0M2.5 15.5q2.4-3 4.8 0t4.7 0 4.8 0 4.7 0" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>`,
  sky: `<path d="M12 2.2c.9 0 1.6 1.1 1.6 2.6v4.6l7.6 4.6v2.2l-7.6-2.4v4.1l2.3 1.8V22L12 21l-3.9 1v-2.3l2.3-1.8v-4.1l-7.6 2.4v-2.2l7.6-4.6V4.8c0-1.5.7-2.6 1.6-2.6z" fill="currentColor"/>`,
  supercharged: `<path d="M13.6 2 4.8 13.4h6.1L9.6 22l9.6-12.2h-6.3z" fill="currentColor"/>`,
  trophy: `<path d="M7 3h10v5a5 5 0 0 1-10 0zM7 5H4a3 3 0 0 0 3.3 4M17 5h3a3 3 0 0 1-3.3 4M12 13v4M9 21h6" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/>`,
  figure: `<circle cx="12" cy="8" r="4" fill="currentColor"/><path d="M4 21a8 8 0 0 1 16 0z" fill="currentColor"/>`,
  // Imaginators: Kaos's own element as a spiral, a Creation Crystal as a
  // cut gem, and a Sensei as the knot of a belt.
  kaos: `<path d="M10.5 12a1.5 1.5 0 0 1 3 0 3 3 0 0 1-6 0 4.5 4.5 0 0 1 9 0 6 6 0 0 1-12 0 7.5 7.5 0 0 1 15 0" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>`,
  crystal: `<path d="M7 3h10l4.5 6L12 21.5 2.5 9z" fill="currentColor" opacity=".35"/><path d="M7 3h10l4.5 6L12 21.5 2.5 9zM2.5 9h19M9.2 3 7.8 9 12 21.5 16.2 9 14.8 3" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/>`,
  sensei: `<path d="M2.5 8.5h19v4.5h-19z" fill="currentColor"/><path d="m10 12-3.2 9M14 12l3.2 9" stroke="currentColor" stroke-width="2.8" stroke-linecap="round"/><rect x="8.8" y="6.8" width="6.4" height="8" rx="1.6" fill="currentColor"/>`,
};

/// Imaginators' battle classes in the game's order, which it numbers 1 to
/// 11 on an Imaginator (NefariousTechSupport's Runes notes on the figure
/// format, read 7 October 2026), each with Omoio's own drawing until the
/// game's symbol is read: a sword, a bow, a sight, a ninja's mask, a glove,
/// a hammer, a staff, crossed sabres, a shield, a launcher, and Kaos's
/// spiral for the class only he has.
const BATTLE_CLASSES: Record<BattleClass, { words: string; shape: string }> = {
  knight: {
    words: "Knight",
    shape: `<path d="M12 1.8 14.2 4.5V15H9.8V4.5z" fill="currentColor"/><path d="M6.5 15.4h11M12 15.4v4.4" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/><circle cx="12" cy="21.2" r="1.7" fill="currentColor"/>`,
  },
  bowslinger: {
    words: "Bowslinger",
    shape: `<path d="M7 2.5c6.5 3.5 6.5 15.5 0 19" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/><path d="M7 2.5v19" stroke="currentColor" stroke-width="1.2"/><path d="M3.5 12h14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><path d="m16 8.2 5.5 3.8-5.5 3.8z" fill="currentColor"/>`,
  },
  quickshot: {
    words: "Quickshot",
    shape: `<circle cx="12" cy="12" r="7" fill="none" stroke="currentColor" stroke-width="2.2"/><path d="M12 2v5.5M12 16.5V22M2 12h5.5M16.5 12H22" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/><circle cx="12" cy="12" r="1.8" fill="currentColor"/>`,
  },
  ninja: {
    words: "Ninja",
    shape: `<path fill-rule="evenodd" d="M12 3a8 8 0 0 0-8 8v3a7 7 0 0 0 7 7h2a7 7 0 0 0 7-7v-3a8 8 0 0 0-8-8zM7.2 10.2h9.6a1.6 1.6 0 0 1 0 3.2H7.2a1.6 1.6 0 0 1 0-3.2z" fill="currentColor"/><path d="M19.6 8.6 22.5 6M20 11l3 .6" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>`,
  },
  brawler: {
    words: "Brawler",
    shape: `<path fill-rule="evenodd" d="M8.5 2.5h6A6.5 6.5 0 0 1 21 9v3.5a5.5 5.5 0 0 1-5.5 5.5H9.5A5.5 5.5 0 0 1 4 12.5V7a4.5 4.5 0 0 1 4.5-4.5zM4 10.6h7.4a1 1 0 0 1 0 2H4z" fill="currentColor"/><rect x="7" y="19" width="10" height="3.2" rx="1" fill="currentColor"/><path d="M9.5 3v5.6M13.5 3v5.6" stroke="currentColor" stroke-width="1" opacity=".35"/>`,
  },
  smasher: {
    words: "Smasher",
    shape: `<rect x="3.5" y="3" width="15" height="7" rx="1.5" fill="currentColor"/><path d="M11 10v11.2" stroke="currentColor" stroke-width="2.8" stroke-linecap="round"/><path d="M18.5 4.5h2v4h-2" fill="currentColor"/>`,
  },
  sorcerer: {
    words: "Sorcerer",
    shape: `<path d="M5.5 21.5 14 9" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/><circle cx="16" cy="6.5" r="3.6" fill="currentColor"/><path d="M5 4.5 6 7l2.5 1L6 9 5 11.5 4 9 1.5 8 4 7z" fill="currentColor"/>`,
  },
  swashbuckler: {
    words: "Swashbuckler",
    shape: `<path d="M4 3c5.5 3 10 9 13.8 15.5M20 3C14.5 6 10 12 6.2 18.5" fill="none" stroke="currentColor" stroke-width="2.3" stroke-linecap="round"/><path d="m15.2 19.6 4.6-2.4M8.8 19.6 4.2 17.2" stroke="currentColor" stroke-width="2.3" stroke-linecap="round"/>`,
  },
  sentinel: {
    words: "Sentinel",
    shape: `<path fill-rule="evenodd" d="M12 2.2 20.5 5.4v6.2c0 5.2-3.6 8.9-8.5 10.3-4.9-1.4-8.5-5.1-8.5-10.3V5.4zM12 7.4l4.2 1.6v2.9c0 2.5-1.7 4.4-4.2 5.3-2.5-.9-4.2-2.8-4.2-5.3V9z" fill="currentColor"/>`,
  },
  bazooker: {
    words: "Bazooker",
    shape: `<path d="M2.8 15.2 16.6 6.4l2.4 3.7-13.8 8.8z" fill="currentColor"/><path d="m8.4 16.4 1.8 4.6" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/><path d="M20.4 4.4 22 2.8M21.6 7.6h1.9M18.6 3.2V1.3" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>`,
  },
  kaos: {
    words: "Kaos",
    shape: `<path d="M10.5 12a1.5 1.5 0 0 1 3 0 3 3 0 0 1-6 0 4.5 4.5 0 0 1 9 0 6 6 0 0 1-12 0 7.5 7.5 0 0 1 15 0" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>`,
  },
};

const BATTLE_ORDER = Object.keys(BATTLE_CLASSES) as BattleClass[];

/// The casings of the Creation Crystals, by the names collectors give them,
/// since Activision named none (Texthead1's Skylander-IDs list, read 7
/// October 2026).
const CASINGS: Record<Casing, string> = {
  angel: "Angel",
  pyramid: "Pyramid",
  lantern: "Lantern",
  rune: "Rune",
  reactor: "Reactor",
  acorn: "Acorn",
  armor: "Armor",
  fanged: "Fanged",
  claw: "Claw",
  rocket: "Rocket",
};

/// How many saved crystals sit side by side in the Imaginators tab, next to
/// the card of the one picked.
const SHELF = 4;

/// The kinds the games' checklists mark apart, with their names, where they
/// sort in an element, and Omoio's own mark where it has one: Imaginators'
/// Senseis first, with a belt, and the villains among them with horns, then
/// SuperChargers, with a bolt, then Giants and Trap Masters, with a crown
/// with a Traptanium crystal at its heart for a Trap Master, and Minis last,
/// with a small figure in a ring.
const CLASSES: Record<FigureClass, { words: string; order: number; shape: string | null }> = {
  sensei: {
    words: "Sensei",
    order: -2,
    shape: MARKS.sensei,
  },
  villain_sensei: {
    words: "Villain Sensei",
    order: -2,
    shape: `<path d="M5 3c-.6 4 .6 6.8 3.4 8.4M19 3c.6 4-.6 6.8-3.4 8.4" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/><path fill-rule="evenodd" d="M12 9a6.8 6.8 0 0 1 6.8 6.8c0 3.4-3 5.7-6.8 5.7s-6.8-2.3-6.8-5.7A6.8 6.8 0 0 1 12 9zM8.4 14.2l2.6 1.4-2.6 1zM15.6 14.2 13 15.6l2.6 1z" fill="currentColor"/>`,
  },
  supercharger: {
    words: "SuperCharger",
    order: -1,
    shape: MARKS.supercharged,
  },
  giant: {
    words: "Giant",
    order: 0,
    shape: null,
  },
  trap_master: {
    words: "Trap Master",
    order: 0,
    shape: `<path d="M3 18.5h18l-1.4-10-4.4 4.2L12 4.5l-3.2 8.2-4.4-4.2z" fill="currentColor" opacity=".55"/><path d="M12 4.5 8.8 12.7l3.2 5.8 3.2-5.8z" fill="currentColor"/><rect x="3" y="19.5" width="18" height="2.2" rx="1.1" fill="currentColor"/>`,
  },
  mini: {
    words: "Mini",
    order: 2,
    shape: `<circle cx="12" cy="12" r="9.5" fill="none" stroke="currentColor" stroke-width="2"/><circle cx="12" cy="9.6" r="2.6" fill="currentColor"/><path d="M7.9 17.2a4.1 4.1 0 0 1 8.2 0z" fill="currentColor"/>`,
  },
};

/// The corner of a tile says when its figure is on the portal, saved, or
/// picked as a swapper's top, with a shape as well as words.
const BADGES = {
  on: ["On the portal", `<path d="m5 12.5 4.5 4.5L19 7.5" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`],
  saved: ["Saved", `<path d="M6 3h12v18l-6-4.5L6 21z" fill="currentColor"/>`],
  picked: ["Top picked", `<path d="M12 3.5 20 13h-5v7.5H9V13H4z" fill="currentColor"/>`],
  new: ["New", `<path d="M12 2.5 14 10l7.5 2-7.5 2-2 7.5-2-7.5L2.5 12 10 10z" fill="currentColor"/>`],
} as const;

/// One tile: a saved figure, a character the emulator can make, or both when
/// the character has been made before.
interface Entry {
  name: string;
  element: FigureElement | null;
  kind: FigureKind | null;
  figure?: Figure;
  offer?: Offer;
  /// A Swap Force swapper: both halves of one character.
  swap?: { top: Offer; bottom: Offer };
  /// A blank Creation Crystal: always made new, never a saved one, since a
  /// crystal holds one Imaginator for good.
  fresh?: true;
}

interface Tab {
  label: string;
  element?: FigureElement;
  icon?: Icon;
  /// One of Omoio's own drawings, for a tab with neither element nor icon.
  mark?: keyof typeof MARKS;
  entries: Entry[];
  /// The villains tab, laid out as a collector's tray rather than a grid.
  tray?: true;
  /// SuperChargers' vehicles, laid out as a garage with a column to each
  /// terrain.
  garage?: true;
  /// Imaginators' Creation Crystals: the user's own, then a row of blank
  /// ones to each element.
  crystals?: true;
  /// Imaginators' Senseis, a column to each battle class.
  senseis?: true;
}

/// The tray's columns, one to each element, as the villains fit the traps,
/// and one for Kaos, who has a trap of his own.
const TRAY: [FigureElement | null, string][] = [...ELEMENTS, [null, "Kaos"]];

const root = document.getElementById("portal")!;

/// The game running, which decides the order of the tabs.
let game: SkylandersGame | null = null;
let family: PadFamily = "generic";
let shown = false;
let asking = false;
let onPortal: string[] = [];
/// The file Omoio put in each slot, with the name the portal gave it then.
/// A figure on the portal is known by its file: two files can carry one
/// name, and the emulator names a file the user brought by its character.
let slotFiles: ({ path: string; name: string } | null)[] = [];
let mine: Figure[] = [];
let offers: Offer[] = [];
/// Trap Team's villains, which the user has caught and which trap holds each.
let villainList: Villain[] = [];
/// The villains caught when the menu was last open, so one caught since
/// stands out once. `null` until the menu has been open.
let caughtBefore: Set<number> | null = null;
let caughtNew = new Set<number>();
let tabs: Tab[] = [];
let tab = 0;
/// The selection: a tile in the grid, or a figure in the row of those on
/// the portal.
let zone: "grid" | "portal" = "grid";
let at = 0;
let chip = 0;
/// Whether the menu has been filled once. Until then the page keeps the
/// "Loading the portal" it was written with.
let ready = false;
/// The top half picked for a swapper, while waiting for its bottom.
let pickedTop: { name: string; top: Offer } | null = null;
/// Whether the running game takes the figures the emulator makes: in
/// Imaginators, whether Cemu's Signature Patch is on. Asked each time the
/// menu opens.
let made: MadeFigures | null = null;
/// Whether Cemu's packs are being downloaded from the menu, which East
/// stops.
let gettingPacks = false;
/// The figures' pictures Omoio has read out of this game, by name, for each
/// figure id the picture of its lowest variant, and the badges this game
/// lacks that another of the user's games had.
let pictures: {
  folder: string;
  names: Set<string>;
  firstOf: Map<number, string>;
  elsewhere: Record<string, string>;
} | null = null;

/// A figure's picture, or its plain version's when its variant has none of
/// its own, or failing that any version's: Trap Team names its pictures by
/// the game's own variants, which Cemu's list doesn't give its Trap Masters.
/// `null` when Omoio has no picture of it.
function pictureOf(id: number | null | undefined, variant: number | null | undefined): string | null {
  if (id == null) return null;
  const four = (value: number) => value.toString(16).padStart(4, "0");
  return fileOf(`${id}-${four(variant ?? 0)}`) ?? fileOf(`${id}-0000`) ?? fileOf(pictures?.firstOf.get(id) ?? "");
}

/// For each figure id, the name of its picture with the lowest variant.
function firstPictures(names: string[]): Map<number, string> {
  const first = new Map<number, string>();
  for (const name of [...names].sort()) {
    const match = /^(\d+)-[0-9a-f]{4}$/.exec(name);
    if (match && !first.has(Number(match[1]))) first.set(Number(match[1]), name);
  }
  return first;
}

/// One of the pictures Omoio read out of the game, by name, when it has it,
/// or a badge from another of the user's games.
function fileOf(name: string): string | null {
  if (!pictures) return null;
  if (pictures.names.has(name)) return convertFileSrc(`${pictures.folder}\\${name}.png`);
  const elsewhere = pictures.elsewhere[name];
  return elsewhere ? convertFileSrc(elsewhere) : null;
}

function filled(): { name: string; slot: number }[] {
  return onPortal.map((name, slot) => ({ name, slot })).filter((figure) => figure.name);
}

/// The figures on the portal in the order the row shows them: by slot,
/// except that a SuperCharged vehicle sits right after its driver, so the
/// two show together.
function placed(): { name: string; slot: number }[] {
  const on = filled();
  for (const pair of superCharged()) {
    const vehicle = on.findIndex((figure) => figure.slot === pair.vehicle);
    const [moved] = on.splice(vehicle, 1);
    on.splice(on.findIndex((figure) => figure.slot === pair.driver) + 1, 0, moved);
  }
  return on;
}

/// Takes in the portal as just read. A slot that has emptied, or holds
/// another figure than the one Omoio put there, no longer holds that file.
function readPortal(names: string[]) {
  onPortal = names;
  slotFiles = names.map((name, slot) => (name && slotFiles[slot]?.name === name ? slotFiles[slot] : null));
}

/// The slot Omoio put the file at `path` in, or -1.
function slotOfFile(path: string | undefined): number {
  return path ? slotFiles.findIndex((file) => file?.path === path) : -1;
}

/// The name the portal shows for a tile's figure, which is the emulator's
/// name for the character rather than the file's.
function portalName(entry: Entry): string {
  const figure = entry.figure;
  const madeAs = figure && offers.find((offer) => offer.id === figure.id && offer.variant === figure.variant);
  return entry.offer?.name ?? madeAs?.name ?? entry.name;
}

/// A swapper half's character, without the "(Top)" or "(Bottom)" the
/// emulator's list adds.
function baseName(name: string): string {
  return name.replace(/\s*\((Top|Bottom)\)\s*$/i, "").trim();
}

function savedFor(offer: Offer): Figure | undefined {
  return mine.find((figure) => figure.id === offer.id && figure.variant === offer.variant);
}

/// The slot holding what a tile stands for, or -1: its saved figure's file
/// when Omoio put that on, or a figure the portal names as it. A character
/// is on whichever file brought it; a saved file goes by its name only in a
/// slot whose file Omoio doesn't know.
function slotOf(entry: Entry): number {
  // A blank crystal is never on: one on the portal of the same casing is
  // another crystal, holding its own Imaginator.
  if (entry.fresh) return -1;
  const byFile = slotOfFile(entry.figure?.path);
  if (byFile >= 0) return byFile;
  const name = portalName(entry);
  return onPortal.findIndex((held, slot) => held === name && (Boolean(entry.offer) || !slotFiles[slot]));
}

function isOn(entry: Entry): boolean {
  if (entry.swap) return [entry.swap.top, entry.swap.bottom].some((half) => onPortal.includes(half.name));
  return slotOf(entry) >= 0;
}

/// Whether the figure in `slot` is of a kind, such as a trap or a vehicle:
/// by its file when Omoio put it there, otherwise by the name the portal
/// gives it.
function holds(slot: number, kind: FigureKind): boolean {
  const file = slotFiles[slot];
  const figure = file ? mine.find((each) => each.path === file.path) : undefined;
  if (figure?.kind) return figure.kind === kind;
  return offers.some((offer) => offer.kind === kind && offer.name === onPortal[slot]);
}

/// The character in `slot`, the same way: by its file when Omoio put it
/// there and knows it, otherwise by the name the portal gives it.
function characterIn(slot: number): { id: number; kind: FigureKind; partner: number | null } | null {
  const file = slotFiles[slot];
  const figure = file ? mine.find((each) => each.path === file.path) : undefined;
  if (figure?.id != null && figure.kind) return { id: figure.id, kind: figure.kind, partner: figure.partner };
  const offer = offers.find((each) => each.name === onPortal[slot]);
  return offer ? { id: offer.id, kind: offer.kind, partner: offer.partner } : null;
}

/// The slot of a figure of the character `id`, whichever variant, or -1.
function slotWith(id: number): number {
  return filled().find((figure) => characterIn(figure.slot)?.id === id)?.slot ?? -1;
}

function vehiclesOn(): { name: string; slot: number }[] {
  return filled().filter((figure) => holds(figure.slot, "vehicle"));
}

/// Whether a Skylander is on the portal to drive a vehicle. Any of them can.
function driverOn(): boolean {
  return filled().some((figure) => holds(figure.slot, "character"));
}

/// Each vehicle on the portal with its own SuperCharger on too, which the
/// game calls SuperCharged, by slot.
function superCharged(): { driver: number; vehicle: number }[] {
  return vehiclesOn().flatMap((vehicle) => {
    const partner = characterIn(vehicle.slot)?.partner;
    const driver = partner == null ? -1 : slotWith(partner);
    return driver >= 0 ? [{ driver, vehicle: vehicle.slot }] : [];
  });
}

function terrainOf(entry: Entry): Terrain | null {
  return entry.offer?.terrain ?? entry.figure?.terrain ?? null;
}

function partnerOf(entry: Entry): number | null {
  return entry.offer?.partner ?? entry.figure?.partner ?? null;
}

/// The emulator's name for a character, by id: its plain version's.
function nameOfId(id: number): string | null {
  return offers.filter((offer) => offer.id === id).sort((a, b) => a.variant - b.variant)[0]?.name ?? null;
}

/// The marked kind a tile's figure is, if any.
function classOf(entry: Entry): FigureClass | null {
  return entry.offer?.class ?? entry.figure?.class ?? null;
}

/// A kind's mark: the game's own badge when Omoio has read it out of the
/// game, Omoio's own drawing when not, or nothing.
function classMark(kind: FigureClass): HTMLElement | null {
  const badge = fileOf(`class-${kind}`);
  if (badge) return image(badge);
  const shape = CLASSES[kind].shape;
  if (!shape) return null;
  const drawn = node("span", "portal-class-shape");
  drawn.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${shape}</svg>`;
  return drawn;
}

/// Where a tile sorts among its element: Giants and Trap Masters first and
/// Minis last, as on the game's own checklist. SuperChargers come before
/// them all, as the Skylanders their game was made for.
function rank(entry: Entry): number {
  const kind = classOf(entry);
  return kind ? CLASSES[kind].order : 1;
}

/// What a tile stands for, to find it again once the lists are made anew.
function keyOf(entry: Entry | undefined): string | null {
  if (entry?.fresh && entry.offer) return `new-${entry.offer.id}-${entry.offer.variant}`;
  if (entry?.offer) return `${entry.offer.id}-${entry.offer.variant}`;
  return entry?.figure ? `file-${entry.figure.path}` : null;
}

/// A Sensei's battle class.
function battleOf(entry: Entry | undefined): BattleClass | null {
  return entry?.offer?.battle_class ?? entry?.figure?.battle_class ?? null;
}

function casingOf(entry: Entry | undefined): Casing | null {
  return entry?.offer?.casing ?? entry?.figure?.casing ?? null;
}

/// Whether a tile is one of Imaginators' own figures, a Sensei or a
/// Creation Crystal, which the game checks for a factory signature.
function signed(entry: Entry): boolean {
  const kind = classOf(entry);
  return (entry.offer?.kind ?? entry.figure?.kind) === "crystal" || kind === "sensei" || kind === "villain_sensei";
}

/// Whether putting a tile on makes a new Sensei or crystal the game would
/// turn away now, since Cemu's Signature Patch isn't on in it.
function turnedAway(entry: Entry | undefined): boolean {
  const check = made?.check ?? "none";
  return Boolean(entry && signed(entry) && !entry.figure && entry.offer) && check !== "none" && check !== "passed";
}

function buildTabs() {
  const kept = tabs[tab]?.label;
  // The garage puts saved vehicles first, so a vehicle made moves up its
  // column, and a crystal made goes in front of the blank ones. The
  // selection stays on what it was on.
  const follows = tabs[tab]?.garage || tabs[tab]?.crystals || tabs[tab]?.senseis;
  const picked = zone === "grid" && follows ? keyOf(tabs[tab]?.entries[at]) : null;
  const byName = (a: Entry, b: Entry) => rank(a) - rank(b) || a.name.localeCompare(b.name);
  const entry = (offer: Offer): Entry => ({
    name: offer.name,
    element: offer.element,
    kind: offer.kind,
    offer,
    figure: savedFor(offer),
  });
  const characters = offers.filter((offer) => offer.kind === "character" && !offer.half);
  const saved: Tab = {
    label: "Saved",
    entries: mine.map((figure) => ({ name: figure.name, element: figure.element, kind: figure.kind, figure })),
  };
  const traps = offers.filter((offer) => offer.kind === "trap").map(entry).sort(byName);
  const trapTab: Tab = { label: "Traps", mark: "trap", entries: traps };
  const villainTab: Tab | null =
    traps.length > 0 && villainList.length > 0 ? { label: "Villains", mark: "villain", entries: [], tray: true } : null;
  const elementTabs: Tab[] = ELEMENTS.map(([element, label]) => ({
    label,
    element,
    entries: characters.filter((offer) => offer.element === element).map(entry).sort(byName),
  }));
  const otherTab: Tab = { label: "Other", entries: characters.filter((offer) => !offer.element).map(entry).sort(byName) };
  const halves = offers.filter((offer) => offer.half);
  const swappers: Entry[] = halves
    .filter((offer) => offer.half === "top")
    .flatMap((top) => {
      const bottom = halves.find((offer) => offer.half === "bottom" && baseName(offer.name) === baseName(top.name));
      return bottom ? [{ name: baseName(top.name), element: top.element, kind: top.kind, swap: { top, bottom } }] : [];
    })
    .sort(byName);
  const swapperTab: Tab = { label: "Swappers", icon: "swapper", entries: swappers };
  const kindTab = (kind: FigureKind): Tab => ({
    label: KINDS.find(([each]) => each === kind)?.[1] ?? "",
    icon: kindIcon(kind) ?? undefined,
    mark: kind === "vehicle" || kind === "trophy" ? kind : undefined,
    entries: offers.filter((offer) => offer.kind === kind).map(entry).sort(byName),
  });
  // SuperChargers' trophies in the order of the terrains, and Kaos's last.
  const trophies = kindTab("trophy");
  const races = (each: Entry) => {
    const at = TERRAINS.findIndex(([terrain]) => terrain === terrainOf(each));
    return at < 0 ? TERRAINS.length : at;
  };
  trophies.entries.sort((a, b) => races(a) - races(b));
  // Imaginators' crystals: the user's own, the one used last first, then
  // the blank ones, a row to each element.
  const crystalTab: Tab = {
    label: "Imaginators",
    mark: "crystal",
    crystals: true,
    entries: [
      ...mine
        .filter((figure) => figure.kind === "crystal")
        .map((figure) => ({ name: figure.name, element: figure.element, kind: figure.kind, figure })),
      ...ELEMENTS.flatMap(([element]) =>
        offers
          .filter((offer) => offer.kind === "crystal" && offer.element === element)
          .map((offer) => ({ name: offer.name, element, kind: offer.kind, offer, fresh: true as const }))
      ),
    ],
  };
  const byBattle = (a: Entry, b: Entry) => battleRank(a) - battleRank(b) || a.name.localeCompare(b.name);
  const senseiTab: Tab = {
    label: "Senseis",
    mark: "sensei",
    senseis: true,
    entries: offers
      .filter((offer) => offer.class === "sensei" || offer.class === "villain_sensei")
      .map(entry)
      .sort(byBattle),
  };
  // The game decides the order. SuperChargers wants a vehicle at every Land,
  // Sea and Sky gate (Game Informer, "21 Things You Need To Know", read 7
  // October 2026), so its garage sits next to Saved, and the pieces from the
  // games before, which do smaller things in it, come last. Imaginators is
  // about the Imaginators made in its Creation Crystals and the Senseis who
  // teach them, so those come next to Saved, and the older pieces last as
  // well; it catches no villains. Everywhere else traps sit next to Saved,
  // since they go on and off all through a Trap Team game, with the
  // villains they hold right after.
  const orders: Partial<Record<SkylandersGame, (Tab | null)[]>> = {
    superchargers: [garage(offers.filter((offer) => offer.kind === "vehicle").map(entry)), ...elementTabs, otherTab, swapperTab, trophies, trapTab, kindTab("item"), kindTab("adventure")],
    imaginators: [crystalTab, senseiTab, ...elementTabs, otherTab, swapperTab, trapTab, kindTab("item"), kindTab("adventure"), kindTab("vehicle"), trophies],
  };
  const order: (Tab | null)[] = (game && orders[game]) || [
    trapTab,
    villainTab,
    ...elementTabs,
    otherTab,
    swapperTab,
    kindTab("item"),
    kindTab("adventure"),
    kindTab("vehicle"),
    kindTab("trophy"),
  ];
  tabs = [saved, ...order.filter((each): each is Tab => Boolean(each && (each.tray || each.entries.length > 0)))];
  const again = tabs.findIndex((each) => each.label === kept);
  tab = again >= 0 ? again : Math.min(tab, tabs.length - 1);
  const found = again >= 0 && picked ? tabs[tab].entries.findIndex((entry) => keyOf(entry) === picked) : -1;
  if (found >= 0) at = found;
}

/// The vehicles tab: a column to each terrain, and in each, the vehicles
/// with a saved figure first, each followed by its variants. The plain one
/// leads its variants, unless only a variant is saved.
function garage(vehicles: Entry[]): Tab {
  const plain = (group: Entry[]) => group.reduce((first, each) => (variantOf(each) < variantOf(first) ? each : first));
  const kept = (group: Entry[]) => group.some((each) => each.figure);
  const byId = new Map<number, Entry[]>();
  for (const each of vehicles) byId.set(each.offer?.id ?? -1, [...(byId.get(each.offer?.id ?? -1) ?? []), each]);
  const groups = [...byId.values()].map((group) =>
    group.sort((a, b) => Number(Boolean(b.figure)) - Number(Boolean(a.figure)) || variantOf(a) - variantOf(b))
  );
  groups.sort((a, b) => Number(kept(b)) - Number(kept(a)) || plain(a).name.localeCompare(plain(b).name));
  const entries = garageColumns(groups.flat()).flatMap((column) => column.entries);
  return { label: "Vehicles", mark: "vehicle", entries, garage: true };
}

function variantOf(entry: Entry): number {
  return entry.offer?.variant ?? entry.figure?.variant ?? 0;
}

/// The garage's columns, one to each terrain in the game's order, keeping
/// the order the vehicles come in. A vehicle Omoio has no terrain for gets
/// a column of its own rather than going missing.
function garageColumns(entries: Entry[]): { terrain: Terrain | null; label: string; entries: Entry[] }[] {
  const columns = [...TERRAINS, [null, "Other"] as const].map(([terrain, label]) => ({
    terrain,
    label,
    entries: entries.filter((entry) => terrainOf(entry) === terrain),
  }));
  return columns.filter((column) => column.entries.length > 0);
}

/// Where a Sensei's class comes in the game's order, one without a class
/// last.
function battleRank(entry: Entry): number {
  const battle = battleOf(entry);
  return battle ? BATTLE_ORDER.indexOf(battle) : BATTLE_ORDER.length;
}

/// The Imaginators tab's rows, as moving through it counts them: the saved
/// crystals `SHELF` to a row, then a row of blank ones to each element, in
/// the order the tab lists them.
function crystalRows(entries: Entry[]): { element: FigureElement | null; entries: Entry[] }[] {
  const saved = entries.filter((each) => !each.fresh);
  const rows: { element: FigureElement | null; entries: Entry[] }[] = [];
  for (let first = 0; first < saved.length; first += SHELF) rows.push({ element: null, entries: saved.slice(first, first + SHELF) });
  for (const [element] of ELEMENTS) {
    const blank = entries.filter((each) => each.fresh && each.element === element);
    if (blank.length > 0) rows.push({ element, entries: blank });
  }
  return rows;
}

/// The Senseis tab's columns, one to each battle class in the game's order,
/// so finding a Knight takes one look. A Sensei Omoio has no class for gets
/// a column of its own rather than going missing.
function senseiColumns(entries: Entry[]): { battle: BattleClass | null; entries: Entry[] }[] {
  const columns: { battle: BattleClass | null; entries: Entry[] }[] = BATTLE_ORDER.map((battle) => ({
    battle,
    entries: entries.filter((each) => battleOf(each) === battle),
  }));
  columns.push({ battle: null, entries: entries.filter((each) => !battleOf(each)) });
  return columns.filter((column) => column.entries.length > 0);
}

/// The tray's columns with the villains in each, in the game's order.
function trayColumns(): { element: FigureElement | null; label: string; villains: Villain[] }[] {
  return TRAY.map(([element, label]) => ({ element, label, villains: villainList.filter((v) => v.element === element) }));
}

/// Every villain in the tray, column by column: what the selection counts in.
function trayOrder(): Villain[] {
  return trayColumns().flatMap((column) => column.villains);
}

/// How many things the selection can be on in the open tab.
function count(): number {
  const current = tabs[tab];
  return current?.tray ? trayOrder().length : (current?.entries.length ?? 0);
}

/// Keeps the selection on something that is there. An empty portal row
/// hands it back to the grid.
function settle() {
  at = Math.max(0, Math.min(at, count() - 1));
  const on = placed().length;
  if (on === 0) zone = "grid";
  chip = Math.max(0, Math.min(chip, on - 1));
}

function node<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const made = document.createElement(tag);
  made.className = className;
  if (text !== undefined) made.textContent = text;
  return made;
}

function drawing(element: FigureElement | null, kind: FigureKind | null): string {
  const shape = element ?? (kind && kind in MARKS ? kind : "figure");
  return `<svg viewBox="0 0 24 24" aria-hidden="true">${MARKS[shape]}</svg>`;
}

/// The kinds of figure with an icon of their own.
function kindIcon(kind: FigureKind | null): Icon | null {
  return kind === "item" || kind === "adventure" ? kind : null;
}

/// The colour a figure is shown in: its element's, or its kind's icon's. A
/// trophy has the colour of the races it is for, and the Kaos Trophy Kaos's.
function tintOf(element: FigureElement | null, kind: FigureKind | null, terrain: Terrain | null = null): string {
  if (!element && kind === "trophy") return `tint-${terrain ?? "kaos"}`;
  return `tint-${element ?? kindIcon(kind) ?? "none"}`;
}

/// Omoio's drawing of a terrain, in the terrain's colour.
function terrainMark(terrain: Terrain): string {
  return `<svg class="portal-terrain-mark tint-${terrain}" viewBox="0 0 24 24" aria-hidden="true">${MARKS[terrain]}</svg>`;
}

/// A terrain's own symbol from the game when Omoio has read it, Omoio's
/// drawing when not.
function terrainShape(into: HTMLElement, terrain: Terrain) {
  const source = fileOf(`terrain-${terrain}`);
  if (source) into.appendChild(painted(source, `tint-${terrain}`));
  else into.insertAdjacentHTML("beforeend", terrainMark(terrain));
}

/// A terrain with its name, in its colour: "Land".
function terrainPart(terrain: Terrain, words = TERRAIN_NAMES.get(terrain) ?? ""): HTMLElement {
  const part = node("span", `portal-terrain tint-${terrain}`);
  terrainShape(part, terrain);
  part.append(words);
  return part;
}

/// A vehicle's driver in the bottom corner of its picture, as a villain's
/// tile has its trap: the driver's own picture when Omoio has it, otherwise
/// a figure in the driver's colour. The element's own shape would repeat
/// the vehicle's, which nearly every driver shares.
function cornerFace(id: number, className: string): HTMLElement {
  const offer = offers.find((each) => each.id === id);
  const corner = node("span", `${className} ${tintOf(offer?.element ?? null, offer?.kind ?? null)}`);
  const face = pictureOf(id, 0);
  if (face) corner.appendChild(image(face));
  else corner.innerHTML = svg(MARKS.figure);
  return corner;
}

function svg(shape: string): string {
  return `<svg viewBox="0 0 24 24" aria-hidden="true">${shape}</svg>`;
}

function image(source: string): HTMLImageElement {
  const made = node("img", "");
  made.src = source;
  made.alt = "";
  made.decoding = "async";
  return made;
}

/// A shape painted in a colour through it, as the game's element symbols
/// and Omoio's own icons are.
function painted(source: string, tint: string): HTMLElement {
  const shape = node("span", `portal-symbol ${tint}`);
  const mask = `url("${source}")`;
  shape.style.maskImage = mask;
  shape.style.webkitMaskImage = mask;
  return shape;
}

/// The game's own symbol for an element, a white shape read out of the
/// game. `null` until Omoio has it.
function symbol(element: FigureElement | null): HTMLElement | null {
  const source = element && fileOf(`element-${element}`);
  return source ? painted(source, `tint-${element}`) : null;
}

function icon(kind: Icon): HTMLElement {
  return painted(ICONS[kind], `tint-${kind} portal-icon`);
}

/// An element's shape: the game's own symbol when Omoio has it, its drawing
/// when not. A figure with no element gets its kind's icon or drawing.
function emblem(into: HTMLElement, element: FigureElement | null, kind: FigureKind | null) {
  const own = element ? null : kindIcon(kind);
  const shape = own ? icon(own) : symbol(element);
  if (shape) into.appendChild(shape);
  else into.insertAdjacentHTML("beforeend", drawing(element, kind));
}

function mark(element: FigureElement | null, kind: FigureKind | null, source: string | null): HTMLElement {
  const badge = node("span", `portal-mark ${tintOf(element, kind)}`);
  if (source) badge.appendChild(image(source));
  else emblem(badge, element, kind);
  return badge;
}

/// How a swapper moves: the game's own Swap Zone badge when Omoio has it,
/// with the word, which a badge alone doesn't give someone new to them.
function movement(moves: Movement): HTMLElement {
  const part = node("span", "portal-move");
  const badge = fileOf(`movement-${moves}`);
  if (badge) part.appendChild(image(badge));
  part.append(MOVEMENT_NAMES[moves]);
  return part;
}

/// What stands in for a figure's picture until Omoio has the game's: a
/// Creation Crystal's cut gem in its element's colour, which tells it apart
/// from the element's Skylanders, and for anything else its element's shape.
function artless(into: HTMLElement, entry: Entry) {
  if (entry.kind === "crystal") into.insertAdjacentHTML("beforeend", svg(MARKS.crystal));
  else emblem(into, entry.element, entry.kind);
}

/// The picture spot at the top of a tile, with its corner badge: the
/// figure's own picture from the game when Omoio has it, its element's
/// drawing when not. A swapper is its bottom with a top laid over it: its
/// own, or while a top is picked, that one, so each bottom shows the mix.
function picture(entry: Entry, badge: keyof typeof BADGES | null): HTMLElement {
  const spot = node("span", `portal-art ${tintOf(entry.element, entry.kind, terrainOf(entry))}`);
  const sources = entry.swap
    ? [entry.swap.bottom, pickedTop?.top ?? entry.swap.top].map((half) => pictureOf(half.id, half.variant))
    : [pictureOf(entry.offer?.id ?? entry.figure?.id, entry.offer?.variant ?? entry.figure?.variant)];
  if (sources.every((source) => source)) {
    for (const source of sources) spot.appendChild(image(source!));
  } else {
    artless(spot, entry);
  }
  const kind = classOf(entry);
  const mark = kind && classMark(kind);
  if (mark) {
    const corner = node("span", `portal-class-corner ${kind}`);
    corner.appendChild(mark);
    spot.appendChild(corner);
  }
  if (badge) {
    const [words, shape] = BADGES[badge];
    const corner = node("span", `portal-badge ${badge}`);
    corner.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${shape}</svg>`;
    corner.append(words);
    spot.appendChild(corner);
  }
  const partner = partnerOf(entry);
  if (partner != null && (entry.offer?.kind ?? entry.figure?.kind) === "vehicle") {
    spot.appendChild(cornerFace(partner, "portal-art-face"));
  }
  return spot;
}

/// The line under a tile's name: the element in its colour and shape, or
/// the kind of figure when it has no element, the series where the name
/// doesn't give it, how a swapper moves, which its bottom decides, and
/// where a vehicle goes or what races a trophy is for.
function kindLine(entry: Entry): HTMLElement {
  const line = node("span", `portal-kind ${tintOf(entry.element, entry.kind, terrainOf(entry))}`);
  if (entry.element) {
    emblem(line, entry.element, null);
    line.append(ELEMENT_NAMES.get(entry.element) ?? "");
  } else if (entry.kind) {
    const own = kindIcon(entry.kind);
    if (own) line.appendChild(icon(own));
    else if (entry.kind in MARKS) line.insertAdjacentHTML("beforeend", svg(MARKS[entry.kind]));
    line.append(KIND_NAMES[entry.kind] ?? "");
  }
  const kind = classOf(entry);
  if (kind) {
    const named = node("span", `portal-class ${kind}`);
    const mark = classMark(kind);
    if (mark) named.appendChild(mark);
    named.append(CLASSES[kind].words);
    line.appendChild(named);
  }
  const series = entry.offer?.series ?? entry.figure?.series;
  if (series) line.appendChild(node("span", "portal-series", `Series ${series}`));
  const moves = entry.swap ? entry.swap.bottom.movement : (entry.offer?.movement ?? entry.figure?.movement);
  if (moves) line.appendChild(movement(moves));
  const terrain = terrainOf(entry);
  if (terrain) line.appendChild(terrainPart(terrain));
  return line;
}

/// What a SuperCharger drives, under its name, the way a swapper's tile
/// says how it moves: its vehicle's name with the vehicle's terrain.
function drivesLine(entry: Entry): HTMLElement | null {
  const partner = partnerOf(entry);
  if (classOf(entry) !== "supercharger" || partner == null) return null;
  const vehicle = offers.find((offer) => offer.id === partner);
  const name = nameOfId(partner);
  if (!name) return null;
  const line = node("span", "portal-drives");
  if (vehicle?.terrain) terrainShape(line, vehicle.terrain);
  line.append(`Drives ${name}`);
  return line;
}

/// A battle class's symbol: the game's own white shape when Omoio has read
/// it, painted in the text's colour, Omoio's drawing when not.
function battleMark(battle: BattleClass): HTMLElement {
  const source = fileOf(`class-${battle}`);
  if (source) return painted(source, "tint-ink portal-battle-shape");
  const drawn = node("span", "portal-battle-shape");
  drawn.innerHTML = svg(BATTLE_CLASSES[battle].shape);
  return drawn;
}

/// A battle class with its name: "Knight".
function battlePart(battle: BattleClass): HTMLElement {
  const part = node("span", "portal-battle");
  part.append(battleMark(battle), BATTLE_CLASSES[battle].words);
  return part;
}

/// A Sensei's battle class under its element, the way a SuperCharger's tile
/// says what it drives.
function battleLine(entry: Entry): HTMLElement | null {
  const battle = battleOf(entry);
  if (!battle) return null;
  const line = node("span", "portal-drives");
  line.appendChild(battlePart(battle));
  return line;
}

/// "A, B and C".
function listed(names: string[]): string {
  return names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}` : (names[0] ?? "");
}

/// What a trophy unlocks in SuperChargers, and what a piece from an earlier
/// game does in it, a line to each thing. A saved trap that holds a villain
/// names it.
function doesInSuperChargers(entry: Entry): string[] {
  const kind = entry.offer?.kind ?? entry.figure?.kind;
  const id = entry.offer?.id ?? entry.figure?.id;
  if (kind === "trophy") {
    const unlocks = entry.offer?.unlocks ?? offers.find((offer) => offer.id === id)?.unlocks;
    if (!unlocks) return [];
    if (unlocks.tracks.length === 0) return [`Race as ${listed(unlocks.villains)} in Sky races`];
    return [
      `Race as ${listed(unlocks.villains)}, once caught in Boss Pursuit`,
      `Opens ${listed(unlocks.tracks)}, and the Mirror and SuperVillain Cups`,
    ];
  }
  if (kind === "trap") {
    const held = villainList.find((villain) => villain.id === entry.figure?.holds?.villain);
    if (held) return [`Opens ${held.name}'s Skystones card, and gives a special attack`];
  }
  const words = kind ? OLDER_PIECES[kind] : undefined;
  return words ? [words] : [];
}

/// What a piece from an earlier game does in SuperChargers. A magic item and
/// an adventure pack's pieces each add a Legendary Treasure to Skylanders
/// Academy (Activision's "Characters and Magic Items Issues FAQ", question
/// 6, and the Skylanders wiki's "Magic Item" and "Adventure Pack", read 7
/// October 2026). A trap gives the Skylander or the vehicle a special attack
/// of its element, and a villain in it opens the villain's Skystones
/// Overdrive card, since SuperChargers catches no villains (Activision's
/// Characters FAQ, question 1, and the wiki's "Trap", read the same day).
const OLDER_PIECES: Partial<Record<FigureKind, string>> = {
  item: "Adds a Legendary Treasure to the Academy",
  adventure: "Adds a Legendary Treasure to the Academy",
  trap: "Gives a special attack. A villain in it opens its Skystones card",
};

/// What a piece from an earlier game does in Imaginators, where every older
/// toy works (Activision's "Skylanders Imaginators Toy FAQ", question 1,
/// read 7 October 2026). A magic item or a trap gives gold, an adventure
/// pack's piece goes into the collection with a little treasure the first
/// time it goes on (the Skylanders wiki's "Magic Item", "Traps" and
/// "Adventure Pack"). SuperChargers' vehicles race in Skylanders Racing,
/// which the map opens, and traps work there as in SuperChargers
/// (Activision's Imaginators FAQ, questions 6 and 7), and a trophy opens its
/// villains' vehicles for racing (readers' answers on
/// skylanderscharacterlist.com), all read the same day.
const IMAGINATORS_PIECES: Partial<Record<FigureKind, string>> = {
  item: "Gives the Skylander on the portal gold",
  adventure: "Goes in the collection, with a little treasure the first time",
  trap: "Gives 500 gold, and a special attack in racing",
  vehicle: "Races in Skylanders Racing, from the map",
  trophy: "Opens its villains' vehicles for racing",
};

function doesInImaginators(entry: Entry): string[] {
  const kind = entry.offer?.kind ?? entry.figure?.kind;
  const words = kind ? IMAGINATORS_PIECES[kind] : undefined;
  return words ? [words] : [];
}

/// What an older piece does in the game running, under its name, in the
/// games where it does something else than in its own.
const NOTES: Partial<Record<SkylandersGame, (entry: Entry) => string[]>> = {
  superchargers: doesInSuperChargers,
  imaginators: doesInImaginators,
};

function noteLines(entry: Entry): HTMLElement[] {
  const lines = (game && NOTES[game]?.(entry)) || [];
  return lines.map((words) => node("span", "portal-note", words));
}

function renderHead(): HTMLElement {
  const head = node("div", "portal-head");
  head.appendChild(node("div", "portal-title", "Portal"));
  const row = node("div", "portal-on");
  const on = placed();
  if (on.length === 0) row.appendChild(node("span", "portal-quiet", "Nothing on the portal."));
  const pairs = superCharged();
  const unmanned = !driverOn();
  // A SuperCharged pair sits in one frame, its driver first, with the mark
  // the game's lightning stands for.
  let frame: HTMLElement | null = null;
  on.forEach((figure, index) => {
    const known = offers.find((offer) => offer.name === figure.name);
    const button = node("button", zone === "portal" && index === chip ? "portal-chip sel" : "portal-chip");
    button.append(
      mark(known?.element ?? null, known?.kind ?? null, pictureOf(known?.id, known?.variant)),
      node("span", "", figure.name)
    );
    if (unmanned && holds(figure.slot, "vehicle")) {
      const alone = node("span", "portal-chip-alone");
      alone.innerHTML = svg(MARKS.figure);
      alone.append("Needs a driver");
      button.appendChild(alone);
    }
    // A trap says which villain it brings with it.
    const villain = heldIn(figure.slot);
    if (villain) {
      button.append(node("span", "portal-with", "with"), node("span", "", villain.name));
      const face = fileOf(`villain-${villain.id}`);
      if (face) {
        const picture = image(face);
        picture.className = "portal-chip-villain";
        button.appendChild(picture);
      }
    }
    button.onclick = () => {
      zone = "portal";
      chip = index;
      void act(takeOff);
    };
    if (pairs.some((pair) => pair.driver === figure.slot)) {
      frame = node("span", "portal-pair");
      frame.setAttribute("role", "group");
      frame.setAttribute("aria-label", "SuperCharged");
      const label = node("span", "portal-pair-mark");
      label.innerHTML = svg(MARKS.supercharged);
      label.append("SuperCharged");
      frame.append(label, button);
      row.appendChild(frame);
    } else if (frame && pairs.some((pair) => pair.vehicle === figure.slot)) {
      frame.appendChild(button);
      frame = null;
    } else {
      row.appendChild(button);
    }
  });
  head.appendChild(row);
  return head;
}

function renderTabs(): HTMLElement {
  const nav = node("div", "portal-nav");
  const bar = node("div", "portal-tabs");
  bar.setAttribute("role", "tablist");
  tabs.forEach((each, index) => {
    const button = node("button", index === tab ? "portal-tab sel" : "portal-tab");
    button.setAttribute("role", "tab");
    button.setAttribute("aria-selected", String(index === tab));
    // An element without the game's own symbol yet gets Omoio's drawing of
    // it, as its figures' tiles do, so every tab has a shape to go by.
    if (each.element) {
      const own = symbol(each.element);
      if (own) button.appendChild(own);
      else button.insertAdjacentHTML("beforeend", `<svg class="portal-tab-mark tint-${each.element}" viewBox="0 0 24 24" aria-hidden="true">${MARKS[each.element]}</svg>`);
    } else if (each.icon) button.appendChild(icon(each.icon));
    else if (each.mark) button.insertAdjacentHTML("beforeend", `<svg class="portal-tab-mark ${each.mark}" viewBox="0 0 24 24" aria-hidden="true">${MARKS[each.mark]}</svg>`);
    button.append(each.label);
    button.onclick = () => showTab(index);
    bar.appendChild(button);
  });
  nav.append(node("span", "portal-bumper", nameOf(family, "LB")), bar, node("span", "portal-bumper", nameOf(family, "RB")));
  return nav;
}

function renderBody(): HTMLElement {
  if (tabs[tab]?.tray) return renderTray();
  if (tabs[tab]?.garage) return renderGarage();
  if (tabs[tab]?.crystals) return renderCrystals();
  if (tabs[tab]?.senseis) return renderSenseis();
  const body = node("div", "portal-body");
  const current = tabs[tab];
  if (!current || current.entries.length === 0) {
    body.appendChild(
      node(
        "div",
        "portal-quiet",
        asking
          ? "Getting the characters…"
          : "No saved figures yet. Choose a character under its element and it is saved here."
      )
    );
    return body;
  }
  const grid = node("div", "portal-grid");
  current.entries.forEach((entry, index) => {
    const on = isOn(entry);
    const chosen = Boolean(entry.swap) && pickedTop?.name === entry.name;
    const tile = node(
      "button",
      `portal-item${zone === "grid" && index === at ? " sel" : ""}${on ? " on" : ""}${chosen ? " picked" : ""}`
    );
    const badge = chosen ? "picked" : on ? "on" : entry.offer && entry.figure ? "saved" : null;
    tile.append(picture(entry, badge), node("span", "portal-name", entry.name), kindLine(entry));
    const drives = drivesLine(entry) ?? battleLine(entry);
    if (drives) tile.appendChild(drives);
    tile.append(...noteLines(entry));
    tile.onclick = () => {
      zone = "grid";
      at = index;
      void act(choose);
    };
    grid.appendChild(tile);
  });
  body.appendChild(grid);
  return body;
}

// ---- the villains ----

/// An empty slot in the tray: the hollow a villain sits in once caught.
const SLOT = `<svg class="portal-slot" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 1.6 21 6.8v10.4L12 22.4 3 17.2V6.8z"/><text x="12" y="15.3" text-anchor="middle">?</text></svg>`;

/// The shapes the tally's three counts go by, as well as by their words.
const TALLY_SHAPES = {
  trapped: `<path d="M12 2.5 20.5 7.3v9.4L12 21.5 3.5 16.7V7.3z" fill="currentColor"/>`,
  loose: `<circle cx="12" cy="12" r="8.5" fill="none" stroke="currentColor" stroke-width="2"/>`,
  none: `<path d="M12 2.5 20.5 7.3v9.4L12 21.5 3.5 16.7V7.3z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-dasharray="2.6 2.2"/>`,
};

/// The saved trap figure that holds a villain.
function trapFigure(villain: Villain): Figure | undefined {
  return mine.find((figure) => figure.path === villain.trap?.path);
}

/// The name the portal gives the trap holding a villain.
function trapName(villain: Villain): string {
  const figure = trapFigure(villain);
  return figure ? portalName({ name: figure.name, element: null, kind: null, figure }) : (villain.trap?.name ?? "");
}

/// The slot the trap holding a villain is in, or -1: by its file when Omoio
/// put it on, by its name in a slot whose file Omoio doesn't know.
function trapSlot(villain: Villain): number {
  if (!villain.trap) return -1;
  const byFile = slotOfFile(villain.trap.path);
  if (byFile >= 0) return byFile;
  const name = trapName(villain);
  return onPortal.findIndex((held, slot) => held === name && !slotFiles[slot]);
}

function trapOn(villain: Villain): boolean {
  return trapSlot(villain) >= 0;
}

/// The villain the figure in `slot` holds: the one in its file when Omoio
/// knows which file it is, otherwise the one in the only saved trap of its
/// name.
function heldIn(slot: number): Villain | undefined {
  const file = slotFiles[slot];
  if (file) return villainList.find((villain) => villain.trap?.path === file.path);
  const holding = villainList.filter((villain) => villain.trap && trapName(villain) === onPortal[slot]);
  return holding.length === 1 ? holding[0] : undefined;
}

/// A villain's picture: in its trap's frame when caught, or as it escaped
/// when caught before and in no trap now.
function villainPicture(villain: Villain): string | null {
  if (!villain.caught) return null;
  const own = fileOf(`villain-${villain.id}`);
  return villain.trap ? own : (fileOf(`villain-${villain.id}-loose`) ?? own);
}

function elementWords(element: FigureElement | null): string {
  return element ? (ELEMENT_NAMES.get(element) ?? "") : "Kaos";
}

/// How to catch a villain not caught yet: with a trap of its element, and
/// Kaos with his own.
function catchWith(villain: Villain): string {
  if (!villain.element) return "Only the Kaos trap can hold it.";
  const words = elementWords(villain.element);
  return `Catch it with ${/^[AEIOU]/.test(words) ? "an" : "a"} ${words} trap.`;
}

/// How many villains are caught, how many of those sit in a trap, and a bar
/// that fills as the collection does.
function renderTally(): HTMLElement {
  const total = villainList.length;
  const trapped = villainList.filter((villain) => villain.trap).length;
  const caught = villainList.filter((villain) => villain.caught).length;
  const tally = node("div", "portal-tally");
  const said = node("div", "portal-count");
  said.append(node("b", "", String(caught)), node("span", "", `of ${total} villains caught`));
  const bar = node("div", "portal-bar");
  bar.setAttribute("aria-hidden", "true");
  for (const [part, share] of [["trapped", trapped], ["loose", caught - trapped]] as const) {
    const fill = node("i", part);
    fill.style.width = `${(share / total) * 100}%`;
    bar.appendChild(fill);
  }
  const legend = node("div", "portal-legend");
  const counts: [keyof typeof TALLY_SHAPES, string, number][] = [
    ["trapped", "In a trap", trapped],
    ["loose", "Caught, not in a trap", caught - trapped],
    ["none", "Not caught yet", total - caught],
  ];
  for (const [part, words, number] of counts) {
    const item = node("span", part);
    item.insertAdjacentHTML("beforeend", `<svg viewBox="0 0 24 24" aria-hidden="true">${TALLY_SHAPES[part]}</svg>`);
    item.append(words, node("b", "", String(number)));
    legend.appendChild(item);
  }
  tally.append(said, bar, legend);
  return tally;
}

/// One villain in its column: its picture in the slot once caught, with the
/// trap that holds it in the corner.
function villainTile(villain: Villain, index: number): HTMLElement {
  const state = villain.trap ? "trapped" : villain.caught ? "loose" : "none";
  const fresh = caughtNew.has(villain.id);
  const tile = node("button", `portal-villain ${state}${zone === "grid" && index === at ? " sel" : ""}${fresh ? " new" : ""}`);
  const where = villain.trap ? `in ${trapName(villain)}` : villain.caught ? "caught, not in a trap" : "not caught yet";
  tile.setAttribute("aria-label", `${villain.name}, ${where}`);
  const art = node("span", "portal-villain-art");
  art.insertAdjacentHTML("beforeend", SLOT);
  const face = villainPicture(villain);
  if (face) art.appendChild(image(face));
  if (villain.trap) {
    const badge = node("span", "portal-villain-trap");
    const trap = pictureOf(villain.trap.id, villain.trap.variant);
    if (trap) badge.appendChild(image(trap));
    else badge.insertAdjacentHTML("beforeend", `<svg viewBox="0 0 24 24" aria-hidden="true">${MARKS.trap}</svg>`);
    art.appendChild(badge);
  }
  if (fresh) art.appendChild(node("span", "portal-villain-new", "New"));
  tile.append(art, node("span", "portal-villain-name", villain.name));
  tile.onclick = () => {
    zone = "grid";
    at = index;
    void act(choose);
  };
  return tile;
}

/// The villain picked in the tray: its picture, its element, the trap that
/// holds it, and the rest of its element.
function renderVillain(villain: Villain | undefined): HTMLElement {
  const card = node("div", `portal-villain-card tint-${villain?.element ?? "kaos"}`);
  if (!villain) return card;
  const art = node("div", "portal-villain-card-art");
  const face = villainPicture(villain);
  if (face) art.appendChild(image(face));
  else art.insertAdjacentHTML("beforeend", SLOT);
  const line = node("div", "portal-kind");
  if (villain.element) emblem(line, villain.element, null);
  line.append(villain.element ? `${elementWords(villain.element)} villain` : "Kaos");
  card.append(art, node("div", "portal-villain-card-name", villain.name), line);
  if (villain.trap) {
    card.appendChild(node("div", "portal-label", "In this trap"));
    const held = node("div", "portal-held");
    const picture = node("span", "portal-held-art");
    const trap = pictureOf(villain.trap.id, villain.trap.variant);
    if (trap) picture.appendChild(image(trap));
    const words = node("div", "portal-held-words");
    words.append(node("b", "", trapName(villain)), node("span", "", `${elementWords(villain.trap.element)} trap`));
    if (trapOn(villain)) {
      const on = node("span", "portal-held-on");
      on.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${BADGES.on[1]}</svg>`;
      on.append(BADGES.on[0]);
      words.appendChild(on);
    }
    held.append(picture, words);
    card.appendChild(held);
  } else {
    card.appendChild(
      node("p", "portal-villain-note", villain.caught ? "Not in one of your traps now." : `Not caught yet. ${catchWith(villain)}`)
    );
  }
  const kin = villainList.filter((other) => other.element === villain.element);
  if (kin.length > 1) {
    card.appendChild(node("div", "portal-label", `${elementWords(villain.element)} villains`));
    const row = node("div", "portal-kin");
    for (const other of kin) {
      const spot = node("span", other.id === villain.id ? "portal-kin-villain me" : "portal-kin-villain");
      const face = villainPicture(other);
      if (face) spot.appendChild(image(face));
      else spot.insertAdjacentHTML("beforeend", SLOT);
      row.appendChild(spot);
    }
    card.appendChild(row);
  }
  return card;
}

/// The villains tab, laid out as a collector's tray: a column to each
/// element, every villain in it, caught or not, and the one picked beside.
function renderTray(): HTMLElement {
  const body = node("div", "portal-body portal-tray");
  const order = trayOrder();
  const main = node("div", "portal-tray-main");
  const columns = node("div", "portal-tray-columns");
  for (const column of trayColumns()) {
    const tint = `tint-${column.element ?? "kaos"}`;
    const shown = node("div", `portal-tray-column ${tint}`);
    const name = node("div", "portal-tray-column-name");
    if (column.element) emblem(name, column.element, null);
    name.append(column.label);
    const got = column.villains.filter((villain) => villain.caught).length;
    const all = column.villains.length;
    shown.append(name, node("div", got === all ? "portal-tray-column-n full" : "portal-tray-column-n", `${got} of ${all}`));
    for (const villain of column.villains) shown.appendChild(villainTile(villain, order.indexOf(villain)));
    columns.appendChild(shown);
  }
  main.append(renderTally(), columns);
  body.append(main, renderVillain(order[at]));
  return body;
}

// ---- the garage ----

/// SuperChargers' vehicles as a garage: a column to each terrain, so Land,
/// Sea and Sky tell apart at a glance as the tray's elements do, what drives
/// now above them, and the vehicle picked beside. Each column scrolls on its
/// own, so the other two stay in view.
function renderGarage(): HTMLElement {
  const body = node("div", "portal-body portal-tray portal-garage");
  const entries = tabs[tab]?.entries ?? [];
  const main = node("div", "portal-garage-main");
  const columns = node("div", "portal-garage-columns");
  let index = 0;
  for (const column of garageColumns(entries)) {
    const shown = node("div", `portal-garage-column tint-${column.terrain ?? "none"}`);
    shown.dataset.scroll = column.label;
    const head = node("div", "portal-garage-column-head");
    const name = node("div", "portal-tray-column-name");
    if (column.terrain) terrainShape(name, column.terrain);
    name.append(column.label);
    const saved = column.entries.filter((entry) => entry.figure).length;
    const all = column.entries.length;
    head.append(name, node("div", saved === all ? "portal-tray-column-n full" : "portal-tray-column-n", `${saved} of ${all} saved`));
    shown.appendChild(head);
    // A vehicle's variants follow it in one group, set in under it.
    let group: HTMLElement | null = null;
    let groupOf: number | null = null;
    for (const entry of column.entries) {
      const id = entry.offer?.id ?? null;
      if (!group || id !== groupOf) {
        group = node("div", "portal-car-group");
        shown.appendChild(group);
        groupOf = id;
      }
      group.appendChild(vehicleTile(entry, index, group.childElementCount > 0));
      index += 1;
    }
    columns.appendChild(shown);
  }
  main.append(renderGarageStatus(), columns);
  body.append(main, renderVehicle(entries[at]));
  return body;
}

/// One vehicle in its column: its picture with its driver's face in the
/// corner, its name, and whether it is on the portal, saved or new.
function vehicleTile(entry: Entry, index: number, variant: boolean): HTMLElement {
  const on = isOn(entry);
  const charged = on && superCharged().some((pair) => pair.vehicle === slotOf(entry));
  const tile = node(
    "button",
    `portal-car${zone === "grid" && index === at ? " sel" : ""}${on ? " on" : ""}${variant ? " variant" : ""}`
  );
  const driver = partnerOf(entry);
  const driverName = driver == null ? null : nameOfId(driver);
  const terrain = terrainOf(entry);
  const state = charged ? "SuperCharged" : on ? "on the portal" : entry.figure ? "saved" : "new";
  tile.setAttribute(
    "aria-label",
    [entry.name, terrain && `${TERRAIN_NAMES.get(terrain)} vehicle`, driverName && `${driverName} SuperCharges it`, state]
      .filter(Boolean)
      .join(", ")
  );
  const art = node("span", `portal-car-art ${tintOf(entry.element, entry.kind)}`);
  const source = pictureOf(entry.offer?.id ?? entry.figure?.id, variantOf(entry));
  if (source) art.appendChild(image(source));
  else emblem(art, entry.element, entry.kind);
  if (driver != null) art.appendChild(cornerFace(driver, "portal-car-face"));
  const line = node("span", `portal-car-state ${charged ? "charged" : on ? "on" : entry.figure ? "saved" : "new"}`);
  if (charged) line.innerHTML = svg(MARKS.supercharged);
  else if (on) line.innerHTML = svg(BADGES.on[1]);
  else if (entry.figure) line.innerHTML = svg(BADGES.saved[1]);
  else line.innerHTML = svg(BADGES.new[1]);
  line.append(charged ? "SuperCharged" : on ? "On the portal" : entry.figure ? "Saved" : "New");
  const words = node("span", "portal-car-words");
  words.append(node("span", "portal-car-name", entry.name), line);
  tile.append(art, words);
  tile.onclick = () => {
    zone = "grid";
    at = index;
    void act(choose);
  };
  return tile;
}

/// What drives now, above the garage: the vehicle on the portal and who is
/// at its wheel, or that none is on.
function renderGarageStatus(): HTMLElement {
  const status = node("div", "portal-garage-status");
  const art = node("span", "portal-garage-status-art");
  const words = node("div", "portal-garage-status-words");
  const vehicle = vehiclesOn()[0];
  if (!vehicle) {
    art.innerHTML = svg(MARKS.vehicle);
    words.append(node("b", "", "No vehicle on the portal"), node("span", "", "The game uses one vehicle at a time."));
  } else {
    const face = pictureNamed(vehicle.name)[0];
    if (face) art.appendChild(image(face));
    else art.innerHTML = svg(MARKS.vehicle);
    const pair = superCharged().find((each) => each.vehicle === vehicle.slot);
    const partner = characterIn(vehicle.slot)?.partner;
    const partnerName = partner == null ? null : nameOfId(partner);
    if (pair) {
      status.classList.add("charged");
      words.append(node("b", "", `${vehicle.name} is SuperCharged`), node("span", "", `${onPortal[pair.driver]} is driving it.`));
    } else if (!driverOn()) {
      status.classList.add("alone");
      words.append(
        node("b", "", `${vehicle.name} needs a driver`),
        node("span", "", partnerName ? `Put a Skylander on. ${partnerName} would SuperCharge it.` : "Put a Skylander on to drive it.")
      );
    } else {
      words.append(
        node("b", "", `${vehicle.name} is on the portal`),
        node("span", "", partnerName ? `${partnerName} would SuperCharge it.` : "A Skylander on the portal drives it.")
      );
    }
  }
  status.append(art, words);
  return status;
}

/// The vehicle picked in the garage: its picture, where it goes, its
/// element, the SuperCharger it was made for, and whether its figure is
/// saved and on the portal. A button puts it on with its SuperCharger, for
/// the mouse; the pad has the top face button for that.
function renderVehicle(entry: Entry | undefined): HTMLElement {
  const card = node("div", `portal-villain-card portal-car-card ${tintOf(entry?.element ?? null, "vehicle")}`);
  if (!entry) return card;
  const art = node("div", "portal-villain-card-art");
  const source = pictureOf(entry.offer?.id ?? entry.figure?.id, variantOf(entry));
  if (source) art.appendChild(image(source));
  else emblem(art, entry.element, entry.kind);
  const line = node("div", "portal-kind");
  const terrain = terrainOf(entry);
  if (terrain) line.appendChild(terrainPart(terrain, `${TERRAIN_NAMES.get(terrain)} vehicle`));
  if (entry.element) {
    const element = node("span", `portal-card-element tint-${entry.element}`);
    emblem(element, entry.element, null);
    element.append(ELEMENT_NAMES.get(entry.element) ?? "");
    line.appendChild(element);
  }
  card.append(art, node("div", "portal-villain-card-name", entry.name), line);
  const pair = pairOf(entry);
  const on = isOn(entry);
  if (pair) {
    const driver = pair.driver;
    const driverOnNow = slotWith(driver.id) >= 0;
    const charged = on && driverOnNow;
    card.appendChild(node("div", "portal-label", "Its SuperCharger"));
    const held = node("div", `portal-held${charged ? " charged" : ""}`);
    const face = node("span", `portal-held-art ${tintOf(driver.offer?.element ?? null, "character")}`);
    const picture = pictureOf(driver.id, driver.offer?.variant ?? driver.figure?.variant);
    if (picture) face.appendChild(image(picture));
    else emblem(face, driver.offer?.element ?? null, "character");
    const words = node("div", "portal-held-words");
    words.append(node("b", "", driver.name), node("span", "", driver.figure ? "Saved" : "New, made the first time it goes on"));
    if (driverOnNow) {
      const state = node("span", charged ? "portal-held-on charged" : "portal-held-on");
      state.innerHTML = svg(charged ? MARKS.supercharged : BADGES.on[1]);
      state.append(charged ? "SuperCharged" : BADGES.on[0]);
      words.appendChild(state);
    }
    held.append(face, words);
    card.appendChild(held);
  }
  card.appendChild(node("div", "portal-label", "This figure"));
  const kept = node("div", `portal-car-kept ${entry.figure ? "saved" : "new"}`);
  kept.innerHTML = svg(entry.figure ? BADGES.saved[1] : BADGES.new[1]);
  kept.append(entry.figure ? "Saved, keeps its mods" : "New, made the first time it goes on");
  card.appendChild(kept);
  if (on) {
    const state = node("div", "portal-car-kept on");
    state.innerHTML = svg(BADGES.on[1]);
    state.append(BADGES.on[0]);
    card.appendChild(state);
  }
  if (pair && !pairOn(entry)) {
    const both = node("button", "portal-card-action");
    both.append(node("kbd", "", nameOf(family, "North")), pairWords(entry, pair.other));
    both.onclick = () => {
      zone = "grid";
      void act(pairUp);
    };
    card.appendChild(both);
  }
  card.appendChild(node("p", "portal-villain-note", "Any Skylander can drive it."));
  return card;
}

// ---- Imaginators: Creation Crystals and Senseis ----

/// An element's name, its shape in front, in its colour.
function elementPart(element: FigureElement, words = ELEMENT_NAMES.get(element) ?? ""): HTMLElement {
  const part = node("span", `portal-card-element tint-${element}`);
  emblem(part, element, null);
  part.append(words);
  return part;
}

/// Whether a crystal is a Legendary one, which its variant marks.
function legendary(entry: Entry): boolean {
  return (variantOf(entry) & 0x0400) !== 0;
}

/// A blank crystal's casing as its tile and card name it.
function casingWords(entry: Entry): string {
  const casing = casingOf(entry);
  const words = casing ? CASINGS[casing] : "Crystal";
  return legendary(entry) ? `Legendary ${words}` : words;
}

/// What the game does when a Sensei or a crystal made now goes on, when the
/// Signature Patch isn't on in it, and what to do: a box in the card, and
/// the same words as the notice when one is tried.
function patchWords(): { title: string; detail: string } | null {
  const pack = made?.pack || "Signature Patch";
  switch (made?.check) {
    case "not_downloaded":
      return {
        title: "Cemu's packs aren't here yet",
        detail: `Imaginators checks a factory signature that the Senseis and crystals Omoio makes can't carry. Cemu's ${pack}, one of its community packs, takes that check away. ${nameOf(family, "South")} gets them.`,
      };
    case "off":
      return {
        title: `The ${pack} is off`,
        detail: "Imaginators won't take a Sensei or crystal Omoio makes without it. Turn it on in this game's Community packs in Omoio, then start the game again.",
      };
    case "next_start":
      return {
        title: `The ${pack} counts from the next start`,
        detail: "Close the game and start it again, then make Senseis and crystals here.",
      };
    default:
      return null;
  }
}

function patchBox(): HTMLElement | null {
  const words = patchWords();
  if (!words) return null;
  const box = node("div", "portal-patch");
  box.innerHTML = svg(NOTICE_SHAPES.problem);
  const text = node("div", "portal-patch-words");
  text.append(node("b", "", words.title), node("span", "", words.detail));
  box.appendChild(text);
  return box;
}

/// Imaginators' Creation Crystals: the user's own first, each holding the
/// Imaginator made in it, the one used last first; then a row of blank
/// crystals to each element, a tile to each casing; and the crystal picked
/// beside. A blank one always makes a new crystal in a new file, since a
/// crystal keeps its Imaginator for good: "There are no options to reset a
/// Creation Crystal or change a Battle Class" (Activision's "Skylanders
/// Imaginators Toy FAQ", read 7 October 2026).
function renderCrystals(): HTMLElement {
  const body = node("div", "portal-body portal-tray portal-crystals");
  const entries = tabs[tab]?.entries ?? [];
  const main = node("div", "portal-tray-main portal-shelf-main");
  const saved = entries.filter((each) => !each.fresh);
  const yours = node("div", "portal-shelf-head");
  yours.append(
    node("b", "", "Your Imaginators"),
    node("span", "", saved.length === 0 ? "None yet. Pick a blank crystal below to make one." : saved.length === 1 ? "1 crystal" : `${saved.length} crystals`)
  );
  main.appendChild(yours);
  if (saved.length > 0) {
    const shelf = node("div", "portal-shelf");
    saved.forEach((entry, index) => shelf.appendChild(crystalTile(entry, index)));
    main.appendChild(shelf);
  }
  const blank = node("div", "portal-shelf-head");
  blank.append(node("b", "", "Blank crystals"), node("span", "", "Each one makes a new crystal in a new file."));
  main.appendChild(blank);
  const box = patchBox();
  if (box) main.appendChild(box);
  const forge = node("div", "portal-forge");
  let index = saved.length;
  for (const row of crystalRows(entries)) {
    if (!row.element) continue;
    const line = node("div", `portal-forge-row tint-${row.element}`);
    const label = node("div", "portal-forge-element");
    emblem(label, row.element, null);
    label.append(ELEMENT_NAMES.get(row.element) ?? "");
    line.appendChild(label);
    for (const entry of row.entries) line.appendChild(blankTile(entry, index++));
    forge.appendChild(line);
  }
  main.appendChild(forge);
  body.append(main, renderCrystal(entries[at]));
  return body;
}

/// One of the user's crystals: its picture, its file's name, and the
/// element of the Imaginator in it. Omoio doesn't read the Imaginator's own
/// name out of the crystal.
function crystalTile(entry: Entry, index: number): HTMLElement {
  const on = isOn(entry);
  const tile = node("button", `portal-item${zone === "grid" && index === at ? " sel" : ""}${on ? " on" : ""}`);
  const element = entry.element ? ELEMENT_NAMES.get(entry.element) : null;
  tile.setAttribute("aria-label", [entry.name, element && `${element} Imaginator`, on ? "on the portal" : "saved"].filter(Boolean).join(", "));
  const line = node("span", `portal-kind ${tintOf(entry.element, entry.kind)}`);
  if (entry.element) {
    emblem(line, entry.element, null);
    line.append(`${element} Imaginator`);
  }
  tile.append(picture(entry, on ? "on" : null), node("span", "portal-name", entry.name), line);
  tile.onclick = () => {
    zone = "grid";
    at = index;
    void act(choose);
  };
  return tile;
}

/// A blank crystal of one casing, in its element's row.
function blankTile(entry: Entry, index: number): HTMLElement {
  const tile = node("button", `portal-blank${zone === "grid" && index === at ? " sel" : ""}`);
  const element = entry.element ? ELEMENT_NAMES.get(entry.element) : "";
  tile.setAttribute("aria-label", `New ${element} crystal, ${casingWords(entry)}`);
  const art = node("span", `portal-blank-art ${tintOf(entry.element, entry.kind)}`);
  const source = pictureOf(entry.offer?.id, entry.offer?.variant);
  if (source) art.appendChild(image(source));
  else artless(art, entry);
  const words = node("span", "portal-car-words");
  const state = node("span", "portal-car-state new");
  state.innerHTML = svg(BADGES.new[1]);
  state.append("New");
  words.append(node("span", "portal-car-name", casingWords(entry)), state);
  tile.append(art, words);
  tile.onclick = () => {
    zone = "grid";
    at = index;
    void act(choose);
  };
  return tile;
}

/// The crystal picked: a blank one says what happens when it goes on, one
/// of the user's that it keeps its Imaginator.
function renderCrystal(entry: Entry | undefined): HTMLElement {
  const card = node("div", `portal-villain-card portal-car-card ${tintOf(entry?.element ?? null, "crystal")}`);
  if (!entry) return card;
  const element = entry.element ? (ELEMENT_NAMES.get(entry.element) ?? "") : "";
  const art = node("div", "portal-villain-card-art");
  const source = pictureOf(entry.offer?.id ?? entry.figure?.id, variantOf(entry));
  if (source) art.appendChild(image(source));
  else artless(art, entry);
  const line = node("div", "portal-kind");
  if (entry.element) line.appendChild(elementPart(entry.element, entry.fresh ? element : `${element} Imaginator`));
  if (casingOf(entry)) line.appendChild(node("span", "portal-casing", casingWords(entry)));
  card.append(art, node("div", "portal-villain-card-name", entry.fresh ? `New ${element} crystal` : entry.name), line);
  if (entry.fresh) {
    card.appendChild(node("div", "portal-label", "When it goes on"));
    card.appendChild(node("p", "portal-card-note", "The game asks you to make an Imaginator in it. Its battle class is picked then, and stays for good."));
    card.appendChild(node("div", "portal-label", "This figure"));
    const kept = node("div", "portal-car-kept new");
    kept.innerHTML = svg(BADGES.new[1]);
    kept.append("A new crystal in a new file");
    card.append(kept, node("p", "portal-villain-note", "Your other crystals stay as they are."));
    return card;
  }
  card.appendChild(node("div", "portal-label", "This figure"));
  const kept = node("div", "portal-car-kept saved");
  kept.innerHTML = svg(BADGES.saved[1]);
  kept.append("Saved, keeps its Imaginator");
  card.appendChild(kept);
  if (isOn(entry)) {
    const state = node("div", "portal-car-kept on");
    state.innerHTML = svg(BADGES.on[1]);
    state.append(BADGES.on[0]);
    card.appendChild(state);
  }
  return card;
}

/// How many classes sit side by side in the Senseis tab. Eleven columns in
/// one row would each be too narrow to read from a sofa, so they go in two
/// rows of six, and the last place shows the Senseis on the portal.
const BAND = 6;

/// Imaginators' Senseis, a column to each battle class, so "I need a
/// Knight" takes one look, and the Sensei picked beside.
function renderSenseis(): HTMLElement {
  const body = node("div", "portal-body portal-tray portal-dojo");
  const entries = tabs[tab]?.entries ?? [];
  const main = node("div", "portal-tray-main portal-dojo-main");
  const box = patchBox();
  if (box) main.appendChild(box);
  const classes = node("div", "portal-dojo-classes");
  let index = 0;
  for (const column of senseiColumns(entries)) {
    const shown = node("div", `portal-dojo-class${column.battle === "kaos" ? " tint-kaos" : ""}`);
    const head = node("div", "portal-dojo-class-head");
    if (column.battle) head.appendChild(battlePart(column.battle));
    else head.append("Other");
    shown.appendChild(head);
    for (const entry of column.entries) shown.appendChild(senseiTile(entry, index++));
    classes.appendChild(shown);
  }
  classes.appendChild(renderDojoStatus());
  main.appendChild(classes);
  body.append(main, renderSensei(entries[at]));
  return body;
}

/// One Sensei in its class's column: its picture, a mark when it is a
/// villain, its name and its element.
function senseiTile(entry: Entry, index: number): HTMLElement {
  const on = isOn(entry);
  const tile = node("button", `portal-sensei${zone === "grid" && index === at ? " sel" : ""}${on ? " on" : ""}`);
  const element = entry.element ? (ELEMENT_NAMES.get(entry.element) ?? "") : "";
  const battle = battleOf(entry);
  const villain = classOf(entry) === "villain_sensei";
  const state = on ? "on the portal" : entry.figure ? "saved" : "new";
  tile.setAttribute(
    "aria-label",
    [entry.name, `${element} ${battle ? BATTLE_CLASSES[battle].words : ""} ${villain ? "Villain Sensei" : "Sensei"}`.replace(/\s+/g, " "), state].join(", ")
  );
  const art = node("span", `portal-car-art ${tintOf(entry.element, entry.kind)}`);
  const source = pictureOf(entry.offer?.id ?? entry.figure?.id, variantOf(entry));
  if (source) art.appendChild(image(source));
  else emblem(art, entry.element, entry.kind);
  if (villain) {
    const horns = node("span", "portal-sensei-villain");
    horns.innerHTML = svg(CLASSES.villain_sensei.shape!);
    art.appendChild(horns);
  }
  if (entry.figure) {
    const kept = node("span", "portal-sensei-saved");
    kept.innerHTML = svg(BADGES.saved[1]);
    art.appendChild(kept);
  }
  const words = node("span", "portal-car-words");
  const line = node("span", `portal-sensei-element ${tintOf(entry.element, entry.kind)}`);
  emblem(line, entry.element, null);
  line.append(element);
  words.append(node("span", "portal-car-name", entry.name), line);
  tile.append(art, words);
  tile.onclick = () => {
    zone = "grid";
    at = index;
    void act(choose);
  };
  return tile;
}

/// The Senseis on the portal now, in the place after the last class.
function renderDojoStatus(): HTMLElement {
  const status = node("div", "portal-dojo-status");
  const on = filled().filter((figure) =>
    offers.some((offer) => offer.name === figure.name && (offer.class === "sensei" || offer.class === "villain_sensei"))
  );
  status.appendChild(node("div", "portal-label", "On the portal"));
  if (on.length === 0) {
    status.append(node("b", "", "No Sensei"), node("span", "", "A Sensei opens its element's realms on the map."));
    return status;
  }
  for (const figure of on) {
    const known = offers.find((offer) => offer.name === figure.name);
    const row = node("span", "portal-dojo-on");
    row.append(mark(known?.element ?? null, known?.kind ?? null, pictureOf(known?.id, known?.variant)), figure.name);
    status.appendChild(row);
  }
  return status;
}

/// What a Sensei does besides being played, a line to each thing. Its
/// element's Sensei Realms on the map open to a Sensei of that element
/// only (Activision's "Skylanders Imaginators Gameplay FAQ", question 1).
/// Adding one teaches the Imaginators of its class one of their four Secret
/// Techniques, its class's Sensei Shrine gives it Sky-Chi, its super move,
/// and each new Sensei raises the level the Imaginators can reach by one
/// (the Skylanders wiki's "Battle Classes", "Sky-Chi" and "Imaginators").
/// Kaos has a class of his own and no Shrine. Crash and Cortex open
/// Thumpin' Wumpa Islands (the wiki's "Senseis"). All read 7 October 2026.
function senseiDoes(entry: Entry): string[] {
  const battle = battleOf(entry);
  const element = entry.element && entry.element !== "kaos" ? ELEMENT_NAMES.get(entry.element) : null;
  const lines: string[] = [];
  if (element) lines.push(`Opens the ${element} Sensei Realms on the map`);
  if (battle && battle !== "kaos") {
    const words = BATTLE_CLASSES[battle].words;
    lines.push(`Teaches ${words} Imaginators a Secret Technique`);
    lines.push(`Learns Sky-Chi at the ${words} Sensei Shrine`);
  }
  lines.push("The first time on, your Imaginators can go up one more level");
  const id = entry.offer?.id ?? entry.figure?.id;
  if (id === 630 || id === 631) lines.push("Opens Thumpin' Wumpa Islands");
  return lines;
}

/// The Sensei picked: its element and class, what it does in the game, and
/// whether its figure is saved and on the portal.
function renderSensei(entry: Entry | undefined): HTMLElement {
  const card = node("div", `portal-villain-card portal-car-card ${tintOf(entry?.element ?? null, entry?.kind ?? null)}`);
  if (!entry) return card;
  const art = node("div", "portal-villain-card-art");
  const source = pictureOf(entry.offer?.id ?? entry.figure?.id, variantOf(entry));
  if (source) art.appendChild(image(source));
  else emblem(art, entry.element, entry.kind);
  const line = node("div", "portal-kind");
  if (entry.element) line.appendChild(elementPart(entry.element));
  // Kaos's element and class are both his own, and saying so twice says
  // nothing more.
  const battle = battleOf(entry);
  if (battle && battle !== entry.element) line.appendChild(battlePart(battle));
  const kind = classOf(entry);
  if (kind) {
    const named = node("span", `portal-class ${kind}`);
    const shape = classMark(kind);
    if (shape) named.appendChild(shape);
    named.append(CLASSES[kind].words);
    line.appendChild(named);
  }
  card.append(art, node("div", "portal-villain-card-name", entry.name), line);
  card.appendChild(node("div", "portal-label", "What it does"));
  const does = node("ul", "portal-does");
  for (const words of senseiDoes(entry)) does.appendChild(node("li", "", words));
  card.appendChild(does);
  card.appendChild(node("div", "portal-label", "This figure"));
  const kept = node("div", `portal-car-kept ${entry.figure ? "saved" : "new"}`);
  kept.innerHTML = svg(entry.figure ? BADGES.saved[1] : BADGES.new[1]);
  kept.append(entry.figure ? "Saved, keeps its levels" : "New, made the first time it goes on");
  card.appendChild(kept);
  if (isOn(entry)) {
    const state = node("div", "portal-car-kept on");
    state.innerHTML = svg(BADGES.on[1]);
    state.append(BADGES.on[0]);
    card.appendChild(state);
  }
  return card;
}

/// One of a SuperCharger and its vehicle: the character, and its saved
/// figure when there is one, otherwise what the emulator makes it from.
interface Pick {
  id: number;
  name: string;
  figure?: Figure;
  offer?: Offer;
}

/// The figure to put on for a character: its saved figure when there is
/// one, the one used last first, otherwise the emulator's plain version.
function pickFor(id: number): Pick | null {
  const figure = mine.find((each) => each.id === id);
  if (figure) {
    const offer = offers.find((each) => each.id === id && each.variant === figure.variant);
    return { id, name: offer?.name ?? figure.name, figure, offer };
  }
  const offer = offers.filter((each) => each.id === id).sort((a, b) => a.variant - b.variant)[0];
  return offer ? { id, name: offer.name, offer } : null;
}

/// Whether the game running makes anything of a SuperCharger with its own
/// vehicle. Imaginators races SuperChargers' vehicles, and nothing found
/// says the two do anything together there, so the top button stays free.
function pairsUp(): boolean {
  return game !== "imaginators";
}

/// The pair a tile stands in: a SuperCharger and its own vehicle, the tile's
/// own figure as one of them and `other` as the one to go with it.
function pairOf(entry: Entry | undefined): { driver: Pick; vehicle: Pick; other: Pick } | null {
  if (!entry || entry.swap || !pairsUp()) return null;
  const id = entry.offer?.id ?? entry.figure?.id;
  const partner = partnerOf(entry);
  if (id == null || partner == null) return null;
  const own: Pick = { id, name: entry.name, figure: entry.figure, offer: entry.offer };
  const other = pickFor(partner);
  if (!other) return null;
  const vehicle = (entry.offer?.kind ?? entry.figure?.kind) === "vehicle";
  return vehicle ? { driver: other, vehicle: own, other } : { driver: own, vehicle: other, other };
}

/// What the top face button does on a tile: puts its pair on, or adds the
/// other one when the tile's character, in any variant, is on already.
function pairWords(entry: Entry, other: Pick): string {
  const own = entry.offer?.id ?? entry.figure?.id;
  return own != null && slotWith(own) >= 0 ? `Add ${other.name}` : `Put on with ${other.name}`;
}

/// Whether both of a tile's pair are on the portal, whichever variants.
function pairOn(entry: Entry | undefined): boolean {
  const pair = pairOf(entry);
  return Boolean(pair && slotWith(pair.driver.id) >= 0 && slotWith(pair.vehicle.id) >= 0);
}

/// What the bottom face button does on a tile.
function southWords(entry: Entry | undefined): string {
  if (entry?.swap) return pickedTop ? "Pick bottom" : "Pick top";
  if (turnedAway(entry) && made?.check === "not_downloaded") return "Get Cemu's packs";
  return entry?.fresh ? "Make crystal" : "Put on";
}

function renderFoot(): HTMLElement {
  const foot = node("div", "portal-foot");
  const entry = tabs[tab]?.entries[at];
  const hints: [string, string][] = [];
  if (zone === "portal") {
    hints.push(["South", "Take off"]);
  } else if (tabs[tab]?.tray) {
    const villain = trayOrder()[at];
    if (villain?.trap && trapOn(villain)) hints.push(["West", `Take ${trapName(villain)} off`]);
    else if (villain?.trap) hints.push(["South", `Put ${trapName(villain)} on`]);
  } else {
    hints.push(["South", southWords(entry)]);
    if (entry && isOn(entry)) hints.push(["West", "Take off"]);
    const pair = pairOf(entry);
    if (entry && pair && !pairOn(entry)) hints.push(["North", pairWords(entry, pair.other)]);
  }
  hints.push(["East", gettingPacks ? "Stop" : pickedTop ? "Back" : "Close"]);
  const row = node("div", "portal-hints");
  for (const [input, words] of hints) {
    const hint = node("span", "portal-hint");
    hint.append(node("kbd", "", nameOf(family, input)), words);
    row.appendChild(hint);
  }
  foot.append(row);
  return foot;
}

/// Draws the whole menu again, keeping where the grid and tabs were
/// scrolled so moving doesn't make the list jump.
function render() {
  if (!ready) return;
  settle();
  const scrolled = root.querySelector(".portal-body")?.scrollTop ?? 0;
  const tabsScrolled = root.querySelector(".portal-tabs")?.scrollLeft ?? 0;
  // The garage's columns scroll on their own, each kept by its name.
  const columnsScrolled = new Map(
    [...root.querySelectorAll<HTMLElement>("[data-scroll]")].map((column) => [column.dataset.scroll, column.scrollTop])
  );
  const panel = node("div", "portal-panel");
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", "Portal");
  panel.append(renderHead(), renderTabs(), renderBody(), renderFoot());
  root.replaceChildren(panel);
  panel.querySelector<HTMLElement>(".portal-body")!.scrollTop = scrolled;
  panel.querySelector<HTMLElement>(".portal-tabs")!.scrollLeft = tabsScrolled;
  for (const column of panel.querySelectorAll<HTMLElement>("[data-scroll]")) {
    column.scrollTop = columnsScrolled.get(column.dataset.scroll) ?? 0;
  }
  panel
    .querySelector(".portal-item.sel, .portal-villain.sel, .portal-car.sel, .portal-blank.sel, .portal-sensei.sel")
    ?.scrollIntoView({ block: "nearest" });
  panel.querySelector(".portal-tab.sel")?.scrollIntoView({ block: "nearest", inline: "nearest" });
}

// ---- notices ----

/// What the menu is doing or has done, shown as a card in the corner of the
/// screen like a Windows notification, large enough to read from a sofa.
/// Work under way and hints stay until something replaces them; a finished
/// job, a problem and a word about why nothing happened go by themselves.
interface Notice {
  kind: "working" | "done" | "problem" | "hint" | "info";
  title: string;
  detail?: string;
  /// The figure it is about, its picture's layers bottom first: a swapper
  /// is two. Left out when Omoio has no picture of a layer.
  picture?: (string | null)[];
  /// A vehicle SuperCharged: done, with the lightning for its mark.
  cheer?: true;
}

const NOTICE_LASTS: Partial<Record<Notice["kind"], number>> = { done: 3500, problem: 8000, info: 5000 };

const NOTICE_SHAPES: Record<Exclude<Notice["kind"], "working">, string> = {
  done: BADGES.on[1],
  problem: `<path d="M12 6v7.5M12 18h.01" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>`,
  hint: BADGES.picked[1],
  info: `<path d="M12 11v6.5M12 6.5h.01" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>`,
};

const card = node("div", "portal-notice");
card.setAttribute("role", "status");
document.body.appendChild(card);
let notice: Notice | null = null;
let noticeTimer = 0;

/// The kind's mark: a spinner while working, a shape otherwise.
function noticeMark(kind: Notice["kind"], cheer = false): HTMLElement {
  if (kind === "working") return node("span", "portal-spinner");
  const mark = node("span", "portal-notice-shape");
  mark.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${cheer ? MARKS.supercharged : NOTICE_SHAPES[kind]}</svg>`;
  return mark;
}

/// Shows a notice in place of the one up now, or with `null` takes it away.
function notify(next: Notice | null) {
  window.clearTimeout(noticeTimer);
  notice = next;
  if (!next) {
    card.classList.remove("shown");
    return;
  }
  const art = node("span", "portal-notice-art");
  const layers = next.picture ?? [];
  if (layers.length > 0 && layers.every(Boolean)) {
    for (const source of layers) art.appendChild(image(source!));
    art.appendChild(node("span", "portal-notice-corner")).appendChild(noticeMark(next.kind, next.cheer));
  } else {
    art.appendChild(noticeMark(next.kind, next.cheer));
  }
  const words = node("span", "portal-notice-words");
  words.append(node("strong", "", next.title));
  if (next.detail) words.append(node("span", "", next.detail));
  card.replaceChildren(art, words);
  card.className = `portal-notice ${next.kind}${next.cheer ? " cheer" : ""} shown`;
  const lasts = NOTICE_LASTS[next.kind];
  if (lasts) noticeTimer = window.setTimeout(() => notify(null), lasts);
}

function problem(err: unknown, otherwise: string): Notice {
  return { kind: "problem", title: typeof err === "string" ? err : otherwise };
}

/// The picture of a figure on the portal, by the name the portal gives it.
function pictureNamed(name: string): (string | null)[] {
  const known = offers.find((offer) => offer.name === name);
  return [pictureOf(known?.id, known?.variant)];
}

function showTab(index: number) {
  if (tabs.length === 0) return;
  tab = (index + tabs.length) % tabs.length;
  at = 0;
  zone = "grid";
  // Leaving a tab drops a picked top, and the notice asking for its bottom.
  if (pickedTop) notify(null);
  pickedTop = null;
  render();
}

function moveGrid(move: Move) {
  const total = tabs[tab]?.entries.length ?? 0;
  const lastRow = Math.floor((total - 1) / COLUMNS);
  if (move === "left" && at % COLUMNS > 0) at -= 1;
  else if (move === "right" && at % COLUMNS < COLUMNS - 1 && at + 1 < total) at += 1;
  else if (move === "down" && at + COLUMNS < total) at += COLUMNS;
  else if (move === "down" && Math.floor(at / COLUMNS) < lastRow) at = total - 1;
  else if (move === "up" && at >= COLUMNS) at -= COLUMNS;
  else if (move === "up" && placed().length > 0) {
    zone = "portal";
    chip = Math.min(at, placed().length - 1);
  }
  render();
}

/// Up and down a column of the tray, across to the next column at the same
/// height or its last villain, and up off the top to the portal row.
function moveTray(move: Move) {
  moveColumns(
    move,
    trayColumns().map((column) => column.villains.length)
  );
}

/// The same through the garage: across goes straight to the next terrain,
/// so a Sea vehicle is one press from a Land one.
function moveGarage(move: Move) {
  moveColumns(
    move,
    garageColumns(tabs[tab]?.entries ?? []).map((column) => column.entries.length)
  );
}

/// Moves through columns of `sizes` things each, counted as one list.
function moveColumns(move: Move, sizes: number[]) {
  let column = 0;
  let row = at;
  while (column < sizes.length - 1 && row >= sizes[column]) row -= sizes[column++];
  const first = (index: number) => sizes.slice(0, index).reduce((sum, size) => sum + size, 0);
  if (move === "left" && column > 0) at = first(column - 1) + Math.min(row, sizes[column - 1] - 1);
  else if (move === "right" && column < sizes.length - 1) at = first(column + 1) + Math.min(row, sizes[column + 1] - 1);
  else if (move === "down" && row + 1 < sizes[column]) at += 1;
  else if (move === "up" && row > 0) at -= 1;
  else if (move === "up" && placed().length > 0) {
    zone = "portal";
    chip = Math.min(column, placed().length - 1);
  }
  render();
}

/// Through the Imaginators tab: across a row, and up or down to the same
/// place in the next row, or its last crystal, and up off the top to the
/// portal row.
function moveRows(move: Move) {
  const sizes = crystalRows(tabs[tab]?.entries ?? []).map((row) => row.entries.length);
  let row = 0;
  let column = at;
  while (row < sizes.length - 1 && column >= sizes[row]) column -= sizes[row++];
  const first = (index: number) => sizes.slice(0, index).reduce((sum, size) => sum + size, 0);
  if (move === "left" && column > 0) at -= 1;
  else if (move === "right" && column < sizes[row] - 1) at += 1;
  else if (move === "down" && row < sizes.length - 1) at = first(row + 1) + Math.min(column, sizes[row + 1] - 1);
  else if (move === "up" && row > 0) at = first(row - 1) + Math.min(column, sizes[row - 1] - 1);
  else if (move === "up" && placed().length > 0) {
    zone = "portal";
    chip = Math.min(column, placed().length - 1);
  }
  render();
}

/// Through the Senseis: up and down a class's column, across to the next
/// class at the same height, and on from the bottom of a column in the first
/// row of classes to the top of the one under it.
function moveDojo(move: Move) {
  const sizes = senseiColumns(tabs[tab]?.entries ?? []).map((column) => column.entries.length);
  let column = 0;
  let row = at;
  while (column < sizes.length - 1 && row >= sizes[column]) row -= sizes[column++];
  const first = (index: number) => sizes.slice(0, index).reduce((sum, size) => sum + size, 0);
  const to = (target: number, height: number) => {
    at = first(target) + Math.min(height, sizes[target] - 1);
  };
  const start = column - (column % BAND);
  const end = Math.min(start + BAND, sizes.length) - 1;
  if (move === "left" && column > start) to(column - 1, row);
  else if (move === "right" && column < end) to(column + 1, row);
  else if (move === "down" && row + 1 < sizes[column]) at += 1;
  else if (move === "down" && start + BAND < sizes.length) to(Math.min(column + BAND, sizes.length - 1), 0);
  else if (move === "up" && row > 0) at -= 1;
  else if (move === "up" && start > 0) to(column - BAND, sizes[column - BAND] - 1);
  else if (move === "up" && placed().length > 0) {
    zone = "portal";
    chip = Math.min(column, placed().length - 1);
  }
  render();
}

function movePortal(move: Move) {
  if (move === "left") chip -= 1;
  else if (move === "right") chip += 1;
  else if (move === "down") zone = "grid";
  render();
}

/// Calls to the emulator's portal under way or waiting their turn. They go
/// one at a time, in the order asked: each works in the emulator's own
/// window, and two at once would close it under each other. The menu can
/// close and open again while one runs.
let pending = 0;
let line: Promise<unknown> = Promise.resolve();
/// What the change under way says, said again if the menu opens again
/// before it is done.
let working: Notice | null = null;
/// Whether something the user asked for is under way. Putting a trap on can
/// take two changes, and a press in between would pick the same free slot.
let acting = false;

function inTurn<T>(call: () => Promise<T>): Promise<T> {
  pending += 1;
  const run = line.then(() => call()).finally(() => {
    pending -= 1;
  });
  line = run.catch(() => undefined);
  return run;
}

/// Does what the user asked for, unless the portal is busy.
async function act(what: () => Promise<unknown>) {
  if (acting || pending > 0) return;
  acting = true;
  try {
    await what();
  } finally {
    acting = false;
  }
}

/// What a change gives back: the portal afterwards, and the file put in a
/// slot when one was.
interface Outcome {
  names: string[];
  slot?: number;
  path?: string;
}

/// One change to the portal, in its turn. Says whether it went through.
/// When it didn't, the portal is read again, since the emulator may have
/// done part of it.
async function change(saying: Notice, job: () => Promise<Outcome>, said: (names: string[]) => Notice): Promise<boolean> {
  working = saying;
  notify(saying);
  let done = false;
  try {
    const outcome = await inTurn(job);
    readPortal(outcome.names);
    if (outcome.slot !== undefined && outcome.path && onPortal[outcome.slot]) {
      slotFiles[outcome.slot] = { path: outcome.path, name: onPortal[outcome.slot] };
    }
    done = true;
    working = null;
    notify(said(onPortal));
  } catch (err) {
    working = null;
    notify(problem(err, "That didn't work. Try again."));
    const names = await inTurn(portalFigures).catch(() => null);
    if (names) readPortal(names);
  }
  mine = await listFigures(true).catch(() => mine);
  buildTabs();
  render();
  return done;
}

/// The first empty slot, or -1 when the portal is full. Before the portal
/// has been read, slot 1.
function freeSlot(): number {
  return onPortal.length === 0 ? 0 : onPortal.indexOf("");
}

function takeOffSlot(slot: number, name: string): Promise<boolean> {
  const picture = pictureNamed(name);
  return change(
    { kind: "working", title: `Taking ${name} off…`, picture },
    async () => ({ names: await portalClear(slot) }),
    () => ({ kind: "done", title: `${name} is off the portal`, picture })
  );
}

/// Takes off the figure picked in the portal row, or the one the selected
/// tile stands for when it is on the portal.
async function takeOff() {
  settle();
  if (zone === "portal") {
    const figure = placed()[chip];
    if (figure) await takeOffSlot(figure.slot, figure.name);
    return;
  }
  if (tabs[tab]?.tray) {
    const villain = trayOrder()[at];
    const slot = villain ? trapSlot(villain) : -1;
    if (slot >= 0) await takeOffSlot(slot, onPortal[slot]);
    return;
  }
  const entry = tabs[tab]?.entries[at];
  if (!entry) return;
  if (!entry.swap) {
    const slot = slotOf(entry);
    if (slot >= 0) await takeOffSlot(slot, onPortal[slot]);
    return;
  }
  for (const name of [entry.swap.top.name, entry.swap.bottom.name]) {
    const slot = onPortal.indexOf(name);
    if (slot >= 0 && !(await takeOffSlot(slot, name))) return;
  }
}

/// Puts one figure on the portal in the first free slot: the saved one when
/// there is one, otherwise a new one the emulator makes. The real portal has
/// one place for a trap, so a trap already on comes off first, as it would
/// by hand; how a game takes two at once is unknown. SuperChargers uses one
/// vehicle at a time, also with two players, one driving and one firing
/// (Wikipedia, Activision's SuperChargers FAQ, question 12, and Co-Optimus'
/// co-op review, read 7 October 2026), and two vehicles on a portal can
/// leave it showing the figure going on for ever (Activision's Gameplay FAQ,
/// question 1). So a vehicle already on comes off first too, and the notice
/// says so. Says whether the figure is on now.
async function putOn(name: string, figure: Figure | undefined, offer: Offer | undefined): Promise<boolean> {
  const kind = offer?.kind ?? figure?.kind;
  if (kind === "trap") {
    for (const other of placed().filter((each) => holds(each.slot, "trap"))) {
      if (!(await takeOffSlot(other.slot, other.name))) return false;
    }
  }
  const leaving = kind === "vehicle" ? vehiclesOn() : [];
  for (const other of leaving) {
    if (!(await takeOffSlot(other.slot, other.name))) return false;
  }
  const slot = freeSlot();
  if (slot < 0) {
    notify({ kind: "problem", title: "The portal is full", detail: "Take a figure off first." });
    return false;
  }
  const picture = [pictureOf(offer?.id ?? figure?.id, offer?.variant ?? figure?.variant)];
  const blank = !figure && kind === "crystal";
  const detail = cameOff(leaving.map((each) => each.name)) ?? (blank ? "The game asks you to make an Imaginator in it." : undefined);
  let done = false;
  if (figure) {
    done = await change(
      { kind: "working", title: `Putting ${name} on the portal…`, picture },
      async () => ({ names: await portalLoad(slot, figure.path), slot, path: figure.path }),
      (names) => ({ kind: "done", title: `${names[slot] || name} is on the portal`, detail, picture })
    );
  } else if (offer) {
    done = await change(
      { kind: "working", title: `Making ${offer.name}…`, detail: "A new figure, kept for next time.", picture },
      async () => ({ ...(await portalCreate(slot, offer)), slot }),
      (names) => ({ kind: "done", title: `${names[slot] || offer.name} is on the portal`, detail, picture })
    );
  }
  return done && Boolean(onPortal[slot]);
}

/// Says which vehicles came off to make way for another.
function cameOff(names: string[]): string | undefined {
  return names.length > 0 ? `${names.join(" and ")} came off. The game uses one vehicle at a time.` : undefined;
}

/// Once the vehicle or SuperCharger `id` has gone on: cheers when that makes
/// a vehicle SuperCharged, and says when the vehicle has nobody on the
/// portal to drive it, which the top face button fixes from its tile. Any
/// other pair on the portal, such as another player's, leaves the notice
/// about this figure as it is.
function tellDriving(id: number, leaving: string[]) {
  const ours = (slot: number) => characterIn(slot)?.id === id;
  const pair = superCharged().find((each) => ours(each.driver) || ours(each.vehicle));
  if (pair) {
    const vehicle = onPortal[pair.vehicle];
    return notify({
      kind: "done",
      cheer: true,
      title: `${vehicle} is SuperCharged with ${onPortal[pair.driver]}`,
      detail: cameOff(leaving),
      picture: pictureNamed(vehicle),
    });
  }
  const vehicle = vehiclesOn().find((each) => ours(each.slot));
  if (!vehicle || driverOn()) return;
  const partner = characterIn(vehicle.slot)?.partner;
  const driver = partner == null ? null : pickFor(partner)?.name;
  const words = [
    leaving.length > 0 ? `${leaving.join(" and ")} came off.` : "",
    "Put a Skylander on with it.",
    driver ? `${nameOf(family, "North")} puts ${driver} on.` : "",
  ];
  notify({
    kind: "hint",
    title: `${vehicle.name} needs a driver`,
    detail: words.filter(Boolean).join(" "),
    picture: pictureNamed(vehicle.name),
  });
}

/// Puts a SuperCharger and its own vehicle on together, so the vehicle is
/// SuperCharged: the driver first, then the vehicle, taking off the vehicle
/// already on. Each is the saved figure when there is one, so a vehicle
/// keeps its mods; one of either already on, whatever its variant, stays.
async function pairUp() {
  settle();
  if (zone !== "grid" || tabs[tab]?.tray) return;
  const pair = pairOf(tabs[tab]?.entries[at]);
  if (!pair) return;
  const driverIn = slotWith(pair.driver.id) >= 0;
  const vehicleIn = slotWith(pair.vehicle.id) >= 0;
  if (driverIn && vehicleIn) return tellDriving(pair.vehicle.id, []);
  const leaving = vehicleIn ? [] : vehiclesOn().map((each) => each.name);
  const needed = Number(!driverIn) + Number(!vehicleIn);
  const free = onPortal.length === 0 ? needed : onPortal.filter((name) => !name).length + leaving.length;
  if (free < needed) {
    return notify({
      kind: "problem",
      title: "The portal is full",
      detail: needed > 1 ? `${pair.driver.name} and ${pair.vehicle.name} need two places. Take a figure off first.` : "Take a figure off first.",
    });
  }
  if (!driverIn && !(await putOn(pair.driver.name, pair.driver.figure, pair.driver.offer))) return;
  if (!vehicleIn && !(await putOn(pair.vehicle.name, pair.vehicle.figure, pair.vehicle.offer))) return;
  tellDriving(pair.vehicle.id, leaving);
}

/// Both halves of a swapper, one after the other, each the saved figure when
/// there is one. The top and bottom may be of different characters.
async function putSwapper(top: Offer, bottom: Offer) {
  const halves = [top, bottom].filter((half) => !onPortal.includes(half.name));
  const free = onPortal.length === 0 ? halves.length : onPortal.filter((name) => !name).length;
  if (free < halves.length) {
    return notify({ kind: "problem", title: "A swapper needs two free places", detail: "Take a figure off first." });
  }
  for (const half of halves) {
    if (!(await putOn(half.name, savedFor(half), half))) return;
  }
  const same = baseName(top.name) === baseName(bottom.name);
  notify({
    kind: "done",
    title: same
      ? `${baseName(top.name)} is on the portal`
      : `${baseName(top.name)} and ${baseName(bottom.name)} are on the portal`,
    picture: [bottom, top].map((half) => pictureOf(half.id, half.variant)),
  });
}

/// Puts the trap that holds the picked villain on the portal, which brings
/// the villain into the game with it. A villain in no trap says why.
async function chooseVillain() {
  const villain = trayOrder()[at];
  if (!villain) return;
  if (!villain.trap) {
    return notify(
      villain.caught
        ? { kind: "info", title: `${villain.name} isn't in one of your traps`, picture: [villainPicture(villain)] }
        : { kind: "info", title: `${villain.name} isn't caught yet`, detail: catchWith(villain) }
    );
  }
  const slot = trapSlot(villain);
  if (slot >= 0) return already(slot);
  await putOn(trapName(villain), trapFigure(villain), undefined);
}

/// Says the figure in `slot` is on the portal already.
function already(slot: number) {
  const name = onPortal[slot];
  notify({ kind: "done", title: `${name} is already on the portal`, picture: pictureNamed(name) });
}

/// Puts the selected figure on the portal: the saved one when there is one,
/// otherwise a new one the emulator makes.
async function choose() {
  settle();
  if (zone === "portal") return takeOff();
  if (tabs[tab]?.tray) return chooseVillain();
  const entry = tabs[tab]?.entries[at];
  if (!entry) return;
  if (entry.swap) {
    if (!pickedTop) {
      const top = entry.swap.top;
      pickedTop = { name: entry.name, top };
      notify({ kind: "hint", title: "Now pick the bottom", detail: `Top: ${entry.name}`, picture: [pictureOf(top.id, top.variant)] });
      return render();
    }
    const top = pickedTop.top;
    pickedTop = null;
    return putSwapper(top, entry.swap.bottom);
  }
  const slot = slotOf(entry);
  if (slot >= 0) return already(slot);
  if (turnedAway(entry)) return explainPatch();
  const id = entry.offer?.id ?? entry.figure?.id;
  const vehicle = (entry.offer?.kind ?? entry.figure?.kind) === "vehicle";
  const leaving = vehicle ? vehiclesOn().map((each) => each.name) : [];
  const done = await putOn(entry.name, entry.figure, entry.offer);
  if (done && id != null && pairsUp() && (vehicle || classOf(entry) === "supercharger")) tellDriving(id, leaving);
}

/// Says why Imaginators would turn away a Sensei or a crystal made now, and
/// what to do about it, before one is made. Without Cemu's packs, gets them.
async function explainPatch() {
  if (made?.check === "not_downloaded") return getPacks();
  const words = patchWords();
  if (words) notify({ kind: "hint", ...words });
}

/// Downloads Cemu's community packs, which bring the Signature Patch, with
/// the share done in the notice. East stops it.
async function getPacks() {
  gettingPacks = true;
  const getting: Notice = { kind: "working", title: "Getting Cemu's packs…", detail: "Imaginators needs the Signature Patch from them." };
  notify(getting);
  render();
  const stopListening = await onCommunityProgress((progress) => {
    if (notice?.kind === "working" && progress.total > 0) {
      notify({ ...getting, detail: `${Math.round((progress.bytes / progress.total) * 100)}% done` });
    }
  }).catch(() => null);
  try {
    await refreshCommunity("wiiu");
    made = await portalMadeFigures().catch(() => made);
    notify({
      kind: "hint",
      title: "Cemu's packs are here",
      detail: "Close the game and start it again, and Imaginators takes the Senseis and crystals made here.",
    });
  } catch (err) {
    notify(problem(err, "Couldn't get Cemu's packs. Try again."));
  } finally {
    stopListening?.();
    gettingPacks = false;
    render();
  }
}

/// The characters come from the emulator's own figure maker the first time
/// only; Omoio keeps the list after that, so later openings are instant.
async function loadOffers() {
  if (offers.length > 0 || asking) return;
  asking = true;
  const getting: Notice = { kind: "working", title: "Getting the characters…" };
  // While the page still says it is loading, that says enough.
  if (ready) notify(getting);
  render();
  try {
    offers = await inTurn(figureCharacters);
    if (notice === getting) notify(null);
  } catch (err) {
    notify(problem(err, "Couldn't get the characters."));
  } finally {
    asking = false;
    buildTabs();
    // With nothing saved yet, open on the first element, not an empty tab.
    // In SuperChargers that isn't the garage, the second tab there: a
    // vehicle needs a Skylander on the portal to drive it (Stevivor's review
    // of the game, read 7 October 2026), so a Skylander comes first. Nothing
    // found says what Imaginators asks for first, a Skylander, a Sensei or a
    // crystal, so it opens there too.
    const firstElement = tabs.findIndex((each) => each.element);
    const elementFirst = game === "superchargers" || game === "imaginators";
    if (mine.length === 0 && tab === 0 && tabs.length > 1) tab = elementFirst && firstElement > 0 ? firstElement : 1;
    render();
  }
}

/// Reads what is on the portal, after any change under way. Usually quick,
/// so the notice only comes up when the emulator takes its time. The saved
/// figures and the villains show at once, and again once the portal is
/// read, in case a change under way made a figure meanwhile.
async function refresh() {
  const looking: Notice = { kind: "working", title: "Reading the portal…" };
  const slow = window.setTimeout(() => ready && !working && notify(looking), 400);
  const reading = inTurn(portalFigures).catch((err: unknown) => {
    notify(problem(err, "Couldn't read the portal."));
    return null;
  });
  // A villain caught since the menu was last open stands out until the next time.
  const before = caughtBefore;
  await takeLists(before);
  const names = await reading;
  window.clearTimeout(slow);
  if (names) {
    readPortal(names);
    if (notice === looking) notify(null);
  }
  await takeLists(before);
}

/// Lists the saved figures and the villains, marking those caught since
/// `before`.
async function takeLists(before: Set<number> | null) {
  const [files, found] = await Promise.all([listFigures(true), listVillains().catch(() => [] as Villain[])]);
  mine = files;
  villainList = found;
  const caught = new Set(found.filter((villain) => villain.caught).map((villain) => villain.id));
  caughtNew = before ? new Set([...caught].filter((id) => !before.has(id))) : new Set();
  caughtBefore = caught;
  buildTabs();
  render();
}

// ---- the pad ----

let held = new Set<string>();
const downSince = new Map<string, number>();
let lastRepeat = 0;
let reading = false;

async function readPads(): Promise<Set<string>> {
  try {
    return new Set(await padsHeld());
  } catch {
    return new Set();
  }
}

function press(input: string) {
  const move = MOVES[input];
  if (move) {
    if (zone === "portal") return movePortal(move);
    const current = tabs[tab];
    if (current?.tray) return moveTray(move);
    if (current?.garage) return moveGarage(move);
    if (current?.crystals) return moveRows(move);
    if (current?.senseis) return moveDojo(move);
    return moveGrid(move);
  }
  if (input === "LB") return showTab(tab - 1);
  if (input === "RB") return showTab(tab + 1);
  if (input === "East") {
    if (gettingPacks) return void cancelCommunity();
    if (!pickedTop) return void closePortalMenu();
    pickedTop = null;
    notify(null);
    return render();
  }
  if (input === "South") void act(choose);
  else if (input === "West") void act(takeOff);
  else if (input === "North") void act(pairUp);
}

window.setInterval(async () => {
  if (!shown || reading) return;
  reading = true;
  try {
    const now = await readPads();
    const time = Date.now();
    for (const input of now) {
      if (!held.has(input)) {
        downSince.set(input, time);
        press(input);
      } else if (
        MOVES[input] &&
        time - (downSince.get(input) ?? time) > REPEAT_AFTER &&
        time - lastRepeat > REPEAT_EVERY
      ) {
        lastRepeat = time;
        press(input);
      }
    }
    held = now;
  } finally {
    reading = false;
  }
}, 50);

document.addEventListener("keydown", (event) => {
  const keys: Record<string, () => void> = {
    ArrowUp: () => press("Up"),
    ArrowDown: () => press("Down"),
    ArrowLeft: () => press("Left"),
    ArrowRight: () => press("Right"),
    PageUp: () => press("LB"),
    PageDown: () => press("RB"),
    Enter: () => press("South"),
    Delete: () => press("West"),
    Backspace: () => press("West"),
    Escape: () => press("East"),
    // P for pair: a SuperCharger and its vehicle together.
    p: () => press("North"),
    P: () => press("North"),
  };
  const act = keys[event.key];
  if (act) {
    event.preventDefault();
    // A key held down repeats; only the arrows should, as the pad's
    // directions do, so a held Enter never makes a second figure.
    if (event.repeat && !event.key.startsWith("Arrow")) return;
    act();
  }
});

/// Shown again: what is already held down, such as the button that opened
/// the menu, is not taken as a press, and a change still under way says so.
async function show() {
  notify(working);
  try {
    family = (await portalMenuFamily()) as PadFamily;
    game = await portalGame().catch(() => null);
    made = await portalMadeFigures().catch(() => null);
    held = await readPads();
    zone = "grid";
    shown = true;
    // Read again each time, since pictures can be got while the game runs.
    pictures = await figurePictures()
      .then((found) => ({
        folder: found.folder,
        names: new Set(found.names),
        firstOf: firstPictures(found.names),
        elsewhere: found.elsewhere,
      }))
      .catch(() => null);
    await refresh();
    await loadOffers();
  } finally {
    // Filled once, the menu draws itself from then on, a problem or not.
    if (!ready) {
      ready = true;
      render();
    }
  }
}

void onPortalMenu((state) => {
  family = state.family as PadFamily;
  if (state.open) void show();
  else shown = false;
});
void show();
