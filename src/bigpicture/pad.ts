import { controllerView, padInput, saveController, type ControllerView } from "../api";
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
import { h, listPage, navButton, row, type Choice, type Kit, type Screen } from "./kit";

function problem(err: unknown, fallback: string): string {
  return typeof err === "string" ? err : fallback;
}

/// A player's pad across the whole screen: the drawing and labels of the
/// desktop's Controller screen, moved through with the pad. It changes the
/// layout for every game; a game's own layout stays the desktop's.
export function padScreen(kit: Kit, cap: (input: string) => string, start: number): Screen {
  let shown = start;
  let view: ControllerView | null = null;
  let asked = false;

  /// What a place does, for a question about it: "Cross on PS3, B on Wii U".
  const meaning = (known: ControllerView, place: string) =>
    known.consoles
      .filter((console) => console.buttons[place])
      .map((console) => `${console.buttons[place]} on ${console.name}`)
      .join(", ");

  /// Asks for the button that does `place`: pressed on the player's own pad
  /// when it is in, chosen from a list when it isn't.
  async function change(place: string): Promise<void> {
    if (!view) return;
    const known = view;
    const player = known.players[shown];
    const family = player.pad.family;
    const buttons = { ...player.buttons };
    const input = player.connected
      ? await kit.record("Press a button", `On ${player.pad.name}, for ${meaning(known, place)}.`, player.pad.device)
      : await kit.pick(
          meaning(known, place),
          known.inputs.map((each): Choice => ({ value: each, label: nameOf(family, each) })),
          buttons[place]
        );
    if (!input || input === buttons[place]) return;
    assignInput(buttons, place, input);
    try {
      await saveController("", shown + 1, player.pad, buttons);
      player.buttons = buttons;
    } catch (err) {
      kit.say(problem(err, "Couldn't save the controller settings."));
    }
  }

  const screen: Screen = {
    section: "settings",
    first: () => "place:South",
    tab(step) {
      if (!view) return;
      shown = (shown + step + view.players.length) % view.players.length;
      screen.left = undefined;
      kit.redraw(screen);
    },
    tabName: () => "Player",
    draw() {
      const page = h("div", "bp-screen bp-scroll bp-padpage");
      const head = h("div", "bp-padpage-head");
      const titles = h("div");
      titles.append(h("div", "bp-listpage-over", "Controllers"), h("h1", "bp-page-title", `Player ${shown + 1}`));
      head.append(titles);
      page.append(head);
      if (!view) {
        if (!asked) {
          asked = true;
          controllerView("")
            .then((found) => (view = found))
            .catch((err) => kit.say(problem(err, "Couldn't read the controller settings.")))
            .finally(() => kit.redraw(screen));
        }
        page.append(h("div", "bp-quiet", "Reading…"));
        return page;
      }
      const known = view;
      const player = known.players[shown];
      const family = player.pad.family;
      const buttons = player.buttons;

      // Changed with the bumpers, and clicked with a mouse; the highlight
      // stays on the pad.
      const tabs = h("div", "bp-tabs");
      tabs.innerHTML = cap("LB");
      known.players.forEach((each, at) => {
        const tab = h("button", `bp-tab${at === shown ? " on" : ""}`);
        tab.tabIndex = -1;
        tab.append(h("span", `bp-pad-dot${each.connected ? " on" : ""}`), `Player ${at + 1}`);
        tab.onclick = () => {
          shown = at;
          screen.left = undefined;
          kit.redraw(screen);
        };
        tabs.append(tab);
      });
      tabs.insertAdjacentHTML("beforeend", cap("RB"));
      head.append(tabs);

      const bar = h("div", "bp-padpage-bar");
      const which = navButton("bp-btn", "pad:which", async () => {
        const choices = known.pads.map((pad): Choice => {
          const isIn = known.connected.some((c) => c.device === pad.device);
          return { value: pad.device, label: isIn ? `${pad.name} (plugged in)` : pad.name };
        });
        const picked = await kit.pick(`Pad for player ${shown + 1}`, choices, player.pad.device);
        const pad = known.pads.find((each) => each.device === picked);
        if (!pad || pad.device === player.pad.device) return;
        try {
          await saveController("", shown + 1, pad, buttons);
          view = await controllerView("");
        } catch (err) {
          kit.say(problem(err, "Couldn't save the controller settings."));
        }
        kit.redraw(screen);
      });
      which.dataset.hint = "Change";
      which.append(h("span", `bp-pad-dot${player.connected ? " on" : ""}`), `${player.pad.name} · ${player.connected ? "Plugged in" : "Not plugged in"}`);
      bar.append(which);
      if (Object.entries(buttons).some(([place, input]) => place !== input)) {
        const reset = navButton("bp-btn", "pad:reset", () =>
          kit.ask({
            title: "Reset this player's buttons?",
            text: `Every button on ${player.pad.name} goes back to doing what is printed on it.`,
            confirm: "Reset",
            run: async () => {
              const plain = Object.fromEntries(Object.keys(buttons).map((place) => [place, place]));
              try {
                await saveController("", shown + 1, player.pad, plain);
                player.buttons = plain;
              } catch (err) {
                kit.say(problem(err, "Couldn't save the controller settings."));
              }
              kit.redraw(screen);
            },
          })
        );
        reset.textContent = "Reset buttons";
        bar.append(reset);
      }
      page.append(bar);

      const rows: HTMLElement[] = [];
      const place = (key: string): HTMLElement => {
        const line = navButton("pad-place", `place:${key}`, async () => {
          await change(key);
          kit.redraw(screen);
        });
        line.dataset.hint = "Change";
        line.dataset.part = partOf(buttons[key]);
        line.dataset.place = key;
        line.innerHTML = `<span class="cap">${capFace(family, buttons[key])}</span>`;
        line.append(consoleWords(known.consoles, key));
        rows.push(line);
        return line;
      };
      const callouts: Callout[] = SHOULDERS.map(([key, side]) => ({ key, at: partOf(buttons[key]), side, rows: [place(key)] }));
      callouts.push(
        { key: "face", at: "Face", side: "right", rows: FACE.map(place) },
        { key: "middle", at: "Guide", side: "below", rows: MIDDLE.map(place) }
      );
      for (const group of GROUPS) {
        const line = navButton("pad-group", `group:${group.key}`, () =>
          kit.open(directionsScreen(kit, known, shown, group, change))
        );
        line.dataset.hint = "Open";
        if (group.key !== "Dpad") line.dataset.part = group.key;
        line.innerHTML = `<span class="cap">${capFace(family, group.key === "Dpad" ? "Dpad" : buttons[group.key])}</span>`;
        const words = h("span", "pad-words");
        words.append(h("span", "pad-word", group.title), h("span", "pad-console", "Directions"));
        line.append(words);
        callouts.push({ key: group.key, at: group.key, side: group.side, rows: [line] });
      }
      const { stage, art } = padStage(family, callouts);
      page.append(stage);
      page.append(
        h(
          "p",
          "bp-list-note bp-padpage-note",
          player.connected
            ? `Press anything on ${player.pad.name} and it lights up. Pick a label to give it another button.`
            : `${player.pad.name} isn't plugged in. Labels choose from a list until it is.`
        )
      );

      // What is held on this player's pad lights on the drawing and on its
      // label, a few times a second, until the screen is drawn again.
      let busy = false;
      const timer = window.setInterval(async () => {
        if (!stage.isConnected) {
          window.clearInterval(timer);
          return;
        }
        if (busy || !player.connected) return;
        busy = true;
        try {
          const held = new Set((await padInput(player.pad.device).catch(() => null)) ?? []);
          for (const el of art.querySelectorAll("[data-part]")) {
            el.classList.toggle("down", [...held].some((input) => partOf(input) === el.getAttribute("data-part")));
          }
          for (const line of rows) line.classList.toggle("flash", held.has(buttons[line.dataset.place!]));
        } finally {
          busy = false;
        }
      }, 80);
      return page;
    },
  };
  return screen;
}

/// The d-pad's or a stick's directions as a list, since they are rarely
/// changed and sit too close together on the drawing to point at.
function directionsScreen(
  kit: Kit,
  known: ControllerView,
  index: number,
  group: { title: string; places: string[] },
  change: (place: string) => Promise<void>
): Screen {
  const screen: Screen = {
    section: "settings",
    first: () => `dir:${group.places[0]}`,
    draw() {
      const { page, list } = listPage(`Player ${index + 1}`, group.title);
      const player = known.players[index];
      for (const place of group.places) {
        const value = h("span", "bp-row-value bp-caps");
        value.innerHTML = `<span class="cap">${capFace(player.pad.family, player.buttons[place])}</span>`;
        // A stick's press is a button of its own on some consoles, the
        // Wii's 2 among them, so it says what it is on each.
        const pressed = place === "LS" || place === "RS";
        const on = known.consoles
          .filter((console) => console.buttons[place])
          .map((console) => `${console.buttons[place]} on ${console.name}`)
          .join(", ");
        const label = pressed && on ? `${DIRECTION[place]}, ${on}` : DIRECTION[place];
        const line = row(`dir:${place}`, label, value, async () => {
          await change(place);
          kit.redraw(screen);
        });
        line.dataset.hint = "Change";
        list.append(line);
      }
      return page;
    },
  };
  return screen;
}
