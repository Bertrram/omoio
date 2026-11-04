import {
  cancelUpdate,
  gameUpdates,
  installUpdate,
  listGames,
  onUpdateProgress,
  type GameUpdate,
  type UpdateProgress,
} from "../api";
import { store } from "../state";

function formatSize(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(bytes / 1024 ** 2)} MB`;
}

/// Whether version `a` comes after `b`, piece by piece as numbers, the same
/// way the backend compares them: 01.10 comes after 01.09.
function isNewer(a: string, b: string): boolean {
  const pa = a.split(".").map((piece) => parseInt(piece, 10) || 0);
  const pb = b.split(".").map((piece) => parseInt(piece, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = pa[i] ?? 0;
    const y = pb[i] ?? 0;
    if (x !== y) return x > y;
  }
  return false;
}

/// Time left from the pace so far. Nothing in the first few seconds, when
/// the pace is still mostly noise.
function timeLeft(startedAt: number, done: number, total: number): string {
  const seconds = (Date.now() - startedAt) / 1000;
  if (!startedAt || seconds < 5 || done <= 0) return "";
  const left = ((total - done) / done) * seconds;
  if (left < 60) return "Less than a minute left";
  const minutes = Math.round(left / 60);
  if (minutes < 60) return `About ${minutes} min left`;
  return `About ${Math.floor(minutes / 60)} h ${minutes % 60} min left`;
}

export async function openUpdates(
  titleId: string,
  title: string,
  installedWhenOpened: string | null,
  onChanged: () => void
): Promise<void> {
  // Moves as updates are installed, so the heading and the "Installed" mark
  // do not go on describing the version the game was on when this opened.
  let installed = installedWhenOpened;
  // The version the game's own files are, which is where a first update
  // starts from.
  const shipped = store.get().games?.find((g) => g.title_id === titleId)?.version ?? "";
  const scrim = document.createElement("div");
  scrim.className = "scrim";
  const sheet = document.createElement("div");
  sheet.className = "sheet wide";
  scrim.appendChild(sheet);
  document.body.appendChild(scrim);
  requestAnimationFrame(() => scrim.classList.add("on"));

  let busy = false;
  let startedAt = 0;
  function close() {
    if (busy) return;
    scrim.classList.remove("on");
    setTimeout(() => scrim.remove(), 200);
  }
  scrim.onclick = (e) => {
    if (e.target === scrim) close();
  };

  function showProgress(target: GameUpdate, progress: UpdateProgress | null) {
    const pct =
      progress && progress.total > 0
        ? Math.min(100, Math.round((progress.bytes / progress.total) * 100))
        : 0;
    sheet.innerHTML = `
      <div class="sheet-h">Installing version ${target.version}</div>
      <div class="sheet-p" id="update-step"></div>
      <div class="progress-row" style="margin-top:14px">
        <div class="progress-label">
          <span id="update-doing"></span>
          <span class="pct">${pct}%</span>
        </div>
        <div class="progress"><div class="progress-fill" style="width:${pct}%"></div></div>
      </div>
      <div class="note plain" id="update-left"></div>
      <div class="sheet-actions">
        <button class="btn ghost" id="cancel-update">Cancel</button>
      </div>
    `;
    sheet.querySelector<HTMLElement>("#update-step")!.textContent = !progress
      ? "Working out which updates are needed…"
      : progress.steps > 1
        ? `Update ${progress.step} of ${progress.steps}, version ${progress.version}. Each one needs the one before it.`
        : `Version ${progress.version}.`;
    sheet.querySelector<HTMLElement>("#update-doing")!.textContent = !progress
      ? ""
      : progress.installing
        ? `Installing version ${progress.version}…`
        : `Downloading ${formatSize(progress.bytes)} of ${formatSize(progress.total)}`;
    sheet.querySelector<HTMLElement>("#update-left")!.textContent = progress
      ? timeLeft(startedAt, progress.bytes, progress.total)
      : "";
    sheet.querySelector<HTMLButtonElement>("#cancel-update")!.onclick = () => {
      cancelUpdate();
    };
  }

  async function run(update: GameUpdate) {
    busy = true;
    startedAt = 0;
    showProgress(update, null);
    const unlisten = await onUpdateProgress((progress) => {
      if (!startedAt) startedAt = Date.now();
      showProgress(update, progress);
    });
    try {
      await installUpdate(titleId, update);
      installed = update.version;
      store.setGames(await listGames());
      busy = false;
      onChanged();
      await show(`Version ${update.version} installed.`);
    } catch (err) {
      busy = false;
      // Every update that finished before this one stays installed, and the
      // version the game now runs says so.
      const games = await listGames();
      store.setGames(games);
      installed = games.find((g) => g.title_id === titleId)?.running_version ?? installed;
      onChanged();
      const message =
        err === "cancelled"
          ? "Stopped. The updates that had finished stay installed."
          : typeof err === "string"
            ? err
            : "Couldn't install that update.";
      await show(message);
    } finally {
      unlisten();
    }
  }

  async function show(notice?: string) {
    sheet.innerHTML = `
      <div class="sheet-h">Official updates</div>
      <div class="sheet-p"></div>
      ${notice ? `<div class="notice" style="margin-top:14px"></div>` : ""}
      <div class="settings-scroll" id="update-list">
        <div class="sheet-p">Checking with Sony…</div>
      </div>
      <div class="sheet-foot">
        <span class="cfg-v" id="update-count"></span>
        <div class="sheet-actions"><button class="btn solid" id="updates-done">Done</button></div>
      </div>
    `;
    sheet.querySelector<HTMLElement>(".sheet-p")!.textContent = installed
      ? `${title} is on version ${installed}.`
      : `${title} has no update installed.`;
    if (notice) sheet.querySelector<HTMLElement>(".notice")!.textContent = notice;
    sheet.querySelector<HTMLButtonElement>("#updates-done")!.onclick = close;

    const list = sheet.querySelector<HTMLElement>("#update-list")!;
    const count = sheet.querySelector<HTMLElement>("#update-count")!;

    let updates: GameUpdate[];
    try {
      updates = await gameUpdates(titleId);
    } catch (err) {
      list.textContent = "";
      const failed = document.createElement("div");
      failed.className = "sheet-p";
      failed.textContent =
        typeof err === "string" ? err : "Couldn't reach Sony's update service.";
      list.appendChild(failed);
      return;
    }

    list.textContent = "";
    if (updates.length === 0) {
      const none = document.createElement("div");
      none.className = "sheet-p";
      none.textContent = "Sony never published an update for this game.";
      list.appendChild(none);
      count.textContent = "None published";
      return;
    }
    count.textContent = `${updates.length} published`;

    // Going back is possible, but a save written by a newer version may not
    // load on an older one, and that part cannot be undone. Said once, above
    // the list, rather than on every row.
    const warning = document.createElement("div");
    warning.className = "notice";
    warning.style.marginBottom = "14px";
    warning.textContent =
      "Each update needs the one before it, so installing one also installs any older ones still missing. You can go back to an older version later, but a saved game made on a newer one may not load on it.";
    list.appendChild(warning);

    const from = installed ?? shipped;
    for (const update of updates) {
      const row = document.createElement("div");
      row.className = update.version === installed ? "setting changed" : "setting";

      const left = document.createElement("div");
      const name = document.createElement("div");
      name.className = "setting-k";
      name.textContent = `Version ${update.version}`;
      const detail = document.createElement("div");
      detail.className = "setting-path";
      const needed = updates.filter(
        (u) => isNewer(u.version, from) && !isNewer(u.version, update.version)
      );
      detail.textContent =
        needed.length > 1
          ? `${needed.length} updates to install, ${formatSize(needed.reduce((sum, u) => sum + u.size, 0))} in all · needs firmware ${update.firmware}`
          : `${formatSize(update.size)} · needs firmware ${update.firmware}`;
      left.append(name, detail);

      const right = document.createElement("div");
      right.className = "row-actions";
      if (update.version === installed) {
        const mark = document.createElement("span");
        mark.className = "status go";
        mark.textContent = "Installed";
        right.appendChild(mark);
      } else {
        const install = document.createElement("button");
        install.className = "small-btn";
        install.textContent = "Install";
        install.onclick = () => run(update);
        right.appendChild(install);
      }

      row.append(left, right);
      list.appendChild(row);
    }
  }

  await show();
}
