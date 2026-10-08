import { open } from "@tauri-apps/plugin-dialog";
import {
  cancelImport,
  droppedKind,
  gameWarning,
  getGamesFolder,
  importArchive,
  importCheck,
  importGame,
  listGames,
  onImportProgress,
  onScanProgress,
  scanFolder,
  setGamesFolder,
  type ImportCheck,
  type ImportProgress,
  type ImportWarning,
  type ScanProgress,
  type ScanResult,
} from "../api";
import { store } from "../state";

/// Disc images, which are played where they are: the Wii U's, which Cemu
/// reads with the user's keys, and the Wii's and GameCube's, which Dolphin
/// reads (backends/dolphin/disc.rs). A .bin picked here is one only when its
/// header says so.
const DISC_IMAGES = ["wud", "wux", "iso", "gcm", "wbfs", "rvz", "wia", "gcz", "ciso", "tgc", "bin"];

function formatGB(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}

export function openImportSheet(): void {
  sheet();
}

/// Paths dropped on the window skip the choices: the user has already said what
/// they want imported.
///
/// Kept separate from `openImportSheet` so neither can be wired to `onclick`
/// by mistake, which would hand a PointerEvent in as a list of paths.
export function importDropped(paths: string[]): void {
  sheet(paths);
}

function sheet(dropped?: string[]): void {
  const scrim = document.createElement("div");
  scrim.className = "scrim";
  const sheet = document.createElement("div");
  sheet.className = "sheet";
  scrim.appendChild(sheet);
  document.body.appendChild(scrim);
  requestAnimationFrame(() => scrim.classList.add("on"));

  let busy = false;
  /// Set while the warning waits for an answer, so closing the sheet any
  /// way, Escape or a click outside, answers it as Cancel.
  let answer: ((importIt: boolean) => void) | null = null;

  function close() {
    if (busy) return;
    if (answer) {
      const cancel = answer;
      answer = null;
      cancel(false);
    }
    scrim.classList.remove("on");
    setTimeout(() => scrim.remove(), 200);
  }

  scrim.onclick = (e) => {
    if (e.target === scrim) close();
  };
  document.addEventListener("keydown", function onKey(e) {
    if (e.key === "Escape" && !busy) {
      document.removeEventListener("keydown", onKey);
      close();
    }
  });

  function showChoices(note?: string) {
    busy = false;
    sheet.innerHTML = `
      <div class="sheet-h">Import a game</div>
      <div class="sheet-p">Point Omoio at a folder you've already unpacked, a .7z or .zip archive, a Wii&nbsp;U .wua, or a Wii&nbsp;U, Wii or GameCube disc image.</div>
      ${note ? `<div class="notice" style="margin-top:16px">${note}</div>` : ""}
      <div class="sheet-actions">
        <button class="btn ghost" id="pick-folder">Choose a folder</button>
        <button class="btn solid" id="pick-archive">Choose a file</button>
      </div>
      <div class="sheet-alt">
        <button class="link-btn" id="pick-scan">Scan a folder for games</button>
      </div>
    `;
    sheet.querySelector<HTMLButtonElement>("#pick-folder")!.onclick = pickFolder;
    sheet.querySelector<HTMLButtonElement>("#pick-archive")!.onclick = pickArchive;
    sheet.querySelector<HTMLButtonElement>("#pick-scan")!.onclick = pickScan;
  }

  /// `counter` is set only when more than one thing was dropped, so a single
  /// import does not get a needless "1 of 1".
  function showProgress(progress: ImportProgress, counter = "") {
    const pct =
      progress.stage === "unpacking" && progress.total > 0
        ? Math.min(100, Math.round((progress.bytes / progress.total) * 100))
        : null;
    const label =
      progress.stage === "identifying"
        ? "Reading the game…"
        : `Unpacking… ${formatGB(progress.bytes)} of ${formatGB(progress.total)}`;
    sheet.innerHTML = `
      <div class="sheet-h">Importing${counter ? ` ${counter}` : ""}</div>
      <div class="progress-row" style="margin-top:14px">
        <div class="progress-label">
          <span>${label}</span>
          ${pct !== null ? `<span class="pct">${pct}%</span>` : ""}
        </div>
        <div class="progress">
          <div class="progress-fill${pct === null ? " indeterminate" : ""}"${
            pct !== null ? ` style="width:${pct}%"` : ""
          }></div>
        </div>
      </div>
      <div class="sheet-actions">
        <button class="btn ghost" id="cancel-import">Cancel</button>
      </div>
    `;
    sheet.querySelector<HTMLButtonElement>("#cancel-import")!.onclick = () => {
      cancelImport();
    };
  }

  /// One game's warning, filled in by text so a game's own name never goes
  /// through innerHTML.
  function warningItem(warning: ImportWarning): HTMLElement {
    const item = document.createElement("li");
    item.className = "warn-item";
    item.innerHTML = `
      <div class="warn-item-h"><span class="warn-item-name"></span><span class="warn-item-console"></span></div>
      <div class="warn-item-p"></div>
    `;
    item.querySelector<HTMLElement>(".warn-item-name")!.textContent = warning.title;
    item.querySelector<HTMLElement>(".warn-item-console")!.textContent = warning.console_name;
    item.querySelector<HTMLElement>(".warn-item-p")!.textContent = warning.rating;
    if (warning.better) {
      const better = document.createElement("div");
      better.className = "warn-item-better";
      better.textContent = warning.better;
      item.appendChild(better);
    }
    return item;
  }

  /// Asks before a game that may not run well is imported. True for Import
  /// anyway; Cancel, Escape and a click outside the sheet are all false.
  function confirmWarning(warning: ImportWarning): Promise<boolean> {
    busy = false;
    sheet.setAttribute("role", "alertdialog");
    sheet.setAttribute("aria-labelledby", "warn-title");
    sheet.setAttribute("aria-describedby", "warn-rating");
    sheet.innerHTML = `
      <div class="warn-kicker">May not run well</div>
      <div class="sheet-h" id="warn-title"></div>
      <div class="warn-console"></div>
      <div class="sheet-p warn-rating" id="warn-rating"></div>
      ${warning.better ? `<div class="warn-better"></div>` : ""}
      <div class="sheet-actions">
        <button class="btn ghost" id="warn-cancel">Cancel</button>
        <button class="btn solid" id="warn-import">Import anyway</button>
      </div>
    `;
    sheet.querySelector<HTMLElement>("#warn-title")!.textContent = warning.title;
    sheet.querySelector<HTMLElement>(".warn-console")!.textContent = warning.console_name;
    sheet.querySelector<HTMLElement>("#warn-rating")!.textContent = warning.rating;
    const better = sheet.querySelector<HTMLElement>(".warn-better");
    if (better) better.textContent = warning.better;

    return new Promise((resolve) => {
      answer = (importIt) => {
        sheet.removeAttribute("role");
        sheet.removeAttribute("aria-labelledby");
        sheet.removeAttribute("aria-describedby");
        resolve(importIt);
      };
      const choose = (importIt: boolean) => {
        const chosen = answer;
        answer = null;
        chosen?.(importIt);
      };
      sheet.querySelector<HTMLButtonElement>("#warn-cancel")!.onclick = () => {
        choose(false);
        close();
      };
      const importIt = sheet.querySelector<HTMLButtonElement>("#warn-import")!;
      importIt.onclick = () => choose(true);
      importIt.focus();
    });
  }

  /// Games that went in and may not run well, said together once the
  /// import is done.
  function showWarnings(heading: string, lines: string[], warnings: ImportWarning[]) {
    busy = false;
    sheet.innerHTML = `
      <div class="sheet-h"></div>
      ${lines.length > 0 ? `<div class="sheet-p"></div>` : ""}
      <div class="sec-h warn-list-h">May not run well</div>
      <ul class="warn-list"></ul>
      <div class="sheet-actions">
        <button class="btn solid" id="warn-done">Done</button>
      </div>
    `;
    sheet.querySelector<HTMLElement>(".sheet-h")!.textContent = heading;
    const said = sheet.querySelector<HTMLElement>(".sheet-p");
    if (said) said.textContent = lines.join(" ");
    const list = sheet.querySelector<HTMLElement>(".warn-list")!;
    for (const warning of warnings) list.appendChild(warningItem(warning));
    const done = sheet.querySelector<HTMLButtonElement>("#warn-done")!;
    done.onclick = close;
    done.focus();
  }

  /// Imports one game, and says first when it may not run well. That comes
  /// before the wait: an archive is told by the small file inside that names
  /// its game. One that can only be told once unpacked is checked after.
  async function importOne(path: string, kind: "archive" | "other") {
    busy = true;
    showProgress({ stage: "identifying", bytes: 0, total: 0 });
    let check: ImportCheck = { checked: true, warning: null };
    try {
      check = await importCheck(path);
    } catch {
      // Nothing to warn about. The import itself says what is wrong.
    }
    if (check.warning && !(await confirmWarning(check.warning))) return;

    await run(kind === "archive" ? "unpacking" : "identifying", async () => {
      const game = await (kind === "archive" ? importArchive(path) : importGame(path));
      if (check.checked) return [];
      const later = await gameWarning(game.title_id).catch(() => null);
      return later ? [later] : [];
    });
  }

  async function pickFolder() {
    const picked = await open({ directory: true, multiple: false, title: "Choose a game folder" });
    if (typeof picked !== "string") return;
    await importOne(picked, "other");
  }

  /// Unpacking needs somewhere to put 19 GB, so that is settled before an
  /// archive starts rather than after the wait. False when the user backed out.
  async function haveGamesFolder(): Promise<boolean> {
    if (await getGamesFolder()) return true;
    const chosen = await open({
      directory: true,
      multiple: false,
      title: "Choose where to keep your games",
    });
    if (typeof chosen !== "string") return false;
    await setGamesFolder(chosen);
    return true;
  }

  async function pickArchive() {
    const picked = await open({
      multiple: false,
      directory: false,
      title: "Choose a game file",
      filters: [{ name: "Game archive, .wua or disc image", extensions: ["7z", "zip", "wua", ...DISC_IMAGES] }],
    });
    if (typeof picked !== "string") return;
    // A .wua or disc image is played where it is, as an unpacked folder is.
    if (new RegExp(`\\.(wua|${DISC_IMAGES.join("|")})$`, "i").test(picked)) {
      await importOne(picked, "other");
      return;
    }
    if (!(await haveGamesFolder())) return;

    await importOne(picked, "archive");
  }

  function showScanProgress(progress: ScanProgress) {
    // Finding the games and reading them are two different waits. Counting
    // folders found says something is happening before a total is knowable.
    const label =
      progress.stage === "looking"
        ? `Looking for games… ${progress.done} found`
        : `Reading ${progress.title}`;
    const pct =
      progress.stage === "reading" && progress.total > 0
        ? Math.min(100, Math.round((progress.done / progress.total) * 100))
        : null;
    sheet.innerHTML = `
      <div class="sheet-h">Scanning</div>
      <div class="progress-row" style="margin-top:14px">
        <div class="progress-label">
          <span class="scan-label"></span>
          ${pct !== null ? `<span class="pct">${progress.done} of ${progress.total}</span>` : ""}
        </div>
        <div class="progress">
          <div class="progress-fill${pct === null ? " indeterminate" : ""}"${
            pct !== null ? ` style="width:${pct}%"` : ""
          }></div>
        </div>
      </div>
      <div class="sheet-actions">
        <button class="btn ghost" id="cancel-scan">Cancel</button>
      </div>
    `;
    // A game's own title, so never through innerHTML.
    sheet.querySelector<HTMLElement>(".scan-label")!.textContent = label;
    sheet.querySelector<HTMLButtonElement>("#cancel-scan")!.onclick = () => {
      cancelImport();
    };
  }

  function showScanResult(result: ScanResult) {
    busy = false;
    const lines = [
      result.added === 0
        ? ""
        : result.added === 1
          ? "1 game added."
          : `${result.added} games added.`,
      result.already_there > 0 ? `${result.already_there} already in your library.` : "",
      result.cancelled ? "Stopped early." : "",
    ].filter(Boolean);
    if (result.warnings.length > 0) {
      showWarnings("Games added", lines, result.warnings);
      return;
    }
    sheet.innerHTML = `
      <div class="sheet-h">${result.added > 0 ? "Games added" : "Nothing new"}</div>
      <div class="sheet-p">${
        result.added === 0 && result.already_there === 0 && !result.cancelled
          ? "No games were found in that folder."
          : lines.join(" ")
      }</div>
      <div class="sheet-actions">
        <button class="btn solid" id="scan-done">Done</button>
      </div>
    `;
    sheet.querySelector<HTMLButtonElement>("#scan-done")!.onclick = close;
  }

  async function pickScan() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose a folder to scan",
    });
    if (typeof picked !== "string") return;

    busy = true;
    showScanProgress({ stage: "looking", done: 0, total: 0, title: "" });
    const unlisten = await onScanProgress(showScanProgress);
    try {
      const result = await scanFolder(picked);
      store.setGames(await listGames());
      showScanResult(result);
    } catch (err) {
      busy = false;
      showChoices(typeof err === "string" ? err : "Couldn't scan that folder.");
    } finally {
      unlisten();
    }
  }

  /// Runs an import with its progress on screen. `job` hands back anything
  /// to warn about that could only be known once the game was in.
  async function run(stage: ImportProgress["stage"], job: () => Promise<ImportWarning[]>) {
    busy = true;
    showProgress({ stage, bytes: 0, total: 0 });
    const unlisten = await onImportProgress(showProgress);
    try {
      const warnings = await job();
      store.setGames(await listGames());
      busy = false;
      if (warnings.length > 0) {
        showWarnings("Game added", [], warnings);
      } else {
        close();
      }
    } catch (err) {
      busy = false;
      if (err === "cancelled") {
        showChoices();
      } else {
        showChoices(typeof err === "string" ? err : "Couldn't import that game.");
      }
    } finally {
      unlisten();
    }
  }

  /// Imports what was dropped, one at a time so a failure part way through
  /// still leaves everything before it in the library. One game gets its
  /// warning before it is imported; several are not stopped for one by one,
  /// and what may not run well is said once at the end, as a scan does.
  async function runDropped(paths: string[]) {
    // Anything that is not an archive goes to the emulators to read, so a
    // file they cannot take gets their reason rather than a general one.
    const kindOf = async (path: string) =>
      (await droppedKind(path)) === "archive" ? ("archive" as const) : ("other" as const);

    if (paths.length === 1) {
      const kind = await kindOf(paths[0]);
      if (kind === "archive" && !(await haveGamesFolder())) {
        showChoices();
        return;
      }
      await importOne(paths[0], kind);
      return;
    }

    busy = true;
    const counter = (at: number) => `${at + 1} of ${paths.length}`;
    let added = 0;
    let problem = "";
    const warnings: ImportWarning[] = [];

    for (const [at, path] of paths.entries()) {
      const kind = await kindOf(path);
      if (kind === "archive" && !(await haveGamesFolder())) break;

      showProgress({ stage: kind === "archive" ? "unpacking" : "identifying", bytes: 0, total: 0 }, counter(at));
      const unlisten = await onImportProgress((progress) => showProgress(progress, counter(at)));
      try {
        const game = await (kind === "archive" ? importArchive(path) : importGame(path));
        added += 1;
        const warning = await gameWarning(game.title_id).catch(() => null);
        if (warning) warnings.push(warning);
      } catch (err) {
        // Cancelling stops the run rather than moving to the next one: the
        // Cancel button means this, not this one.
        if (err === "cancelled") break;
        problem ||= typeof err === "string" ? err : "Couldn't import that game.";
      } finally {
        unlisten();
      }
    }

    store.setGames(await listGames());
    busy = false;
    if (warnings.length > 0) {
      const count = added === 1 ? "1 game added." : `${added} games added.`;
      showWarnings(added === 1 ? "Game added" : "Games added", [count, problem].filter(Boolean), warnings);
      return;
    }
    if (added > 0 && !problem) {
      close();
      return;
    }
    showChoices(problem || "Nothing was imported.");
  }

  if (dropped && dropped.length > 0) {
    void runDropped(dropped);
  } else {
    showChoices();
  }
}
