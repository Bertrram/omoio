import {
  controllerView,
  forgetController,
  padInput,
  saveController,
  setUpController,
  type ControllerView,
  type Pad,
  type PlayerSetup,
} from "../api";
import { capFace, partOf } from "../components/padArt";
import { keyNames, keyboardLayout, nameOf, type KeyLayout } from "../components/padNames";
import {
  DIRECTION,
  FACE,
  GROUPS,
  KEY_GROUPS,
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

/// No controller, chosen for a player so no pad is given to them by itself.
/// Kept in the layout as Omoio keeps it (core/pad_layout.rs, `Pad::none`):
/// no device, and a handler of its own.
const NO_PAD: Pad = { device: "", name: "No controller", handler: "None", family: "generic" };

function isNoPad(pad: Pad): boolean {
  return pad.handler === NO_PAD.handler;
}

/// The keyboard and mouse have a handler of their own in the layout
/// (core/pad_layout.rs, `KEYBOARD`).
function isKeyboard(pad: Pad): boolean {
  return pad.handler === "Keyboard";
}

/// Something to say once the screen is drawn again, such as why a change was
/// not saved, as a change draws the whole screen anew.
let told: string | null = null;

/// The mouse moving each way, for the right stick's directions, which can
/// have it back after a key was put there.
const MOUSE_MOVE: Record<string, string> = {
  "RS Y+": "MouseMoveUp",
  "RS Y-": "MouseMoveDown",
  "RS X-": "MouseMoveLeft",
  "RS X+": "MouseMoveRight",
};

/// The mouse's buttons by `MouseEvent.button`, named as core/keys.rs names
/// them.
const MOUSE_BUTTON = ["MouseLeft", "MouseMiddle", "MouseRight", "MouseBack", "MouseForward"];

/// A second press soon after the click that started a wait is that click
/// made double, not an answer. Windows' own double-click time is 500 ms by
/// default.
const DOUBLE_CLICK_MS = 500;

/// A place as the first console with it calls it, for a line about it:
/// "Cross on PS3". The directions and the sticks' presses by their own
/// names, such as "Left stick up".
function placeName(view: ControllerView, place: string): string {
  if (DIRECTION[place]) return nameOf("keyboard", place);
  const named = view.consoles.find((each) => each.buttons[place]);
  return named ? `${named.buttons[place]} on ${named.name}` : place;
}

/// Gives a keyboard player's `place` this one input. Another place that had
/// it loses it, so no key ever does two things by accident, and one left
/// with nothing takes what `place` had, as a pad's buttons swap. The mouse
/// moving only ever turns a stick, so it is not handed on. Returns a place
/// left with nothing even so.
function giveKey(buttons: Record<string, string>, place: string, input: string): string | null {
  const was = (buttons[place] ?? "")
    .split(" ")
    .filter((each) => each && !Object.values(MOUSE_MOVE).includes(each))
    .join(" ");
  let emptied: string | null = null;
  for (const other of Object.keys(buttons)) {
    if (other === place) continue;
    const inputs = buttons[other].split(" ").filter((each) => each);
    if (!inputs.includes(input)) continue;
    buttons[other] = inputs.filter((each) => each !== input).join(" ") || was;
    if (!buttons[other]) emptied = other;
  }
  buttons[place] = input;
  return emptied;
}

/// Keeps the rest of a mouse press that answered a wait from doing anything
/// else: its release, the click after it, and the menu a right click opens,
/// which Windows sends a moment after the release.
function swallowRestOfPress(): void {
  const kinds = ["mouseup", "click", "auxclick", "contextmenu"];
  const done = () => {
    for (const kind of kinds) window.removeEventListener(kind, eat, true);
  };
  function eat(event: Event): void {
    event.preventDefault();
    event.stopPropagation();
    if (event.type === "mouseup") window.setTimeout(done, 250);
  }
  for (const kind of kinds) window.addEventListener(kind, eat, true);
  window.setTimeout(done, LISTEN_MS);
}

/// A wheel turned one notch sends several events. Once the first has
/// answered a wait, the rest don't scroll the screen.
function swallowRestOfWheel(): void {
  const eat = (event: Event) => event.preventDefault();
  window.addEventListener("wheel", eat, { capture: true, passive: false });
  window.setTimeout(() => window.removeEventListener("wheel", eat, true), 300);
}

function subtitleOf(view: ControllerView): string {
  return view.connected.length === 0
    ? "Nothing plugged in right now"
    : view.connected.length === 1
      ? "1 controller plugged in"
      : `${view.connected.length} controllers plugged in`;
}

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

/// Puts the keyboard's focus back where it was before a change drew the
/// screen again.
function restoreFocus(content: HTMLElement): void {
  if (!refocus) return;
  const again = refocus;
  refocus = null;
  requestAnimationFrame(() => content.querySelector<HTMLElement>(`[data-key="${again}"]`)?.focus());
}

/// A keyboard-and-mouse player: every place in a list with the keys and
/// mouse inputs on it, each changed by picking it and then pressing a key
/// or a mouse button, and how fast the mouse turns a stick. `said` is a line
/// left from before the screen was drawn again.
function keyboardBoard(
  view: ControllerView,
  scope: string,
  player: PlayerSetup,
  layout: KeyLayout,
  head: HTMLElement,
  content: HTMLElement,
  said: string | null
): { parts: HTMLElement[]; how: HTMLElement } {
  const number = shownPlayer + 1;
  const buttons: Record<string, string> = { ...player.buttons };

  // Says what a wait for a key wants, and why a key was refused, to screen
  // readers as well.
  const live = document.createElement("div");
  live.className = "note plain key-live";
  live.setAttribute("aria-live", "polite");
  live.textContent = said ?? "";

  const save = async (next: Record<string, string>, mouse = player.mouse) => {
    try {
      await saveController(scope, number, player.pad, next, mouse);
    } catch (err) {
      told = typeof err === "string" ? err : "Couldn't save the controller settings.";
    }
    store.redraw();
  };

  // Only once a key differs from the layout the keyboard starts with, so it
  // is there exactly when it would do something. No keys saved is that
  // layout (core/pad_layout.rs, `Player::with_buttons`).
  if (view.inputs.some((place) => (buttons[place] ?? "") !== (view.keyboard.defaults[place] ?? ""))) {
    const undo = document.createElement("button");
    undo.className = "small-btn";
    undo.dataset.key = "reset-keys";
    undo.textContent = "Reset keys";
    undo.onclick = () => {
      refocus = `player:${shownPlayer}`;
      void save({});
    };
    head.append(undo);
  }

  const [slowest, fastest] = view.keyboard.mouse_range;
  const speed = document.createElement("label");
  speed.className = "key-speed";
  const range = document.createElement("input");
  range.type = "range";
  range.min = String(slowest);
  range.max = String(fastest);
  range.step = "10";
  range.value = String(player.mouse);
  range.dataset.key = "mouse-speed";
  const percent = document.createElement("span");
  percent.className = "key-speed-value";
  percent.textContent = `${player.mouse}%`;
  range.oninput = () => (percent.textContent = `${range.value}%`);
  range.onchange = () => {
    refocus = "mouse-speed";
    void save(buttons, Number(range.value));
  };
  speed.append("Mouse speed", range, percent);

  const rows = new Map<string, { el: HTMLButtonElement; inputs: HTMLElement }>();
  let listening: { place: string; since: number; timer: number } | null = null;

  const paint = () => {
    for (const [place, row] of rows) {
      const waiting = listening?.place === place;
      row.el.classList.toggle("listening", waiting);
      row.inputs.replaceChildren();
      if (waiting) {
        const asking = document.createElement("span");
        asking.className = "pad-asking";
        asking.textContent = "Press a key or click";
        row.inputs.append(asking);
        continue;
      }
      const names = keyNames(buttons[place] ?? "", layout);
      if (names.length === 0) {
        const nothing = document.createElement("span");
        nothing.className = "key-or";
        nothing.textContent = "Nothing";
        row.inputs.append(nothing);
      }
      names.forEach((name, at) => {
        if (at > 0) {
          const or = document.createElement("span");
          or.className = "key-or";
          or.textContent = "or";
          row.inputs.append(or);
        }
        const cap = document.createElement("span");
        cap.className = "cap";
        cap.textContent = name;
        row.inputs.append(cap);
      });
    }
  };

  // While a place waits, the next key pressed anywhere, or the next mouse
  // button or wheel turn on this screen, is its answer. The press or click
  // that picked the place is over by then, as a button is picked once it is
  // let go, and a key held down repeating is not a new press.
  const stop = () => {
    if (!listening) return;
    window.clearTimeout(listening.timer);
    listening = null;
    window.removeEventListener("keydown", onKey, true);
    window.removeEventListener("mousedown", onMouse, true);
    window.removeEventListener("wheel", onWheel, true);
    paint();
  };
  const giveUpLater = () =>
    window.setTimeout(() => {
      stop();
      live.textContent = "";
    }, LISTEN_MS);

  const take = (input: string) => {
    const place = listening!.place;
    stop();
    live.textContent = "";
    if (buttons[place] === input) return;
    refocus = `key:${place}`;
    const next = { ...buttons };
    const emptied = giveKey(next, place, input);
    if (emptied) told = `${placeName(view, emptied)} has nothing on it now. Pick it to give it a key.`;
    void save(next);
  };

  function onKey(event: KeyboardEvent): void {
    if (!listening) return;
    if (!content.isConnected) return stop();
    // Tab, Space and Backspace would otherwise move the focus, press the
    // button or scroll while the key is meant for the game.
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat) return;
    if (event.code === "Escape") {
      stop();
      live.textContent = "";
      return;
    }
    const refused = view.keyboard.refused.find(([code]) => code === event.code);
    if (refused || !view.keyboard.keys.includes(event.code)) {
      live.textContent = refused ? refused[1] : "That key belongs to Windows or the emulators, so it can't be a button.";
      window.clearTimeout(listening.timer);
      listening.timer = giveUpLater();
      return;
    }
    take(event.code);
  }

  function onMouse(event: MouseEvent): void {
    if (!listening) return;
    if (!content.isConnected) return stop();
    // A click elsewhere, such as on the sidebar, does what it was meant to
    // and stops the wait.
    if (!(event.target instanceof Node) || !content.contains(event.target)) {
      stop();
      live.textContent = "";
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    swallowRestOfPress();
    if (event.detail > 1 && Date.now() - listening.since < DOUBLE_CLICK_MS) return;
    const input = MOUSE_BUTTON[event.button];
    if (input) take(input);
  }

  function onWheel(event: WheelEvent): void {
    if (!listening) return;
    if (!content.isConnected) return stop();
    if (!(event.target instanceof Node) || !content.contains(event.target) || event.deltaY === 0) return;
    event.preventDefault();
    swallowRestOfWheel();
    take(event.deltaY < 0 ? "WheelUp" : "WheelDown");
  }

  const listen = (place: string) => {
    stop();
    listening = { place, since: Date.now(), timer: giveUpLater() };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onMouse, true);
    window.addEventListener("wheel", onWheel, { capture: true, passive: false });
    live.textContent = "Press a key or click. Escape stops.";
    paint();
  };

  const row = (place: string, words: HTMLElement): HTMLElement => {
    const line = document.createElement("div");
    line.className = "key-row";
    const el = document.createElement("button");
    el.className = "key-place";
    el.dataset.key = `key:${place}`;
    const inputs = document.createElement("span");
    inputs.className = "key-inputs";
    el.append(words, inputs);
    el.onclick = () => listen(place);
    rows.set(place, { el, inputs });
    line.append(el);
    const move = MOUSE_MOVE[place];
    if (move && !(buttons[place] ?? "").split(" ").includes(move)) {
      const mouse = document.createElement("button");
      mouse.className = "link-btn key-mouse";
      mouse.dataset.key = `mouse:${place}`;
      mouse.textContent = "Use the mouse";
      mouse.onclick = () => {
        const next = { ...buttons };
        giveKey(next, place, move);
        refocus = `key:${place}`;
        void save(next);
      };
      line.append(mouse);
    }
    return line;
  };

  const groups = document.createElement("div");
  groups.className = "key-groups";
  for (const group of KEY_GROUPS) {
    const box = document.createElement("div");
    box.className = "key-group";
    const heading = document.createElement("div");
    heading.className = "sec-h";
    heading.textContent = group.title;
    box.append(heading);
    for (const place of group.places) {
      // Within a direction group the direction alone says enough, and a
      // stick's press says what it is on each console, as on a pad's list.
      const pressed = place === "LS" || place === "RS";
      let words: HTMLElement;
      if (DIRECTION[place] && !pressed) {
        words = document.createElement("span");
        words.className = "pad-words";
        words.textContent = DIRECTION[place];
      } else {
        words = consoleWords(view.consoles, place);
        if (pressed) words.prepend(`${DIRECTION[place]} `);
      }
      box.append(row(place, words));
    }
    groups.append(box);
  }
  paint();

  const how = document.createElement("div");
  how.className = "note plain";
  how.textContent =
    "The mouse aims on PS3 and Wii. On Wii U the mouse can't aim or click yet, so the arrow keys turn the right stick and each button answers only its key, such as J, K or L. Insert opens the portal menu, and changes take effect the next time a game starts.";

  return { parts: [speed, live, groups], how };
}

export async function renderController(): Promise<View> {
  const scope = store.get().controllerScope ?? "";
  const view = await controllerView(scope);
  if (shownPlayer >= view.players.length) shownPlayer = 0;
  const content = document.createElement("div");
  content.className = "controller";

  const note = document.createElement("div");
  note.className = "note plain";
  const said = told;
  told = null;

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
  const none = isNoPad(pad);
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
    if (isNoPad(option) || isKeyboard(option)) continue;
    const isIn = view.connected.some((c) => c.device === option.device);
    picker.add(new Option(isIn ? `${option.name} (plugged in)` : option.name, option.device));
  }
  picker.add(new Option(view.keyboard.pad.name, view.keyboard.pad.device));
  picker.add(new Option(NO_PAD.name, NO_PAD.device));
  picker.value = none ? NO_PAD.device : pad.device;
  picker.onchange = async () => {
    const chosen =
      picker.value === NO_PAD.device
        ? NO_PAD
        : picker.value === view.keyboard.pad.device
          ? view.keyboard.pad
          : view.pads.find((p) => p.device === picker.value);
    if (!chosen) return;
    // Between the keyboard and a pad, the buttons go with the device
    // (core/pad_layout.rs, `give`).
    try {
      await saveController(scope, shownPlayer + 1, chosen, player.buttons);
    } catch (err) {
      told = typeof err === "string" ? err : "Couldn't save the controller settings.";
    }
    store.redraw();
  };
  const state = document.createElement("span");
  state.className = "pad-board-state";
  state.innerHTML = `<span class="dot${player.connected ? " on" : ""}"></span>`;
  state.append(none ? "No controller" : player.connected ? "Plugged in" : "Waiting for this pad");
  boardHead.append(boardTitle, picker, state);

  const how = document.createElement("div");
  how.className = "note plain";
  how.textContent =
    "One layout works in every emulator. A pad plugged in that no player has takes the place of the first player whose pad is missing when you press Play. A player set to No controller keeps none. A virtual joystick such as vJoy only plays once you pick it. Changes take effect the next time a game starts.";

  if (isKeyboard(pad)) {
    const layout = await keyboardLayout();
    const { parts, how } = keyboardBoard(view, scope, player, layout, boardHead, content, said);
    board.append(boardHead, ...parts);
    content.append(board, how);
    restoreFocus(content);
    return { title: "Controller", subtitle: subtitleOf(view), content };
  }
  note.textContent = said ?? "";

  // A player with no controller has no buttons to show or test, and the
  // layout they had is kept for when they get a pad again.
  if (none) {
    const empty = document.createElement("div");
    empty.className = "note plain";
    empty.textContent = `Player ${shownPlayer + 1} has no controller, so games see no one there. Pick a pad above to give them one.`;
    board.append(boardHead, empty);
    content.append(board, how);
    return { title: "Controller", subtitle: subtitleOf(view), content };
  }

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
      // A stick's press is a button of its own on some consoles, the Wii's 2
      // among them, so it says what it is on each.
      const pressed = place === "LS" || place === "RS";
      const words = pressed ? consoleWords(view.consoles, place) : document.createElement("span");
      words.className = "pad-words";
      if (pressed) words.prepend(`${DIRECTION[place]} `);
      else words.textContent = DIRECTION[place];
      box.append(target(place, "pad-dir", words));
    }
    dirs.append(box);
  }
  paint();

  board.append(boardHead, stage, foot, dirs);
  content.appendChild(board);

  content.appendChild(how);
  restoreFocus(content);

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

  return { title: "Controller", subtitle: subtitleOf(view), content };
}

