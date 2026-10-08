import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import {
  emulatorVersions,
  finishSetup,
  getAccount,
  getFirmwareVersion,
  getRpcs3Version,
  installCemu,
  installDolphin,
  installFirmware,
  installRpcs3,
  listRegions,
  needsSetup,
  onCemuInstallProgress,
  onDolphinInstallProgress,
  onRpcs3InstallProgress,
  setRegion,
  setUsername,
  type Console,
  type InstallProgress,
  type RegionChoice,
} from "../api";
import { store } from "../state";

/// Sony publishes the firmware free. We never fetch it: the user downloads the
/// file themselves and points us at it, which is what RPCS3 does too.
const SONY_FIRMWARE_PAGE =
  "https://www.playstation.com/en-us/support/hardware/ps3/system-software/";

const STAGE: Record<InstallProgress["stage"], string> = {
  checking: "Looking for the latest build",
  downloading: "Downloading",
  verifying: "Checking the download",
  extracting: "Unpacking",
  done: "Done",
};

type Emulator = {
  console: Console;
  games: string;
  name: string;
  install: () => Promise<string>;
  progress: (handler: (progress: InstallProgress) => void) => Promise<() => void>;
};

/// The consoles Omoio plays, each with the emulator it downloads for them.
const EMULATORS: Emulator[] = [
  { console: "ps3", games: "PS3 games", name: "RPCS3", install: installRpcs3, progress: onRpcs3InstallProgress },
  { console: "wiiu", games: "Wii U games", name: "Cemu", install: installCemu, progress: onCemuInstallProgress },
  {
    console: "wii",
    games: "Wii and GameCube games",
    name: "Dolphin",
    install: installDolphin,
    progress: onDolphinInstallProgress,
  },
];

/// Asked once, on the first run.
///
/// It does the work rather than listing it. Everything here was previously
/// something the user had to discover on their own, after being turned away
/// from the Play button with an instruction and no route.
export async function openSetupIfNeeded(): Promise<void> {
  if (!(await needsSetup())) return;

  const scrim = document.createElement("div");
  scrim.className = "scrim";
  const sheet = document.createElement("div");
  sheet.className = "sheet";
  scrim.appendChild(sheet);
  document.body.appendChild(scrim);
  requestAnimationFrame(() => scrim.classList.add("on"));

  function close() {
    scrim.classList.remove("on");
    setTimeout(() => scrim.remove(), 200);
  }

  await askWhoAndWhere(sheet);
  for (const emulator of await askWhatToPlay(sheet)) {
    await getEmulator(sheet, emulator);
  }
  await getFirmware(sheet);

  await finishSetup();
  close();
}

async function askWhoAndWhere(sheet: HTMLElement): Promise<void> {
  const [account, regions] = await Promise.all([getAccount(), listRegions()]);

  sheet.innerHTML = `
    <div class="sheet-h">Before you play</div>
    <div class="sheet-p">Both can be changed later in Settings.</div>

    <div class="setting">
      <div>
        <div class="setting-k">Username</div>
        <div class="setting-hint">The name games show for you.</div>
      </div>
      <div class="row-actions">
        <input class="text-in" id="setup-name" maxlength="16" spellcheck="false">
      </div>
    </div>

    <div class="setting">
      <div>
        <div class="setting-k">Region</div>
        <div class="setting-hint">Sets the language PS3 games start in. It doesn't change Wii U games.</div>
      </div>
      <div class="row-actions">
        <select class="select" id="setup-region"></select>
      </div>
    </div>

    <div class="note plain" id="setup-note"></div>
    <div class="sheet-actions">
      <button class="btn solid" id="setup-next">Continue</button>
    </div>
  `;

  const name = sheet.querySelector<HTMLInputElement>("#setup-name")!;
  name.value = account.username || "User";

  const region = sheet.querySelector<HTMLSelectElement>("#setup-region")!;
  for (const choice of regions as RegionChoice[]) {
    const option = document.createElement("option");
    option.value = choice.id;
    option.textContent = `${choice.name} · ${choice.language}`;
    option.selected = choice.id === account.region;
    region.appendChild(option);
  }
  if (!account.region) region.value = "eu-en";

  const note = sheet.querySelector<HTMLElement>("#setup-note")!;
  const next = sheet.querySelector<HTMLButtonElement>("#setup-next")!;

  await new Promise<void>((done) => {
    next.onclick = async () => {
      next.disabled = true;
      try {
        await setUsername(name.value);
        // The region needs RPCS3's config, which may not exist yet on a fresh
        // machine. Remembered and applied once the emulator is there.
        pendingRegion = region.value;
        done();
      } catch (err) {
        note.textContent = typeof err === "string" ? err : "Couldn't save that.";
        next.disabled = false;
      }
    };
  });
}

let pendingRegion = "";

/// Which consoles to play, and so which emulators to download. Nothing is
/// chosen for the user except what is already installed, and Omoio with no
/// emulator could start nothing, so at least one it has to be.
async function askWhatToPlay(sheet: HTMLElement): Promise<Emulator[]> {
  const versions = await emulatorVersions();
  const chosen = new Set<Console>(
    versions.filter((v) => v.version).map((v) => v.console)
  );

  sheet.innerHTML = `
    <div class="sheet-h">What do you want to play?</div>
    <div class="sheet-p">Choose at least one. Omoio downloads the emulator for each and keeps it up to date. You can add more later under Emulators.</div>
    <div id="setup-emulators"></div>
    <div class="sheet-actions">
      <button class="btn solid" id="setup-next">Continue</button>
    </div>
  `;
  const list = sheet.querySelector<HTMLElement>("#setup-emulators")!;
  const next = sheet.querySelector<HTMLButtonElement>("#setup-next")!;
  const refresh = () => {
    next.disabled = chosen.size === 0;
  };

  for (const emulator of EMULATORS) {
    const row = document.createElement("div");
    row.className = "setting";
    const left = document.createElement("div");
    const games = document.createElement("div");
    games.className = "setting-k";
    games.textContent = emulator.games;
    const with_ = document.createElement("div");
    with_.className = "setting-hint";
    with_.textContent = `With ${emulator.name}`;
    left.append(games, with_);

    const right = document.createElement("div");
    right.className = "row-actions";
    const toggle = document.createElement("button");
    toggle.setAttribute("role", "switch");
    toggle.setAttribute("aria-label", emulator.games);
    toggle.innerHTML = `<span class="switch-dot"></span>`;
    const paint = () => {
      const on = chosen.has(emulator.console);
      toggle.className = on ? "switch on" : "switch";
      toggle.setAttribute("aria-checked", String(on));
    };
    toggle.onclick = () => {
      if (chosen.has(emulator.console)) chosen.delete(emulator.console);
      else chosen.add(emulator.console);
      paint();
      refresh();
    };
    paint();
    right.appendChild(toggle);
    row.append(left, right);
    list.appendChild(row);
  }
  refresh();

  await new Promise<void>((done) => {
    next.onclick = () => {
      if (chosen.size > 0) done();
    };
  });
  return EMULATORS.filter((emulator) => chosen.has(emulator.console));
}

/// Downloads a chosen emulator without being asked again.
///
/// This is Omoio's own download of an official build, so there is nothing more
/// for the user to decide. It shows what it is doing and can be skipped, but
/// it starts on its own.
async function getEmulator(sheet: HTMLElement, emulator: Emulator): Promise<void> {
  const versions = await emulatorVersions();
  if (versions.some((v) => v.console === emulator.console && v.version)) {
    if (emulator.console === "ps3") await applyRegion();
    return;
  }

  sheet.innerHTML = `
    <div class="sheet-h"></div>
    <div class="sheet-p">Omoio downloads it and manages it for you. Nothing to install by hand.</div>
    <div class="progress-row" style="margin-top:14px">
      <div class="progress-label">
        <span id="setup-stage">Starting</span>
        <span class="pct" id="setup-pct"></span>
      </div>
      <div class="progress"><div class="progress-fill indeterminate" id="setup-bar"></div></div>
    </div>
    <div class="note plain" id="setup-note"></div>
    <div class="sheet-actions">
      <button class="btn ghost" id="setup-skip">Skip for now</button>
    </div>
  `;
  sheet.querySelector<HTMLElement>(".sheet-h")!.textContent = `Getting ${emulator.name}`;

  const stage = sheet.querySelector<HTMLElement>("#setup-stage")!;
  const pct = sheet.querySelector<HTMLElement>("#setup-pct")!;
  const bar = sheet.querySelector<HTMLElement>("#setup-bar")!;
  const note = sheet.querySelector<HTMLElement>("#setup-note")!;
  const skip = sheet.querySelector<HTMLButtonElement>("#setup-skip")!;

  const unlisten = await emulator.progress((p) => {
    stage.textContent = STAGE[p.stage];
    if (p.stage === "downloading" && p.total > 0) {
      const done = Math.round((p.bytes / p.total) * 100);
      pct.textContent = `${done}%`;
      bar.classList.remove("indeterminate");
      bar.style.width = `${done}%`;
    } else {
      pct.textContent = "";
      bar.classList.add("indeterminate");
      bar.style.width = "";
    }
  });

  await new Promise<void>((done) => {
    skip.onclick = () => done();
    emulator
      .install()
      .then(async (version) => {
        if (emulator.console === "ps3") {
          store.setRpcs3Version(version);
          await applyRegion();
        }
        done();
      })
      .catch(() => {
        note.textContent = `Couldn't get ${emulator.name}. You can install it later from Emulators.`;
        skip.textContent = "Continue";
      });
  });
  unlisten();
}

/// The region is two lines in RPCS3's config, which only exists once the
/// emulator does.
async function applyRegion(): Promise<void> {
  if (!pendingRegion) return;
  try {
    await setRegion(pendingRegion);
  } catch {
    // Settings can still set it; not worth stopping setup over.
  }
  pendingRegion = "";
}

/// Firmware is the one thing Omoio cannot fetch. Sony publishes it free, the
/// user downloads it, and we take the file from there.
async function getFirmware(sheet: HTMLElement): Promise<void> {
  if (!(await getRpcs3Version())) return;
  if (await getFirmwareVersion()) return;

  sheet.innerHTML = `
    <div class="sheet-h">PS3 firmware</div>
    <div class="sheet-p">Games need it. Sony publishes it free, and you download the file yourself.</div>
    <div class="note plain" id="setup-note"></div>
    <div class="sheet-actions">
      <button class="btn ghost" id="setup-skip">Skip for now</button>
      <button class="btn ghost" id="setup-open">Open Sony's page</button>
      <button class="btn solid" id="setup-pick">Choose the file</button>
    </div>
  `;

  const note = sheet.querySelector<HTMLElement>("#setup-note")!;
  const skip = sheet.querySelector<HTMLButtonElement>("#setup-skip")!;
  const pick = sheet.querySelector<HTMLButtonElement>("#setup-pick")!;
  sheet.querySelector<HTMLButtonElement>("#setup-open")!.onclick = () =>
    openUrl(SONY_FIRMWARE_PAGE);

  await new Promise<void>((done) => {
    skip.onclick = () => done();
    pick.onclick = async () => {
      const picked = await open({
        multiple: false,
        directory: false,
        title: "Choose the firmware file",
        filters: [{ name: "PS3 firmware", extensions: ["PUP", "pup"] }],
      });
      if (typeof picked !== "string") return;

      pick.disabled = true;
      note.textContent = "Installing…";
      try {
        const version = await installFirmware(picked);
        store.setFirmwareVersion(version);
        done();
      } catch (err) {
        note.textContent =
          typeof err === "string" ? err : "Couldn't read that firmware file.";
        pick.disabled = false;
      }
    };
  });
}
