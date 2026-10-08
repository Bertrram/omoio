import "./styles/tokens.css";
import "./styles/app.css";

import {
  bigPicture,
  fetchCovers,
  getFirmwareVersion,
  getRpcs3Version,
  listGames,
  onBigPicture,
  onGameFullscreen,
  onGameStarted,
  onGameStopped,
  onPadNotFound,
  playingGame,
} from "./api";
import { renderBigPicture } from "./bigpicture/bigPicture";
import { renderPlayingBar, showPlayingNote } from "./components/playingBar";
import { openSetupIfNeeded } from "./components/setupSheet";
import { updateEmulatorsInBackground } from "./components/emulatorUpdates";
import { startUpdateChecks } from "./components/omoioUpdate";
import { watchForDroppedGames } from "./components/dropZone";
import { store, type ViewId } from "./state";
import { renderTitlebar } from "./components/titlebar";
import { renderSidebar } from "./components/sidebar";
import { renderDetail } from "./components/detail";
import { renderLibrary } from "./views/library";
import { renderCatalogue } from "./views/catalogue";
import { renderHomebrew } from "./views/homebrew";
import { renderController } from "./views/controller";
import { renderEmulators } from "./views/emulators";
import { renderUpdates } from "./views/updates";
import { renderSystem } from "./views/system";
import { renderLogs } from "./views/logs";
import { renderSettings } from "./views/settings";
import type { View } from "./views/view";

const VIEWS: Record<ViewId, () => View | Promise<View>> = {
  library: renderLibrary,
  catalogue: renderCatalogue,
  homebrew: renderHomebrew,
  controller: renderController,
  emulators: renderEmulators,
  updates: renderUpdates,
  system: renderSystem,
  logs: renderLogs,
  settings: renderSettings,
};

const app = document.getElementById("app")!;

const topbar = document.createElement("div");
topbar.className = "topbar";
topbar.innerHTML = `
  <div>
    <div class="view-title"></div>
    <div class="view-sub"></div>
  </div>
  <div class="search">
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/></svg>
    <input type="text" placeholder="Search your games…" />
  </div>
`;
const viewTitle = topbar.querySelector<HTMLElement>(".view-title")!;
const viewSub = topbar.querySelector<HTMLElement>(".view-sub")!;

const searchBox = topbar.querySelector<HTMLInputElement>(".search input")!;

// One box, searching whatever is on screen. The library holds a handful of
// games and filters as you type; the catalogue holds thousands and is asked
// once you pause, so the grid does not rebuild faster than it can be read.
let searchDelay: number | undefined;
searchBox.oninput = () => {
  window.clearTimeout(searchDelay);
  if (store.get().view === "catalogue") {
    searchDelay = window.setTimeout(() => store.setCatalogueQuery(searchBox.value), 200);
  } else {
    store.setSearch(searchBox.value);
  }
};

const content = document.createElement("div");
content.className = "content";

const main = document.createElement("main");
main.className = "main";
main.append(topbar, content);

const shell = document.createElement("div");
shell.className = "shell";
shell.append(renderSidebar(), main, renderDetail());

app.append(renderTitlebar(), shell, renderBigPicture());

const topbarNormal = [...topbar.children];

let renderToken = 0;
let searchingIn: ViewId | null = null;
// What the top bar is currently showing. Putting the same children back is not
// free: it takes the search box out of the document and returns it, which drops
// focus, so typing a second letter was impossible.
let barShowing: string | null = null;

store.subscribe((state) => {
  // Big Picture draws its own screens over all of this, so the desktop is
  // left as it was and picked up again when Big Picture closes.
  document.body.classList.toggle("bp-on", state.bigPicture);
  if (state.bigPicture) {
    renderToken++;
    return;
  }

  // While a game runs, its picture covers the content area, so the top bar
  // becomes the controls for it and the view underneath is left alone.
  if (state.playing) {
    if (barShowing !== state.playing.title_id) {
      barShowing = state.playing.title_id;
      topbar.replaceChildren(renderPlayingBar(state.playing));
    }
    content.replaceChildren();
    return;
  }
  if (barShowing !== null) {
    barShowing = null;
    topbar.replaceChildren(...topbarNormal);
  }

  // Only when the view changes, so what is being typed is never overwritten.
  if (state.view !== searchingIn) {
    searchingIn = state.view;
    const catalogue = state.view === "catalogue";
    searchBox.placeholder = catalogue ? "Search every game…" : "Search your games…";
    searchBox.value = catalogue ? state.catalogueQuery : state.search;
    // Nothing else has a search, so the box goes away rather than sitting
    // there doing nothing.
    searchBox.parentElement!.classList.toggle(
      "gone",
      state.view !== "catalogue" && state.view !== "library"
    );
  }

  const token = ++renderToken;
  Promise.resolve(VIEWS[state.view]()).then((view) => {
    if (token !== renderToken || store.get().playing) return;
    viewTitle.textContent = view.title;
    viewSub.textContent = view.subtitle;
    content.replaceChildren(view.content);
  });
});

getRpcs3Version().then((version) => store.setRpcs3Version(version));
getFirmwareVersion().then((version) => store.setFirmwareVersion(version));
// Omoio's own updates wait for the library, so the first moments after the
// window opens go to showing it.
listGames()
  .then((games) => store.setGames(games))
  .finally(startUpdateChecks);
playingGame().then((playing) => store.setPlaying(playing));
bigPicture().then((state) => store.setBigPicture(state));
void onBigPicture((state) => store.setBigPicture(state));

// Covers for games that appeared since the last look, however they came in.
// RAWG is asked about each game once, so looking again costs nothing for the
// ones already asked about, and nothing is asked while covers are off.
const coversAskedFor = new Set<string>();
store.subscribe(({ games }) => {
  if (!games || games.every((game) => coversAskedFor.has(game.title_id))) return;
  games.forEach((game) => coversAskedFor.add(game.title_id));
  const shown = games.filter((game) => game.cover_source === "rawg").length;
  fetchCovers()
    .then(async (found) => {
      if (found > shown) store.setGames(await listGames());
    })
    .catch(() => {});
});

// Asked once, before anything else is worth doing. Emulator updates wait for
// it, so a first run never downloads the same emulator twice.
void openSetupIfNeeded().then(() => updateEmulatorsInBackground());

// Dragging a game onto the window imports it, the same as the Import button.
void watchForDroppedGames();

onGameStarted((playing) => store.setPlaying(playing));
onGameFullscreen((on) => store.setGameFullscreen(on));
onPadNotFound(showPlayingNote);
onGameStopped(() => {
  store.setGameFullscreen(false);
  store.setPlaying(null);
  // A game quit from behind Big Picture is no longer waiting there.
  store.setBigPicture({ on: store.get().bigPicture, suspended: false });
});
