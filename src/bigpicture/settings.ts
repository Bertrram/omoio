import { getVersion } from "@tauri-apps/api/app";
import {
  backUpSaves,
  controllerView,
  emulatorVersions,
  fetchCovers,
  communityPacks,
  gameSaves,
  gameSettings,
  getAccount,
  getHardwareInfo,
  getSettings,
  listGames,
  listRegions,
  portalButton,
  cancelCommunity,
  onCommunityProgress,
  refreshCommunity,
  restoreSaves,
  setCovers,
  setGameSettings,
  setCommunityPack,
  setPortalButton,
  setRegion,
  setStartInBigPicture,
  type Account,
  type ControllerView,
  type EmulatorVersion,
  type Game,
  type GameOption,
  type GameSettings,
  type HardwareInfo,
  type Packs,
  type RegionChoice,
  type SaveBackup,
  type Settings,
} from "../api";
import { GROUP_TITLES } from "../components/gameSettingsSheet";
import { store } from "../state";
import { h, heading, listPage, navButton, row, switchRow, type Choice, type Kit, type Screen, type Section } from "./kit";
import { padScreen } from "./pad";

/// Settings in Big Picture: the ones a controller can change. Anything that
/// needs typing, a folder or a file stays on the desktop, and says so rather
/// than being left out without a word.

function problem(err: unknown, fallback: string): string {
  return typeof err === "string" ? err : fallback;
}

function gigabytes(bytes: number): string {
  return `${Math.round(bytes / 1024 ** 3)} GB`;
}

// ---- the Settings screen ----

export type Category = "general" | "controllers" | "system";

const CATEGORIES: [Category, string][] = [
  ["general", "General"],
  ["controllers", "Controllers"],
  ["system", "System"],
];

/// Which emulator runs which console, for the System list.
const EMULATOR_NAMES: Record<EmulatorVersion["console"], string> = {
  ps3: "RPCS3",
  wiiu: "Cemu",
  wii: "Dolphin",
  gamecube: "Dolphin",
};

interface General {
  settings: Settings;
  account: Account;
  regions: RegionChoice[];
}

interface Controllers {
  view: ControllerView;
  portal: string;
}

interface System {
  hardware: HardwareInfo;
  emulators: EmulatorVersion[];
  version: string;
}

export function settingsScreen(kit: Kit, cap: (input: string) => string, start: Category = "general"): Screen {
  let category = start;
  let general: General | null = null;
  let controllers: Controllers | null = null;
  let system: System | null = null;
  const asked = new Set<Category>();
  let pane: HTMLElement | null = null;

  const screen: Screen = {
    section: "settings",
    first: () => `cat:${category}`,
    draw() {
      const page = h("div", "bp-screen bp-settings");
      const cats = h("nav", "bp-cats");
      cats.setAttribute("aria-label", "Settings");
      cats.append(h("h1", "bp-page-title", "Settings"));
      for (const [id, label] of CATEGORIES) {
        const button = navButton(`bp-cat${id === category ? " on" : ""}`, `cat:${id}`, () => {}, "cats");
        button.textContent = label;
        button.dataset.hint = "";
        button.dataset.category = id;
        cats.append(button);
      }
      pane = h("div", "bp-cat-pane bp-scroll");
      page.append(cats, pane);
      fill();
      return page;
    },
    // Landing on a category shows it; there is nothing to press.
    landed(el) {
      const id = el.dataset.category as Category | undefined;
      if (!id || id === category) return;
      category = id;
      el.parentElement?.querySelectorAll(".bp-cat").forEach((each) => each.classList.toggle("on", each === el));
      fill();
    },
  };

  function fill(): void {
    if (!pane) return;
    const list = h("div", "bp-list");
    const shown = category === "general" ? drawGeneral() : category === "controllers" ? drawControllers() : drawSystem();
    list.append(...shown);
    pane.replaceChildren(list);
  }

  function load(which: Category, job: () => Promise<void>): HTMLElement[] {
    if (!asked.has(which)) {
      asked.add(which);
      job()
        .catch((err) => kit.say(problem(err, "Couldn't read these settings.")))
        .finally(() => kit.redraw(screen));
    }
    return [h("div", "bp-quiet", "Reading…")];
  }

  function drawGeneral(): HTMLElement[] {
    if (!general) {
      return load("general", async () => {
        const [settings, account, regions] = await Promise.all([getSettings(), getAccount(), listRegions()]);
        general = { settings, account, regions };
      });
    }
    const { settings, account, regions } = general;
    const rows: HTMLElement[] = [heading("Big Picture")];
    rows.push(
      switchRow(
        "gen:start",
        "Open Omoio in Big Picture",
        settings.start_in_big_picture,
        async (next) => {
          await setStartInBigPicture(next);
          settings.start_in_big_picture = next;
        },
        "Starts on this screen, for a PC under a TV."
      )
    );

    rows.push(heading("Games"));
    if (settings.rawg_key) {
      rows.push(
        switchRow(
          "gen:covers",
          "Real covers from RAWG",
          settings.covers,
          async (next) => {
            await setCovers(next);
            settings.covers = next;
            if (next) await fetchCovers().catch(() => 0);
            store.setGames(await listGames());
          },
          "Box art in place of the tiles Omoio draws."
        )
      );
    } else {
      rows.push(
        row("gen:covers", "Real covers from RAWG", "Off", undefined, "Needs a free RAWG key, added in Settings on the desktop.")
      );
    }
    const region = regions.find((choice) => choice.id === account.region);
    rows.push(
      row(
        "gen:region",
        "Region",
        region ? `${region.name} · ${region.language}` : "Not set",
        async () => {
          const choices: Choice[] = regions.map((choice) => ({ value: choice.id, label: `${choice.name} · ${choice.language}` }));
          const picked = await kit.pick("Region", choices, account.region);
          if (picked === null || picked === account.region) return;
          try {
            await setRegion(picked);
            account.region = picked;
          } catch (err) {
            kit.say(problem(err, "Couldn't change the region."));
          }
          kit.redraw(screen);
        },
        "The language PS3 games start in."
      ),
      row("gen:name", "Username", account.username || "Not set", undefined, "Changed in Settings on the desktop, where there is a keyboard.")
    );
    return rows;
  }

  function drawControllers(): HTMLElement[] {
    if (!controllers) {
      return load("controllers", async () => {
        const [view, portal] = await Promise.all([controllerView(""), portalButton()]);
        controllers = { view, portal };
      });
    }
    const { view } = controllers;
    const rows: HTMLElement[] = [heading("Players")];
    view.players.forEach((player, at) => {
      const state = h("span", `bp-row-value${player.connected ? " live" : ""}`, player.connected ? player.pad.name : "Not plugged in");
      const line = row(
        `pad:${at}`,
        `Player ${at + 1}`,
        state,
        // Read again on the way back, since the pad screen may change it.
        () => {
          controllers = null;
          asked.delete("controllers");
          kit.open(padScreen(kit, cap, at));
        },
        player.connected ? "Buttons and pad" : `${player.pad.name}, buttons and pad`
      );
      line.dataset.hint = "Open";
      rows.push(line);
    });

    rows.push(heading("Buttons"));
    const chord = h("span", "bp-row-value bp-caps");
    chord.innerHTML = `${cap("Back")}<span>+</span>${cap("Start")}`;
    rows.push(row("pad:chord", "Back to Big Picture from a game", chord, undefined, "Press both together while playing."));

    const portalMenu = (store.get().games ?? []).some((game) => game.portal_menu);
    if (portalMenu && controllers) {
      const known = controllers;
      const current = h("span", "bp-row-value bp-caps");
      current.innerHTML = cap(known.portal);
      rows.push(
        row("pad:portal", "Skylanders portal menu", current, async () => {
          const pressed = await kit.record(
            "Press a button",
            "It opens the portal menu during a Skylanders game. The Home button is the best choice, since games don't use it."
          );
          if (!pressed) return;
          try {
            await setPortalButton(pressed);
            known.portal = pressed;
          } catch (err) {
            kit.say(problem(err, "Couldn't save that button."));
          }
          kit.redraw(screen);
        }, "The button that opens it while playing.")
      );
    }
    rows.push(row("pad:games", "A game's own layout", "On the desktop", undefined, "Set on the Controller screen, under Layout for."));
    return rows;
  }

  function drawSystem(): HTMLElement[] {
    if (!system) {
      return load("system", async () => {
        const [hardware, emulators, version] = await Promise.all([getHardwareInfo(), emulatorVersions(), getVersion()]);
        system = { hardware, emulators, version };
      });
    }
    const { hardware, emulators, version } = system;
    const rows: HTMLElement[] = [heading("This computer")];
    const cores = hardware.cpu.physical_cores
      ? `${hardware.cpu.physical_cores} cores, ${hardware.cpu.logical_cores} threads`
      : `${hardware.cpu.logical_cores} threads`;
    rows.push(
      row("sys:cpu", "Processor", hardware.cpu.brand, undefined, cores),
      row(
        "sys:gpu",
        "Graphics",
        hardware.gpu?.name ?? "Not detected",
        undefined,
        hardware.gpu ? `${gigabytes(hardware.gpu.dedicated_memory_bytes)} of its own memory` : undefined
      ),
      row("sys:ram", "Memory", gigabytes(hardware.memory.total_bytes)),
      row(
        "sys:display",
        "Display",
        hardware.display ? `${hardware.display.width} × ${hardware.display.height}` : "Not detected",
        undefined,
        hardware.display ? `${hardware.display.refresh_hz} Hz` : undefined
      )
    );
    rows.push(heading("Emulators"));
    // Dolphin answers for both of its consoles; it is one emulator.
    const listed = new Set<string>();
    for (const emulator of emulators) {
      const name = EMULATOR_NAMES[emulator.console];
      if (listed.has(name)) continue;
      listed.add(name);
      rows.push(row(`sys:${emulator.console}`, name, emulator.version ?? "Not installed"));
    }
    rows.push(row("sys:firmware", "PS3 firmware", store.get().firmwareVersion ?? "Not installed"));
    rows.push(heading("Omoio"), row("sys:omoio", "Version", version));
    return rows;
  }

  return screen;
}

// ---- a game's own settings ----

/// The values a number can be set to from a list. The step is the smallest
/// of a few round ones that keeps the list short enough to scroll.
function numberChoices(option: GameOption): string[] {
  const span = option.max - option.min;
  const step = [1, 2, 5, 10, 25, 50, 100, 250, 500, 1000].find((each) => span / each <= 40) ?? Math.ceil(span / 40);
  const values: string[] = [];
  for (let value = Math.ceil(option.min / step) * step; value <= option.max; value += step) values.push(String(value));
  if (!values.includes(String(option.min))) values.unshift(String(option.min));
  if (!values.includes(String(option.max))) values.push(String(option.max));
  return values;
}

/// Whether a setting can be changed with the pad: a choice, a switch or a
/// number with bounds. Text needs a keyboard.
function padFriendly(option: GameOption): boolean {
  return option.kind === "choice" || option.kind === "switch" || (option.kind === "number" && option.max > option.min);
}

function shown(option: GameOption, value: string | undefined, fallback: string): string {
  if (value === undefined) return fallback;
  if (option.kind === "switch") return value === "true" ? "On" : "Off";
  return value;
}

export function gameOptionsScreen(kit: Kit, game: Game, section: Section): Screen {
  let data: GameSettings | null = null;
  let failed = "";
  let asked = false;

  const screen: Screen = {
    section,
    first: () => {
      const option = data && common(data)[0];
      return option ? `opt:${option.key}` : "opt:none";
    },
    draw() {
      const { page, list } = listPage(game.title, "Settings");
      if (failed) {
        list.append(row("opt:none", failed, ""));
        return page;
      }
      if (!data) {
        if (!asked) {
          asked = true;
          gameSettings(game.title_id)
            .then((found) => (data = found))
            .catch((err) => (failed = problem(err, "Couldn't read this game's settings.")))
            .finally(() => kit.redraw(screen));
        }
        list.append(h("div", "bp-quiet", "Reading…"));
        return page;
      }
      const known = data;
      const options = common(known);
      if (options.length === 0) {
        list.append(row("opt:none", "Nothing to change yet", "", undefined, "Start the game once and its settings appear here."));
        return page;
      }
      const fallback = `${known.emulator} default`;
      if (store.get().playing?.title_id === game.title_id) {
        list.append(h("p", "bp-list-note", "Changes take effect the next time the game starts."));
      }
      let group = "";
      for (const option of options) {
        if (option.group !== group) {
          group = option.group;
          list.append(heading(GROUP_TITLES[group] ?? group));
        }
        const value = known.chosen[option.key];
        const why = known.reasons[option.key];
        const line = row(
          `opt:${option.key}`,
          option.label,
          shown(option, value, fallback),
          () => void change(known, option, fallback),
          why ? `Omoio sets this for this game. ${why}` : option.hint || undefined
        );
        line.classList.toggle("changed", value !== undefined);
        list.append(line);
      }
      if (Object.keys(known.chosen).length > 0) {
        list.append(
          row("opt:reset", `Put everything back to ${fallback}`, "", () =>
            kit.ask({
              title: "Put every setting back?",
              text: `${game.title} goes back to ${known.emulator}'s own settings.`,
              confirm: "Put back",
              run: async () => {
                await setGameSettings(game.title_id, {});
                known.chosen = {};
                kit.redraw(screen);
              },
            })
          )
        );
      }
      return page;
    },
  };

  /// The settings the desktop shows under Common, in the emulator's order of
  /// groups, less those that need a keyboard.
  function common(settings: GameSettings): GameOption[] {
    return settings.common_groups.flatMap((group) =>
      settings.options.filter((option) => option.common && option.group === group && padFriendly(option))
    );
  }

  async function change(settings: GameSettings, option: GameOption, fallback: string): Promise<void> {
    const values =
      option.kind === "switch"
        ? [
            ["true", "On"],
            ["false", "Off"],
          ]
        : (option.kind === "choice" ? option.choices : numberChoices(option)).map((value) => [value, value]);
    const choices: Choice[] = [{ value: "", label: fallback }, ...values.map(([value, label]) => ({ value, label }))];
    const current = settings.chosen[option.key] ?? "";
    const picked = await kit.pick(option.label, choices, current);
    if (picked === null || picked === current) return;
    // Choosing the default removes the setting rather than writing a value,
    // so Omoio never states a preference the user did not express.
    const next = { ...settings.chosen };
    if (picked === "") delete next[option.key];
    else next[option.key] = picked;
    try {
      await setGameSettings(game.title_id, next);
      settings.chosen = next;
    } catch (err) {
      kit.say(problem(err, "Couldn't save that setting."));
    }
    kit.redraw(screen);
  }

  return screen;
}

// ---- patches ----

export function packsScreen(kit: Kit, game: Game, section: Section): Screen {
  let found: Packs | null = null;
  let asked = false;
  /// What the download is doing, while it runs.
  let getting: string | null = null;

  const screen: Screen = {
    section,
    first: () => {
      const first = found?.packs.find((pack) => pack.applies);
      return first ? `pack:${first.id}` : "pack:get";
    },
    draw() {
      const { page, list } = listPage(game.title, "Community packs");
      if (!found) {
        if (!asked) {
          asked = true;
          void read();
        }
        list.append(h("div", "bp-quiet", "Reading…"));
        return page;
      }
      const { have_list, waiting, packs, source } = found;
      list.append(
        h(
          "p",
          "bp-list-note",
          `Made by ${source || "the emulator's community"}. A pack that is on without being asked says why; the rest stay off until you turn them on.`
        )
      );
      if (!have_list) {
        list.append(row("pack:none", "Not downloaded yet", "", undefined, "Download them to see what has been made for this game."));
      } else if (waiting) {
        list.append(row("pack:none", waiting, ""));
      } else if (packs.length === 0) {
        list.append(row("pack:none", "No packs for this game", "", undefined, "Nobody has made one yet."));
      }
      const kinds = [...new Set(packs.map((pack) => pack.kind))];
      for (const kind of kinds) {
        if (kinds.length > 1) list.append(heading(kind));
        for (const pack of packs.filter((each) => each.kind === kind)) {
          if (!pack.applies) {
            const line = row(`pack:${pack.id}`, pack.name, "Other version", undefined, pack.needs ?? undefined);
            line.classList.add("spare");
            list.append(line);
            continue;
          }
          const picked = Object.fromEntries(pack.choices.map((choice) => [choice.name, choice.chosen]));
          list.append(
            switchRow(
              `pack:${pack.id}`,
              pack.name,
              pack.on,
              async (next) => {
                try {
                  await setCommunityPack(game.title_id, { id: pack.id, on: next, choices: picked });
                } catch (err) {
                  kit.say(problem(err, "Couldn't change that pack."));
                  throw err;
                }
                await read();
              },
              pack.on_because ?? (pack.about || pack.by || undefined)
            )
          );
          // A pack's own choices, such as its frame rate, while it is on.
          if (!pack.on) continue;
          for (const choice of pack.choices) {
            const name = choice.name.replace(/:$/, "") || "Preset";
            const line = row(`pack:${pack.id}:${choice.name}`, name, choice.chosen, async () => {
              const options: Choice[] = choice.options.map((option) => ({ value: option, label: option }));
              const next = await kit.pick(`${pack.name}: ${name}`, options, choice.chosen);
              if (next === null || next === choice.chosen) return;
              try {
                await setCommunityPack(game.title_id, { id: pack.id, on: true, choices: { ...picked, [choice.name]: next } });
              } catch (err) {
                kit.say(problem(err, "Couldn't change that pack."));
              }
              await read();
            });
            line.classList.add("sub");
            list.append(line);
          }
        }
      }
      // Pressed again while it runs, it stops.
      const get = row(
        "pack:get",
        have_list ? "Check for new packs" : "Download packs",
        getting ?? "",
        async () => {
          if (getting) {
            await cancelCommunity();
            return;
          }
          getting = "Starting…";
          kit.redraw(screen);
          const unlisten = onCommunityProgress((progress) => {
            getting =
              progress.stage === "downloading" && progress.total > 0
                ? `${Math.min(100, Math.round((progress.bytes / progress.total) * 100))}%`
                : progress.stage === "verifying"
                  ? "Checking…"
                  : progress.stage === "extracting"
                    ? "Unpacking…"
                    : "Starting…";
            kit.redraw(screen);
          });
          try {
            await refreshCommunity(game.console);
            await read();
          } catch (err) {
            if (err !== "cancelled") kit.say(problem(err, "Couldn't download the packs."));
          } finally {
            getting = null;
            void unlisten.then((stop) => stop());
            kit.redraw(screen);
          }
        },
        getting ? "Press again to stop." : undefined
      );
      list.append(get);
      return page;
    },
  };

  async function read(): Promise<void> {
    found = await communityPacks(game.title_id);
    kit.redraw(screen);
  }

  return screen;
}

// ---- saved games ----

function formatWhen(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const mb = bytes / 1024 ** 2;
  return mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.round(bytes / 1024)} KB`;
}

export function savesScreen(kit: Kit, game: Game, section: Section): Screen {
  let saves: [boolean, SaveBackup[]] | null = null;
  let asked = false;
  let working = false;

  const screen: Screen = {
    section,
    first: () => "save:now",
    draw() {
      const { page, list } = listPage(game.title, "Saved games");
      if (!saves) {
        if (!asked) {
          asked = true;
          void read();
        }
        list.append(h("div", "bp-quiet", "Reading…"));
        return page;
      }
      const [hasSaves, backups] = saves;
      list.append(h("p", "bp-list-note", "A copy is taken by itself before every update. Reinstalling the emulator leaves these alone."));
      const now = row("save:now", "Back up now", working ? "Copying…" : "", async () => {
        if (working) return;
        working = true;
        kit.redraw(screen);
        try {
          const made = await backUpSaves(game.title_id);
          kit.say(made ? "Saves copied." : "Nothing new to copy.");
          await read();
        } catch (err) {
          kit.say(problem(err, "Couldn't copy the saves."));
        } finally {
          working = false;
          kit.redraw(screen);
        }
      }, hasSaves ? undefined : "The game hasn't saved anything yet.");
      if (!hasSaves) now.setAttribute("aria-disabled", "true");
      list.append(now);

      list.append(heading(backups.length === 1 ? "1 copy kept" : `${backups.length} copies kept`));
      for (const backup of backups) {
        list.append(
          row(`save:${backup.made}`, formatWhen(backup.made), `${formatBytes(backup.bytes)} · ${backup.folders === 1 ? "1 save" : `${backup.folders} saves`}`, () => {
            if (store.get().playing?.title_id === game.title_id) {
              kit.say("Quit the game first, then put the copy back.");
              return;
            }
            kit.ask({
              title: "Put this copy back?",
              text: `The saves ${game.title} has now are copied aside first, so nothing is lost.`,
              confirm: "Put it back",
              run: async () => {
                try {
                  await restoreSaves(game.title_id, backup.made);
                  kit.say("Saves put back.");
                } catch (err) {
                  kit.say(problem(err, "Couldn't put that copy back."));
                }
                await read();
              },
            });
          })
        );
      }
      return page;
    },
  };

  async function read(): Promise<void> {
    try {
      saves = await gameSaves(game.title_id);
    } catch (err) {
      kit.say(problem(err, "Couldn't read the saved games."));
      saves = [false, []];
    }
    kit.redraw(screen);
  }

  return screen;
}
