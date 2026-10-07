import type { BigPictureState, Console, Game, Listing, Playing } from "./api";

/// A catalogue title opened in the side panel.
export interface CatalogueSelection {
  listing: Listing;
}

/// How many catalogue games a page shows, and how many more each "Show more"
/// adds.
export const CATALOGUE_PAGE = 60;

/// What the catalogue is narrowed to and sorted by, apart from the search.
export interface CatalogueChoice {
  console: Console | null;
  region: string;
  runs: string;
  hideDemos: boolean;
  sort: string;
  limit: number;
}

export type ViewId =
  | "library"
  | "catalogue"
  | "homebrew"
  | "controller"
  | "emulators"
  | "updates"
  | "system"
  | "logs"
  | "settings";

interface AppState {
  view: ViewId;
  /// `undefined` until the first check comes back. `null` means it came back
  /// and there is nothing installed, which reads very differently.
  rpcs3Version: string | null | undefined;
  firmwareVersion: string | null | undefined;
  /// `undefined` until the library has been read. An empty array means it was
  /// read and there is nothing in it, which is what "No games yet" is for.
  games: Game[] | undefined;
  /// The library search box.
  search: string;
  /// The one console the library shows, or every console.
  libraryConsole: Console | null;
  /// The catalogue has its own, over every PS3 game rather than yours.
  catalogueQuery: string;
  /// What the catalogue is narrowed to and sorted by.
  catalogueFilter: CatalogueChoice;
  /// Whose controller layout is on screen: empty for every game, or a title id.
  controllerScope: string;
  /// The catalogue title open in the side panel, if any.
  catalogueSelected: CatalogueSelection | null;
  notice: string | null;
  playing: Playing | null;
  gameFullscreen: boolean;
  selected: string | null;
  /// Omoio across the whole screen, drawn for a controller.
  bigPicture: boolean;
  /// The running game is off the screen, waiting behind Big Picture.
  suspended: boolean;
}

type Listener = (state: AppState) => void;

// One small observable store for the whole app. No external state library:
// views subscribe and re-render themselves when the view changes. This is
// enough for the ~15 components in the app; revisit only if that stops
// being true.
class Store {
  private state: AppState = {
    view: "library",
    rpcs3Version: undefined,
    firmwareVersion: undefined,
    games: undefined,
    search: "",
    libraryConsole: null,
    catalogueQuery: "",
    catalogueFilter: {
      console: null,
      region: "",
      runs: "",
      hideDemos: false,
      sort: "",
      limit: CATALOGUE_PAGE,
    },
    controllerScope: "",
    catalogueSelected: null,
    notice: null,
    playing: null,
    gameFullscreen: false,
    selected: null,
    bigPicture: false,
    suspended: false,
  };
  private listeners = new Set<Listener>();

  get(): AppState {
    return this.state;
  }

  setView(view: ViewId): void {
    if (view === this.state.view) return;
    this.state = { ...this.state, view };
    this.notify();
  }

  setRpcs3Version(version: string | null): void {
    this.state = { ...this.state, rpcs3Version: version };
    this.notify();
  }

  setFirmwareVersion(version: string | null): void {
    this.state = { ...this.state, firmwareVersion: version };
    this.notify();
  }

  setGames(games: Game[]): void {
    this.state = { ...this.state, games };
    this.notify();
  }

  setSearch(search: string): void {
    if (search === this.state.search) return;
    this.state = { ...this.state, search };
    this.notify();
  }

  setLibraryConsole(libraryConsole: Console | null): void {
    if (libraryConsole === this.state.libraryConsole) return;
    this.state = { ...this.state, libraryConsole };
    this.notify();
  }

  /// Redraw without anything having changed. The catalogue needs it after
  /// fetching the list, where the state is the same but the answer is not.
  redraw(): void {
    this.notify();
  }

  setCatalogueSelected(catalogueSelected: CatalogueSelection | null): void {
    this.state = { ...this.state, catalogueSelected };
    this.notify();
  }

  setControllerScope(controllerScope: string): void {
    if (controllerScope === this.state.controllerScope) return;
    this.state = { ...this.state, controllerScope };
    this.notify();
  }

  /// A change to anything but the page length starts again from one page, so
  /// a new filter never opens sixty rows down.
  setCatalogueFilter(patch: Partial<CatalogueChoice>): void {
    const catalogueFilter = { ...this.state.catalogueFilter, limit: CATALOGUE_PAGE, ...patch };
    this.state = { ...this.state, catalogueFilter };
    this.notify();
  }

  setCatalogueQuery(catalogueQuery: string): void {
    if (catalogueQuery === this.state.catalogueQuery) return;
    this.state = {
      ...this.state,
      catalogueQuery,
      catalogueFilter: { ...this.state.catalogueFilter, limit: CATALOGUE_PAGE },
    };
    this.notify();
  }

  setNotice(notice: string | null): void {
    this.state = { ...this.state, notice };
    this.notify();
  }

  setPlaying(playing: Playing | null): void {
    this.state = { ...this.state, playing };
    this.notify();
  }

  setGameFullscreen(gameFullscreen: boolean): void {
    this.state = { ...this.state, gameFullscreen };
    this.notify();
  }

  setSelected(selected: string | null): void {
    this.state = { ...this.state, selected };
    this.notify();
  }

  setBigPicture({ on, suspended }: BigPictureState): void {
    if (on === this.state.bigPicture && suspended === this.state.suspended) return;
    this.state = { ...this.state, bigPicture: on, suspended };
    this.notify();
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    listener(this.state);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    for (const listener of this.listeners) listener(this.state);
  }
}

export const store = new Store();
