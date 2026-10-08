import { convertFileSrc } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  addToLibrary,
  cancelCompatibility,
  catalogue,
  getSettings,
  listGames,
  onCompatProgress,
  refreshCompatibility,
  type CatalogueView,
  type Console,
  type Listing,
} from "../api";
import { placeholderArt } from "../components/art";
import { coverFor, knownCover } from "../components/catalogueCovers";
import { openImportSheet } from "../components/importSheet";
import { rawgCredit } from "../components/rawgCredit";
import { CATALOGUE_PAGE, store, type CatalogueChoice } from "../state";
import { chips, emptyState, type View } from "./view";

function tag(className: string, text: string): HTMLElement {
  const span = document.createElement("span");
  span.className = className;
  span.textContent = text;
  return span;
}

function card(listing: Listing, covers: boolean, region: string, selected: boolean): HTMLElement {
  // A div rather than a button: it holds the Add button, and a button cannot
  // hold another. Role and keys make it act like one.
  const card = document.createElement("div");
  card.className = selected ? "card on" : "card";
  card.tabIndex = 0;
  card.setAttribute("role", "button");
  const open = () => store.setCatalogueSelected({ listing });
  card.onclick = open;
  card.onkeydown = (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      open();
    }
  };

  card.innerHTML = `
    <div class="art">
      ${placeholderArt(listing.key, listing.name)}
    </div>
    <div class="card-name"></div>
    <div class="meta"></div>
  `;
  // A game's own name, so never through innerHTML.
  card.querySelector<HTMLElement>(".card-name")!.textContent = listing.name;

  // A Skylanders game can't be played without figures on the portal, so
  // whether Omoio's portal menu works in this version is said on its tile.
  if (listing.portal_menu !== null) {
    const works = listing.portal_menu;
    const mark = tag(works ? "badge go" : "badge warn", works ? "Works in Omoio" : "Not in Omoio yet");
    if (listing.portal_note) mark.title = listing.portal_note;
    card.querySelector<HTMLElement>(".art")!.appendChild(mark);
  }

  // One release is named by its id. A game released several times, or listed
  // without ids, by its console; every release is in the side panel.
  const meta = card.querySelector<HTMLElement>(".meta")!;
  meta.appendChild(
    listing.releases.length === 1
      ? tag("id", listing.releases[0].title_id)
      : tag("region", listing.console_name)
  );
  if (listing.regions.length > 0) {
    const lead = listing.regions.includes(region) ? region : listing.regions[0];
    const more = listing.regions.length - 1;
    meta.appendChild(tag("region", more > 0 ? `${lead} +${more}` : lead));
  }
  if (listing.status.label) {
    meta.appendChild(tag(`status ${listing.status.tone} tiny`, listing.status.label));
  }

  if (covers) {
    // RAWG's art is wide, like the tile, so it fills it.
    const art = card.querySelector<HTMLElement>(".art")!;
    const show = (path: string) => {
      art.querySelector("svg")?.remove();
      art.insertAdjacentHTML("afterbegin", `<img src="${convertFileSrc(path)}" alt="">`);
    };
    const known = knownCover(listing.key);
    if (known) {
      show(known);
    } else {
      // After this tick, once the view is on screen, so a tile that is about
      // to be replaced by the next keystroke is never asked about.
      setTimeout(() => {
        void coverFor(listing.key, listing.name, listing.console, () => card.isConnected).then(
          (path) => {
            if (path && card.isConnected) show(path);
          }
        );
      }, 0);
    }
  }

  const action = document.createElement("button");
  action.className = "small-btn wide";
  const release = listing.releases[0];
  if (listing.owned) {
    action.textContent = "In your library";
    action.disabled = true;
  } else if (release) {
    action.textContent = "Add to library";
    action.onclick = async (event) => {
      event.stopPropagation();
      action.disabled = true;
      try {
        await addToLibrary(listing.console, release.title_id, listing.name);
        action.textContent = "In your library";
        store.setGames(await listGames());
        store.redraw();
      } catch {
        action.textContent = "Already there";
      }
    };
  } else {
    // Without a title id there is nothing to note it down by, so the way in
    // is its own files.
    action.textContent = "Import game";
    action.onclick = (event) => {
      event.stopPropagation();
      openImportSheet();
    };
  }
  card.appendChild(action);

  return card;
}

const REGIONS: [string, string][] = [
  ["", "All"],
  ["EU", "EU"],
  ["US", "US"],
  ["JP", "JP"],
  ["Asia", "Asia"],
  ["KR", "KR"],
];

/// By tone rather than by each list's own words, since every emulator rates
/// games on its own scale.
const RUNS: [string, string][] = [
  ["", "Any"],
  ["go", "Plays well"],
  ["warn", "With problems"],
  ["bad", "Doesn't run"],
];

const SORTS: [string, string][] = [
  ["", "A to Z"],
  ["runs", "Runs best"],
];

function filterBar(view: CatalogueView, choice: CatalogueChoice): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "filter-bar";
  const set = (patch: Partial<CatalogueChoice>) => store.setCatalogueFilter(patch);

  const consoles: [Console | null, string][] = [
    [null, "All"],
    ...view.consoles.map(({ console, name }): [Console | null, string] => [console, name]),
  ];
  bar.appendChild(chips("Console", consoles, choice.console, (console) => set({ console })));
  bar.appendChild(chips("Region", REGIONS, choice.region, (region) => set({ region })));
  bar.appendChild(chips("Runs", RUNS, choice.runs, (runs) => set({ runs })));
  bar.appendChild(chips("Sort", SORTS, choice.sort, (sort) => set({ sort })));

  const demos = document.createElement("button");
  demos.className = choice.hideDemos ? "chip on" : "chip";
  demos.textContent = "Hide demos";
  demos.onclick = () => set({ hideDemos: !choice.hideDemos });
  const group = document.createElement("div");
  group.className = "filter-group";
  group.appendChild(demos);
  bar.appendChild(group);

  return bar;
}

/// Lists being downloaded, by console, so a view rebuilt halfway through shows
/// the same download instead of starting another.
const fetching = new Map<string, Promise<void>>();

/// Lists already fetched without being asked this run. One that fails or is
/// stopped waits for the button rather than trying again on every redraw.
const startedAlone = new Set<string>();

function fetchList(key: string, console?: Console): Promise<void> {
  const job = refreshCompatibility(console)
    .finally(() => fetching.delete(key))
    .then(() => store.redraw());
  fetching.set(key, job);
  return job;
}

/// Downloads compatibility lists, with progress and a way to stop, since the
/// biggest takes around twenty seconds. With `start`, a list that is not here
/// is fetched without waiting for the button.
function listGetter(label: string, console?: Console, start = false): HTMLElement {
  const box = document.createElement("div");
  box.className = "list-getter";

  const get = document.createElement("button");
  get.className = "small-btn";
  get.textContent = label;

  const bar = document.createElement("div");
  bar.className = "progress-row gone";
  bar.innerHTML = `
    <div class="progress-label"><span>Getting the list…</span><span class="pct"></span></div>
    <div class="progress"><div class="progress-fill" style="width:0%"></div></div>
  `;
  const fill = bar.querySelector<HTMLElement>(".progress-fill")!;
  const pct = bar.querySelector<HTMLElement>(".pct")!;

  const stop = document.createElement("button");
  stop.className = "link-btn gone";
  stop.textContent = "Stop";
  stop.onclick = () => void cancelCompatibility();

  function watch(job: Promise<void>) {
    get.classList.add("gone");
    bar.classList.remove("gone");
    stop.classList.remove("gone");
    const unlisten = onCompatProgress((progress) => {
      const done =
        progress.total > 0 ? Math.min(100, Math.round((progress.bytes / progress.total) * 100)) : 0;
      fill.style.width = `${done}%`;
      pct.textContent = `${done}%`;
    });
    void job
      .catch((err: unknown) => {
        bar.classList.add("gone");
        stop.classList.add("gone");
        get.classList.remove("gone");
        get.textContent = err === "cancelled" ? label : "Couldn't get it. Try again";
      })
      .finally(() => void unlisten.then((stopListening) => stopListening()));
  }

  const key = console ?? "every";
  get.onclick = () => watch(fetchList(key, console));
  const running = fetching.get(key);
  if (running) {
    watch(running);
  } else if (start && !startedAlone.has(key)) {
    startedAlone.add(key);
    watch(fetchList(key, console));
  }

  box.append(get, bar, stop);
  return box;
}

/// The search box is the shared one in the top bar rather than one of this
/// screen's own. This whole view is rebuilt on every keystroke, and a field
/// rebuilt under the cursor loses focus and what was typed into it.
export async function renderCatalogue(): Promise<View> {
  const { catalogueQuery, catalogueFilter: choice } = store.get();
  const query = catalogueQuery ?? "";
  const [view, settings] = await Promise.all([
    catalogue({
      query,
      console: choice.console,
      region: choice.region,
      runs: choice.runs,
      hide_demos: choice.hideDemos,
      sort: choice.sort,
      limit: choice.limit,
    }),
    getSettings(),
  ]);
  const covers = settings.covers && Boolean(settings.rawg_key);

  if (!view.have_list) {
    const content = emptyState(
      "No list yet",
      "The catalogue comes from each emulator's compatibility list. Omoio fetches them by itself, and after that it works offline."
    );
    content.appendChild(listGetter("Get the lists", undefined, true));
    return { title: "Catalogue", subtitle: "Nothing to browse yet", content };
  }

  const content = document.createElement("div");
  content.appendChild(filterBar(view, choice));

  // A console whose list is not here yet is said plainly, rather than looking
  // like a console with no games.
  const waitingFor = view.missing.filter((console) => !choice.console || choice.console === console);
  for (const console of waitingFor) {
    const name = view.consoles.find((c) => c.console === console)?.name ?? "";
    const row = document.createElement("div");
    row.className = "missing-row";
    row.append(
      tag("missing-text", `${name} games aren't in the catalogue yet.`),
      listGetter(`Get the ${name} list`, console, true)
    );
    content.appendChild(row);
  }

  if (view.shown.length === 0) {
    if (waitingFor.length === 0) {
      const none = document.createElement("div");
      none.className = "sheet-p";
      none.style.marginTop = "18px";
      none.textContent = query ? "No game by that name." : "Nothing matches these filters.";
      content.appendChild(none);
      const clear = document.createElement("button");
      clear.className = "link-btn";
      clear.textContent = "Clear filters";
      clear.onclick = () =>
        store.setCatalogueFilter({ console: null, region: "", runs: "", hideDemos: false, sort: "" });
      content.appendChild(clear);
    }
    return { title: "Catalogue", subtitle: "No matches", content };
  }

  const grid = document.createElement("div");
  grid.className = "grid";
  const picked = store.get().catalogueSelected?.listing.key;
  for (const listing of view.shown) {
    grid.appendChild(card(listing, covers, choice.region, listing.key === picked));
  }
  content.appendChild(grid);

  if (view.total > view.shown.length) {
    const row = document.createElement("div");
    row.className = "more-row";
    const more = document.createElement("button");
    more.className = "small-btn";
    more.textContent = `Show ${Math.min(CATALOGUE_PAGE, view.total - view.shown.length)} more`;
    more.onclick = () => store.setCatalogueFilter({ limit: choice.limit + CATALOGUE_PAGE });
    row.appendChild(more);
    content.appendChild(row);
  }

  const credits = document.createElement("div");
  credits.className = "credits";
  for (const source of view.sources) {
    const link = document.createElement("button");
    link.className = "link-btn";
    link.textContent = source.label;
    link.onclick = () => void openUrl(source.url);
    credits.appendChild(link);
  }
  if (covers) credits.appendChild(rawgCredit());
  content.appendChild(credits);

  const subtitle =
    view.total > view.shown.length
      ? `${view.shown.length} of ${view.total.toLocaleString()}`
      : `${view.total.toLocaleString()} ${view.total === 1 ? "game" : "games"}`;

  return { title: "Catalogue", subtitle, content };
}
