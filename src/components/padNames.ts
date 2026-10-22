import type { PadFamily } from "../api";

/// What each place on a pad is called on each kind of pad, for anything that
/// names a button to the player. A place a kind does not name differently
/// falls through to COMMON.
export const FAMILY_NAMES: Record<PadFamily, Record<string, string>> = {
  xbox: {
    South: "A",
    East: "B",
    West: "X",
    North: "Y",
    LB: "LB",
    RB: "RB",
    LT: "LT",
    RT: "RT",
    Back: "View",
    Start: "Menu",
    Guide: "Guide",
  },
  playstation: {
    South: "Cross",
    East: "Circle",
    West: "Square",
    North: "Triangle",
    LB: "L1",
    RB: "R1",
    LT: "L2",
    RT: "R2",
    LS: "L3",
    RS: "R3",
    Back: "Share",
    Start: "Options",
    Guide: "Home",
  },
  nintendo: {
    South: "B",
    East: "A",
    West: "Y",
    North: "X",
    LB: "L",
    RB: "R",
    LT: "ZL",
    RT: "ZR",
    Back: "Minus",
    Start: "Plus",
    Guide: "Home",
  },
  generic: {
    South: "Bottom button",
    East: "Right button",
    West: "Left button",
    North: "Top button",
    LB: "Left bumper",
    RB: "Right bumper",
    LT: "Left trigger",
    RT: "Right trigger",
    Back: "Back",
    Start: "Start",
    Guide: "Home",
  },
  // A keyboard player's places are named by what each console calls them,
  // so the keyboard has no names of its own.
  keyboard: {},
};

const COMMON: Record<string, string> = {
  LS: "Left stick press",
  RS: "Right stick press",
  Up: "D-pad up",
  Down: "D-pad down",
  Left: "D-pad left",
  Right: "D-pad right",
  "LS Y+": "Left stick up",
  "LS Y-": "Left stick down",
  "LS X-": "Left stick left",
  "LS X+": "Left stick right",
  "RS Y+": "Right stick up",
  "RS Y-": "Right stick down",
  "RS X-": "Right stick left",
  "RS X+": "Right stick right",
};

export function nameOf(family: PadFamily, input: string): string {
  return FAMILY_NAMES[family][input] ?? COMMON[input] ?? input;
}

/// What the keyboard's named keys and the mouse are called for people. The
/// keys that print a character are named by it instead (`keyName`).
const KEY_WORDS: Record<string, string> = {
  Backspace: "Backspace",
  Tab: "Tab",
  Enter: "Enter",
  ShiftLeft: "Left Shift",
  ShiftRight: "Right Shift",
  Space: "Space",
  CapsLock: "Caps Lock",
  NumpadMultiply: "Num *",
  NumpadSubtract: "Num -",
  NumpadAdd: "Num +",
  NumpadDecimal: "Num .",
  NumpadEnter: "Num Enter",
  NumpadDivide: "Num /",
  Home: "Home",
  End: "End",
  PageUp: "Page Up",
  PageDown: "Page Down",
  Delete: "Delete",
  ArrowUp: "Up arrow",
  ArrowDown: "Down arrow",
  ArrowLeft: "Left arrow",
  ArrowRight: "Right arrow",
  MouseLeft: "Left click",
  MouseRight: "Right click",
  MouseMiddle: "Middle click",
  MouseBack: "Mouse back button",
  MouseForward: "Mouse forward button",
  WheelUp: "Wheel up",
  WheelDown: "Wheel down",
  MouseMoveLeft: "Mouse left",
  MouseMoveRight: "Mouse right",
  MouseMoveUp: "Mouse up",
  MouseMoveDown: "Mouse down",
};

/// What the keys that print a character print on a US keyboard, for when
/// the keyboard's own layout can't be read.
const US_PRINTS: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Backslash: "\\",
  IntlBackslash: "\\",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

/// What each key prints on the keyboard in use, by `KeyboardEvent.code`.
export type KeyLayout = ReadonlyMap<string, string> | null;

type LayoutReader = { keyboard?: { getLayoutMap(): Promise<ReadonlyMap<string, string>> } };

/// The keyboard's own layout, so the key right of T reads Z on a German
/// keyboard. Null where the browser can't tell, and the keys then read as on
/// a US keyboard. It never holds the screen up for long.
export async function keyboardLayout(): Promise<KeyLayout> {
  try {
    const reader = (navigator as Navigator & LayoutReader).keyboard;
    if (!reader) return null;
    const late = new Promise<null>((done) => window.setTimeout(() => done(null), 300));
    return await Promise.race([reader.getLayoutMap(), late]);
  } catch {
    return null;
  }
}

/// A key or mouse input as people call it: "W", "Space", "Left click". A key
/// that prints a character is named by what it prints on this keyboard.
export function keyName(input: string, layout: KeyLayout): string {
  const named = KEY_WORDS[input];
  if (named) return named;
  if (input.startsWith("Numpad")) return `Num ${input.slice("Numpad".length)}`;
  const printed = layout?.get(input);
  // A dead key, such as the accent key on many European keyboards, may come
  // back as nothing or as more than one character.
  if (printed && [...printed].length === 1) {
    const upper = printed.toUpperCase();
    return [...upper].length === 1 ? upper : printed;
  }
  if (/^Key[A-Z]$/.test(input)) return input.slice("Key".length);
  if (/^Digit[0-9]$/.test(input)) return input.slice("Digit".length);
  return US_PRINTS[input] ?? input;
}

/// A place's inputs, kept separated by spaces, each as people call it.
export function keyNames(inputs: string, layout: KeyLayout): string[] {
  return inputs
    .split(" ")
    .filter((input) => input)
    .map((input) => keyName(input, layout));
}
