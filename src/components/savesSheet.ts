import {
  backUpSaves,
  EMULATOR_OF,
  forgetBackup,
  gameSaves,
  restoreSaves,
  type Game,
  type SaveBackup,
} from "../api";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const mb = bytes / 1024 ** 2;
  return mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.round(bytes / 1024)} KB`;
}

function formatWhen(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export async function openSaves(game: Game, onChanged: () => void): Promise<void> {
  const titleId = game.title_id;
  // A copy is taken before an update only where Omoio installs updates.
  const beforeUpdates = game.features.updates;
  const scrim = document.createElement("div");
  scrim.className = "scrim";
  const sheet = document.createElement("div");
  sheet.className = "sheet wide";
  scrim.appendChild(sheet);
  document.body.appendChild(scrim);
  requestAnimationFrame(() => scrim.classList.add("on"));

  let busy = false;
  function close() {
    if (busy) return;
    scrim.classList.remove("on");
    setTimeout(() => scrim.remove(), 200);
  }
  scrim.onclick = (e) => {
    if (e.target === scrim) close();
  };

  const head = document.createElement("div");
  head.innerHTML = `
    <div class="sheet-h">Saved games</div>
    <div class="sheet-p"></div>
  `;
  const emulator = EMULATOR_OF[game.console];
  head.querySelector<HTMLElement>(".sheet-p")!.textContent = beforeUpdates
    ? `${game.title}. One is taken automatically before every update, and reinstalling ${emulator} does not touch these.`
    : `${game.title}. Reinstalling ${emulator} does not touch these.`;
  sheet.appendChild(head);

  const list = document.createElement("div");
  list.className = "settings-scroll";
  sheet.appendChild(list);

  const foot = document.createElement("div");
  foot.className = "sheet-foot";
  const note = document.createElement("span");
  note.className = "cfg-v";
  const actions = document.createElement("div");
  actions.className = "sheet-actions";
  const backUp = document.createElement("button");
  backUp.className = "btn ghost";
  backUp.textContent = "Back up now";
  const done = document.createElement("button");
  done.className = "btn solid";
  done.textContent = "Done";
  done.onclick = close;
  actions.append(backUp, done);
  foot.append(note, actions);
  sheet.appendChild(foot);

  function message(text: string): HTMLElement {
    const line = document.createElement("div");
    line.className = "sheet-p";
    line.textContent = text;
    return line;
  }

  async function show(notice?: string) {
    list.textContent = "";
    if (notice) {
      const said = document.createElement("div");
      said.className = "notice";
      said.style.marginBottom = "14px";
      said.textContent = notice;
      list.appendChild(said);
    }

    const [hasSaves, backups] = await gameSaves(titleId);
    backUp.disabled = !hasSaves;

    if (backups.length === 0) {
      list.appendChild(
        message(
          !hasSaves
            ? "This game hasn't saved anything yet, so there is nothing to copy."
            : beforeUpdates
              ? "No copies kept yet. Back up now, or let the next update do it."
              : "No copies kept yet. Back up now to keep one."
        )
      );
      note.textContent = hasSaves ? "Nothing kept yet" : "No saves yet";
      return;
    }
    note.textContent = backups.length === 1 ? "1 copy kept" : `${backups.length} copies kept`;

    for (const backup of backups) {
      list.appendChild(backupRow(backup));
    }
  }

  function backupRow(backup: SaveBackup): HTMLElement {
    const row = document.createElement("div");
    row.className = "setting";

    const left = document.createElement("div");
    const when = document.createElement("div");
    when.className = "setting-k";
    when.textContent = formatWhen(backup.made);
    const detail = document.createElement("div");
    detail.className = "setting-path";
    detail.textContent = `${formatSize(backup.bytes)} · ${
      backup.saves === 1 ? "1 save" : `${backup.saves} saves`
    }`;
    left.append(when, detail);

    const right = document.createElement("div");
    right.className = "row-actions";

    const restore = document.createElement("button");
    restore.className = "small-btn";
    restore.textContent = "Restore";
    restore.onclick = async () => {
      // Replacing saves is the one thing here that destroys something, so it
      // asks first and the button says what it will do.
      if (restore.textContent === "Restore") {
        restore.textContent = "Replace saves?";
        restore.classList.add("danger");
        return;
      }
      busy = true;
      restore.disabled = true;
      restore.textContent = "Restoring…";
      try {
        await restoreSaves(titleId, backup.made);
        busy = false;
        onChanged();
        await show("Saves restored. What was there was copied aside first.");
      } catch (err) {
        busy = false;
        await show(typeof err === "string" ? err : "Couldn't restore that copy.");
      }
    };

    const forget = document.createElement("button");
    forget.className = "link-btn";
    forget.textContent = "Delete";
    forget.onclick = async () => {
      if (forget.textContent === "Delete") {
        forget.textContent = "Sure?";
        return;
      }
      await forgetBackup(titleId, backup.made);
      onChanged();
      await show();
    };

    right.append(restore, forget);
    row.append(left, right);
    return row;
  }

  backUp.onclick = async () => {
    backUp.disabled = true;
    backUp.textContent = "Backing up…";
    try {
      const made = await backUpSaves(titleId);
      onChanged();
      await show(made ? "Copy taken." : "This game hasn't saved anything yet.");
    } catch (err) {
      await show(typeof err === "string" ? err : "Couldn't copy the saves.");
    } finally {
      backUp.textContent = "Back up now";
      backUp.disabled = false;
    }
  };

  await show();
}
