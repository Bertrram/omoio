import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  cancelRpcs3Install,
  getHardwareInfo,
  getSettings,
  installFirmware,
  installRpcs3,
  onRpcs3InstallProgress,
  type HardwareInfo,
  type InstallProgress,
} from "../api";
import { store } from "../state";
import type { View } from "./view";

// Sony publishes the firmware free but we never fetch it: the user downloads
// the PUP from here themselves and points us at it.
const SONY_FIRMWARE_PAGE = "https://www.playstation.com/en-us/support/hardware/ps3/system-software/";

function bytesToGB(bytes: number): number {
  return Math.round(bytes / 1024 ** 3);
}

function hwRow(label: string, value: string, note: string): string {
  return `<div class="hw-row"><span class="hw-k">${label}</span><span class="hw-v">${value}</span><span class="hw-n">${note}</span></div>`;
}

function renderHardware(hw: HardwareInfo): HTMLElement {
  const cpuNote = hw.cpu.physical_cores
    ? `${hw.cpu.physical_cores} cores / ${hw.cpu.logical_cores} threads`
    : `${hw.cpu.logical_cores} threads`;
  const gpuValue = hw.gpu?.name ?? "Not detected";
  const gpuNote = hw.gpu ? `${bytesToGB(hw.gpu.dedicated_memory_bytes)} GB` : "";
  const displayValue = hw.display ? `${hw.display.width} × ${hw.display.height}` : "Not detected";
  const displayNote = hw.display ? `${hw.display.refresh_hz} Hz` : "";

  const el = document.createElement("div");
  el.className = "sec";
  el.innerHTML = `
    <div class="sec-h">Detected hardware</div>
    ${hwRow("CPU", hw.cpu.brand, cpuNote)}
    ${hwRow("GPU", gpuValue, gpuNote)}
    ${hwRow("Memory", `${bytesToGB(hw.memory.total_bytes)} GB`, "")}
    ${hwRow("Display", displayValue, displayNote)}
  `;
  return el;
}

const STAGE_LABEL: Record<InstallProgress["stage"], string> = {
  checking: "Checking for the latest build…",
  runtime: "Installing Microsoft's Visual C++ runtime…",
  downloading: "Downloading…",
  verifying: "Verifying…",
  extracting: "Extracting…",
  done: "Done",
};

function statusRow(label: string, valueId: string): HTMLElement {
  const row = document.createElement("div");
  row.className = "cfg-row";
  row.innerHTML = `<span class="cfg-k">${label}</span><span class="cfg-v" id="${valueId}"></span>`;
  return row;
}

function renderEmulator(
  rpcs3Version: string | null | undefined,
  firmwareVersion: string | null | undefined
): HTMLElement {
  const el = document.createElement("div");
  el.className = "sec";
  el.innerHTML = `<div class="sec-h">Emulator</div>`;

  const rpcs3Status = statusRow("RPCS3", "rpcs3-version");
  const rpcs3Action = document.createElement("div");
  rpcs3Action.className = "row-actions";
  const firmwareStatus = statusRow("PS3 firmware", "firmware-version");
  const firmwareAction = document.createElement("div");
  firmwareAction.className = "row-actions";
  el.append(rpcs3Status, rpcs3Action, firmwareStatus, firmwareAction);

  const rpcs3VersionEl = rpcs3Status.querySelector<HTMLElement>("#rpcs3-version")!;
  const firmwareVersionEl = firmwareStatus.querySelector<HTMLElement>("#firmware-version")!;

  let installedRpcs3 = rpcs3Version;

  function showRpcs3Idle(current: string | null | undefined) {
    installedRpcs3 = current;
    rpcs3VersionEl.textContent = current ?? "Not installed";
    rpcs3Action.innerHTML = current
      ? ""
      : `<button class="small-btn" id="install-rpcs3">Install RPCS3</button>`;
    rpcs3Action
      .querySelector<HTMLButtonElement>("#install-rpcs3")
      ?.addEventListener("click", startRpcs3Install);
  }

  function showRpcs3Progress(progress: InstallProgress) {
    const pct =
      progress.stage === "downloading" && progress.total > 0
        ? Math.round((progress.bytes / progress.total) * 100)
        : null;
    rpcs3Action.innerHTML = `
      <div class="progress-row" style="flex:1">
        <div class="progress-label">
          <span>${STAGE_LABEL[progress.stage]}</span>
          ${pct !== null ? `<span class="pct">${pct}%</span>` : ""}
        </div>
        <div class="progress"><div class="progress-fill" style="width:${pct ?? 8}%"></div></div>
      </div>
      <button class="small-btn" id="cancel-install">Cancel</button>
    `;
    rpcs3Action.querySelector<HTMLButtonElement>("#cancel-install")?.addEventListener("click", () => {
      cancelRpcs3Install();
    });
  }

  async function startRpcs3Install() {
    showRpcs3Progress({ stage: "checking", bytes: 0, total: 0 });
    const unlisten = await onRpcs3InstallProgress(showRpcs3Progress);
    try {
      const installed = await installRpcs3();
      store.setRpcs3Version(installed);
      showRpcs3Idle(installed);
      // Firmware can only be added once RPCS3 is there to unpack it.
      showFirmwareIdle(store.get().firmwareVersion);
    } catch (err) {
      if (err === "cancelled") {
        showRpcs3Idle(null);
      } else {
        console.error("RPCS3 install failed:", err);
        rpcs3Action.innerHTML = `<div class="progress-error">Couldn't install RPCS3. Check your internet connection and try again.</div>`;
      }
    } finally {
      unlisten();
    }
  }

  function showFirmwareIdle(current: string | null | undefined, note?: string) {
    firmwareVersionEl.textContent = current ?? "Not installed";
    if (current) {
      firmwareAction.innerHTML = "";
      return;
    }
    firmwareAction.innerHTML = `
      <button class="small-btn" id="open-sony">Open Sony's download page</button>
      <button class="small-btn" id="choose-pup"${installedRpcs3 ? "" : " disabled"}>Choose PUP file…</button>
      ${note ? `<div class="progress-error">${note}</div>` : ""}
    `;
    firmwareAction.querySelector<HTMLButtonElement>("#open-sony")?.addEventListener("click", () => {
      openUrl(SONY_FIRMWARE_PAGE);
    });
    firmwareAction
      .querySelector<HTMLButtonElement>("#choose-pup")
      ?.addEventListener("click", chooseFirmware);
  }

  async function chooseFirmware() {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "PS3 firmware", extensions: ["pup"] }],
    });
    if (typeof selected !== "string") return;

    firmwareAction.innerHTML = `
      <div class="progress-row" style="flex:1">
        <div class="progress-label"><span>Installing firmware…</span></div>
        <div class="progress"><div class="progress-fill indeterminate"></div></div>
      </div>
    `;
    try {
      const installed = await installFirmware(selected);
      store.setFirmwareVersion(installed);
      showFirmwareIdle(installed);
    } catch (err) {
      console.error("Firmware install failed:", err);
      showFirmwareIdle(null, typeof err === "string" ? err : "Couldn't install that firmware.");
    }
  }

  showRpcs3Idle(rpcs3Version);
  showFirmwareIdle(firmwareVersion);
  return el;
}

/// What Omoio set the picture to, and why, so the choice can be seen rather than
/// having happened behind the user’s back.
function renderPicture(hw: HardwareInfo, scale: number | null): HTMLElement {
  const el = document.createElement("div");
  el.className = "sec";
  const display = hw.display ? `${hw.display.width}×${hw.display.height}` : "";
  el.innerHTML = `
    <div class="sec-h">Picture</div>
    ${hwRow("Scale", scale ? `${scale}%` : "Not set yet", display)}
    <div class="note plain"></div>
  `;
  el.querySelector<HTMLElement>(".note")!.textContent = scale
    ? `Games draw at ${scale}% of the PS3’s 1280×720, sized for this display and graphics card.`
    : "Set the first time a game starts, from this display and graphics card.";
  return el;
}

export async function renderSystem(): Promise<View> {
  const [hw, settings] = await Promise.all([getHardwareInfo(), getSettings()]);
  const { rpcs3Version, firmwareVersion } = store.get();

  const content = document.createElement("div");
  content.className = "hw";
  content.appendChild(renderHardware(hw));
  content.appendChild(renderPicture(hw, settings.tuned_scale));
  content.appendChild(renderEmulator(rpcs3Version, firmwareVersion));

  return {
    title: "System",
    subtitle: hw.gpu ? `${hw.cpu.brand} · ${hw.gpu.name}` : hw.cpu.brand,
    content,
  };
}
