import { openImportSheet } from "./importSheet";
import { store, type ViewId } from "../state";

interface NavItem {
  id: ViewId;
  label: string;
  icon: string;
}

const GAMES_NAV: NavItem[] = [
  { id: "library", label: "Library", icon: '<rect x="2" y="2.5" width="12" height="11" rx="1.5"/><path d="M5.5 2.5v11"/>' },
  { id: "catalogue", label: "Catalogue", icon: '<circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/>' },
  { id: "homebrew", label: "Homebrew", icon: '<path d="M8 2v8M5 7l3 3 3-3M3 13h10"/>' },
];

const SETUP_NAV: NavItem[] = [
  {
    id: "controller",
    label: "Controller",
    icon: '<path d="M5 5h6a3 3 0 0 1 3 3.3l-.3 2.4a1.5 1.5 0 0 1-2.7.7L9.9 10H6.1L5 11.4a1.5 1.5 0 0 1-2.7-.7L2 8.3A3 3 0 0 1 5 5z"/><path d="M5.2 6.9v2.2M4.1 8h2.2"/><path d="M10.6 7.6h.01M11.6 8.6h.01"/>',
  },
  {
    id: "emulators",
    label: "Emulators",
    icon: '<path d="M8 2.2l5.8 2.9L8 8 2.2 5.1z"/><path d="M2.2 8L8 10.9 13.8 8"/><path d="M2.2 10.9L8 13.8l5.8-2.9"/>',
  },
];

const MAINTENANCE_NAV: NavItem[] = [
  { id: "updates", label: "Updates", icon: '<path d="M13.5 8a5.5 5.5 0 1 1-1.9-4.2M13 2v3.5h-3.5"/>' },
  {
    id: "system",
    label: "System",
    icon: '<circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M12.6 3.4l-1.4 1.4M4.8 11.2l-1.4 1.4"/>',
  },
  {
    id: "logs",
    label: "Logs",
    icon: '<rect x="2.5" y="2" width="11" height="12" rx="1.5"/><path d="M5 5.5h6M5 8h6M5 10.5h3.5"/>',
  },
  {
    id: "settings",
    label: "Settings",
    icon: '<path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11"/><circle cx="5.5" cy="4.5" r="1.6" fill="var(--panel)"/><circle cx="10" cy="8" r="1.6" fill="var(--panel)"/><circle cx="6.5" cy="11.5" r="1.6" fill="var(--panel)"/>',
  },
];

function navLabel(text: string): HTMLElement {
  const el = document.createElement("div");
  el.className = "nav-label";
  el.textContent = text;
  return el;
}

function navButton(item: NavItem): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.className = "nav-item";
  btn.dataset.view = item.id;
  btn.innerHTML = `
    <svg class="nav-ico" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">${item.icon}</svg>
    <span>${item.label}</span>
  `;
  btn.onclick = () => {
    if (!store.get().playing) store.setView(item.id);
  };
  return btn;
}

/// Blank while the first check is still out. Saying "Not installed" before we
/// know makes a working install look like a fresh one for the first moment
/// after the window opens.
function installedVersion(known: string | null | undefined): string {
  if (known === undefined) return "";
  return known ?? "Not installed";
}

export function renderSidebar(): HTMLElement {
  const side = document.createElement("aside");
  side.className = "side";

  side.appendChild(navLabel("Games"));
  GAMES_NAV.forEach((item) => side.appendChild(navButton(item)));

  side.appendChild(navLabel("Setup"));
  SETUP_NAV.forEach((item) => side.appendChild(navButton(item)));

  side.appendChild(navLabel("Maintenance"));
  MAINTENANCE_NAV.forEach((item) => side.appendChild(navButton(item)));

  // While a game runs, its picture fills the space every page opens in. A
  // page that looked chosen and showed nothing read as a fault, so the pages
  // dim and this says why.
  const away = document.createElement("div");
  away.className = "nav-note gone";
  away.textContent = "Pages open again when the game stops.";
  side.appendChild(away);

  const foot = document.createElement("div");
  foot.className = "side-foot";
  foot.innerHTML = `
    <div class="stat"><span>RPCS3</span><span id="rpcs3-stat">Not installed</span></div>
    <div class="stat"><span>Firmware</span><span id="firmware-stat">Not installed</span></div>
    <button class="import-btn" id="import-game">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M8 3.5v9M3.5 8h9"/></svg>
      Import game
    </button>
  `;
  side.appendChild(foot);

  const rpcs3Stat = foot.querySelector<HTMLElement>("#rpcs3-stat")!;
  const firmwareStat = foot.querySelector<HTMLElement>("#firmware-stat")!;
  foot.querySelector<HTMLButtonElement>("#import-game")!.onclick = openImportSheet;

  store.subscribe((state) => {
    const playing = state.playing !== null;
    away.classList.toggle("gone", !playing);
    side.querySelectorAll<HTMLButtonElement>(".nav-item").forEach((btn) => {
      btn.classList.toggle("on", !playing && btn.dataset.view === state.view);
      if (playing) btn.setAttribute("aria-disabled", "true");
      else btn.removeAttribute("aria-disabled");
    });
    rpcs3Stat.textContent = installedVersion(state.rpcs3Version);
    firmwareStat.textContent = installedVersion(state.firmwareVersion);
  });

  return side;
}
