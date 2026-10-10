import {
  cancelCemuInstall,
  cancelDolphinInstall,
  cancelRpcs3Install,
  emulatorUpdates,
  installCemu,
  installDolphin,
  installRpcs3,
  onCemuInstallProgress,
  onDolphinInstallProgress,
  onRpcs3InstallProgress,
  type Console,
  type EmulatorUpdate,
  type InstallProgress,
} from "../api";
import { store } from "../state";

/// Dolphin runs both of its consoles from one install.
const dolphin = { install: installDolphin, progress: onDolphinInstallProgress, cancel: cancelDolphinInstall };

/// The same install each emulator gets the first time, so an update is
/// downloaded, checked and unpacked exactly like a fresh one.
const INSTALL: Record<
  Console,
  {
    install: () => Promise<string>;
    progress: (handler: (progress: InstallProgress) => void) => Promise<() => void>;
    cancel: () => Promise<void>;
  }
> = {
  ps3: { install: installRpcs3, progress: onRpcs3InstallProgress, cancel: cancelRpcs3Install },
  wiiu: { install: installCemu, progress: onCemuInstallProgress, cancel: cancelCemuInstall },
  wii: dolphin,
  gamecube: dolphin,
};

const STAGE: Record<InstallProgress["stage"], string> = {
  checking: "Starting",
  runtime: "Installing Microsoft's Visual C++ runtime",
  downloading: "Downloading",
  verifying: "Checking the download",
  extracting: "Unpacking",
  done: "Done",
};

/// Brings every installed emulator up to its newest official release without
/// asking, once each time Omoio starts. It never begins while a game runs.
export async function updateEmulatorsInBackground(): Promise<void> {
  let behind: EmulatorUpdate[];
  try {
    behind = await emulatorUpdates();
  } catch {
    return;
  }
  for (const update of behind) {
    if (store.get().playing) return;
    await updateOne(update);
  }
}

async function updateOne(update: EmulatorUpdate): Promise<void> {
  const how = INSTALL[update.console];

  const box = document.createElement("div");
  box.className = "emu-updating";
  box.setAttribute("role", "status");
  box.innerHTML = `
    <div class="progress-label"><span class="stage"></span><span class="pct"></span></div>
    <div class="progress"><div class="progress-fill indeterminate"></div></div>
    <button class="link-btn">Stop</button>
  `;
  const stage = box.querySelector<HTMLElement>(".stage")!;
  const pct = box.querySelector<HTMLElement>(".pct")!;
  const fill = box.querySelector<HTMLElement>(".progress-fill")!;
  box.querySelector<HTMLButtonElement>(".link-btn")!.onclick = () => void how.cancel();
  stage.textContent = `Updating ${update.name}`;
  document.body.appendChild(box);

  const unlisten = await how.progress((progress) => {
    stage.textContent = `Updating ${update.name} · ${STAGE[progress.stage]}`;
    if (progress.stage === "downloading" && progress.total > 0) {
      const done = Math.min(100, Math.round((progress.bytes / progress.total) * 100));
      fill.classList.remove("indeterminate");
      fill.style.width = `${done}%`;
      pct.textContent = `${done}%`;
    } else {
      fill.classList.add("indeterminate");
      fill.style.width = "";
      pct.textContent = "";
    }
  });

  let said: string;
  try {
    const version = await how.install();
    if (update.console === "ps3") store.setRpcs3Version(version);
    said = `${update.name} is updated to ${version}.`;
    store.redraw();
  } catch (err) {
    // A stop comes during the download, before anything is replaced.
    said =
      err === "cancelled"
        ? `Stopped. ${update.name} stays as it was.`
        : `Couldn't update ${update.name}. Omoio tries again next time it starts.`;
  } finally {
    unlisten();
  }

  box.replaceChildren(Object.assign(document.createElement("div"), { textContent: said }));
  setTimeout(() => box.remove(), 6000);
}
