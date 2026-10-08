import { listGames, setGameFullscreen, stopGame, type Playing } from "../api";
import { store } from "../state";

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
      <button class="small-btn" id="toggle-fullscreen">Fullscreen</button>
      <button class="small-btn danger" id="stop-game">Stop</button>
    </div>
  `;
  bar.querySelector<HTMLElement>(".playing-name")!.textContent = playing.title;

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

  bar.querySelector<HTMLButtonElement>("#stop-game")!.onclick = async () => {
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
