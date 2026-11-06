import { renameToyFigure, type ToyFile } from "../api";
import { node } from "./dom";
import { hintRow } from "./input";
import { notify, problem } from "./notices";

/// The box a player names one of their own figure files in, over the Infinity
/// Base and Toy Pad menus. The name is typed on the keyboard. A pad can't
/// type, but its bottom face button saves what is there and its right one
/// leaves, so a pad player is never stuck in it.

/// How long a name may be. Windows takes far longer ones, but a tile shows
/// about this much.
const LONGEST = 60;

let open: {
  file: ToyFile;
  noun: string;
  box: HTMLElement;
  field: HTMLInputElement;
  saving: boolean;
  saved: (path: string) => Promise<void>;
} | null = null;

export function renaming(): boolean {
  return open !== null;
}

/// Opens the box on `file`, its name typed in it and selected, so typing
/// replaces it. `saved` is given the file's new path once it has one.
export function startRename(file: ToyFile, noun: string, saved: (path: string) => Promise<void>) {
  // The box covers the menu, so a click can't change the menu beneath it
  // while a name is typed, nor take the keyboard out of the box.
  const box = node("div", "portal-rename");
  box.addEventListener("pointerdown", (event) => {
    if (event.target !== field) event.preventDefault();
  });
  const card = node("div", "portal-rename-card");
  card.setAttribute("role", "dialog");
  card.setAttribute("aria-label", `Rename ${file.name}`);
  const field = node("input", "portal-rename-field");
  field.type = "text";
  field.value = file.name;
  field.maxLength = LONGEST;
  field.spellcheck = false;
  field.setAttribute("aria-label", `New name for the ${noun}`);
  field.addEventListener("keydown", (event) => {
    // The menu's own keys stay out of the box while it is typed in.
    event.stopPropagation();
    if (event.key === "Enter") {
      event.preventDefault();
      if (!event.repeat) void saveRename();
    } else if (event.key === "Escape") {
      event.preventDefault();
      stopRename();
    }
  });
  box.appendChild(card);
  card.append(
    node("div", "portal-rename-title", `Rename ${file.name}`),
    field,
    hintRow(
      [
        ["South", "Save"],
        ["East", "Cancel"],
      ],
      false,
    ),
  );
  open = { file, noun, box, field, saving: false, saved };
  attachRename(document.getElementById("portal")!);
  field.select();
}

/// A press of the pad while the box is open: the bottom face button saves
/// and the right one leaves. Nothing else does anything.
export function pressRename(input: string) {
  if (input === "South") void saveRename();
  else if (input === "East") stopRename();
}

/// Closes the box, renaming nothing.
export function stopRename() {
  if (!open) return;
  open.box.remove();
  open = null;
}

/// Puts the box back over the menu after the menu has drawn itself again,
/// with the keyboard still in it.
export function attachRename(root: HTMLElement) {
  if (!open) return;
  if (open.box.parentElement !== root) root.appendChild(open.box);
  open.field.focus();
}

async function saveRename() {
  if (!open || open.saving) return;
  const now = open;
  now.saving = true;
  try {
    const path = await renameToyFigure(now.file.path, now.field.value);
    if (open === now) stopRename();
    await now.saved(path);
  } catch (err) {
    now.saving = false;
    notify(problem(err, `Couldn't rename that ${now.noun}.`));
    if (open === now) now.field.focus();
  }
}
