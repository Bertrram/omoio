import { convertFileSrc } from "@tauri-apps/api/core";
import {
  addToLibrary,
  figurePictures,
  gameCompatibility,
  communityPacks,
  gameSaves,
  gameSettings,
  gameUpdates,
  getFirmwareVersion,
  launchGame,
  launchWarning,
  listGames,
  padsHeld,
  portalButton,
  refreshCompatibility,
  getFigurePictures,
  onFigurePictures,
  removeGame,
  revealFolder,
  setPortalButton,
  stopFigurePictures,
  CONSOLE_SHORT,
  type Game,
} from "../api";
import { fitCovers, placeholderArt } from "./art";
import { firmwareActions } from "./firmware";
import { nameOf } from "./padNames";
import type { CatalogueSelection } from "../state";
import { openGameSettings } from "./gameSettingsSheet";
import { openImportSheet } from "./importSheet";
import { rawgCredit } from "./rawgCredit";
import { knownCover } from "./catalogueCovers";
import { openPacks, packCount } from "./packsSheet";
import { openSaves } from "./savesSheet";
import { openUpdates } from "./updatesSheet";
import { store } from "../state";

function formatSize(bytes: number): string {
  if (bytes <= 0) return "unknown";
  const gb = bytes / 1024 ** 3;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(bytes / 1024 ** 2)} MB`;
}

function row(label: string, value: string, tone = ""): string {
  return `<div class="row"><span class="row-k">${label}</span><span class="row-v ${tone}">${value}</span></div>`;
}

/// Line icons for the panel's rows, on the sidebar's 16-unit grid.
const ICON = {
  version: '<path d="M13.5 8a5.5 5.5 0 1 1-1.9-4.2M13 2v3.5h-3.5"/>',
  saves: '<path d="M3 2.5h7.8l2.7 2.7v8.3H3z"/><path d="M5.5 2.5v3h4.5v-3M5.3 13.5V9.5h5.4v4"/>',
  settings:
    '<path d="M2.5 4.5h1.4M7.1 4.5h6.4M2.5 8h5.9M11.6 8h1.9M2.5 11.5h2.4M8.1 11.5h5.4"/><circle cx="5.5" cy="4.5" r="1.6"/><circle cx="10" cy="8" r="1.6"/><circle cx="6.5" cy="11.5" r="1.6"/>',
  packs: '<rect x="1.6" y="5.7" width="12.8" height="4.6" rx="2.3" transform="rotate(-45 8 8)"/><path d="M6.6 6.6l2.8 2.8"/>',
  portal: '<ellipse cx="8" cy="6.5" rx="5.5" ry="2.3"/><path d="M2.5 6.5v3c0 1.3 2.5 2.3 5.5 2.3s5.5-1 5.5-2.3v-3"/>',
  pictures: '<rect x="2" y="3" width="12" height="10" rx="1.5"/><circle cx="6" cy="6.5" r="1.2"/><path d="M2.5 12l3.5-3.5 2.5 2.5 2-2 3 3"/>',
  folder: '<path d="M2 4.5a1 1 0 0 1 1-1h3l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>',
  chevron: '<path d="M6 3.5L10.5 8 6 12.5"/>',
};

function icon(paths: string, className = "d-ico"): string {
  return `<svg class="${className}" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">${paths}</svg>`;
}

/// A row that opens a sheet, with where things stand on its right.
function listRow(id: string, paths: string, label: string): string {
  return `<button class="d-row" id="detail-${id}">${icon(paths)}<span class="d-row-k">${label}</span><span class="d-row-v"></span>${icon(ICON.chevron, "d-chev")}</button>`;
}

function fill(body: HTMLElement, hero: HTMLElement, game: Game): void {
  hero.innerHTML = game.cover
    ? `<img class="cover" src="${convertFileSrc(game.cover)}" alt="">`
    : placeholderArt(game.title_id, game.title);
  fitCovers(hero);
  if (game.cover_source === "rawg") hero.appendChild(rawgCredit());

  body.innerHTML = `
    <div class="d-head">
      <div class="d-name"><div class="d-title"></div><div class="d-sub"></div></div>
      <span class="status gone" id="detail-compat-badge"></span>
    </div>
    <button class="play" id="detail-play">
      <svg viewBox="0 0 12 14" fill="currentColor"><path d="M1 1l10 6-10 6z"/></svg>Play
    </button>
    <div class="note" id="detail-note"></div>
    <div class="d-list" id="detail-list">
      ${listRow("update", ICON.version, "Game version")}
      ${listRow("saves", ICON.saves, "Saved games")}
      ${listRow("settings", ICON.settings, "Settings")}
      ${listRow("packs", ICON.packs, "Community packs")}
    </div>
    <div class="note plain" id="detail-list-note"></div>
    <div class="note plain gone" id="detail-compat">
      <span id="detail-compat-note"></span>
      <button class="link-btn gone" id="detail-compat-get"></button>
    </div>
    <div class="gone" id="detail-portal">
      <div class="d-group-h">
        <span class="sec-h">Skylanders</span>
        <button class="help-dot" id="detail-portal-help" aria-label="How the portal menu works" aria-expanded="false">?</button>
      </div>
      <div class="note plain gone" id="detail-portal-about"></div>
      <div class="d-list">
        <button class="d-row" id="detail-portal-button">
          ${icon(ICON.portal)}<span class="d-row-k">Portal menu button</span><span class="d-key"></span>
        </button>
        <button class="d-row" id="detail-pictures">
          ${icon(ICON.pictures)}<span class="d-row-k">Figure pictures</span><span class="d-row-v"></span>
          <span class="d-row-bar gone"><span class="d-row-fill"></span></span>
        </button>
      </div>
      <div class="note plain" id="detail-pictures-note"></div>
    </div>
    <div class="d-foot">
      <button class="link-btn" id="detail-reveal">${icon(ICON.folder, "")}Show files</button>
      <button class="link-btn" id="detail-remove">Remove from library</button>
    </div>
  `;
  // Set through textContent so a game's own name is never treated as markup.
  body.querySelector<HTMLElement>(".d-title")!.textContent = game.title;
  const offers = game.features;
  body.querySelector<HTMLElement>(".d-sub")!.textContent = [
    CONSOLE_SHORT[game.console],
    game.title_id,
    game.size_bytes > 0 ? formatSize(game.size_bytes) : "",
    // A game without an update list has its version nowhere else.
    !offers.updates && game.version ? `version ${game.version}` : "",
  ]
    .filter(Boolean)
    .join(" · ");

  const note = body.querySelector<HTMLElement>("#detail-note")!;
  const play = body.querySelector<HTMLButtonElement>("#detail-play")!;
  const list = body.querySelector<HTMLElement>("#detail-list")!;
  const listNote = body.querySelector<HTMLElement>("#detail-list-note")!;
  const reveal = body.querySelector<HTMLButtonElement>("#detail-reveal")!;
  const valueOf = (id: string) => body.querySelector<HTMLElement>(`#detail-${id} .d-row-v`)!;
  play.disabled = !game.available;

  // Only what this game's emulator can do. A row with nothing behind it is
  // left out rather than shown as a button that does nothing.
  for (const [on, id] of [
    [offers.updates, "update"],
    [offers.saves, "saves"],
    [offers.settings, "settings"],
    [offers.packs, "packs"],
  ] as const) {
    if (!on) body.querySelector(`#detail-${id}`)?.classList.add("gone");
  }
  if (!offers.updates && !offers.saves && !offers.settings && !offers.packs) list.classList.add("gone");

  if (!game.set_up) {
    // Noted from the catalogue and never imported. It says what to do and
    // gives you the way to do it, rather than reporting a fault.
    note.textContent = "Import this game's files to play it.";
    const importIt = document.createElement("button");
    importIt.className = "small-btn wide";
    importIt.textContent = "Import game";
    importIt.onclick = openImportSheet;
    note.after(importIt);

    // Nothing about a version, saves or the emulator means anything until
    // the files are here. How well it runs still does: it says whether this
    // is worth setting up at all. Remove stays, since taking the note back
    // off the list has to stay possible.
    list.classList.add("gone");
    reveal.classList.add("gone");
  } else if (!game.available) {
    note.textContent = "Reconnect the drive this game is on to play it.";
    reveal.classList.add("gone");
  }

  // A PS3 game won't start without the firmware, so the panel says so before
  // Play is pressed and offers the way to add it right here. Only once RPCS3
  // is there, since it is what unpacks the file, and only once the version
  // has been read: undefined means not known yet, not missing.
  let askedForFirmware = false;
  const askForFirmware = () => {
    if (askedForFirmware) return;
    askedForFirmware = true;
    note.textContent = "Add PS3 firmware first, then you can play. Sony publishes it free: download the file, then choose it here.";
    note.after(firmwareActions());
  };
  const { rpcs3Version, firmwareVersion } = store.get();
  if (game.console === "ps3" && game.set_up && game.available && rpcs3Version && firmwareVersion === null) {
    askForFirmware();
  }

  reveal.onclick = async () => {
    try {
      await revealFolder(game.path);
    } catch {
      listNote.textContent = "Couldn't open the game's folder.";
    }
  };

  // The button that opens the portal menu over a Skylanders game, shown only
  // for a game the menu works in. Other Skylanders games show nothing rather
  // than a menu that won't open.
  if (game.portal_menu && game.set_up) {
    body.querySelector("#detail-portal")!.classList.remove("gone");
    const keyRow = body.querySelector<HTMLButtonElement>("#detail-portal-button")!;
    const key = keyRow.querySelector<HTMLElement>(".d-key")!;
    const help = body.querySelector<HTMLButtonElement>("#detail-portal-help")!;
    const about = body.querySelector<HTMLElement>("#detail-portal-about")!;
    const picturesNote = body.querySelector<HTMLElement>("#detail-pictures-note")!;
    about.textContent =
      "Skylanders games need a toy portal, and the emulator pretends one is plugged in. While playing, press this button to open the portal menu over the game. Pick any character under its element, or an item or adventure pack, and it goes on the portal and is saved with its progress. Several can be on at once. It all works with the pad. The home button is the best choice, since games don't use it. If Windows' Game Bar opens instead, switch off its controller button in Windows Settings, under Gaming, Xbox Game Bar.";
    help.onclick = () => {
      const open = !about.classList.toggle("gone");
      help.setAttribute("aria-expanded", String(open));
    };

    const label = (place: string) => (place === "Guide" ? "Home button" : nameOf("generic", place));
    const showKey = async () => {
      key.textContent = label(await portalButton());
    };
    void showKey();

    // Waits a few seconds for a press on any pad. A stick pushed a little
    // is not a press, so a pad resting off centre never records.
    let recording = false;
    keyRow.onclick = async () => {
      if (recording) return;
      recording = true;
      key.classList.add("on");
      key.textContent = "Press a button…";
      const down = new Set(await padsHeld());
      const until = Date.now() + 6000;
      let pressed: string | undefined;
      while (!pressed && Date.now() < until) {
        await new Promise((resolve) => setTimeout(resolve, 60));
        const now = await padsHeld();
        pressed = now.find((input) => !down.has(input) && !/^(LS|RS) [XY][+-]$/.test(input));
        for (const input of [...down]) if (!now.includes(input)) down.delete(input);
      }
      if (pressed) {
        try {
          await setPortalButton(pressed);
        } catch (err) {
          picturesNote.textContent = typeof err === "string" ? err : "Couldn't save that button.";
        }
      }
      recording = false;
      key.classList.remove("on");
      await showKey();
    };

    // Each figure's own picture, read out of the user's copy of the game by
    // a small program Omoio fetches the first time. The menu works without
    // them, so the row only offers.
    const pictures = body.querySelector<HTMLButtonElement>("#detail-pictures")!;
    const picturesValue = pictures.querySelector<HTMLElement>(".d-row-v")!;
    const picturesBar = pictures.querySelector<HTMLElement>(".d-row-bar")!;
    const picturesFill = pictures.querySelector<HTMLElement>(".d-row-fill")!;
    const showPictures = (count: number) => {
      picturesValue.className = count > 0 ? "d-row-v" : "d-row-v ask";
      picturesValue.textContent = count > 0 ? `${count} pictures` : "Get pictures";
      pictures.title = count > 0 ? "Read them again" : "";
      picturesNote.textContent =
        count > 0
          ? ""
          : "Shows each figure's own picture in the portal menu, read from your copy of the game. Takes under a minute.";
    };
    const countPictures = () =>
      figurePictures(game.title_id)
        .then((found) => found.names.length)
        .catch(() => 0);
    void countPictures().then(showPictures);

    // A disc image can't be read as it is. The first press says what a
    // temporary copy would take; pressing again makes it, and it is gone
    // again once the pictures are read.
    const gigabytes = (bytes: number) => `${Math.ceil(bytes / 2 ** 30)} GB`;
    const sayWithSizes = (parts: (string | number)[]) => {
      picturesNote.replaceChildren(
        ...parts.map((part) => {
          if (typeof part === "string") return document.createTextNode(part);
          const size = document.createElement("span");
          size.className = "n";
          size.textContent = gigabytes(part);
          return size;
        })
      );
    };
    let reading = false;
    let copyAgreed = false;
    pictures.onclick = async () => {
      if (reading) {
        await stopFigurePictures();
        return;
      }
      reading = true;
      const copy = copyAgreed;
      copyAgreed = false;
      picturesValue.className = "d-row-v";
      picturesValue.innerHTML = `<span class="pct"></span><span class="d-key">Stop</span>`;
      const pct = picturesValue.querySelector<HTMLElement>(".pct")!;
      picturesNote.textContent = copy ? "Cemu is making a temporary copy of the game…" : "Reading the pictures from your game…";
      picturesFill.style.width = "0%";
      picturesBar.classList.remove("gone");
      const unlisten = onFigurePictures((progress) => {
        if (progress.title_id !== game.title_id || progress.of === 0) return;
        const done = Math.round((progress.done / progress.of) * 100);
        picturesFill.style.width = `${done}%`;
        pct.textContent = `${done}%`;
        picturesNote.textContent =
          progress.step === "copy" ? "Cemu is making a temporary copy of the game…" : "Reading the pictures from your game…";
      });
      try {
        const got = await getFigurePictures(game.title_id, copy);
        if ("pictures" in got) {
          showPictures(got.pictures);
        } else {
          showPictures(await countPictures());
          const { need, free } = got.copy;
          if (free >= need) {
            copyAgreed = true;
            picturesValue.className = "d-row-v ask";
            picturesValue.textContent = "Make a copy";
            sayWithSizes([
              "Omoio can't read this copy of the game as it is. Cemu can make a temporary copy to read the pictures from, and Omoio deletes it afterwards. It needs ",
              need,
              " free for a few minutes, and ",
              free,
              " is free.",
            ]);
          } else {
            sayWithSizes(["Reading the pictures needs ", need, " free for a few minutes, and ", free, " is free. Free some room first."]);
          }
        }
      } catch (err) {
        showPictures(await countPictures());
        picturesNote.textContent = typeof err === "string" ? err : "Couldn't read the pictures. Try again.";
      } finally {
        reading = false;
        picturesBar.classList.add("gone");
        void unlisten.then((stopListening) => stopListening());
      }
    };
  }
  const start = async () => {
    play.disabled = true;
    note.textContent = "Starting…";
    try {
      await launchGame(game.title_id);
      store.setSelected(null);
    } catch (err) {
      note.textContent = typeof err === "string" ? err : "Couldn't start this game.";
      play.disabled = false;
      // The firmware can go missing or be removed outside Omoio, which leaves
      // the version read at startup behind. Asking again after a failed start
      // still shows the way to add it.
      if (game.console === "ps3" && store.get().rpcs3Version && (await getFirmwareVersion()) === null) {
        askForFirmware();
        if (store.get().firmwareVersion !== null) store.setFirmwareVersion(null);
      }
    }
  };
  play.onclick = async () => {
    play.disabled = true;
    // A game nobody can answer says so first, and starts only if asked to.
    const warning = await launchWarning(game.title_id).catch(() => null);
    if (!warning) return start();
    note.textContent = warning;
    const anyway = document.createElement("button");
    anyway.className = "link-btn";
    anyway.textContent = "Start anyway";
    anyway.onclick = () => void start();
    note.append(" ", anyway);
    play.disabled = false;
  };

  // The sheet asks Sony when it opens. Opening a game should not quietly
  // make that request, so the row only shows the version it runs.
  const running = game.update_version ?? game.version;
  const updateValue = valueOf("update");
  updateValue.classList.add("mono");
  updateValue.textContent = running ?? "unknown";
  const reload = async () => store.setGames(await listGames());
  body.querySelector<HTMLButtonElement>("#detail-update")!.onclick = () =>
    openUpdates(game.title_id, game.title, running, () => {
      void reload();
      void showPackCount();
    });

  const showSaves = async () => {
    const [hasSaves, backups] = await gameSaves(game.title_id);
    valueOf("saves").textContent = !hasSaves
      ? "None yet"
      : backups.length === 0
        ? "Not backed up"
        : backups.length === 1
          ? "1 copy"
          : `${backups.length} copies`;
  };
  if (offers.saves && game.set_up) void showSaves();
  body.querySelector<HTMLButtonElement>("#detail-saves")!.onclick = () =>
    openSaves(game.title_id, game.title, showSaves);

  const compat = body.querySelector<HTMLElement>("#detail-compat")!;
  const badge = body.querySelector<HTMLElement>("#detail-compat-badge")!;
  const compatNote = body.querySelector<HTMLElement>("#detail-compat-note")!;
  const getList = body.querySelector<HTMLButtonElement>("#detail-compat-get")!;
  const showCompat = async () => {
    const result = await gameCompatibility(game.title_id);
    badge.textContent = result.label;
    badge.className = `status ${result.tone}`;
    compat.classList.remove("gone");
    compatNote.textContent = result.checked
      ? `${result.explanation} Last reported ${result.checked}.`
      : result.explanation;
    // Only offered when it would do something: the list is missing, or old
    // enough that a game's result may have moved on.
    getList.classList.toggle("gone", !result.stale);
    getList.textContent = result.have_list ? "Check for newer results" : "Get the list";
  };
  getList.onclick = async () => {
    getList.disabled = true;
    getList.textContent = "Getting…";
    try {
      await refreshCompatibility(game.console);
      await showCompat();
    } catch (err) {
      compatNote.textContent =
        typeof err === "string" ? err : "Couldn't get the compatibility list.";
    } finally {
      getList.disabled = false;
    }
  };
  if (offers.compatibility) void showCompat();

  const showPackCount = async () => {
    const packs = await communityPacks(game.title_id);
    const value = valueOf("packs");
    // Not downloaded yet, the row offers it rather than reporting a lack.
    value.classList.toggle("ask", !packs.have_list);
    value.textContent = packs.have_list ? packCount(packs) : "Download";
  };
  if (offers.packs && game.set_up) void showPackCount();
  body.querySelector<HTMLButtonElement>("#detail-packs")!.onclick = () =>
    openPacks(game.title_id, game.title, game.console, showPackCount);

  const settingsRow = body.querySelector<HTMLButtonElement>("#detail-settings")!;
  const showSettingsCount = async () => {
    try {
      const { chosen } = await gameSettings(game.title_id);
      const changed = Object.keys(chosen).length;
      settingsRow.disabled = false;
      valueOf("settings").textContent = changed === 0 ? "Default" : `${changed} changed`;
    } catch (err) {
      // A Wii U disc image's settings are filed under an id only known once
      // the game has run, which the message says.
      settingsRow.disabled = true;
      valueOf("settings").textContent = "";
      listNote.textContent = typeof err === "string" ? err : "Couldn't read this game's settings.";
    }
  };
  if (offers.settings && game.set_up) void showSettingsCount();
  settingsRow.onclick = () => openGameSettings(game.title_id, game.title, showSettingsCount);

  body.querySelector<HTMLButtonElement>("#detail-remove")!.onclick = async () => {
    await removeGame(game.title_id);
    store.setSelected(null);
    store.setGames(await listGames());
  };
}

/// A game from the catalogue: maybe owned, maybe never seen, so everything
/// here is about whether it is worth getting and what is known about it.
function fillListing(body: HTMLElement, hero: HTMLElement, { listing }: CatalogueSelection): void {
  const cover = knownCover(listing.key);
  if (cover) {
    hero.innerHTML = `<img src="${convertFileSrc(cover)}" alt="">`;
    hero.appendChild(rawgCredit());
  } else {
    hero.innerHTML = placeholderArt(listing.key, listing.name);
  }
  body.innerHTML = `
    <div class="d-title"></div>
    <div class="d-sub"></div>
    <button class="play" id="listing-add"></button>
    <div class="note" id="listing-note"></div>
    <div class="sec">
      <div class="sec-h">How well it runs</div>
      <div class="compat"><span class="status" id="listing-compat"></span></div>
      <div class="note plain" id="listing-compat-note"></div>
    </div>
    <div class="sec gone" id="listing-updates-sec">
      <div class="sec-h">Official updates</div>
      <button class="small-btn wide" id="listing-updates">Check for updates</button>
      <div class="note plain" id="listing-updates-note"></div>
    </div>
    <div class="sec gone" id="listing-packs-sec">
      <div class="sec-h">Community packs</div>
      <div class="note plain" id="listing-packs"></div>
    </div>
    <div class="sec gone" id="listing-releases">
      <div class="sec-h">Releases</div>
      <div id="listing-releases-rows"></div>
    </div>
  `;
  // A game's own name, so never through innerHTML.
  body.querySelector<HTMLElement>(".d-title")!.textContent = listing.name;
  body.querySelector<HTMLElement>(".d-sub")!.textContent = [
    listing.console_name,
    listing.regions.join(" "),
    listing.kind,
  ]
    .filter(Boolean)
    .join(" · ");

  const note = body.querySelector<HTMLElement>("#listing-note")!;
  const add = body.querySelector<HTMLButtonElement>("#listing-add")!;
  // The release a region filter picked, or else the first one listed.
  const release = listing.releases[0];
  if (listing.owned) {
    add.textContent = "In your library";
    add.disabled = true;
  } else if (release) {
    add.textContent = "Add to library";
    add.onclick = async () => {
      add.disabled = true;
      try {
        await addToLibrary(listing.console, release.title_id, listing.name);
        add.textContent = "In your library";
        note.textContent = "Import its files from the library to play it.";
        store.setGames(await listGames());
      } catch (err) {
        note.textContent = typeof err === "string" ? err : "Couldn't add that game.";
        add.disabled = false;
      }
    };
  } else {
    // Without a title id there is nothing to note it down by, so the way in is
    // its own files.
    add.textContent = "Import game";
    add.onclick = () => openImportSheet();
  }

  const badge = body.querySelector<HTMLElement>("#listing-compat")!;
  const compatNote = body.querySelector<HTMLElement>("#listing-compat-note")!;
  badge.textContent = listing.status.label || "No result";
  badge.className = `status ${listing.status.tone || "mute"}`;
  compatNote.textContent = listing.status.explanation || "Nobody has reported on this game yet.";
  // Some lists also say when a release's result was last reported.
  if (release && listing.features.compatibility) {
    void gameCompatibility(release.title_id).then((compat) => {
      if (compat.checked && compat.label === listing.status.label) {
        compatNote.textContent = `${listing.status.explanation} Last reported ${compat.checked}.`;
      }
    });
  }

  if (release && listing.features.updates) {
    body.querySelector("#listing-updates-sec")!.classList.remove("gone");
    // Asked only when pressed, the same as for a game in the library: it is a
    // request to Sony, and opening a title should not quietly make one.
    const updates = body.querySelector<HTMLButtonElement>("#listing-updates")!;
    const updatesNote = body.querySelector<HTMLElement>("#listing-updates-note")!;
    updates.onclick = async () => {
      updates.disabled = true;
      updates.textContent = "Checking…";
      try {
        const found = await gameUpdates(release.title_id);
        updatesNote.textContent =
          found.length === 0
            ? "Sony never published an update for this game."
            : found.length === 1
              ? `Sony published one update, version ${found[0].version}.`
              : `Sony published ${found.length} updates, up to version ${found[0].version}.`;
      } catch (err) {
        updatesNote.textContent = typeof err === "string" ? err : "Couldn't reach Sony's update service.";
      } finally {
        updates.disabled = false;
        updates.textContent = "Check for updates";
      }
    };
  }

  if (release && listing.features.packs) {
    body.querySelector("#listing-packs-sec")!.classList.remove("gone");
    const packsNote = body.querySelector<HTMLElement>("#listing-packs")!;
    void communityPacks(release.title_id, listing.console).then(({ have_list, waiting, packs }) => {
      packsNote.textContent = !have_list
        ? "Not downloaded yet. Open the game from your library to get them."
        : waiting
          ? waiting
          : packs.length === 0
            ? "Nobody has made one for this game yet."
            : packs.length === 1
              ? "One made for this game."
              : `${packs.length} made for this game.`;
    });
  }

  if (listing.releases.length > 0) {
    body.querySelector("#listing-releases")!.classList.remove("gone");
    const rows = body.querySelector<HTMLElement>("#listing-releases-rows")!;
    rows.innerHTML = listing.releases.map((r) => row(r.region || "Other", r.title_id)).join("");
  }
}

export function renderDetail(): HTMLElement {
  const detail = document.createElement("aside");
  detail.className = "detail hidden";
  detail.innerHTML = `
    <div class="d-hero">
      <div class="d-art"></div>
      <button class="d-close" aria-label="Close">
        <svg viewBox="0 0 10 10"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" stroke-width="1.4"/></svg>
      </button>
    </div>
    <div class="d-body"></div>
  `;
  const hero = detail.querySelector<HTMLElement>(".d-art")!;
  const body = detail.querySelector<HTMLElement>(".d-body")!;

  detail.querySelector<HTMLButtonElement>(".d-close")!.onclick = () => {
    store.setSelected(null);
    store.setCatalogueSelected(null);
  };

  let shownListing: string | null = null;

  // The content makes room for the panel at once, and the panel slides over
  // the gap, so the grid is laid out once rather than on every frame.
  const show = (open: boolean) => {
    detail.classList.toggle("hidden", !open);
    detail.parentElement?.classList.toggle("with-detail", open);
  };

  store.subscribe((state) => {
    if (!state.playing && state.view === "catalogue" && state.catalogueSelected) {
      show(true);
      // Filled once per pick. Typing in the search box notifies too, and
      // refilling would ask for the same answers again on every letter.
      const picked = state.catalogueSelected.listing.key;
      if (picked !== shownListing) {
        shownListing = picked;
        fillListing(body, hero, state.catalogueSelected);
      }
      return;
    }
    shownListing = null;

    // This panel belongs to the library. It stays out of the way while a game
    // runs, since the picture covers this side of the window, and while any
    // other screen is up, where a game selected earlier is not what you are
    // looking at.
    const game =
      state.playing || state.view !== "library"
        ? undefined
        : state.games?.find((g) => g.title_id === state.selected);
    show(Boolean(game));
    if (game) fill(body, hero, game);
  });

  return detail;
}
