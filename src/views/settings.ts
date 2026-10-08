import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  addFigures,
  cancelRpcs3Install,
  clearSessionLogs,
  forgetAllGames,
  getAccount,
  getPlaces,
  getSettings,
  installFirmware,
  installRpcs3,
  listGames,
  listRegions,
  onRpcs3InstallProgress,
  revealFolder,
  setGamesFolder,
  setKeepSessions,
  setRegion,
  setStartFullscreen,
  setStartInBigPicture,
  setCovers,
  setRawgKey,
  fetchCovers,
  figures,
  setUsername,
  type InstallProgress,
  type Places,
  type Settings,
} from "../api";
import { updateControls } from "../components/omoioUpdate";
import { store } from "../state";
import type { View } from "./view";

const SONY_FIRMWARE_PAGE =
  "https://www.playstation.com/en-us/support/hardware/ps3/system-software/";

function row(label: string, hint?: string): { row: HTMLElement; right: HTMLElement } {
  const row = document.createElement("div");
  row.className = "setting";
  const left = document.createElement("div");
  const name = document.createElement("div");
  name.className = "setting-k";
  name.textContent = label;
  left.appendChild(name);
  if (hint) {
    const sub = document.createElement("div");
    sub.className = "setting-hint";
    sub.textContent = hint;
    left.appendChild(sub);
  }
  const right = document.createElement("div");
  right.className = "row-actions";
  row.append(left, right);
  return { row, right };
}

function toggle(on: boolean, onChange: (next: boolean) => Promise<void>): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = on ? "switch on" : "switch";
  button.setAttribute("role", "switch");
  button.setAttribute("aria-checked", String(on));
  button.innerHTML = `<span class="switch-dot"></span>`;
  let state = on;
  button.onclick = async () => {
    state = !state;
    button.className = state ? "switch on" : "switch";
    button.setAttribute("aria-checked", String(state));
    await onChange(state);
  };
  return button;
}

function value(text: string): HTMLElement {
  const el = document.createElement("span");
  el.className = "cfg-v";
  el.textContent = text;
  return el;
}

function button(label: string, run: (b: HTMLButtonElement) => Promise<void> | void): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = "small-btn";
  b.textContent = label;
  b.onclick = () => run(b);
  return b;
}

/// Anything that undoes something asks once more in the same button, so a
/// misplaced click never costs anything.
function confirming(label: string, confirm: string, run: () => Promise<void>): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = "small-btn danger";
  b.textContent = label;
  let armed = false;
  b.onclick = async () => {
    if (!armed) {
      armed = true;
      b.textContent = confirm;
      setTimeout(() => {
        if (armed) {
          armed = false;
          b.textContent = label;
        }
      }, 4000);
      return;
    }
    armed = false;
    b.disabled = true;
    await run();
    b.textContent = "Done";
  };
  return b;
}

function section(title: string, ...children: HTMLElement[]): HTMLElement {
  const sec = document.createElement("div");
  sec.className = "sec";
  const heading = document.createElement("div");
  heading.className = "sec-h";
  heading.textContent = title;
  sec.append(heading, ...children);
  return sec;
}

export async function renderSettings(): Promise<View> {
  const [places, settings, version]: [Places, Settings, string] = await Promise.all([
    getPlaces(),
    getSettings(),
    getVersion(),
  ]);
  const { rpcs3Version, firmwareVersion } = store.get();

  const content = document.createElement("div");
  content.className = "hw";

  // ---- playing ----
  const fullscreen = row("Start games in fullscreen", "Otherwise the game fills Omoio's window. F11 switches either way while playing.");
  fullscreen.right.appendChild(
    toggle(settings.start_fullscreen, (next) => setStartFullscreen(next))
  );

  const bigPicture = row(
    "Open Omoio in Big Picture",
    "For a PC under a TV. Big Picture fills the screen and works with a controller. View and Menu together bring it back during a game."
  );
  bigPicture.right.appendChild(
    toggle(settings.start_in_big_picture, (next) => setStartInBigPicture(next))
  );

  const games = row("Games folder", "Where archives are unpacked. Games imported as folders stay where they are.");
  const folderValue = value(places.games_folder ?? "Not chosen yet");
  folderValue.classList.add("path");
  games.right.append(
    folderValue,
    button("Change", async () => {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Choose where to keep your games",
      });
      if (typeof picked !== "string") return;
      await setGamesFolder(picked);
      folderValue.textContent = picked;
    })
  );

  // ---- the console games see ----
  const account = await getAccount();
  const regions = await listRegions();

  const nameRow = row("Username", "The name games show for you.");
  const nameInput = document.createElement("input");
  nameInput.className = "text-in";
  nameInput.maxLength = 16;
  nameInput.spellcheck = false;
  nameInput.value = account.username;
  const nameSaid = document.createElement("span");
  nameSaid.className = "cfg-v";
  nameInput.onchange = async () => {
    try {
      nameInput.value = await setUsername(nameInput.value);
      nameSaid.textContent = "Saved";
    } catch (err) {
      nameSaid.textContent = typeof err === "string" ? err : "Couldn't save that name.";
    }
  };
  nameRow.right.append(nameSaid, nameInput);

  const regionRow = row("Region", "Sets the language PS3 games start in. It doesn't change games for other consoles.");
  const regionSelect = document.createElement("select");
  regionSelect.className = "select";
  for (const choice of regions) {
    const option = document.createElement("option");
    option.value = choice.id;
    option.textContent = `${choice.name} · ${choice.language}`;
    option.selected = choice.id === account.region;
    regionSelect.appendChild(option);
  }
  const regionSaid = document.createElement("span");
  regionSaid.className = "cfg-v";
  regionSelect.onchange = async () => {
    try {
      await setRegion(regionSelect.value);
      regionSaid.textContent = "Saved";
    } catch (err) {
      regionSaid.textContent = typeof err === "string" ? err : "Couldn't change the region.";
    }
  };
  regionRow.right.append(regionSaid, regionSelect);

  content.appendChild(section("Console", nameRow.row, regionRow.row));
  content.appendChild(section("Playing", fullscreen.row, bigPicture.row, games.row));

  // ---- cover art ----
  const rawgRow = row(
    "Real covers from RAWG",
    "Box art in place of the generated tiles. Needs a free RAWG key, which stays on this computer."
  );
  const keyRow = row("RAWG key");
  const keyInput = document.createElement("input");
  keyInput.className = "text-in";
  keyInput.type = "password";
  keyInput.spellcheck = false;
  keyInput.placeholder = "Paste your key";
  keyInput.value = settings.rawg_key ?? "";
  const coversSaid = document.createElement("span");
  coversSaid.className = "cfg-v";
  const getKey = document.createElement("button");
  getKey.className = "link-btn";
  getKey.textContent = "Get a free key";
  getKey.onclick = () => void openUrl("https://rawg.io/apidocs");
  const lookUp = async () => {
    coversSaid.textContent = "Looking up covers…";
    try {
      const found = await fetchCovers();
      coversSaid.textContent = found === 1 ? "1 cover" : `${found} covers`;
    } catch (err) {
      coversSaid.textContent = typeof err === "string" ? err : "Couldn't reach RAWG.";
    }
    store.setGames(await listGames());
  };
  keyInput.onchange = async () => {
    await setRawgKey(keyInput.value.trim());
    if (settings.covers && keyInput.value.trim()) await lookUp();
  };
  rawgRow.right.appendChild(
    toggle(settings.covers, async (next) => {
      await setCovers(next);
      settings.covers = next;
      keyRow.row.classList.toggle("gone", !next);
      if (next && keyInput.value.trim()) await lookUp();
      else store.setGames(await listGames());
    })
  );
  keyRow.right.append(coversSaid, getKey, keyInput);
  keyRow.row.classList.toggle("gone", !settings.covers);
  content.appendChild(section("Cover art", rawgRow.row, keyRow.row));

  // ---- emulator, and putting it right when it breaks ----
  const rpcs3Row = row("RPCS3", "Reinstalling replaces the emulator. Your games and saves are untouched.");
  const rpcs3Value = value(rpcs3Version ?? "Not installed");
  const rpcs3Progress = document.createElement("span");
  rpcs3Progress.className = "cfg-v";
  // Only there while something is running, so there is nothing to press by
  // mistake when nothing is happening.
  const cancel = button("Cancel", () => cancelRpcs3Install());
  cancel.hidden = true;

  const reinstall = button(rpcs3Version ? "Reinstall" : "Install", async (b) => {
    b.disabled = true;
    cancel.hidden = false;
    const unlisten = await onRpcs3InstallProgress((p: InstallProgress) => {
      rpcs3Progress.textContent =
        p.stage === "downloading" && p.total > 0
          ? `Downloading ${Math.round((p.bytes / p.total) * 100)}%`
          : `${p.stage}…`;
    });
    try {
      const installed = await installRpcs3();
      store.setRpcs3Version(installed);
      rpcs3Value.textContent = installed;
      rpcs3Progress.textContent = "";
      b.textContent = "Reinstall";
    } catch (err) {
      rpcs3Progress.textContent = err === "cancelled" ? "" : "Couldn't install RPCS3.";
    } finally {
      unlisten();
      cancel.hidden = true;
      b.disabled = false;
    }
  });
  rpcs3Row.right.append(rpcs3Progress, rpcs3Value, cancel, reinstall);

  const firmwareRow = row("PS3 firmware", "Sony publishes it free. Download it yourself, then point Omoio at the file.");
  const firmwareValue = value(firmwareVersion ?? "Not installed");
  firmwareRow.right.append(
    firmwareValue,
    button("Sony's page", () => {
      openUrl(SONY_FIRMWARE_PAGE);
    }),
    button(firmwareVersion ? "Replace" : "Install", async (b) => {
      const picked = await open({
        multiple: false,
        directory: false,
        title: "Choose a PS3 firmware file",
        filters: [{ name: "PS3 firmware", extensions: ["pup"] }],
      });
      if (typeof picked !== "string") return;
      b.disabled = true;
      firmwareValue.textContent = "Installing…";
      try {
        const version = await installFirmware(picked);
        store.setFirmwareVersion(version);
        firmwareValue.textContent = version;
      } catch (err) {
        firmwareValue.textContent = typeof err === "string" ? err : "Couldn't install that firmware.";
      } finally {
        b.disabled = false;
      }
    })
  );

  content.appendChild(section("Emulator", rpcs3Row.row, firmwareRow.row));

  // ---- toy figures, for the portal menu over a game ----
  const counted = (n: number) => (n === 1 ? "1 figure" : `${n} figures`);
  const figuresRow = row(
    "Toy figures",
    "Figure files you already have. You don't need any: the portal menu makes a new figure of any character. Press the pad's home button (Guide, PS or Home) while playing a Skylanders game to open it."
  );
  const figuresSaid = value(counted((await figures()).length));
  figuresRow.right.append(
    figuresSaid,
    button("Add figures", async () => {
      const picked = await open({
        multiple: true,
        directory: false,
        title: "Choose your figure files",
        filters: [{ name: "Figure files", extensions: ["sky", "bin", "dump", "dmp"] }],
      });
      const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
      if (paths.length === 0) return;
      try {
        const added = await addFigures(paths);
        const total = counted((await figures()).length);
        figuresSaid.textContent = added < paths.length ? `${total} · some were already there` : total;
      } catch (err) {
        figuresSaid.textContent = typeof err === "string" ? err : "Couldn't add those files.";
      }
    }),
    button("Open folder", () => revealFolder(places.figures))
  );
  content.appendChild(section("Toy figures", figuresRow.row));

  // ---- session logs ----
  const keep = row("Keep logs for", "Older sessions and their logs are deleted to save space.");
  const keepValue = document.createElement("select");
  keepValue.className = "select";
  for (const n of [5, 10, 20, 50, 100]) {
    const option = document.createElement("option");
    option.value = String(n);
    option.textContent = `${n} sessions`;
    option.selected = n === settings.keep_sessions;
    keepValue.appendChild(option);
  }
  keepValue.onchange = () => setKeepSessions(Number(keepValue.value));
  keep.right.appendChild(keepValue);

  const logsRow = row("Session logs", places.logs);
  logsRow.right.append(
    button("Open folder", () => revealFolder(places.logs)),
    confirming("Delete all", "Click again to delete", () => clearSessionLogs())
  );

  content.appendChild(section("Session logs", keep.row, logsRow.row));

  // ---- Omoio itself ----
  const versionRow = row("Omoio", "Looks for a new version when it starts and every few hours after.");
  const [updateSaid, updateButton] = updateControls();
  versionRow.right.append(updateSaid, value(version), updateButton);
  content.appendChild(section("Updates", versionRow.row));

  // ---- library ----
  const count = store.get().games?.length ?? 0;
  const libraryRow = row(
    "Library",
    `${count} ${count === 1 ? "game" : "games"} · ${places.library}`
  );
  libraryRow.right.append(
    button("Open folder", () => revealFolder(places.library)),
    confirming("Forget all games", "Click again to forget", async () => {
      await forgetAllGames();
      store.setGames(await listGames());
    })
  );

  const coversRow = row("Cover art", places.covers);
  coversRow.right.appendChild(button("Open folder", () => revealFolder(places.covers)));

  const emulatorFolder = row("RPCS3 folder", places.rpcs3);
  emulatorFolder.right.appendChild(button("Open folder", () => revealFolder(places.rpcs3)));

  content.appendChild(section("Files", libraryRow.row, coversRow.row, emulatorFolder.row));

  const note = document.createElement("div");
  note.className = "note plain";
  note.textContent =
    "Forgetting games empties the list only. Omoio never deletes the game files themselves.";
  content.appendChild(note);

  return { title: "Settings", subtitle: `Omoio ${version}`, content };
}
