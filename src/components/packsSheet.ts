import {
  cancelCommunity,
  communityPacks,
  onCommunityProgress,
  refreshCommunity,
  setCommunityPack,
  type Console,
  type InstallProgress,
  type Pack,
  type Packs,
} from "../api";

const STAGE: Record<InstallProgress["stage"], string> = {
  checking: "Finding the newest packs…",
  downloading: "Downloading…",
  verifying: "Checking the download…",
  extracting: "Unpacking…",
  done: "Done",
};

function toggle(on: boolean): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = on ? "switch on" : "switch";
  button.setAttribute("role", "switch");
  button.setAttribute("aria-checked", String(on));
  const dot = document.createElement("span");
  dot.className = "switch-dot";
  button.appendChild(dot);
  return button;
}

function line(className: string, text: string): HTMLElement {
  const el = document.createElement("div");
  el.className = className;
  el.textContent = text;
  return el;
}

/// The packs that are on, of those that can be: "1 of 4 on".
export function packCount({ have_list, waiting, with_emulator, packs }: Packs): string {
  if (!have_list) return with_emulator ? "Not installed" : "Not downloaded";
  if (waiting) return "After first play";
  const fits = packs.filter((pack) => pack.applies);
  const on = fits.filter((pack) => pack.on).length;
  if (fits.length === 0) return "None";
  return on === 0 ? `${fits.length} available` : `${on} of ${fits.length} on`;
}

function packRow(pack: Pack, change: (on: boolean, choices: Record<string, string>) => Promise<void>): HTMLElement {
  const row = document.createElement("div");
  row.className = pack.applies ? "setting pack" : "setting pack spare";
  const left = document.createElement("div");
  left.className = "pack-words";
  left.appendChild(line("setting-k", pack.name));
  // One Omoio turns on by itself keeps its line once switched off, saying
  // what that costs, so it stands out as a warning then.
  if (pack.on_because) {
    left.appendChild(line(pack.on || !pack.applies ? "setting-hint" : "setting-hint warn", pack.on_because));
  }
  if (pack.about) left.appendChild(line("setting-hint", pack.about));
  if (pack.by) left.appendChild(line("setting-hint", pack.by));
  if (pack.needs) left.appendChild(line("setting-hint warn", pack.needs));

  // A pack's own choices, such as its frame rate, show while it is on.
  const picked = Object.fromEntries(pack.choices.map((choice) => [choice.name, choice.chosen]));
  if (pack.on && pack.choices.length > 0) {
    const choices = document.createElement("div");
    choices.className = "pack-choices";
    for (const choice of pack.choices) {
      const label = document.createElement("label");
      label.className = "pack-choice";
      label.append(choice.name.replace(/:$/, "") || "Preset");
      const select = document.createElement("select");
      select.className = "select";
      for (const option of choice.options) select.add(new Option(option, option));
      select.value = choice.chosen;
      select.onchange = () => void change(true, { ...picked, [choice.name]: select.value });
      label.append(select);
      choices.append(label);
    }
    left.appendChild(choices);
  }

  const right = document.createElement("div");
  right.className = "row-actions";
  if (pack.applies) {
    const control = toggle(pack.on);
    control.setAttribute("aria-label", pack.name);
    control.onclick = async () => {
      control.disabled = true;
      // The switch moves at once, and back if the change can't be saved.
      control.classList.toggle("on", !pack.on);
      try {
        await change(!pack.on, picked);
      } catch {
        control.classList.toggle("on", pack.on);
      } finally {
        control.disabled = false;
      }
    };
    right.appendChild(control);
  }
  row.append(left, right);
  return row;
}

export async function openPacks(titleId: string, title: string, console: Console, onChanged: () => void): Promise<void> {
  const scrim = document.createElement("div");
  scrim.className = "scrim";
  const sheet = document.createElement("div");
  sheet.className = "sheet wide";
  scrim.appendChild(sheet);
  document.body.appendChild(scrim);
  requestAnimationFrame(() => scrim.classList.add("on"));

  let downloading = false;
  function close() {
    if (downloading) void cancelCommunity();
    scrim.classList.remove("on");
    setTimeout(() => scrim.remove(), 200);
  }
  scrim.onclick = (e) => {
    if (e.target === scrim) close();
  };

  const head = document.createElement("div");
  head.innerHTML = `<div class="sheet-h">Community packs</div><div class="sheet-p"></div>`;
  const intro = head.querySelector<HTMLElement>(".sheet-p")!;
  sheet.appendChild(head);

  const bar = document.createElement("div");
  bar.className = "progress-row pack-progress gone";
  bar.innerHTML = `
    <div class="progress-label"><span class="stage"></span><span class="pct"></span></div>
    <div class="progress"><div class="progress-fill"></div></div>
  `;
  sheet.appendChild(bar);

  const body = document.createElement("div");
  body.className = "settings-scroll";
  sheet.appendChild(body);

  const foot = document.createElement("div");
  foot.className = "sheet-foot";
  const note = document.createElement("span");
  note.className = "cfg-v";
  const actions = document.createElement("div");
  actions.className = "sheet-actions";
  const get = document.createElement("button");
  get.className = "btn ghost";
  const done = document.createElement("button");
  done.className = "btn solid";
  done.textContent = "Done";
  done.onclick = close;
  actions.append(get, done);
  foot.append(note, actions);
  sheet.appendChild(foot);

  const empty = (text: string) => body.appendChild(line("sheet-p", text));

  async function show() {
    const packs = await communityPacks(titleId, console);
    body.textContent = "";
    intro.textContent = `Made by ${packs.source || "the emulator's community"} for ${title}. A pack that is on without being asked says why; the rest stay off until you turn them on.`;
    get.textContent = packs.have_list ? "Check for new packs" : "Download packs";
    // Packs that come with the emulator have nothing to download.
    get.classList.toggle("gone", packs.with_emulator);
    note.textContent = packCount(packs);
    if (!packs.have_list) {
      empty(packs.waiting ?? "Download the packs to see what has been made for this game.");
      return;
    }
    if (packs.waiting) {
      empty(packs.waiting);
      return;
    }
    if (packs.packs.length === 0) {
      empty("Nobody has made a pack for this game yet.");
      return;
    }
    // Grouped by kind, with a heading once there is more than one.
    const kinds = [...new Set(packs.packs.map((pack) => pack.kind))];
    for (const kind of kinds) {
      if (kinds.length > 1) body.appendChild(line("sec-h pack-kind", kind));
      for (const pack of packs.packs.filter((each) => each.kind === kind)) {
        body.appendChild(
          packRow(pack, async (on, choices) => {
            try {
              await setCommunityPack(titleId, { id: pack.id, on, choices });
            } catch (err) {
              note.textContent = typeof err === "string" ? err : "Couldn't change that pack.";
              throw err;
            }
            onChanged();
            await show();
          })
        );
      }
    }
  }

  const stage = bar.querySelector<HTMLElement>(".stage")!;
  const pct = bar.querySelector<HTMLElement>(".pct")!;
  const fill = bar.querySelector<HTMLElement>(".progress-fill")!;
  get.onclick = async () => {
    if (downloading) {
      await cancelCommunity();
      return;
    }
    downloading = true;
    const was = get.textContent;
    get.textContent = "Cancel";
    stage.textContent = STAGE.checking;
    pct.textContent = "";
    fill.style.width = "0%";
    bar.classList.remove("gone");
    const unlisten = onCommunityProgress((progress) => {
      stage.textContent = STAGE[progress.stage];
      if (progress.stage === "downloading" && progress.total > 0) {
        const share = Math.min(100, Math.round((progress.bytes / progress.total) * 100));
        fill.style.width = `${share}%`;
        pct.textContent = `${share}%`;
      } else {
        pct.textContent = "";
      }
    });
    try {
      await refreshCommunity(console);
      onChanged();
      await show();
    } catch (err) {
      note.textContent =
        err === "cancelled" ? "Stopped. The packs you had are as they were." : typeof err === "string" ? err : "Couldn't download the packs.";
      get.textContent = was;
    } finally {
      downloading = false;
      bar.classList.add("gone");
      void unlisten.then((stop) => stop());
    }
  };

  await show();
}
