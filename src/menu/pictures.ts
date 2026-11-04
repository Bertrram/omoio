import { convertFileSrc } from "@tauri-apps/api/core";
import { figurePictures } from "../api";
import { image } from "./dom";

/// The pictures of the running game's pieces for the Disney Infinity base
/// menu and the LEGO Dimensions Toy Pad menu, read out of the player's own
/// copy of the game by the same program as the portal menu's, and named by
/// each piece's number as the emulators' lists give it
/// (src-tauri/src/figure_pictures.rs). Omoio's own drawing stands in for a
/// piece it has no picture of, and for one whose picture won't load.

let pictures: { folder: string; names: Set<string> } | null = null;
/// Pictures that wouldn't load, so the menu, drawn again on every press,
/// goes straight to the drawing for them.
const broken = new Set<string>();

/// Read again each time the menu opens, since the pictures can be got from
/// the game's page while the game runs.
export async function readPictures() {
  pictures = await figurePictures()
    .then((found) => ({ folder: found.folder, names: new Set(found.names) }))
    .catch(() => null);
}

/// A piece's own picture by its number, or `null` when Omoio has none.
export function pictureOf(number: number | null | undefined): string | null {
  if (number == null || !pictures?.names.has(String(number))) return null;
  const source = convertFileSrc(`${pictures.folder}\\${number}.png`);
  return broken.has(source) ? null : source;
}

/// Puts a piece's picture first in `into`, with the class `pictured` on it,
/// or else `shape`, Omoio's drawing. A picture that fails to load gives way
/// to the drawing where it was.
export function drawPiece(into: HTMLElement, source: string | null, shape: string) {
  if (!source) {
    into.insertAdjacentHTML("afterbegin", shape);
    return;
  }
  const picture = image(source);
  picture.onerror = () => {
    broken.add(source);
    into.classList.remove("pictured");
    picture.insertAdjacentHTML("afterend", shape);
    picture.remove();
  };
  into.classList.add("pictured");
  into.prepend(picture);
}
