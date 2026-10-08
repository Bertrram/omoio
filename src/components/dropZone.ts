import { getCurrentWebview } from "@tauri-apps/api/webview";
import { importDropped } from "./importSheet";
import { store } from "../state";

/// Files dragged onto the window import the same way the Import button does.
///
/// The browser's own drag events never fire here: the window is native, so the
/// paths arrive from Tauri rather than from the DOM, and they are real paths
/// rather than File objects. That is what makes this work for a 19 GB folder.
export async function watchForDroppedGames(): Promise<void> {
  const overlay = document.createElement("div");
  overlay.className = "drop-over";
  overlay.innerHTML = `
    <div class="drop-card">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M12 3v12M7 10l5 5 5-5M4 20h16"/>
      </svg>
      <div class="drop-t">Drop to import</div>
      <div class="drop-s">A game folder, a disc image, or a .7z or .zip archive.</div>
    </div>
  `;
  document.body.appendChild(overlay);

  const show = (on: boolean) => overlay.classList.toggle("on", on);

  await getCurrentWebview().onDragDropEvent((event) => {
    // Nothing is offered while a game is running: the window is the game's
    // picture then, and there is nowhere for this to land.
    if (store.get().playing) return;

    if (event.payload.type === "enter") {
      show(event.payload.paths.length > 0);
    } else if (event.payload.type === "leave") {
      show(false);
    } else if (event.payload.type === "drop") {
      show(false);
      if (event.payload.paths.length > 0) {
        importDropped(event.payload.paths);
      }
    }
  });
}
