import {
  controllerView,
  forgetController,
  padInput,
  saveController,
  setUpController,
  type ControllerView,
} from "../api";
import { capFace, partOf } from "../components/padArt";
import { nameOf } from "../components/padNames";
import {
  DIRECTION,
  FACE,
  GROUPS,
  MIDDLE,
  SHOULDERS,
  assignInput,
  consoleWords,
  padStage,
  type Callout,
} from "../components/padStage";
import { store } from "../state";
import type { View } from "./view";

/// The player whose buttons are on screen, and whether the directions are
/// open. Kept across redraws, so a change does not jump back to player 1.
let shownPlayer = 0;
let directionsOpen = false;
/// What had the keyboard's focus before a change drew the screen again.
let refocus: string | null = null;

/// How long a label waits for a press before giving up.
const LISTEN_MS = 6000;

function scopePicker(scope: string): HTMLElement {
  const games = (store.get().games ?? []).filter((game) => game.set_up);
  const label = document.createElement("label");
  label.className = "pad-scope";
  label.append("Layout for");
  const select = document.createElement("select");
  select.className = "select";
  select.add(new Option("Every game", ""));
  for (const game of games) select.add(new Option(game.title, game.title_id));
  select.value = scope;
  select.onchange = () => store.setControllerScope(select.value);
  label.append(select);
  return label;
}

/// One player as a tab: their number, their pad, and whether it is in.
function playerTab(view: ControllerView, index: number): HTMLButtonElement {
  const player = view.players[index];
  const tab = document.createElement("button");
  tab.className = index === shownPlayer ? "player-tab on" : "player-tab";
  tab.setAttribute("role", "tab");
  tab.setAttribute("aria-selected", String(index === shownPlayer));
  tab.dataset.key = `player:${index}`;
  tab.innerHTML = `
    <span class="player-num">${index + 1}</span>
    <span class="player-tab-words">
      <span class="player-tab-name">Player ${index + 1}</span>
      <span class="player-tab-pad"><span class="dot"></span><span class="player-tab-pad-name"></span></span>
    </span>
  `;
  tab.querySelector<HTMLElement>(".player-tab-pad-name")!.textContent = player.pad.name;
  tab.querySelector(".dot")!.classList.toggle("on", player.connected);
  tab.onclick = () => {
    shownPlayer = index;
    refocus = tab.dataset.key!;
    store.redraw();
  };
  return tab;
}

export async function renderController(): Promise<View> {
  const scope = store.get().controllerScope ?? "";
  const view = await controllerView(scope);
  if (shownPlayer >= view.players.length) shownPlayer = 0;
  const content = document.createElement("div");
  content.className = "controller";

  const note = document.createElement("div");
  note.className = "note plain";

  const head = document.createElement("div");
  head.className = "pad-top";
  const status = document.createElement("div");
  status.className = "pad-status";
  const plugged = view.players.filter((p) => p.connected).length;
  status.textContent = !view.saved
    ? "Nothing saved yet. Pressing Play sets up these four players."
    : scope && !view.own
      ? "This game uses the layout for every game. Changing a button gives it its own."
      : plugged === 0
        ? "No pads plugged in. Each player waits for theirs."
        : `${plugged} of ${view.players.length} players have their pad plugged in.`;

  const actions = document.createElement("div");
  actions.className = "row-actions";
  actions.appendChild(scopePicker(scope));
  if (scope && view.own) {
    const back = document.createElement("button");
    back.className = "link-btn";
    back.textContent = "Use the layout for every game";
    back.onclick = async () => {
      await forgetController(scope);
      store.redraw();
    };
    actions.appendChild(back);
  }
  const reset = document.createElement("button");
  reset.className = "small-btn";
  reset.textContent = view.saved ? "Reset all players" : "Save these players";
  reset.onclick = async () => {
    try {
      await setUpController(scope);
      store.redraw();
    } catch (err) {
      note.textContent = typeof err === "string" ? err : "Couldn't set up the controllers.";
    }
  };
  actions.appendChild(reset);
  head.append(status, actions);
  content.append(head, note);

  const tabs = document.createElement("div");
  tabs.className = "player-tabs";
  tabs.setAttribute("role", "tablist");
  tabs.setAttribute("aria-label", "Players");
  view.players.forEach((_, index) => tabs.appendChild(playerTab(view, index)));
  content.appendChild(tabs);

  // The chosen player: their pad drawn with what each button does around
  // it, and the pad they have.
  const player = view.players[shownPlayer];
  const pad = player.pad;
  const family = pad.family;
  const buttons: Record<string, string> = { ...player.buttons };

  const board = document.createElement("div");
  board.className = "pad-board";
  const boardHead = document.createElement("div");
  boardHead.className = "pad-board-head";
  const boardTitle = document.createElement("div");
  boardTitle.className = "pad-board-title";
  boardTitle.textContent = `Player ${shownPlayer + 1}`;
  const picker = document.createElement("select");
  picker.className = "select";
  picker.setAttribute("aria-label", `Pad for player ${shownPlayer + 1}`);
  for (const option of view.pads) {
    const isIn = view.connected.some((c) => c.device === option.device);
    picker.add(new Option(isIn ? `${option.name} (plugged in)` : option.name, option.device));
  }
  picker.value = pad.device;
  picker.onchange = async () => {
    const chosen = view.pads.find((p) => p.device === picker.value);
    if (!chosen) return;
    try {
      await saveController(scope, shownPlayer + 1, chosen, player.buttons);
      note.textContent = "";
    } catch (err) {
      note.textContent = typeof err === "string" ? err : "Couldn't save the controller settings.";
    }
    store.redraw();
  };
  const state = document.createElement("span");
  state.className = "pad-board-state";
  state.innerHTML = `<span class="dot${player.connected ? " on" : ""}"></span>`;
  state.append(player.connected ? "Plugged in" : "Waiting for this pad");
  boardHead.append(boardTitle, picker, state);

  const save = async () => {
    try {
      await saveController(scope, shownPlayer + 1, pad, buttons);
      note.textContent = "";
    } catch (err) {
      note.textContent = typeof err === "string" ? err : "Couldn't save the controller settings.";
    }
    store.redraw();
  };

  const assign = (place: string, input: string) => assignInput(buttons, place, input);

  // Only once this player's buttons differ from the pad's own, so it is
  // there exactly when it would do something.
  if (Object.entries(buttons).some(([place, input]) => place !== input)) {
    const undo = document.createElement("button");
    undo.className = "small-btn";
    undo.dataset.key = "reset-buttons";
    undo.textContent = "Reset buttons";
    undo.onclick = () => {
      for (const place of Object.keys(buttons)) buttons[place] = place;
      refocus = `player:${shownPlayer}`;
      void save();
    };
    boardHead.append(undo);
  }

  // Everything that can be given a button, by place: the labels around the
  // drawing and the rows of directions.
  const targets = new Map<string, { el: HTMLButtonElement; key: HTMLElement; words: HTMLElement }>();
  let listening: { place: string; until: number } | null = null;

  const paint = () => {
    for (const [place, target] of targets) {
      const waiting = listening?.place === place;
      target.el.classList.toggle("listening", waiting);
      target.key.innerHTML = waiting ? "…" : capFace(family, buttons[place]);
      target.words.classList.toggle("gone", waiting);
      target.el.querySelector(".pad-asking")?.remove();
      if (waiting) {
        const asking = document.createElement("span");
        asking.className = "pad-asking";
        asking.textContent = "Press a button";
        target.words.after(asking);
      }
    }
  };
  const stopListening = () => {
    listening = null;
    paint();
  };

  /// Without the pad to press, a label opens a list of every input instead.
  const choose = (place: string, target: { el: HTMLButtonElement; key: HTMLElement }) => {
    const select = document.createElement("select");
    select.className = "select pad-choose";
    select.setAttribute("aria-label", "Button");
    for (const input of view.inputs) select.add(new Option(nameOf(family, input), input));
    select.value = buttons[place];
    select.onchange = () => {
      assign(place, select.value);
      refocus = target.el.dataset.key ?? null;
      void save();
    };
    select.onblur = () => {
      if (select.isConnected) select.replaceWith(target.key);
    };
    select.onclick = (event) => event.stopPropagation();
    target.key.replaceWith(select);
    select.focus();
    // Opens the list at once where the browser allows it; where it doesn't,
    // the focused list opens on the next click or key.
    try {
      select.showPicker();
    } catch {
      return;
    }
  };

  const target = (place: string, className: string, words: HTMLElement): HTMLButtonElement => {
    const el = document.createElement("button");
    el.className = className;
    el.dataset.key = `place:${place}`;
    el.dataset.part = partOf(buttons[place]);
    const key = document.createElement("span");
    key.className = "cap";
    el.append(key, words);
    const entry = { el, key, words };
    targets.set(place, entry);
    el.onclick = () => {
      if (!player.connected) return choose(place, entry);
      listening = listening?.place === place ? null : { place, until: Date.now() + LISTEN_MS };
      paint();
    };
    el.onkeydown = (event) => {
      if (event.key === "Escape" && listening?.place === place) stopListening();
    };
    return el;
  };

  const dirs = document.createElement("div");
  dirs.className = directionsOpen ? "pad-dirs" : "pad-dirs gone";
  const openDirections = (group: string) => {
    directionsOpen = true;
    dirs.classList.remove("gone");
    dirsToggle.setAttribute("aria-expanded", "true");
    const first = dirs.querySelector<HTMLElement>(`[data-group="${group}"] button`);
    first?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    first?.focus({ preventScroll: true });
  };

  const callouts: Callout[] = SHOULDERS.map(([place, side]) => ({
    key: place,
    at: partOf(buttons[place]),
    side,
    rows: [target(place, "pad-place", consoleWords(view.consoles, place))],
  }));
  callouts.push(
    { key: "face", at: "Face", side: "right", rows: FACE.map((place) => target(place, "pad-place", consoleWords(view.consoles, place))) },
    { key: "middle", at: "Guide", side: "below", rows: MIDDLE.map((place) => target(place, "pad-place", consoleWords(view.consoles, place))) }
  );
  for (const group of GROUPS) {
    const el = document.createElement("button");
    el.className = "pad-group";
    el.dataset.key = `group:${group.key}`;
    if (group.key !== "Dpad") el.dataset.part = group.key;
    el.innerHTML = `<span class="cap">${capFace(family, group.key === "Dpad" ? "Dpad" : buttons[group.key])}</span>`;
    const words = document.createElement("span");
    words.className = "pad-words";
    words.innerHTML = `<span class="pad-word"></span><span class="pad-console">Directions</span>`;
    words.querySelector(".pad-word")!.textContent = group.title;
    el.append(words);
    el.onclick = () => openDirections(group.key);
    callouts.push({ key: group.key, at: group.key, side: group.side, rows: [el] });
  }
  const { stage, art } = padStage(family, callouts);

  const foot = document.createElement("div");
  foot.className = "pad-board-foot";
  const test = document.createElement("span");
  test.textContent = player.connected
    ? `Press anything on ${pad.name} and it lights up. Pick a label to give it another button.`
    : `Switch on ${pad.name} to test it and press new buttons. Until then, labels choose from a list.`;
  const dirsToggle = document.createElement("button");
  dirsToggle.className = "link-btn";
  dirsToggle.textContent = "D-pad and stick directions";
  dirsToggle.setAttribute("aria-expanded", String(directionsOpen));
  dirsToggle.onclick = () => {
    directionsOpen = !directionsOpen;
    dirs.classList.toggle("gone", !directionsOpen);
    dirsToggle.setAttribute("aria-expanded", String(directionsOpen));
  };
  foot.append(test, dirsToggle);

  for (const group of GROUPS) {
    const box = document.createElement("div");
    box.className = "pad-dir-group";
    box.dataset.group = group.key;
    const heading = document.createElement("div");
    heading.className = "sec-h";
    heading.textContent = group.title;
    box.append(heading);
    for (const place of group.places) {
      const words = document.createElement("span");
      words.className = "pad-words";
      words.textContent = DIRECTION[place];
      box.append(target(place, "pad-dir", words));
    }
    dirs.append(box);
  }
  paint();

  board.append(boardHead, stage, foot, dirs);
  content.appendChild(board);

  const how = document.createElement("div");
  how.className = "note plain";
  how.textContent =
    "One layout works in every emulator. A pad plugged in that no player has takes the place of the first player whose pad is missing when you press Play. Changes take effect the next time a game starts.";
  content.appendChild(how);

  if (refocus) {
    const again = refocus;
    refocus = null;
    requestAnimationFrame(() => content.querySelector<HTMLElement>(`[data-key="${again}"]`)?.focus());
  }

  // While this screen is up, the pad is read several times a second: what is
  // held lights on the drawing and on its label, and a label waiting for a
  // press takes the first button that goes down. A pad switched on or off
  // draws the screen again. It all stops once the screen is replaced. A pad
  // that is not there is asked about less often.
  let held = new Set<string>();
  let busy = false;
  const timer = window.setInterval(async () => {
    if (!content.isConnected) {
      window.clearInterval(timer);
      return;
    }
    if (busy) return;
    busy = true;
    try {
      const answer = await padInput(pad.device);
      if ((answer !== null) !== player.connected) {
        window.clearInterval(timer);
        store.redraw();
        return;
      }
      if (answer === null) return;
      const now = new Set(answer);
      for (const el of art.querySelectorAll("[data-part]")) {
        el.classList.toggle("down", [...now].some((input) => partOf(input) === el.getAttribute("data-part")));
      }
      for (const [place, entry] of targets) entry.el.classList.toggle("flash", now.has(buttons[place]));
      if (listening) {
        const pressed = [...now].find((input) => !held.has(input));
        if (pressed) {
          const place = listening.place;
          listening = null;
          assign(place, pressed);
          refocus = targets.get(place)?.el.dataset.key ?? null;
          void save();
        } else if (Date.now() > listening.until) {
          stopListening();
        }
      }
      held = now;
    } finally {
      busy = false;
    }
  }, player.connected ? 60 : 500);

  const subtitle =
    view.connected.length === 0
      ? "Nothing plugged in right now"
      : view.connected.length === 1
        ? "1 controller plugged in"
        : `${view.connected.length} controllers plugged in`;
  return { title: "Controller", subtitle, content };
}

