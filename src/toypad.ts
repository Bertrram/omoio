import "./styles/tokens.css";
import "./styles/portal.css";
import "./styles/toys.css";
import {
  closePortalMenu,
  deleteToyFigure,
  onPortalMenu,
  portalMenuFamily,
  portalMenuNote,
  toyCreate,
  toyFiles,
  toyGame,
  toyLoad,
  toyOffers,
  toyPlaces,
  toypadLights,
  toypadMove,
  toyRemove,
  type PadLight,
  type ToyFile,
  type ToyGame,
  type ToyOffer,
} from "./api";
import { node, svg } from "./menu/dom";
import { bumper, drive, hintRow, HOLD, keyHeld, MOVES, named, openedBy, skipHeld, type Move } from "./menu/input";
import { notice, notify, problem, type Notice } from "./menu/notices";
import { drawPiece, pictureOf, readPictures } from "./menu/pictures";
import { attachRename, pressRename, renaming, startRename, stopRename } from "./menu/rename";
import { act, acting, inTurn } from "./menu/turns";

/// The LEGO Dimensions Toy Pad menu, drawn by Omoio over the running game in
/// the window the portal menu uses, and used with the pad alone. The Toy Pad
/// is on the left as the real one is laid out: the round centre pad with one
/// place, and a pad on each side of it with three, each drawn in the colour
/// the game last lit it. The shoulder buttons go from place to place, and the
/// list beside the Toy Pad offers what can go on the place picked: the
/// player's own tags first, the ones used lately first, then what the
/// emulator can make. The d-pad or left stick moves, the bottom face button
/// puts a tag on, the left one takes off what is on the place, the top one
/// picks a tag up to move it to another place, or held deletes one of the
/// player's tags, and the right one closes. Mouse and keyboard work as well,
/// with the same keys as the portal menu.
///
/// Omoio never reads a tag file: one it made it knows from its own record,
/// and one the player brought only by the name the emulator gives it once it
/// is on the Toy Pad. It makes minifigures and Blank Tags, never a vehicle
/// from its number: the game writes a vehicle or gadget to a Blank Tag when
/// the player builds it. A tag shows its own picture where Omoio has read the
/// game's (src/menu/pictures.ts).

/// How many tiles sit side by side, which is also how far up or down moves.
const COLUMNS = 4;

type Piece = "minifigure" | "vehicle" | "blank_tag";

/// What the menu says about the one line a Blank Tag needs: the game writes
/// the vehicle or gadget the player builds to the tag on the centre pad (the
/// words of Cemu's pull request 1371 that added the Toy Pad, "a 'Blank Tag'
/// which can be used to store gadgets from the game", merged 11 November
/// 2024; players' reports of the game's "Write to tag" when the tag is in the
/// middle), read 4 November 2026.
const BLANK_LINE = "The game writes a vehicle or gadget to a Blank Tag on the centre pad when you build it.";

/// Omoio's own drawing of each kind of tag until it has pictures: a
/// minifigure, a car for the vehicles and gadgets, and a Blank Tag as an
/// empty tag waiting for one.
const PIECES: Record<Piece, { words: string; shape: string }> = {
  minifigure: {
    words: "Minifigure",
    shape: `<rect x="10.2" y="1.2" width="3.6" height="1.9" rx=".6" fill="currentColor"/><rect x="8.2" y="2.9" width="7.6" height="6.2" rx="2.3" fill="currentColor"/><path d="M7.4 10.1h9.2l1.7 6.3H5.7z" fill="currentColor"/><path d="M6.6 17.1h10.8v5.4h-4.6v-3h-1.6v3H6.6z" fill="currentColor"/>`,
  },
  vehicle: {
    words: "Vehicle or gadget",
    shape: `<path fill-rule="evenodd" d="M2.5 16.2v-3.6c0-.5.3-.9.8-1l2.9-.7 2.4-4c.2-.3.5-.5.9-.5h5.9c.3 0 .6.1.8.4l3.1 4.1 1.6.3c.5.1.8.5.8 1v4zM9.9 8.3l-1.7 2.9h3.6V8.3zm3.6 0v2.9h4.2l-2.2-2.9z" fill="currentColor"/><circle cx="7.2" cy="17.4" r="2.5" fill="currentColor"/><circle cx="16.8" cy="17.4" r="2.5" fill="currentColor"/>`,
  },
  blank_tag: {
    words: "For a vehicle or gadget",
    shape: `<circle cx="12" cy="12" r="8.6" fill="none" stroke="currentColor" stroke-width="2" stroke-dasharray="3.4 2.4"/><path d="M12 8.2v7.6M8.2 12h7.6" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/>`,
  },
};

/// A tag the player brought, which Omoio doesn't know until it is on.
const UNKNOWN = `<rect x="3.5" y="3.5" width="17" height="17" rx="3.5" fill="none" stroke="currentColor" stroke-width="2"/><path d="M9.6 9.4a2.5 2.5 0 1 1 3.6 2.2c-.8.4-1.2 1-1.2 1.9v.5M12 17h.01" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>`;

/// An arrow down into an empty place, where a tag held can be put down.
const DROP = `<path d="M12 4.5v11M7 11l5 5 5-5M6 20h12" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>`;

const BADGE_ON = `<path d="m5 12.5 4.5 4.5L19 7.5" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`;

type PadNumber = 1 | 2 | 3;

/// The three pads as the game numbers them, 1 the centre, 2 the left and 3
/// the right, and the places on each, as both emulators lay them out: Cemu's
/// Dimensions page (src/gui/EmulatedUSBDevices/EmulatedUSBDeviceFrame.cpp
/// lines 151 to 161, Omoio's build at v2.6-2) and RPCS3's Dimensions Manager
/// and Figure Mover (rpcs3/rpcs3qt/dimensions_dialog.cpp lines 551 to 553
/// and 585 to 595, master b2e52d6), read 4 November 2026. Places 0, 3 and 4
/// are on the left pad, 1 on the centre pad, and 2, 5 and 6 on the right
/// pad; each side pad's first place is drawn above its other two, as the
/// windows have them.
const PADS: Record<PadNumber, { name: string; where: string; to: string; spots: number[]; side: string }> = {
  1: { name: "Centre pad", where: "on the centre pad", to: "to the centre pad", spots: [1], side: "centre" },
  2: { name: "Left pad", where: "on the left pad", to: "to the left pad", spots: [0, 3, 4], side: "left" },
  3: { name: "Right pad", where: "on the right pad", to: "to the right pad", spots: [2, 5, 6], side: "right" },
};

/// Each place's pad and name, by the emulators' numbers.
const SPOTS: { pad: PadNumber; name: string }[] = [
  { pad: 2, name: "Left pad, first place" },
  { pad: 1, name: "Centre pad" },
  { pad: 3, name: "Right pad, first place" },
  { pad: 2, name: "Left pad, second place" },
  { pad: 2, name: "Left pad, third place" },
  { pad: 3, name: "Right pad, second place" },
  { pad: 3, name: "Right pad, third place" },
];

/// The order the shoulder buttons go through the places: left to right
/// across the Toy Pad, one pad at a time.
const ORDER = [0, 3, 4, 1, 2, 5, 6];

/// Where each direction goes from a place while a tag is held to be moved,
/// on the Toy Pad as it is drawn.
const NEIGHBOURS: Record<number, Partial<Record<Move, number>>> = {
  0: { right: 1, down: 3 },
  3: { up: 0, right: 4 },
  4: { up: 0, left: 3, right: 1 },
  1: { left: 0, right: 2 },
  2: { left: 1, down: 5 },
  5: { up: 2, left: 1, right: 6 },
  6: { up: 2, left: 5 },
};

/// How long one of the game's light ticks is drawn. No document gives a
/// tick's length; the one measurement found, from a capture of the game (a
/// Fade All with a tick time of 30 sent again about 1.2 s later), comes to
/// about 40 ms, so a real Toy Pad has to confirm it.
const TICK = 40;

/// No pad is drawn flashing or pulsing more than three times a second,
/// whatever the game asks (WCAG 2.2, success criterion 2.3.1, Three Flashes
/// or Below Threshold, W3C Recommendation 12 December 2024, read 4 November
/// 2026). A whole pad is a large area on a TV.
const LEAST_CYCLE = 1000 / 3;

/// How often the pads' colours are asked while the menu is up.
const LIGHTS_EVERY = 250;

/// One tile: one of the player's tags, or one the emulator can make.
interface Entry {
  name: string;
  piece: Piece | null;
  /// What the emulator calls the tag, where that isn't its file's name.
  holds?: string;
  file?: ToyFile;
  offer?: ToyOffer;
}

/// A group in the list, under its heading.
interface Group {
  title: string;
  note?: string;
  entries: Entry[];
  /// What the group says while it has nothing.
  quiet?: string;
}

const root = document.getElementById("portal")!;
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

let playing: ToyGame | null = null;
let shown = false;
/// Whether the menu has been filled once. Until then the page keeps the
/// "Loading the Toy Pad" it was written with.
let ready = false;
/// Over a game the menu isn't offered in, the line the game's page says it
/// with, shown in place of the menu (portal_menu::note in
/// src-tauri/src/portal_menu.rs). `null` where the menu works.
let unavailable: string | null = null;
/// What each place holds, by the name the emulator gives it.
let places: string[] = [];
/// The file Omoio put on each place, with the name the emulator gave it
/// then. A tag on the Toy Pad is known by its file: two files can be of one
/// figure, and the emulator names a file the player brought by its figure.
let placeFiles: ({ path: string; name: string } | null)[] = [];
/// Why the Toy Pad couldn't be read, in the backend's words, until it is.
let unread: string | null = null;
let files: ToyFile[] = [];
let offers: ToyOffer[] = [];
let asking = false;
/// The place picked, which the list offers for. The centre pad first, the
/// one in the middle of the Toy Pad.
let place = 1;
let at = 0;
let groups: Group[] = [];
/// What the change under way says, said again if the menu opens again
/// before it is done.
let working: Notice | null = null;
/// A tag picked up to be moved to another place: where it was, and what it
/// is called. Nothing is sent to the emulator until it is put down.
let lifted: { from: number; name: string } | null = null;

function pieceOf(kind: string | null | undefined): Piece | null {
  return kind && kind in PIECES ? (kind as Piece) : null;
}

/// A tag's kind by its number, as the emulators' two lists are numbered:
/// the Blank Tag is 0, the minifigures run on from 1, and the vehicles and
/// gadgets start at 1000 (Cemu's s_listMinis and s_listTokens,
/// src/Cafe/OS/libs/nsyshid/Dimensions.cpp lines 25 and 114, v2.6-2; RPCS3's
/// rpcs3/rpcs3qt/dimensions_dialog.cpp lines 23 to 111 and 112 to 378,
/// master b2e52d6; read 4 November 2026).
function pieceOfNumber(number: number): Piece {
  if (number === 0) return "blank_tag";
  return number < 1000 ? "minifigure" : "vehicle";
}

function offerPiece(offer: ToyOffer): Piece {
  return pieceOf(offer.kind) ?? pieceOfNumber(offer.number);
}

function filePiece(file: ToyFile): Piece | null {
  return pieceOf(file.kind) ?? (file.figure ? pieceOfNumber(file.figure.number) : null);
}

function isMinifigure(name: string): boolean {
  return offers.some((offer) => offer.name === name && offerPiece(offer) === "minifigure");
}

function isBlank(name: string): boolean {
  return offers.some((offer) => offer.name === name && offerPiece(offer) === "blank_tag");
}

/// A tag's name in a sentence: a Blank Tag is one of many, so it is "the
/// Blank Tag", where a minifigure or a vehicle goes by its own name.
function the(name: string): string {
  return isBlank(name) ? `the ${name}` : name;
}

function capital(words: string): string {
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/// What the emulator calls one of the player's tags, where its file's name
/// doesn't say: a Blank Tag the game has written a vehicle to, or a file the
/// player brought and named themselves. "Batman 2" says it is Batman.
function holds(file: ToyFile): string | undefined {
  const name = file.figure?.name;
  return name && !file.name.toLowerCase().includes(name.trim().toLowerCase()) ? name : undefined;
}

/// What the list offers for the place picked: the player's tags, in the
/// order the backend gives them, used lately first; then the Blank Tag, one
/// tile ahead of the eighty-odd minifigures, so a player after a vehicle
/// finds it at once; then the minifigures by name. A minifigure the player
/// has a tag of is offered as that tag only. A Blank Tag is always offered,
/// since each becomes the vehicle or gadget built on it. A vehicle is never
/// made from its number: both emulators' makers would encrypt its number as
/// a character's (Cemu's CreateFigure, Dimensions.cpp lines 707 to 749;
/// RPCS3's create_blank_character, rpcs3/Emu/Io/Dimensions.cpp lines 496 to
/// 529), which the game most likely refuses.
function buildGroups() {
  const fits = (spots: number[]) => spots.includes(place);
  const yours = files.filter((file) => fits(file.places));
  groups = [
    {
      title: "Yours",
      entries: yours.map(yourEntry),
      quiet: yours.length > 0 ? undefined : files.length > 0 ? "None of yours go here." : "Nothing saved yet. A tag made here is kept for next time.",
    },
  ];
  const kept = files.flatMap((file) => (file.figure && filePiece(file) === "minifigure" ? [file.figure.number] : []));
  const offered = (piece: Piece) => offers.filter((offer) => fits(offer.places) && offerPiece(offer) === piece);
  const blanks = offered("blank_tag");
  if (blanks.length > 0) {
    groups.push({
      title: "Vehicles and gadgets",
      note: BLANK_LINE,
      entries: blanks.map((offer) => ({ name: offer.name, piece: "blank_tag", offer })),
    });
  }
  const minifigures = offered("minifigure")
    .filter((offer) => !kept.includes(offer.number))
    .sort((one, other) => one.name.localeCompare(other.name));
  if (minifigures.length > 0) {
    groups.push({ title: "Minifigures", entries: minifigures.map((offer) => ({ name: offer.name, piece: "minifigure", offer })) });
  }
  if (asking && offers.length === 0) groups.push({ title: "", entries: [], quiet: "Getting the figures…" });
}

/// One of the player's tags. A Blank Tag the game has written a vehicle or
/// gadget to is called by what it holds, with its file's name under it,
/// until the player names it themselves.
function yourEntry(file: ToyFile): Entry {
  const piece = filePiece(file);
  if (piece === "vehicle" && file.figure && !file.named) {
    return { name: file.figure.name, piece, holds: file.name, file };
  }
  return { name: file.name, piece, holds: holds(file), file };
}

function entries(): Entry[] {
  return groups.flatMap((each) => each.entries);
}

function selected(): Entry | undefined {
  return entries()[at];
}

/// The list's rows, each group starting a row of its own, as moving through
/// it counts them.
function rowSizes(): number[] {
  return groups.flatMap((each) => {
    const sizes: number[] = [];
    for (let first = 0; first < each.entries.length; first += COLUMNS) sizes.push(Math.min(COLUMNS, each.entries.length - first));
    return sizes;
  });
}

/// Keeps the selection on something that is there.
function settle() {
  at = Math.max(0, Math.min(at, entries().length - 1));
}

/// Takes in the Toy Pad as it was read. A place that has emptied no longer
/// holds the file Omoio put there. One whose name has changed still does
/// when the change is the game's: it writes the vehicle or gadget the player
/// builds to a Blank Tag, and a rebuilt one's next build to the same tag,
/// and the emulator names the tag by what was written (Cemu's WriteBlock,
/// Dimensions.cpp lines 1050 to 1078, v2.6-2). A minifigure's name never
/// changes that way, so a place whose minifigure is another holds another
/// tag.
function readBase(names: string[]) {
  places = names;
  placeFiles = names.map((name, spot) => {
    const known = placeFiles[spot];
    if (!name || !known) return null;
    if (name === known.name) return known;
    return !isMinifigure(known.name) && !isMinifigure(name) ? { path: known.path, name } : null;
  });
  unread = null;
}

/// A move took the tag at `from` to `to`, by the places read afterwards: the
/// file Omoio knows there goes with it.
function followMove(moved: { from: number; to: number }, names: string[]) {
  if (moved.from === moved.to || names[moved.from] || !names[moved.to]) return;
  placeFiles[moved.to] = placeFiles[moved.from] ?? null;
  placeFiles[moved.from] = null;
}

/// The place holding what a tile stands for, or -1: its file when Omoio
/// put that on, or a minifigure the emulator names as it, on a place whose
/// file Omoio doesn't know. Only a minifigure's name says which tag it is:
/// every Blank Tag has the same one, and a vehicle's changes as it is
/// rebuilt.
function placeOf(entry: Entry): number {
  if (entry.file) {
    const byFile = placeFiles.findIndex((file) => file?.path === entry.file!.path);
    if (byFile >= 0) return byFile;
    const name = entry.piece === "minifigure" ? entry.file.figure?.name : undefined;
    return name ? places.findIndex((held, spot) => held === name && !placeFiles[spot]) : -1;
  }
  if (entry.offer && entry.piece === "minifigure") return places.indexOf(entry.offer.name);
  return -1;
}

/// What is on a place, as far as Omoio can tell: by the emulator's name for
/// it among the minifigures and the Blank Tag, as a vehicle or gadget when
/// it isn't one of those, or by its file until the list is read.
function pieceOn(spot: number): Piece | null {
  const name = places[spot];
  if (!name) return null;
  const offer = offers.find((each) => each.name === name);
  if (offer) return offerPiece(offer);
  if (offers.length > 0) return "vehicle";
  const kept = placeFiles[spot] && files.find((file) => file.path === placeFiles[spot]!.path);
  return kept ? filePiece(kept) : null;
}

/// The number of what is on a place, for its picture: its file's figure
/// when Omoio put that on and knows what the game has written to it, or the
/// minifigure the emulator names it as, or one of the player's tags of that
/// name. Only a tag's own record knows a vehicle's number, since Omoio
/// makes none. `null` for a tag Omoio doesn't know.
function numberOn(spot: number): number | null {
  const name = places[spot];
  if (!name) return null;
  const kept = placeFiles[spot] && files.find((file) => file.path === placeFiles[spot]!.path)?.figure;
  if (kept?.name === name) return kept.number;
  return offers.find((offer) => offer.name === name)?.number ?? files.find((file) => file.figure?.name === name)?.figure?.number ?? null;
}

/// The number of what a tile stands for, for its picture: for a Blank Tag
/// the game has built on, the vehicle or gadget on it.
function numberOf(entry: Entry): number | null {
  return entry.offer?.number ?? entry.file?.figure?.number ?? null;
}

function tint(piece: Piece | null): string {
  return `toy-${piece ?? "unknown"}`;
}

function shapeOf(piece: Piece | null): string {
  return svg(piece ? PIECES[piece].shape : UNKNOWN);
}

/// Whether the tag held to be moved can be put down on a place: an empty
/// one, or its own, where it is lifted and put down again, as some puzzles
/// ask. Never on another tag: the emulators would take that one off without
/// a word (Cemu's MoveFigure, Dimensions.cpp lines 751 to 773; RPCS3's
/// move_figure, rpcs3/Emu/Io/Dimensions.cpp lines 470 to 494).
function canGo(spot: number): boolean {
  return Boolean(lifted) && (spot === lifted!.from || !places[spot]);
}

// ---- the lights ----

/// A pad's light as the menu saw it come: the light, and when, on the
/// page's clock, so a fade or a flash drawn again keeps its time.
interface Seen {
  light: PadLight;
  at: number;
}

/// The pads' lights, centre, left and right, as last read.
let seen: (Seen | null)[] = [null, null, null];
/// What the emulator last said of the lights: `undefined` before it was
/// asked, `null` where it can't tell them.
let lightsAnswer: PadLight[] | null | undefined;
/// Whether the next read is the first since the menu was shown. A light that
/// changed while the menu was hidden is drawn as it is now, not as it came.
let firstRead = true;
let readingLights = false;
let lightsTimer = 0;

/// A colour the game sent, as CSS. Black is the game turning a pad off.
function colour([red, green, blue]: number[]): string | null {
  return red + green + blue === 0 ? null : `rgb(${red} ${green} ${blue})`;
}

/// How long one step of a fade takes, and one whole flash, on and off.
function fadeStep(light: PadLight): number {
  return Math.max(light.time * TICK, LEAST_CYCLE / 2);
}

function flashCycle(light: PadLight): number {
  return Math.max((light.time + light.off) * TICK, LEAST_CYCLE);
}

/// What a pad shows once it is still: its new colour, or the one it had
/// before. A fade goes from the old colour to the new one and back, count
/// steps, so an odd count ends on the new colour and an even one on the
/// old, and a count of 0 never stops; a flash of 0xFF never stops (the
/// commands as Ellerbach/LegoDimensions documents them,
/// LegoDimensionsProtocol.md lines 178 to 193 and 216 to 232, commit
/// 58d1f12, read 4 November 2026). What a pad shows once a flash ends isn't
/// documented; the colour before it is the guess. Something that never
/// stops is drawn still in its new colour.
function stillOn(light: PadLight): "lit" | "from" | null {
  switch (light.mode) {
    case "solid":
      return "lit";
    case "fade":
      return light.count === 0 || light.count % 2 === 1 ? "lit" : "from";
    case "flash":
      return light.count === 255 ? "lit" : "from";
    default:
      return null;
  }
}

/// The word under a pad for what it is doing, the only sign of a flash or a
/// pulse where motion is reduced.
function lightWords(light: PadLight | undefined): string {
  if (!light) return "";
  if (light.mode === "random") return "Changing colours";
  if (light.mode === "fade" && light.count === 0) return "Pulsing";
  if (light.mode === "flash" && light.count === 255) return "Flashing";
  const still = stillOn(light);
  return still && colour(still === "lit" ? light.color : light.from) ? "Lit" : "";
}

/// Draws one pad in the light the game last asked of it, over the pad's own
/// unlit surface: the colour before the command under the new one, and the
/// new one fading or flashing over it. A fade or flash that ends is drawn
/// running as well, then still in the colour it ends on, since the game
/// pulses a pad by sending short ones again and again: the one capture of
/// the game found has a Fade All sent again about 1.2 s after the last (see
/// `TICK`), and the Locate keystone gives its hint only on the pad, pulsing
/// green at various rates (RPCS3 issue 16600, read 4 November 2026).
/// With motion reduced, only the colour it settles on, or its new one where
/// it never settles. Drawn again only when the light is new, so a fade isn't
/// started over.
function paintPad(shape: HTMLElement) {
  const pad = Number(shape.dataset.pad) as PadNumber;
  const now = seen[pad - 1];
  const still = reducedMotion.matches;
  const key = now ? `${now.light.changes} ${now.light.mode} ${still}` : `none ${still}`;
  if (shape.dataset.painted === key) return;
  shape.dataset.painted = key;
  const under = shape.querySelector<HTMLElement>(".toypad-glow.from")!;
  const over = shape.querySelector<HTMLElement>(".toypad-glow.lit")!;
  for (const layer of [under, over]) {
    for (const running of layer.getAnimations()) running.cancel();
    layer.style.opacity = "0";
  }
  over.classList.remove("random", "still");
  const light = now?.light;
  const lit = light ? colour(light.color) : null;
  const from = light ? colour(light.from) : null;
  for (const [name, value] of [["--lit", lit], ["--from", from]] as const) {
    if (value) shape.style.setProperty(name, value);
    else shape.style.removeProperty(name);
  }
  const caption = shape.parentElement?.querySelector(".toypad-pad-state");
  if (caption) caption.textContent = lightWords(light);
  shape.parentElement?.setAttribute("aria-label", `${PADS[pad].name}${lightWords(light) ? `, ${lightWords(light).toLowerCase()}` : ", unlit"}`);
  if (!light || !now) return;
  const timed = (animation: Animation) => {
    animation.startTime = now.at;
  };
  if (light.mode === "random") {
    // The pad picks the colours itself and the game never says which
    // (Ellerbach/LegoDimensions, LegoDimensionsProtocol.md lines 255 to 271),
    // so it is drawn changing through every hue, or, still, lit with none.
    over.classList.add("random");
    over.style.opacity = "1";
    if (still) over.classList.add("still");
    else timed(over.animate([{ filter: "hue-rotate(0deg)" }, { filter: "hue-rotate(360deg)" }], { duration: fadeStep(light) * 6, iterations: Infinity }));
    return;
  }
  const settles = stillOn(light);
  const forever = (light.mode === "fade" && light.count === 0) || (light.mode === "flash" && light.count === 255);
  const moving = !still && (light.mode === "fade" || light.mode === "flash") && (forever || light.count > 0);
  if (!moving) {
    if (settles === "lit") over.style.opacity = "1";
    if (settles === "from") under.style.opacity = "1";
    return;
  }
  under.style.opacity = "1";
  const iterations = forever ? Infinity : light.count;
  if (light.mode === "fade") {
    // Evenly, tick by tick, there and back.
    timed(over.animate([{ opacity: 0 }, { opacity: 1 }], { duration: fadeStep(light), iterations, direction: "alternate", fill: "forwards" }));
    return;
  }
  const on = light.time + light.off === 0 ? 1 : light.time / (light.time + light.off);
  timed(
    over.animate(
      [
        { opacity: 1, offset: 0 },
        { opacity: 1, offset: on },
        { opacity: 0, offset: on },
        { opacity: 0, offset: 1 },
      ],
      { duration: flashCycle(light), iterations, fill: "forwards" }
    )
  );
}

function paintLights() {
  for (const shape of root.querySelectorAll<HTMLElement>(".toypad-pad")) paintPad(shape);
}

/// Asks the emulator for the pads' lights. A light is new when the game has
/// changed it since it was last seen (`changes` goes up each time). The PS3
/// answers nothing: RPCS3 answers the game's light commands and keeps none
/// of them (rpcs3/Emu/Io/Dimensions.cpp lines 647 to 659, master b2e52d6).
async function readLights() {
  if (!shown || unavailable || readingLights) return;
  readingLights = true;
  try {
    const read = await toypadLights().catch(() => null);
    if (!shown) return;
    const before = lightsAnswer;
    lightsAnswer = read && read.length === 3 ? read : null;
    const now = performance.now();
    seen = seen.map((was, index) => {
      const light = lightsAnswer?.[index];
      if (!light) return null;
      if (was && was.light.changes === light.changes && was.light.mode === light.mode) return was;
      // Long done, for one that came while the menu wasn't looking.
      return { light, at: firstRead ? now - 864e5 : now };
    });
    firstRead = false;
    if ((before == null) !== (lightsAnswer == null)) render();
    else paintLights();
  } finally {
    readingLights = false;
  }
}

/// Reads the lights about four times a second while the menu is up, and
/// not at all while it is hidden.
function watchLights(on: boolean) {
  window.clearInterval(lightsTimer);
  lightsTimer = 0;
  if (!on) return;
  firstRead = true;
  void readLights();
  lightsTimer = window.setInterval(() => void readLights(), LIGHTS_EVERY);
}

reducedMotion.addEventListener("change", paintLights);

// ---- drawing ----

function renderHead(): HTMLElement {
  const head = node("div", "portal-head");
  head.appendChild(node("div", "portal-title", "Toy Pad"));
  if (playing?.toy === "toypad") head.appendChild(node("div", "toy-game", "LEGO Dimensions"));
  if (unread) head.appendChild(node("div", "toy-unread", unread));
  return head;
}

/// One place on the Toy Pad. A click picks it, on pointer-down, and keeps
/// the focus where it is, as the tab bar's ends do. A click on the place
/// already picked picks its tag up to move it, and while one is held a click
/// on a place puts it down there.
function renderSpot(spot: number): HTMLElement {
  const name = places[spot] ?? "";
  const classes = ["toypad-spot"];
  if (spot === place) classes.push("sel");
  if (name) classes.push("on");
  if (lifted) {
    if (canGo(spot)) classes.push("target");
    else classes.push("blocked");
    if (spot === lifted.from) classes.push("from");
  }
  // The tag held, drawn faintly on the empty place picked, where it would go.
  const ghost = lifted && spot === place && !name ? lifted : null;
  if (ghost) classes.push("ghost");
  const button = node("button", classes.join(" "));
  button.tabIndex = -1;
  button.setAttribute("aria-label", `${SPOTS[spot].name}: ${name || "empty"}`);
  if (spot === place) button.setAttribute("aria-current", "true");
  const piece = ghost ? pieceOn(ghost.from) : name ? pieceOn(spot) : null;
  const tag = node("span", `toypad-tag${name || ghost ? ` ${tint(piece)}` : ""}`);
  if (name || ghost) drawPiece(tag, pictureOf(numberOn(ghost ? ghost.from : spot)), shapeOf(piece));
  // An empty place the tag can go to says so by its shape as well as its
  // ring, since the game may light the pad in the ring's own colour.
  else if (lifted && canGo(spot)) tag.innerHTML = svg(DROP);
  const label = ghost ? ghost.name : name || "Empty";
  button.append(tag, node("span", name ? "toypad-spot-name" : "toypad-spot-name empty", label));
  button.onpointerdown = (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    if (lifted) return void act(() => putDown(spot));
    if (spot === place && places[spot]) return lift();
    showPlace(spot);
  };
  return button;
}

/// One pad: its surface, with the game's light over it in two layers, the
/// colour before and the colour now, its places, and its name with what its
/// light is doing under it.
function renderPad(pad: PadNumber): HTMLElement {
  const side = node("div", `toypad-side ${PADS[pad].side}`);
  side.setAttribute("role", "group");
  const shape = node("div", "toypad-pad");
  shape.dataset.pad = String(pad);
  shape.append(node("span", "toypad-glow from"), node("span", "toypad-glow lit"));
  const spots = node("div", "toypad-spots");
  for (const spot of PADS[pad].spots) spots.appendChild(renderSpot(spot));
  shape.appendChild(spots);
  const caption = node("div", "toypad-caption");
  caption.append(node("span", "toypad-pad-name", PADS[pad].name), node("span", "toypad-pad-state"));
  side.append(shape, caption);
  return side;
}

function renderToyPad(): HTMLElement {
  const toyPad = node("div", "toypad");
  toyPad.setAttribute("role", "group");
  toyPad.setAttribute("aria-label", "The Toy Pad");
  const deck = node("div", "toypad-deck");
  deck.append(renderPad(2), renderPad(1), renderPad(3));
  toyPad.appendChild(deck);
  // Until the PS3 says otherwise, which it doesn't (`readLights`).
  if (playing?.console === "ps3" && !lightsAnswer) {
    toyPad.appendChild(node("p", "toypad-line", "On the PS3, Omoio can't see the colours the game lights the pads in."));
  }
  return toyPad;
}

/// What the place picked takes, or while a tag is held, what putting it
/// down there does.
function placeLine(): string {
  if (!lifted) return "A minifigure, a vehicle or a gadget.";
  const held = the(lifted.name);
  if (place === lifted.from) return `${capital(held)} goes back here, lifted and put down again.`;
  if (!places[place]) return `Put ${held} down here.`;
  return `${capital(the(places[place]))} is here. Pick an empty place.`;
}

/// The place picked, what goes on it, and what is on it with the buttons
/// that move it and take it off, between the ends that go to the place
/// before and after. While a tag is held, the button that puts it back.
function renderPlaceHead(): HTMLElement {
  const nav = node("div", "toy-place-head");
  const words = node("div", "toy-place-words");
  words.append(node("div", "toy-place-name", SPOTS[place].name), node("div", "toy-place-takes", placeLine()));
  nav.append(bumper("LB", "Previous place"), words);
  const name = places[place];
  if (lifted) {
    const cancel = node("button", "toy-off");
    cancel.append(node("kbd", "", named("East")), "Cancel");
    cancel.onclick = cancelMove;
    nav.appendChild(cancel);
  } else if (name) {
    const move = node("button", "toy-off");
    move.append(node("kbd", "", named("North")), "Move");
    move.onclick = lift;
    const off = node("button", "toy-off");
    off.append(node("span", "toy-off-name", name), node("kbd", "", named("West")), "Take off");
    off.onclick = () => void act(takeOff);
    nav.append(move, off);
  }
  nav.appendChild(bumper("RB", "Next place"));
  return nav;
}

function renderTile(entry: Entry, index: number): HTMLElement {
  const spot = placeOf(entry);
  const tile = node("button", `portal-item${index === at ? " sel" : ""}${spot >= 0 ? " on" : ""}`);
  const art = node("span", `portal-art ${tint(entry.piece)}`);
  drawPiece(art, pictureOf(numberOf(entry)), shapeOf(entry.piece));
  if (spot >= 0) {
    const badge = node("span", "portal-badge on");
    badge.innerHTML = svg(BADGE_ON);
    badge.append(capital(PADS[SPOTS[spot].pad].where));
    art.appendChild(badge);
  }
  const kind = node("span", `portal-kind ${tint(entry.piece)}`);
  kind.innerHTML = shapeOf(entry.piece);
  kind.append(entry.piece ? PIECES[entry.piece].words : "Your own tag");
  tile.append(art, node("span", "portal-name", entry.name), kind);
  if (entry.holds) tile.appendChild(node("span", "portal-note", entry.holds));
  tile.onclick = () => {
    if (lifted) return;
    at = index;
    void act(choose);
  };
  return tile;
}

function renderList(): HTMLElement {
  const column = node("div", lifted ? "toy-column toypad-moving" : "toy-column");
  const list = node("div", "portal-body toy-list");
  // While a tag is held, the list waits: what is pressed goes to the places.
  if (lifted) list.setAttribute("inert", "");
  let index = 0;
  for (const group of groups) {
    if (group.title) list.appendChild(node("div", "toy-group", group.title));
    if (group.note) list.appendChild(node("p", "toypad-group-line", group.note));
    if (group.entries.length === 0) {
      if (group.quiet) list.appendChild(node("div", "portal-quiet toy-quiet", group.quiet));
      continue;
    }
    const grid = node("div", "portal-grid toy-grid");
    for (const entry of group.entries) grid.appendChild(renderTile(entry, index++));
    list.appendChild(grid);
  }
  column.append(renderPlaceHead(), list);
  return column;
}

function renderFoot(): HTMLElement {
  const foot = node("div", "portal-foot");
  const hints: [string, string][] = [];
  if (lifted) {
    if (canGo(place)) hints.push(["South", place === lifted.from ? "Put back" : "Put down"]);
    hints.push(["East", "Cancel"]);
    foot.append(hintRow(hints, false));
    return foot;
  }
  const entry = selected();
  if (entry) hints.push(["South", "Put on"]);
  if (places[place]) hints.push(["North", "Move"], ["West", "Take off"]);
  if (deletable(entry)) hints.push(["Start", "Rename"], [HOLD, "Hold to delete"]);
  hints.push(["East", "Close"]);
  foot.append(hintRow(hints, true));
  return foot;
}

/// Over a game the menu isn't offered in, only why, and how to close.
function renderUnavailable(note: string) {
  const panel = node("div", "portal-panel portal-unavailable");
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", "Toy Pad");
  const head = node("div", "portal-head");
  head.appendChild(node("div", "portal-title", "Toy Pad"));
  const foot = node("div", "portal-foot");
  foot.appendChild(hintRow([["East", "Close"]], true));
  panel.append(head, node("p", "portal-unavailable-note", note), foot);
  root.replaceChildren(panel);
}

/// Draws the whole menu again, keeping where the list was scrolled so
/// moving doesn't make it jump, and the pads' lights where they were.
function render() {
  if (!ready) return;
  if (unavailable) return renderUnavailable(unavailable);
  buildGroups();
  settle();
  const scrolled = root.querySelector(".toy-list")?.scrollTop ?? 0;
  const panel = node("div", "portal-panel toy-panel toypad-panel");
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", "Toy Pad");
  const body = node("div", "toy-body");
  body.append(renderToyPad(), renderList());
  panel.append(renderHead(), body, renderFoot());
  root.replaceChildren(panel);
  panel.querySelector<HTMLElement>(".toy-list")!.scrollTop = scrolled;
  if (!lifted) panel.querySelector(".portal-item.sel")?.scrollIntoView({ block: "nearest" });
  paintLights();
  attachRename(root);
  drawHold();
}

// ---- moving ----

function showPlace(spot: number) {
  place = spot;
  at = 0;
  render();
}

/// The place before or after the one picked, in the order across the pads.
function step(by: number): number {
  const index = ORDER.indexOf(place);
  return ORDER[(index + by + ORDER.length) % ORDER.length];
}

/// Across a row, and up or down to the same place in the row before or
/// after, or its last tile, a group's first row coming after the last row of
/// the group before.
function moveList(move: Move) {
  const sizes = rowSizes();
  if (sizes.length === 0) return;
  let row = 0;
  let column = at;
  while (row < sizes.length - 1 && column >= sizes[row]) column -= sizes[row++];
  const first = (index: number) => sizes.slice(0, index).reduce((sum, size) => sum + size, 0);
  if (move === "left" && column > 0) at -= 1;
  else if (move === "right" && column < sizes[row] - 1) at += 1;
  else if (move === "down" && row < sizes.length - 1) at = first(row + 1) + Math.min(column, sizes[row + 1] - 1);
  else if (move === "up" && row > 0) at = first(row - 1) + Math.min(column, sizes[row - 1] - 1);
  render();
}

/// Picks up the tag on the place picked, to put it down on another. Only
/// the menu holds it: the game sees it lifted and put down in one go once
/// a place is picked, as with the emulators' own Move.
function lift() {
  if (acting || !places[place]) return;
  holding = null;
  lifted = { from: place, name: places[place] };
  render();
}

/// Lets go of the tag held without moving it, and picks its place again.
/// Nothing was sent, so the game saw nothing.
function cancelMove() {
  if (lifted) place = lifted.from;
  lifted = null;
  render();
}

// ---- changing the Toy Pad ----

/// What a change gives back: the Toy Pad afterwards, the file put on a
/// place when one was, and the places a tag was moved between.
interface Outcome {
  names: string[];
  place?: number;
  path?: string;
  moved?: { from: number; to: number };
}

/// One change to the Toy Pad, in its turn. Says whether it went through.
/// When it didn't, the Toy Pad is read again, since the emulator may have
/// done part of it. A tag made or put on stays selected, wherever its tile
/// has moved to in the list.
async function change(
  saying: Notice,
  job: () => Promise<Outcome>,
  said: (names: string[]) => Notice,
  moved?: { from: number; to: number }
): Promise<boolean> {
  working = saying;
  notify(saying);
  let done = false;
  let follow: string | undefined;
  try {
    const outcome = await inTurn(job);
    if (moved) followMove(moved, outcome.names);
    readBase(outcome.names);
    if (outcome.place !== undefined && outcome.path && places[outcome.place]) {
      placeFiles[outcome.place] = { path: outcome.path, name: places[outcome.place] };
    }
    follow = outcome.path;
    done = true;
    working = null;
    notify(said(places));
  } catch (err) {
    working = null;
    notify(problem(err, "That didn't work. Try again."));
    const names = await inTurn(toyPlaces).catch(() => null);
    if (names) {
      if (moved) followMove(moved, names);
      readBase(names);
    }
  }
  files = await toyFiles().catch(() => files);
  if (follow) {
    buildGroups();
    const index = entries().findIndex((entry) => entry.file?.path === follow);
    if (index >= 0) at = index;
  }
  render();
  return done;
}

/// A place is cleared before a tag goes on it, as both emulators' own
/// windows do (Cemu's LoadMinifigPath, EmulatedUSBDeviceFrame.cpp line 639,
/// v2.6-2; RPCS3's dimensions_dialog.cpp line 757, master b2e52d6), so what
/// was there comes off, and the notice says so.
function cameOff(before: string, now: string): string | undefined {
  return before && before !== now ? `${capital(the(before))} came off.` : undefined;
}

function takeOffPlace(spot: number): Promise<boolean> {
  const name = places[spot];
  return change(
    { kind: "working", title: `Taking ${the(name)} off…` },
    async () => ({ names: await toyRemove(spot) }),
    () => ({ kind: "done", title: `${capital(the(name))} is off the Toy Pad` })
  );
}

/// Takes off what is on the place picked.
async function takeOff() {
  if (places[place]) await takeOffPlace(place);
}

/// Moves the tag at `from` to `to`, through the emulator's own move, so the
/// game sees it lifted and put down.
function moveTag(from: number, to: number): Promise<boolean> {
  const name = places[from];
  const pad = PADS[SPOTS[to].pad];
  const back = from === to;
  return change(
    { kind: "working", title: back ? `Putting ${the(name)} back…` : `Moving ${the(name)} ${pad.to}…` },
    async () => ({ names: await toypadMove(from, to) }),
    (names) => ({ kind: "done", title: `${capital(the(names[to] || name))} is ${back ? "back " : ""}${pad.where}` }),
    { from, to }
  );
}

/// Puts the tag held down on a place, or says why not.
async function putDown(to: number) {
  const held = lifted;
  if (!held) return;
  place = to;
  if (!canGo(to)) {
    notify({ kind: "info", title: `${capital(the(places[to]))} is on that place`, detail: `Put ${the(held.name)} on an empty place, or back where it was.` });
    render();
    return;
  }
  lifted = null;
  await moveTag(held.from, to);
}

function already(spot: number) {
  notify({ kind: "done", title: `${capital(the(places[spot]))} is already ${PADS[SPOTS[spot].pad].where}` });
}

/// Puts the selected tag on the place picked: the player's file, moved from
/// the place it is on if it is on one, since an emulator holds a file open
/// while it is on (and Cemu, which opens it sharing its writing,
/// src/Common/windows/FileStream_win32.cpp lines 11 to 22, would take it on
/// two places), or a new one the emulator makes.
async function choose() {
  settle();
  const entry = selected();
  if (!entry || lifted) return;
  const target = place;
  const before = places[target] ?? "";
  const now = placeOf(entry);
  if (now === target) return already(target);
  const where = PADS[SPOTS[target].pad].where;
  if (entry.file) {
    const file = entry.file;
    // On another place, with this one free: moved, as the player would.
    if (now >= 0 && !before) {
      await moveTag(now, target);
      return;
    }
    if (now >= 0 && !(await takeOffPlace(now))) return;
    await change(
      { kind: "working", title: `Putting ${the(file.name)} ${where}…` },
      async () => ({ names: await toyLoad(target, file.path), place: target, path: file.path }),
      (names) => ({ kind: "done", title: `${capital(the(names[target] || file.name))} is ${where}`, detail: cameOff(before, names[target]) })
    );
  } else if (entry.offer) {
    // One already on, from a tag the player brought, isn't made again.
    if (now >= 0) return already(now);
    const offer = entry.offer;
    const blank = entry.piece === "blank_tag";
    await change(
      {
        kind: "working",
        title: blank ? `Making a ${offer.name}…` : `Making ${offer.name}…`,
        detail: blank ? BLANK_LINE : "A new tag, kept for next time.",
      },
      async () => {
        const made = await toyCreate(target, offer.number, offer.series);
        return { names: made.places, place: target, path: made.path };
      },
      (names) => ({ kind: "done", title: `${capital(the(names[target] || offer.name))} is ${where}`, detail: cameOff(before, names[target]) })
    );
  }
}

// ---- reading ----

/// The tags the emulator can make come from its own list the first time
/// only; Omoio keeps the list after that, so later openings are instant.
async function loadOffers() {
  if (offers.length > 0 || asking) return;
  asking = true;
  const getting: Notice = { kind: "working", title: "Getting the figures…" };
  // While the page still says it is loading, that says enough.
  if (ready) notify(getting);
  render();
  try {
    offers = await inTurn(toyOffers);
    if (notice === getting) notify(null);
  } catch (err) {
    notify(problem(err, "Couldn't get the figures."));
  } finally {
    asking = false;
    render();
  }
}

/// Reads what is on the Toy Pad, after any change under way. Usually quick,
/// so the notice only comes up when the emulator takes its time. The
/// player's tags show at once, and again once the Toy Pad is read, in case a
/// change under way made a tag meanwhile.
async function refresh() {
  const looking: Notice = { kind: "working", title: "Reading the Toy Pad…" };
  const slow = window.setTimeout(() => ready && !working && notify(looking), 400);
  const reading = inTurn(toyPlaces).catch((err: unknown) => {
    const said = problem(err, "Couldn't read the Toy Pad.");
    unread = said.title;
    notify(said);
    return null;
  });
  files = await toyFiles().catch(() => files);
  render();
  const names = await reading;
  window.clearTimeout(slow);
  if (names) {
    readBase(names);
    if (notice === looking) notify(null);
  }
  files = await toyFiles().catch(() => files);
  render();
}

// ---- the pad ----

/// While a tag is held: the d-pad or stick and the shoulder buttons go from
/// place to place on the Toy Pad, the bottom face button puts it down, and
/// the right one, or the top one again, lets go of it without closing.
function pressMoving(input: string) {
  const move = MOVES[input];
  if (move) {
    const next = NEIGHBOURS[place][move];
    if (next !== undefined) {
      place = next;
      render();
    }
    return;
  }
  if (input === "LB" || input === "RB") {
    place = step(input === "LB" ? -1 : 1);
    return render();
  }
  if (input === "East" || input === "North") return cancelMove();
  if (input === "South") void act(() => putDown(place));
}

function press(input: string) {
  // Over a game the menu isn't offered in, only closing does anything.
  if (unavailable) {
    if (input === "East") void closePortalMenu();
    return;
  }
  if (renaming()) return pressRename(input);
  // Anything else pressed lets go of a tag being held to delete it.
  if (holding && input !== "North") stopHold();
  if (lifted) return pressMoving(input);
  const move = MOVES[input];
  if (move) return moveList(move);
  if (input === "LB") return showPlace(step(-1));
  if (input === "RB") return showPlace(step(1));
  if (input === "East") return void closePortalMenu();
  if (input === "South") void act(choose);
  else if (input === "West") void act(takeOff);
  else if (input === "North") pressNorth();
  else if (input === "Start") rename();
}

/// Opens the box to name the selected tag, unless it is on the Toy Pad,
/// which is dealt with as deleting it is (`startHold`, below).
function rename() {
  const entry = selected();
  if (!deletable(entry) || acting || lifted || holding) return;
  if (placeOf(entry) >= 0) {
    notify({ kind: "info", title: `Take ${the(entry.name)} off the Toy Pad first`, detail: `Then press ${named("Start")} on it to rename it.` });
    return;
  }
  startRename(entry.file, "tag", async (path) => {
    files = await toyFiles().catch(() => files);
    const now = entries().findIndex((each) => each.file?.path === path);
    if (now >= 0) at = now;
    render();
    const name = files.find((file) => file.path === path)?.name;
    if (name) notify({ kind: "done", title: `Renamed to ${name}` });
  });
}

/// Whether the keys are named, so the last press was a key's. The keyboard
/// has a key for each of the top face button's two jobs, P and Y.
function byKeys(): boolean {
  return named(HOLD) !== named("North");
}

/// The top face button picks up the tag on the place picked, and held
/// deletes the selected tile's file, as in the base menu. Where it could do
/// either, the hold starts, and let go within `TAP` it was a press, which
/// picks the tag up (`drive`'s tick, below). P only ever picks up.
function pressNorth() {
  const entry = selected();
  if (places[place] && (byKeys() || !deletable(entry) || placeOf(entry) >= 0)) return lift();
  if (deletable(entry)) startHold("pad");
}

// ---- deleting one of the player's tags ----

/// How long the top face button is held on a tag to delete it, as in the
/// portal menu: long enough that it never happens by chance.
const HOLD_TO_DELETE = 5000;

/// How long the top face button may be down and still be a press, which
/// picks a tag up, rather than the start of a hold: several of the 50 ms
/// reads of the pads (`drive` in src/menu/input.ts), and short beside the
/// time a delete takes.
const TAP = 300;

/// A tag being held to be deleted: its file, since when, and what holds
/// it, the pad or the keyboard's Y.
let holding: { path: string; name: string; since: number; by: "pad" | "key" } | null = null;

function deletable(entry: Entry | undefined): entry is Entry & { file: ToyFile } {
  return Boolean(entry?.file);
}

/// Starts the hold, unless the tag is on the Toy Pad: the emulators write to
/// a tag's file while it is on (Cemu's WriteBlock, Dimensions.cpp lines 1050
/// to 1078; RPCS3's write_block, rpcs3/Emu/Io/Dimensions.cpp lines 311 to
/// 338).
function startHold(by: "pad" | "key") {
  const entry = selected();
  if (!deletable(entry) || holding || acting || lifted) return;
  if (placeOf(entry) >= 0) {
    notify({ kind: "info", title: `Take ${the(entry.name)} off the Toy Pad first`, detail: `Then hold ${named(HOLD)} on it to delete it.` });
    return;
  }
  holding = { path: entry.file.path, name: entry.name, since: Date.now(), by };
  drawHold();
}

/// Let go before the time is up: nothing happens.
function stopHold() {
  holding = null;
  drawHold();
}

/// The bar over the selected tile's picture that fills while its tag is
/// held, and runs back when let go.
function drawHold() {
  const art = root.querySelector<HTMLElement>(".portal-item.sel .portal-art");
  if (!art) return;
  let bar = art.querySelector<HTMLElement>(".portal-hold");
  if (!bar) {
    bar = node("span", "portal-hold");
    bar.append(node("i", ""), node("b", "", "Keep holding to delete"));
    art.appendChild(bar);
  }
  // Not while it may still be a press that picks the tag up.
  const held = holding && !(maybePress(holding) && Date.now() - holding.since < TAP) ? holding : null;
  const share = held ? Math.min(1, (Date.now() - held.since) / HOLD_TO_DELETE) : 0;
  bar.classList.toggle("held", Boolean(held));
  bar.querySelector<HTMLElement>("i")!.style.transform = `scaleX(${share})`;
}

/// Whether a hold, let go soon enough, is a press that picks up the tag on
/// the place picked.
function maybePress(hold: { by: "pad" | "key" }): boolean {
  return hold.by === "pad" && Boolean(places[place]);
}

/// Held long enough: the tag goes to the Recycle Bin, from where it can be
/// put back.
async function finishHold() {
  if (!holding) return;
  const { path, name } = holding;
  holding = null;
  await act(async () => {
    try {
      await deleteToyFigure(path);
      files = await toyFiles().catch(() => files);
      render();
      notify({ kind: "done", title: `${capital(the(name))} is in the Recycle Bin`, detail: "Restore it from there to get it back." });
    } catch (err) {
      render();
      notify(problem(err, "Couldn't delete that tag."));
    }
  });
}

drive({
  press,
  render,
  shown: () => shown,
  tick(now, time) {
    if (holding) {
      const still = holding.by === "pad" ? now.has("North") : keyHeld;
      if (!still) {
        const pressed = maybePress(holding) && time - holding.since < TAP;
        stopHold();
        if (pressed) lift();
      } else if (time - holding.since >= HOLD_TO_DELETE) void finishHold();
      else drawHold();
    }
  },
  hold() {
    if (!lifted && deletable(selected())) startHold("key");
  },
});

/// Shown again: what is already held down, such as the button that opened
/// the menu, is not taken as a press, and a change still under way says so.
async function show() {
  notify(working);
  try {
    openedBy(await portalMenuFamily());
    // Nothing is asked of the Toy Pad over a game the menu isn't offered in.
    unavailable = await portalMenuNote().catch(() => null);
    if (unavailable) {
      await skipHeld();
      shown = true;
      ready = true;
      render();
      return;
    }
    playing = await toyGame().catch(() => null);
    await skipHeld();
    shown = true;
    watchLights(true);
    await readPictures();
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
  openedBy(state.family);
  if (state.open) void show();
  else {
    shown = false;
    holding = null;
    lifted = null;
    stopRename();
    watchLights(false);
  }
});
void show();
