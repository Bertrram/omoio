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
  toyRemove,
  type ToyFigure,
  type ToyFile,
  type ToyGame,
  type ToyOffer,
} from "./api";
import { node, svg } from "./menu/dom";
import { bumper, drive, hintRow, HOLD, keyHeld, MOVES, named, openedBy, skipHeld, type Move } from "./menu/input";
import { notice, notify, problem, type Notice } from "./menu/notices";
import { act, acting, inTurn } from "./menu/turns";

/// The Disney Infinity base menu, drawn by Omoio over the running game in
/// the window the portal menu uses, and used with the pad alone. The base is
/// on the left as the game reads it, three pads: the hexagon, and Player
/// One's and Player Two's round pads, each with two Power Discs under its
/// character. The shoulder buttons go from place to place, and the list
/// beside the base offers only what goes in the place picked, in this game:
/// the player's own figures first, the ones used lately first, then those
/// the emulator can make, by kind. The d-pad or left stick moves, the bottom
/// face button puts a figure on, the left one takes off what is on the
/// place, the top one held deletes one of the player's figures, and the
/// right one closes. Mouse and keyboard work as well, with the same keys as
/// the portal menu.
///
/// A figure made here is kept, and put on from then on, since the game keeps
/// a figure's progress on the figure. Omoio never reads a figure file: one
/// it made it knows from its own record, and one the player brought only by
/// the name the emulator gives it once it is on the base.

/// How many tiles sit side by side, which is also how far up or down moves.
const COLUMNS = 4;

type Piece = "character" | "play_set" | "round_disc" | "hex_disc";

/// Each kind of piece, with the words for it, the heading of its group in
/// the list, the order groups come in, and Omoio's own drawing of it until
/// it has pictures: a figure on its stand, a Play Set's hexagon with a
/// castle in it, and the two shapes of Power Disc.
const PIECES: Record<Piece, { words: string; group: string; order: number; shape: string }> = {
  character: {
    words: "Character",
    group: "Characters",
    order: 0,
    shape: `<circle cx="12" cy="5.4" r="3.2" fill="currentColor"/><path d="M8 17.6v-6.4a4 4 0 0 1 8 0v6.4z" fill="currentColor"/><ellipse cx="12" cy="20" rx="7.2" ry="2.1" fill="currentColor" opacity=".55"/>`,
  },
  play_set: {
    words: "Play Set",
    group: "Play Sets",
    order: 1,
    shape: `<path d="M12 2.2 20.5 7.1v9.8L12 21.8 3.5 16.9V7.1z" fill="currentColor" opacity=".28"/><path d="M12 2.2 20.5 7.1v9.8L12 21.8 3.5 16.9V7.1z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/><path d="M7.8 16.6v-5.4h1.7v1.5h1.4V9.2h2.2v3.5h1.4v-1.5h1.7v5.4z" fill="currentColor"/>`,
  },
  hex_disc: {
    words: "Power Disc",
    group: "Hexagonal Power Discs",
    order: 2,
    shape: `<path fill-rule="evenodd" d="M7 3.3h10l5 8.7-5 8.7H7L2 12zM8.6 6.1 5.2 12l3.4 5.9h6.8l3.4-5.9-3.4-5.9z" fill="currentColor"/><path d="M9.7 8h4.6l2.3 4-2.3 4H9.7L7.4 12z" fill="currentColor"/>`,
  },
  round_disc: {
    words: "Power Disc",
    group: "Round Power Discs",
    order: 3,
    shape: `<path fill-rule="evenodd" d="M12 2.5a9.5 9.5 0 1 1 0 19 9.5 9.5 0 0 1 0-19zm0 2.4a7.1 7.1 0 1 0 0 14.2 7.1 7.1 0 0 0 0-14.2z" fill="currentColor"/><circle cx="12" cy="12" r="5.2" fill="currentColor"/>`,
  },
};

/// A figure the player brought, which Omoio doesn't know until it is on.
const UNKNOWN = `<rect x="3.5" y="3.5" width="17" height="17" rx="3.5" fill="none" stroke="currentColor" stroke-width="2"/><path d="M9.6 9.4a2.5 2.5 0 1 1 3.6 2.2c-.8.4-1.2 1-1.2 1.9v.5M12 17h.01" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>`;

/// The base's nine places, in the emulators' order and by their names:
/// Cemu's (src/gui/EmulatedUSBDevices/EmulatedUSBDeviceFrame.cpp:110 to
/// :118, Omoio's build at v2.6-2), RPCS3's (rpcs3qt/infinity_dialog.cpp:736
/// to :752, master b2e52d6) and Dolphin's (DolphinQt/InfinityBase/
/// InfinityBaseWindow.cpp:83 to :99, 2609a), read 4 November 2026. The game
/// itself sees three pads, places 0 to 2 as the hexagon, 3 to 5 as Player
/// One's and 6 to 8 as Player Two's (DeriveFigurePosition, Cemu's
/// src/Cafe/OS/libs/nsyshid/Infinity.cpp:1079 to :1103), so they are drawn
/// as three pads. The hexagon takes a Play Set and up to three hexagonal
/// Power Discs, and a round pad a character with up to two round Power Discs
/// under it (the Disney Infinity wiki's "Disney Infinity Base", revision
/// 149632, and "Compatibility", revision 127460, read 4 November 2026).
/// Each place's `piece` is what is drawn in it while it is empty, and for a
/// figure on it Omoio doesn't know.
const SPOTS: { name: string; full: string; where: string; takes: string; piece: Piece; big: boolean }[] = [
  { name: "Play Set or Power Disc", full: "Play Set or Power Disc", where: "on the hexagon", takes: "A Play Set, or a hexagonal Power Disc.", piece: "play_set", big: true },
  { name: "Power Disc Two", full: "Power Disc Two", where: "on the hexagon", takes: "A hexagonal Power Disc. Three go on the hexagon.", piece: "hex_disc", big: false },
  { name: "Power Disc Three", full: "Power Disc Three", where: "on the hexagon", takes: "A hexagonal Power Disc. Three go on the hexagon.", piece: "hex_disc", big: false },
  { name: "Player One", full: "Player One", where: "on Player One", takes: "A character.", piece: "character", big: true },
  { name: "Ability One", full: "Player One, Ability One", where: "under Player One", takes: "A round Power Disc, under Player One's character.", piece: "round_disc", big: false },
  { name: "Ability Two", full: "Player One, Ability Two", where: "under Player One", takes: "A round Power Disc, under Player One's character.", piece: "round_disc", big: false },
  { name: "Player Two", full: "Player Two", where: "on Player Two", takes: "A character.", piece: "character", big: true },
  { name: "Ability One", full: "Player Two, Ability One", where: "under Player Two", takes: "A round Power Disc, under Player Two's character.", piece: "round_disc", big: false },
  { name: "Ability Two", full: "Player Two, Ability Two", where: "under Player Two", takes: "A round Power Disc, under Player Two's character.", piece: "round_disc", big: false },
];

/// The outline of each kind of place on the base, drawn at any size.
const HEX_EDGE = `<polygon class="toy-edge" points="50,4 90,27 90,73 50,96 10,73 10,27"/>`;
const ROUND_EDGE = `<circle class="toy-edge" cx="50" cy="50" r="45"/>`;

const BADGE_ON = `<path d="m5 12.5 4.5 4.5L19 7.5" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`;

const GAMES: Record<NonNullable<ToyGame["game"]>, string> = {
  one: "Disney Infinity",
  two: "Disney Infinity 2.0",
  three: "Disney Infinity 3.0",
};

/// One tile: one of the player's files, or a figure the emulator can make.
interface Entry {
  name: string;
  piece: Piece | null;
  series: number | null;
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

let playing: ToyGame | null = null;
let shown = false;
/// Whether the menu has been filled once. Until then the page keeps the
/// "Loading the base" it was written with.
let ready = false;
/// Over a game the menu isn't offered in, the line the game's page says it
/// with, shown in place of the menu (portal_menu::note in
/// src-tauri/src/portal_menu.rs). `null` where the menu works.
let unavailable: string | null = null;
/// What each place holds, by the name the emulator gives it.
let places: string[] = [];
/// The file Omoio put on each place, with the name the emulator gave it
/// then. A figure on the base is known by its file: two files can be of one
/// figure, and the emulator names a file the player brought by its figure.
let placeFiles: ({ path: string; name: string } | null)[] = [];
/// Why the base couldn't be read, in the backend's words, until it is read.
let unread: string | null = null;
let files: ToyFile[] = [];
let offers: ToyOffer[] = [];
let asking = false;
/// The place picked, which the list offers for. Player One first, since a
/// character is what a game asks for first.
let place = 3;
let at = 0;
let groups: Group[] = [];
/// What the change under way says, said again if the menu opens again
/// before it is done.
let working: Notice | null = null;

function pieceOf(kind: string | null | undefined): Piece | null {
  return kind && kind in PIECES ? (kind as Piece) : null;
}

/// "2.0" for the second game's series, as the games are known.
function edition(series: number): string {
  return `${series}.0`;
}

function sameFigure(one: ToyFigure, other: ToyFigure): boolean {
  return one.number === other.number && one.series === other.series;
}

/// A group of the figures the emulator can make: of the running game's own
/// series under the kind's own heading, of an earlier one saying so. A
/// game takes the characters of the earlier games in its Toy Box only, and a
/// hexagonal Power Disc works in the Toy Box in every game (the Disney
/// Infinity wiki's "Compatibility", revision 127460, read 4 November 2026).
function groupFor(piece: Piece, series: number | null): Group {
  const own = series == null || series === playing?.series;
  const older = !own && playing?.series != null;
  return {
    title: own || series == null ? PIECES[piece].group : `${PIECES[piece].group} from ${edition(series)}`,
    note: piece === "character" && older ? "They play in the Toy Box." : piece === "hex_disc" ? "They work in the Toy Box." : undefined,
    entries: [],
  };
}

/// What the list offers for the place picked: the player's files that go
/// there, then what the emulator can make that goes there, newest series
/// first and by name. A figure the player has a file of is offered as that
/// file only, so it goes on with what it has earned.
function buildGroups() {
  const fits = (spots: number[]) => spots.includes(place);
  const yours = files.filter((file) => fits(file.places));
  groups = [
    {
      title: "Yours",
      entries: yours.map((file) => ({ name: file.name, piece: pieceOf(file.kind), series: file.figure?.series ?? null, file })),
      quiet: yours.length > 0 ? undefined : files.length > 0 ? "None of yours go here." : "Nothing saved yet. A figure made here is kept for next time.",
    },
  ];
  const kept = files.flatMap((file) => (file.figure ? [file.figure] : []));
  const makeable = offers
    .filter((offer) => fits(offer.places) && pieceOf(offer.kind) && !kept.some((figure) => sameFigure(figure, offer)))
    .sort(
      (one, other) =>
        PIECES[pieceOf(one.kind)!].order - PIECES[pieceOf(other.kind)!].order ||
        (other.series ?? 0) - (one.series ?? 0) ||
        one.name.localeCompare(other.name)
    );
  let group: Group | null = null;
  let last = "";
  for (const offer of makeable) {
    const piece = pieceOf(offer.kind)!;
    const key = `${piece} ${offer.series}`;
    if (!group || key !== last) {
      group = groupFor(piece, offer.series);
      groups.push(group);
      last = key;
    }
    group.entries.push({ name: offer.name, piece, series: offer.series, offer });
  }
  if (asking && offers.length === 0) groups.push({ title: "", entries: [], quiet: "Getting the figures…" });
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

/// Takes in the base as it was read. A place that has emptied, or holds
/// another figure than the one Omoio put there, no longer holds that file.
function readBase(names: string[]) {
  places = names;
  placeFiles = names.map((name, spot) => (name && placeFiles[spot]?.name === name ? placeFiles[spot] : null));
  unread = null;
}

/// The place holding what a tile stands for, or -1: its file when Omoio
/// put that on, or a figure the emulator names as it. A file the player
/// brought goes by its file only, and a file of Omoio's by its figure's
/// name only on a place whose file Omoio doesn't know.
function placeOf(entry: Entry): number {
  const byFile = entry.file ? placeFiles.findIndex((file) => file?.path === entry.file!.path) : -1;
  if (byFile >= 0) return byFile;
  const name = entry.offer?.name ?? entry.file?.figure?.name;
  if (!name) return -1;
  return places.findIndex((held, spot) => held === name && (Boolean(entry.offer) || !placeFiles[spot]));
}

/// What is on a place, as far as Omoio can tell: by its file, by the
/// emulator's name for it, or by what the place takes.
function pieceOn(spot: number): Piece {
  const kept = placeFiles[spot] && files.find((file) => file.path === placeFiles[spot]!.path);
  return pieceOf(kept?.kind) ?? pieceOf(offers.find((offer) => offer.name === places[spot])?.kind) ?? SPOTS[spot].piece;
}

function tint(piece: Piece | null): string {
  return `toy-${piece ?? "unknown"}`;
}

function shapeOf(piece: Piece | null): string {
  return svg(piece ? PIECES[piece].shape : UNKNOWN);
}

function capital(words: string): string {
  return words.charAt(0).toUpperCase() + words.slice(1);
}

// ---- drawing ----

function renderHead(): HTMLElement {
  const head = node("div", "portal-head");
  head.appendChild(node("div", "portal-title", "Infinity Base"));
  const game = playing?.game ? GAMES[playing.game] : null;
  if (game) head.appendChild(node("div", "toy-game", game));
  if (unread) head.appendChild(node("div", "toy-unread", unread));
  return head;
}

/// One place on the base. A click picks it, on pointer-down, and keeps the
/// focus where it is, as the tab bar's ends do: the keys and the pad move
/// the selection, and the list beside follows.
function renderSpot(spot: number): HTMLElement {
  const info = SPOTS[spot];
  const name = places[spot] ?? "";
  const button = node("button", `toy-spot ${info.big ? "big" : "small"}${spot === place ? " sel" : ""}${name ? " on" : ""}`);
  button.tabIndex = -1;
  button.setAttribute("aria-label", `${info.full}: ${name || "empty"}`);
  if (spot === place) button.setAttribute("aria-current", "true");
  const shape = node("span", "toy-spot-shape");
  shape.innerHTML = `<svg viewBox="0 0 100 100" aria-hidden="true">${info.piece === "character" || info.piece === "round_disc" ? ROUND_EDGE : HEX_EDGE}</svg>`;
  const piece = node("span", `toy-spot-piece ${tint(name ? pieceOn(spot) : info.piece)}`);
  piece.innerHTML = shapeOf(name ? pieceOn(spot) : info.piece);
  shape.appendChild(piece);
  button.append(shape, node("span", "toy-spot-label", info.name));
  button.appendChild(node("span", name ? "toy-spot-name" : "toy-spot-name empty", name || "Empty"));
  button.onpointerdown = (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    showPlace(spot);
  };
  return button;
}

function renderBase(): HTMLElement {
  const base = node("div", "toy-base");
  base.setAttribute("role", "group");
  base.setAttribute("aria-label", "The base");
  const hexagon = node("div", "toy-hexagon");
  hexagon.append(renderSpot(1), renderSpot(0), renderSpot(2));
  const pads = [3, 6].map((first) => {
    const pad = node("div", "toy-player");
    const abilities = node("div", "toy-abilities");
    abilities.append(renderSpot(first + 1), renderSpot(first + 2));
    pad.append(renderSpot(first), abilities);
    return pad;
  });
  base.append(hexagon, ...pads);
  return base;
}

/// The place picked, what goes in it, and what is on it with the button
/// that takes it off, between the ends that go to the place before and
/// after.
function renderPlaceHead(): HTMLElement {
  const nav = node("div", "toy-place-head");
  const words = node("div", "toy-place-words");
  words.append(node("div", "toy-place-name", SPOTS[place].full), node("div", "toy-place-takes", SPOTS[place].takes));
  nav.append(bumper("LB", "Previous place"), words);
  const name = places[place];
  if (name) {
    const off = node("button", "toy-off");
    off.append(node("span", "toy-off-name", name), node("kbd", "", named("West")), "Take off");
    off.onclick = () => void act(takeOff);
    nav.appendChild(off);
  }
  nav.appendChild(bumper("RB", "Next place"));
  return nav;
}

function renderTile(entry: Entry, index: number): HTMLElement {
  const spot = placeOf(entry);
  const tile = node("button", `portal-item${index === at ? " sel" : ""}${spot >= 0 ? " on" : ""}`);
  const art = node("span", `portal-art ${tint(entry.piece)}`);
  art.innerHTML = shapeOf(entry.piece);
  if (spot >= 0) {
    const badge = node("span", "portal-badge on");
    badge.innerHTML = svg(BADGE_ON);
    badge.append(capital(SPOTS[spot].where));
    art.appendChild(badge);
  }
  const kind = node("span", `portal-kind ${tint(entry.piece)}`);
  kind.innerHTML = shapeOf(entry.piece);
  kind.append(entry.piece ? PIECES[entry.piece].words : "Your own file");
  // The series only where it isn't the running game's own.
  if (entry.series != null && entry.series !== playing?.series) kind.appendChild(node("span", "portal-series", edition(entry.series)));
  tile.append(art, node("span", "portal-name", entry.name), kind);
  // An earlier game's character among the player's own, where no heading
  // says it: the running game plays it in the Toy Box only.
  const older = entry.file && entry.piece === "character" && entry.series != null && playing?.series != null && entry.series < playing.series;
  if (older) tile.appendChild(node("span", "portal-note", "Plays in the Toy Box."));
  tile.onclick = () => {
    at = index;
    void act(choose);
  };
  return tile;
}

function renderList(): HTMLElement {
  const column = node("div", "toy-column");
  const list = node("div", "portal-body toy-list");
  let index = 0;
  for (const group of groups) {
    if (group.title) {
      const heading = node("div", "toy-group", group.title);
      if (group.note) heading.appendChild(node("span", "toy-group-note", group.note));
      list.appendChild(heading);
    }
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
  const entry = selected();
  const hints: [string, string][] = [];
  if (entry) hints.push(["South", "Put on"]);
  if (places[place]) hints.push(["West", "Take off"]);
  if (deletable(entry)) hints.push([HOLD, "Hold to delete"]);
  hints.push(["East", "Close"]);
  foot.append(hintRow(hints, true));
  return foot;
}

/// Over a game the menu isn't offered in, only why, and how to close.
function renderUnavailable(note: string) {
  const panel = node("div", "portal-panel portal-unavailable");
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", "Infinity Base");
  const head = node("div", "portal-head");
  head.appendChild(node("div", "portal-title", "Infinity Base"));
  const foot = node("div", "portal-foot");
  foot.appendChild(hintRow([["East", "Close"]], true));
  panel.append(head, node("p", "portal-unavailable-note", note), foot);
  root.replaceChildren(panel);
}

/// Draws the whole menu again, keeping where the list was scrolled so
/// moving doesn't make it jump.
function render() {
  if (!ready) return;
  if (unavailable) return renderUnavailable(unavailable);
  buildGroups();
  settle();
  const scrolled = root.querySelector(".toy-list")?.scrollTop ?? 0;
  const panel = node("div", "portal-panel toy-panel");
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-label", "Infinity Base");
  const body = node("div", "toy-body");
  body.append(renderBase(), renderList());
  panel.append(renderHead(), body, renderFoot());
  root.replaceChildren(panel);
  panel.querySelector<HTMLElement>(".toy-list")!.scrollTop = scrolled;
  panel.querySelector(".portal-item.sel")?.scrollIntoView({ block: "nearest" });
  drawHold();
}

// ---- moving ----

function showPlace(spot: number) {
  place = (spot + SPOTS.length) % SPOTS.length;
  at = 0;
  render();
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

// ---- changing the base ----

/// What a change gives back: the base afterwards, and the file put on a
/// place when one was.
interface Outcome {
  names: string[];
  place?: number;
  path?: string;
}

/// One change to the base, in its turn. Says whether it went through. When
/// it didn't, the base is read again, since the emulator may have done part
/// of it. A figure made or put on stays selected, wherever its tile
/// has moved to in the list.
async function change(saying: Notice, job: () => Promise<Outcome>, said: (names: string[]) => Notice): Promise<boolean> {
  working = saying;
  notify(saying);
  let done = false;
  let follow: string | undefined;
  try {
    const outcome = await inTurn(job);
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
    if (names) readBase(names);
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

/// The emulators clear a place before a figure goes on it (Cemu's
/// LoadFigurePath, EmulatedUSBDeviceFrame.cpp:490, and RPCS3's
/// load_figure_path, infinity_dialog.cpp:877), so what was there comes off,
/// and the notice says so.
function cameOff(before: string, now: string): string | undefined {
  return before && before !== now ? `${before} came off.` : undefined;
}

function takeOffPlace(spot: number): Promise<boolean> {
  const name = places[spot];
  return change(
    { kind: "working", title: `Taking ${name} off…` },
    async () => ({ names: await toyRemove(spot) }),
    () => ({ kind: "done", title: `${name} is off the base` })
  );
}

/// Takes off what is on the place picked.
async function takeOff() {
  if (places[place]) await takeOffPlace(place);
}

function already(spot: number) {
  notify({ kind: "done", title: `${places[spot]} is already ${SPOTS[spot].where}` });
}

/// Puts the selected figure on the place picked: the player's file, moved
/// from the place it is on if it is on one, since an emulator holds a file
/// open while it is on, or a new one the emulator makes.
async function choose() {
  settle();
  const entry = selected();
  if (!entry) return;
  const target = place;
  const before = places[target] ?? "";
  const now = placeOf(entry);
  if (now === target) return already(target);
  const where = SPOTS[target].where;
  if (entry.file) {
    const file = entry.file;
    if (now >= 0 && !(await takeOffPlace(now))) return;
    await change(
      { kind: "working", title: `Putting ${file.name} ${where}…` },
      async () => ({ names: await toyLoad(target, file.path), place: target, path: file.path }),
      (names) => ({ kind: "done", title: `${names[target] || file.name} is ${where}`, detail: cameOff(before, names[target]) })
    );
  } else if (entry.offer) {
    // One already on, from a file the player brought, isn't made again.
    if (now >= 0) return already(now);
    const offer = entry.offer;
    await change(
      { kind: "working", title: `Making ${offer.name}…`, detail: "A new figure, kept for next time." },
      async () => {
        const made = await toyCreate(target, offer.number, offer.series);
        return { names: made.places, place: target, path: made.path };
      },
      (names) => ({ kind: "done", title: `${names[target] || offer.name} is ${where}`, detail: cameOff(before, names[target]) })
    );
  }
}

// ---- reading ----

/// The figures the emulator can make come from its own figure maker the
/// first time only; Omoio keeps the list after that, so later openings are
/// instant.
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

/// Reads what is on the base, after any change under way. Usually quick, so
/// the notice only comes up when the emulator takes its time. The player's
/// files show at once, and again once the base is read, in case a change
/// under way made a figure meanwhile.
async function refresh() {
  const looking: Notice = { kind: "working", title: "Reading the base…" };
  const slow = window.setTimeout(() => ready && !working && notify(looking), 400);
  const reading = inTurn(toyPlaces).catch((err: unknown) => {
    const said = problem(err, "Couldn't read the base.");
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

function press(input: string) {
  // Over a game the menu isn't offered in, only closing does anything.
  if (unavailable) {
    if (input === "East") void closePortalMenu();
    return;
  }
  // Anything else pressed lets go of a figure being held to delete it.
  if (holding && input !== "North") stopHold();
  const move = MOVES[input];
  if (move) return moveList(move);
  if (input === "LB") return showPlace(place - 1);
  if (input === "RB") return showPlace(place + 1);
  if (input === "East") return void closePortalMenu();
  if (input === "South") void act(choose);
  else if (input === "West") void act(takeOff);
  else if (input === "North" && deletable(selected())) startHold("pad");
}

// ---- deleting one of the player's figures ----

/// How long the top face button is held on a figure to delete it, as in the
/// portal menu: long enough that it never happens by chance.
const HOLD_TO_DELETE = 5000;

/// A figure being held to be deleted: its file, since when, and what holds
/// it, the pad or the keyboard's Y.
let holding: { path: string; name: string; since: number; by: "pad" | "key" } | null = null;

function deletable(entry: Entry | undefined): entry is Entry & { file: ToyFile } {
  return Boolean(entry?.file);
}

/// Starts the hold, unless the figure is on the base: the emulators write
/// to a figure's file while it is on (Cemu's WriteBlock, Infinity.cpp:761
/// to :781).
function startHold(by: "pad" | "key") {
  const entry = selected();
  if (!deletable(entry) || holding || acting) return;
  if (placeOf(entry) >= 0) {
    notify({ kind: "info", title: `Take ${entry.name} off the base first`, detail: `Then hold ${named(HOLD)} on it to delete it.` });
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

/// The bar over the selected tile's picture that fills while its figure is
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
  const share = holding ? Math.min(1, (Date.now() - holding.since) / HOLD_TO_DELETE) : 0;
  bar.classList.toggle("held", Boolean(holding));
  bar.querySelector<HTMLElement>("i")!.style.transform = `scaleX(${share})`;
}

/// Held long enough: the figure goes to the Recycle Bin, from where it can
/// be put back.
async function finishHold() {
  if (!holding) return;
  const { path, name } = holding;
  holding = null;
  await act(async () => {
    try {
      await deleteToyFigure(path);
      files = await toyFiles().catch(() => files);
      render();
      notify({ kind: "done", title: `${name} is in the Recycle Bin`, detail: "Restore it from there to get it back." });
    } catch (err) {
      render();
      notify(problem(err, "Couldn't delete that figure."));
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
      if (!still) stopHold();
      else if (time - holding.since >= HOLD_TO_DELETE) void finishHold();
      else drawHold();
    }
  },
  hold() {
    if (deletable(selected())) startHold("key");
  },
});

/// Shown again: what is already held down, such as the button that opened
/// the menu, is not taken as a press, and a change still under way says so.
async function show() {
  notify(working);
  try {
    openedBy(await portalMenuFamily());
    // Nothing is asked of the base over a game the menu isn't offered in.
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
  }
});
void show();
