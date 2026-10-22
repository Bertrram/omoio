import type { PadFamily } from "../api";
import { PAD_BOX, padArt, padSpots } from "./padArt";

/// A label, or a box of labels, beside the drawing with one line to a place
/// on it. Each row may carry `data-part`, the part of the drawing it lights
/// while pointed at.
export interface Callout {
  key: string;
  /// The place on the drawing the line points at.
  at: string;
  side: "left" | "right" | "below";
  rows: HTMLElement[];
}

/// The labels around the drawing. The shoulders get one each, the face
/// buttons and the middle buttons a box each with one line, so lines don't
/// cross the pad to reach them.
export const SHOULDERS: [string, "left" | "right"][] = [
  ["LT", "left"],
  ["LB", "left"],
  ["RT", "right"],
  ["RB", "right"],
];
export const FACE = ["South", "East", "West", "North"];
export const MIDDLE = ["Back", "Guide", "Start"];

/// The d-pad and each stick get one label that opens their directions,
/// which are rarely changed.
export const GROUPS: { key: string; title: string; side: "left" | "right"; places: string[] }[] = [
  { key: "Dpad", title: "D-pad", side: "left", places: ["Up", "Down", "Left", "Right"] },
  { key: "LS", title: "Left stick", side: "left", places: ["LS", "LS Y+", "LS Y-", "LS X-", "LS X+"] },
  { key: "RS", title: "Right stick", side: "right", places: ["RS", "RS Y+", "RS Y-", "RS X-", "RS X+"] },
];

/// Every place as a list in groups, as the labels around the drawing group
/// them, for a player on the keyboard, which has no drawing.
export const KEY_GROUPS: { title: string; places: string[] }[] = [
  { title: "Face buttons", places: FACE },
  { title: "Shoulders and triggers", places: ["LB", "RB", "LT", "RT"] },
  { title: "Middle buttons", places: MIDDLE },
  ...GROUPS.map(({ title, places }) => ({ title, places })),
];

/// Within a group, the direction alone says enough.
export const DIRECTION: Record<string, string> = {
  Up: "Up",
  Down: "Down",
  Left: "Left",
  Right: "Right",
  LS: "Press",
  RS: "Press",
  "LS Y+": "Up",
  "LS Y-": "Down",
  "LS X-": "Left",
  "LS X+": "Right",
  "RS Y+": "Up",
  "RS Y-": "Down",
  "RS X-": "Left",
  "RS X+": "Right",
};

/// Gives `place` this input. An input another place had is swapped over,
/// so no button ever does two things by accident.
export function assignInput(buttons: Record<string, string>, place: string, input: string): void {
  const was = buttons[place];
  for (const other of Object.keys(buttons)) {
    if (other !== place && buttons[other] === input) buttons[other] = was;
  }
  buttons[place] = input;
}

const SVG = "http://www.w3.org/2000/svg";
/// How far before its label a line turns to run level into it, in pixels.
const TURN = 18;

/// The pad with its labels around it. The labels are laid out by the page,
/// in columns either side and a row below; the lines are drawn from where
/// they ended up, and drawn again whenever the stage changes size.
export function padStage(family: PadFamily, callouts: Callout[]): { stage: HTMLElement; art: SVGSVGElement } {
  const stage = document.createElement("div");
  stage.className = "pad-stage";
  const left = document.createElement("div");
  left.className = "pad-col left";
  const right = document.createElement("div");
  right.className = "pad-col right";
  const holder = document.createElement("div");
  holder.className = "pad-art-holder";
  holder.innerHTML = padArt(family);
  const art = holder.querySelector("svg")!;
  const below = document.createElement("div");
  below.className = "pad-below";
  const lines = document.createElementNS(SVG, "svg");
  lines.setAttribute("class", "pad-lines");
  lines.setAttribute("aria-hidden", "true");
  stage.append(left, holder, right, below, lines);

  const spots = padSpots(family);
  // Top to bottom by the place each points at, so lines leave in order.
  const placed = [...callouts].sort((a, b) => spots[a.at][1] - spots[b.at][1]);
  const marks = new Map<string, { line: SVGPathElement; dot: SVGCircleElement; box: HTMLElement }>();
  for (const callout of placed) {
    const box = document.createElement("div");
    box.className = `pad-callout ${callout.side}${callout.rows.length > 1 ? " box" : ""}`;
    box.append(...callout.rows);
    ({ left, right, below })[callout.side].append(box);
    const line = document.createElementNS(SVG, "path");
    line.setAttribute("class", "pad-lead");
    const dot = document.createElementNS(SVG, "circle");
    dot.setAttribute("class", "pad-lead-dot");
    dot.setAttribute("r", "3");
    lines.append(line, dot);
    marks.set(callout.key, { line, dot, box });

    // A row lights its line and its part of the drawing while pointed at.
    for (const row of callout.rows) {
      const lit = (on: boolean) => {
        line.classList.toggle("lit", on);
        dot.classList.toggle("lit", on);
        const part = row.dataset.part;
        if (part) art.querySelector(`[data-part="${part}"]`)?.classList.toggle("hot", on);
      };
      const still = () => document.activeElement === row || row.classList.contains("focused");
      row.addEventListener("mouseenter", () => lit(true));
      row.addEventListener("mouseleave", () => lit(still()));
      row.addEventListener("focus", () => lit(true));
      row.addEventListener("blur", () => lit(false));
    }
  }

  const draw = () => {
    const room = stage.getBoundingClientRect();
    const pic = art.getBoundingClientRect();
    if (room.width === 0 || pic.width === 0) return;
    lines.setAttribute("viewBox", `0 0 ${room.width} ${room.height}`);
    for (const callout of placed) {
      const { line, dot, box } = marks.get(callout.key)!;
      const [sx, sy] = spots[callout.at];
      const ax = pic.left - room.left + ((sx - PAD_BOX.x) / PAD_BOX.width) * pic.width;
      const ay = pic.top - room.top + ((sy - PAD_BOX.y) / PAD_BOX.height) * pic.height;
      const r = box.getBoundingClientRect();
      let d: string;
      if (callout.side === "below") {
        const bx = r.left - room.left + r.width / 2;
        const by = r.top - room.top;
        d = `M${ax} ${ay}V${by - TURN}H${bx}V${by}`;
      } else {
        const by = r.top - room.top + r.height / 2;
        const edge = callout.side === "left" ? r.right - room.left : r.left - room.left;
        const turn = callout.side === "left" ? edge + TURN : edge - TURN;
        d = `M${ax} ${ay}L${turn} ${by}H${edge}`;
      }
      line.setAttribute("d", d);
      dot.setAttribute("cx", String(ax));
      dot.setAttribute("cy", String(ay));
    }
  };
  new ResizeObserver(draw).observe(stage);
  return { stage, art };
}

/// What a place does on each console, as short tagged words: "PS3 Cross
/// Wii U B". A console without that place is left out.
export function consoleWords(consoles: { name: string; buttons: Record<string, string> }[], place: string): HTMLElement {
  const words = document.createElement("span");
  words.className = "pad-words";
  for (const console of consoles) {
    const name = console.buttons[place];
    if (!name) continue;
    const part = document.createElement("span");
    part.className = "pad-word";
    const tag = document.createElement("span");
    tag.className = "pad-console";
    tag.textContent = console.name;
    part.append(tag, document.createTextNode(` ${name}`));
    words.append(part);
  }
  return words;
}
