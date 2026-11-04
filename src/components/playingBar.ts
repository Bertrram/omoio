import { listGames, openPortalMenu, setGameFullscreen, stopGame, type Playing, type Toy } from "../api";
import { store } from "../state";

/// The button over a game whose toy's menu works, by the toy, and what it
/// says when the menu won't open. Skylanders' keeps the words it has always
/// had.
const MENU_BUTTON: Record<Toy, { label: string; failed: string }> = {
  skylanders: { label: "Portal (Insert or Tab)", failed: "Couldn't open the portal menu." },
  infinity: { label: "Infinity Base (Insert or Tab)", failed: "Couldn't open the base menu." },
  toypad: { label: "Toy Pad (Insert or Tab)", failed: "Couldn't open the Toy Pad menu." },
};

/// Shown across the top of the content area while a game is running. The game
/// picture sits below it, so this is the only part of Omoio visible mid-game
/// and it stays deliberately small.
export function renderPlayingBar(playing: Playing): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "playing-bar";
  bar.innerHTML = `
    <span class="playing-dot"></span>
    <span class="playing-name"></span>
    <span class="playing-note" role="status"></span>
    <div class="playing-actions">
      <button class="small-btn gone" id="open-portal">Portal (Insert or Tab)</button>
      <button class="small-btn" id="toggle-fullscreen">Fullscreen</button>
      <button class="small-btn danger" id="stop-game">Stop</button>
    </div>
  `;
  bar.querySelector<HTMLElement>(".playing-name")!.textContent = playing.title;

  // The toy's menu, the portal menu or another, for a player whose pad
  // Omoio can't read, only in a game the menu works in. Insert opens and
  // closes it too while the game or Omoio is in front, and Tab while the
  // game or the menu is (MENU_KEY and SECOND_MENU_KEY in
  // src-tauri/src/portal_menu.rs), which also works once the picture covers
  // the screen and this button is behind it.
  // The list of games may come in after the bar is drawn, as when a game
  // is started from Big Picture before the library was ever shown, so the
  // button is shown once it does. The bar goes when the game ends.
  const portalButton = bar.querySelector<HTMLButtonElement>("#open-portal")!;
  let menu = MENU_BUTTON.skylanders;
  portalButton.onclick = async () => {
    try {
      await openPortalMenu();
    } catch (err) {
      showPlayingNote(typeof err === "string" ? err : menu.failed);
    }
  };
  const stopWatching = store.subscribe(({ games, playing: now }) => {
    if (now?.title_id !== playing.title_id) {
      queueMicrotask(() => stopWatching());
      return;
    }
    const game = games?.find((each) => each.title_id === playing.title_id);
    menu = MENU_BUTTON[game?.toy ?? "skylanders"];
    portalButton.textContent = menu.label;
    portalButton.classList.toggle("gone", !game?.toy_menu);
  });

  const fullscreenButton = bar.querySelector<HTMLButtonElement>("#toggle-fullscreen")!;
  // The key is on the button because once the picture covers the screen the
  // button is behind it, and F11 is then the only way back.
  fullscreenButton.textContent = store.get().gameFullscreen
    ? "Leave fullscreen (F11)"
    : "Fullscreen (F11)";
  fullscreenButton.onclick = async () => {
    const next = !store.get().gameFullscreen;
    await setGameFullscreen(next);
    store.setGameFullscreen(next);
  };

  const stopButton = bar.querySelector<HTMLButtonElement>("#stop-game")!;
  stopButton.onclick = async () => {
    // Dolphin takes a moment to close, as it writes out the figures first.
    stopButton.textContent = "Stopping…";
    await stopGame();
    store.setGameFullscreen(false);
    store.setPlaying(null);
    store.setGames(await listGames());
  };

  return bar;
}

/// Says what went wrong with the running game beside its name, the one place
/// it can be read without leaving the game. It stays until the game stops.
export function showPlayingNote(message: string): void {
  const note = document.querySelector<HTMLElement>(".playing-note");
  if (note) note.textContent = message;
}
