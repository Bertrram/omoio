import { padsConnected, padsHeld, type Pad, type PadFamily } from "../api";
import { nameOf } from "../components/padNames";
import { node } from "./dom";

/// The keys and the pad in a menu over a game, the same in every menu: what
/// each key does, how a held direction repeats, and the names the menu gives
/// the buttons, which follow what was pressed last. Each menu's page says
/// what a press does there (`drive`).

/// The keyboard key that opens and closes the menu, as Omoio reads it
/// (SECOND_MENU_KEY in src-tauri/src/portal_menu.rs; Insert works too, but not
/// every keyboard has one). The page itself does nothing
/// with it: Omoio reads it from wherever it is pressed.
const MENU_KEY = "Tab";

/// Holding the top face button on a saved figure, which deletes it, named as
/// a place of its own: the keyboard holds Y for it, since its top face
/// button, P, pairs.
export const HOLD = "North held";

/// The keys that do each place's job, as the menus' own key handler maps
/// them (the keydown listener in `drive`, below), named as Big Picture
/// names the same keys (src/bigpicture/keys.ts). Backspace and Delete both
/// take a figure off; Backspace is the one named, since "Delete" beside
/// "Take off" would read as deleting the figure.
const KEYS: Record<string, string> = {
  South: "Enter",
  East: "Esc",
  West: "Backspace",
  North: "P",
  [HOLD]: "Y",
  LB: "PgUp",
  RB: "PgDn",
  Start: "F2",
};

/// A direction held down keeps moving after a pause, as on a console.
const REPEAT_AFTER = 380;
const REPEAT_EVERY = 140;

export type Move = "up" | "down" | "left" | "right";

export const MOVES: Record<string, Move> = {
  Up: "up",
  "LS Y+": "up",
  Down: "down",
  "LS Y-": "down",
  Left: "left",
  "LS X-": "left",
  Right: "right",
  "LS X+": "right",
};

/// What a menu's page gives the keys and the pad: what a press of a place
/// on the pad does, how the page draws itself again, whether it is up, what
/// else it looks at each time the pads are read, and what Y pressed down
/// starts there.
export interface Page {
  press(input: string): void;
  render(): void;
  shown(): boolean;
  tick?(now: Set<string>, time: number): void;
  hold?(): void;
}

let page: Page;

/// The kind of pad whose buttons the menu names, as printed on it: the one
/// that opened the menu. `null` while only keys have opened it.
let family: PadFamily | null = null;
/// Whether the menu names the keyboard's keys instead of a pad's buttons:
/// it was opened with Insert, Tab or the Portal button, or a key was pressed
/// since.
let keysNamed = false;

/// Whether Y is held down on the keyboard, for a figure held to be deleted.
export let keyHeld = false;

/// A place on the pad, such as "South", as the menu names it to the player:
/// the key that does its job, or the button as printed on the pad.
export function named(place: string): string {
  if (keysNamed) return KEYS[place] ?? place;
  return nameOf(family ?? "generic", place === HOLD ? "North" : place);
}

/// Takes in what opened the menu, as Omoio tells it (`family` in
/// src-tauri/src/portal_menu.rs): a kind of pad, or "keyboard" for Insert,
/// Tab and the Portal button. The kind of pad that opened it last is kept
/// for when a pad is pressed in a menu a key opened.
export function openedBy(opener: string) {
  keysNamed = opener === "keyboard";
  if (!keysNamed) family = opener as PadFamily;
}

/// The names follow what was pressed last, as Big Picture's hints do (`act`
/// in src/bigpicture/bigPicture.ts): a key the menu takes names the keys.
function pressedKey() {
  if (keysNamed) return;
  keysNamed = true;
  page.render();
}

/// A pad pressed names the pad's buttons again. Omoio reads only what is
/// held on the pads, not on which one, so in a menu only keys have opened
/// the first pad plugged in stands for it, as in Big Picture (`readPads`
/// there).
async function pressedPad() {
  if (!keysNamed) return;
  if (!family) family = (await padsConnected().catch((): Pad[] => []))[0]?.family ?? "generic";
  keysNamed = false;
  page.render();
}

/// The buttons and what each does. With `closing`, East closes the menu.
export function hintRow(hints: [string, string][], closing: boolean): HTMLElement {
  const row = node("div", "portal-hints");
  for (const [input, words] of hints) {
    const hint = node("span", "portal-hint");
    hint.append(node("kbd", "", named(input)));
    // The keyboard's Tab, on every keyboard, opens and closes the menu as
    // the pad's button does, and so does Insert (MENU_KEY and
    // SECOND_MENU_KEY in src-tauri/src/portal_menu.rs), for a player whose
    // pad Omoio can't read, so it is named where the menu says how to close.
    if (input === "East" && closing) hint.append(node("span", "portal-or", "or"), node("kbd", "", MENU_KEY));
    hint.append(words);
    row.appendChild(hint);
  }
  return row;
}

/// The label at an end of the tab bar names the button or key that goes to
/// the tab before or after, and a click on it does the same, through the
/// same `press`. It goes on pointer-down, as all of Omoio's feedback does, and the
/// press is kept from taking the focus: the menu keeps its selection itself,
/// and the keys and the pad move it.
export function bumper(place: "LB" | "RB", words: string): HTMLElement {
  const label = node("button", "portal-bumper", named(place));
  label.tabIndex = -1;
  label.setAttribute("aria-label", words);
  label.onpointerdown = (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    page.press(place);
  };
  return label;
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

/// What is held down now, such as the button that opened the menu, is not
/// taken as a press.
export async function skipHeld() {
  held = await readPads();
}

/// Starts reading the pads and the keyboard for `menu`, the page's own.
export function drive(menu: Page) {
  page = menu;

  window.setInterval(async () => {
    if (!page.shown() || reading) return;
    reading = true;
    try {
      const now = await readPads();
      const time = Date.now();
      for (const input of now) {
        if (!held.has(input)) {
          downSince.set(input, time);
          if (keysNamed) await pressedPad();
          page.press(input);
        } else if (
          MOVES[input] &&
          time - (downSince.get(input) ?? time) > REPEAT_AFTER &&
          time - lastRepeat > REPEAT_EVERY
        ) {
          lastRepeat = time;
          page.press(input);
        }
      }
      held = now;
      page.tick?.(now, time);
    } finally {
      reading = false;
    }
  }, 50);

  document.addEventListener("keyup", (event) => {
    if (event.key === "y" || event.key === "Y") keyHeld = false;
  });

  document.addEventListener("keydown", (event) => {
    // A name being typed (src/menu/rename.ts) keeps its keys.
    if (event.target instanceof HTMLInputElement) return;
    // Y held deletes a saved figure, as the pad's top face button does.
    if (event.key === "y" || event.key === "Y") {
      event.preventDefault();
      if (!event.repeat) {
        keyHeld = true;
        pressedKey();
        page.hold?.();
      }
      return;
    }
    // KEYS at the top names these keys to the player, so it changes with them.
    const keys: Record<string, () => void> = {
      ArrowUp: () => page.press("Up"),
      ArrowDown: () => page.press("Down"),
      ArrowLeft: () => page.press("Left"),
      ArrowRight: () => page.press("Right"),
      PageUp: () => page.press("LB"),
      PageDown: () => page.press("RB"),
      Enter: () => page.press("South"),
      Delete: () => page.press("West"),
      Backspace: () => page.press("West"),
      Escape: () => page.press("East"),
      // F2 renames, as it does a file in Windows.
      F2: () => page.press("Start"),
      // P for pair: a SuperCharger and its vehicle together.
      p: () => page.press("North"),
      P: () => page.press("North"),
    };
    const act = keys[event.key];
    if (act) {
      event.preventDefault();
      // A key held down repeats; only moving through the menu should, the
      // arrows and the page keys, so a held Enter never makes a second figure.
      if (event.repeat && !event.key.startsWith("Arrow") && !event.key.startsWith("Page")) return;
      pressedKey();
      act();
    }
  });
}
