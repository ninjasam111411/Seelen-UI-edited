import { invoke, SeelenCommand, SeelenEvent, Settings, subscribe, Widget } from "@seelen-ui/lib";
import { FolderType, type InstalledGame, type StartMenuItem } from "@seelen-ui/lib/types";
import { lazyRune } from "libs/ui/svelte/utils/LazyRune.svelte.ts";
import { monitors } from "libs/ui/svelte/runes/monitors.svelte.ts";
import { get } from "svelte/store";
import { locale, t } from "./i18n/index.ts";

// ─── Types ────────────────────────────────────────────────────────────────────

export type LaunchSpec =
  | { type: "uri"; uri: string }
  | { type: "exe"; path: string; args?: string | null }
  | { type: "app"; path: string; umid: string | null; displayName: string }
  | { type: "file"; path: string };

export interface Disc {
  id: string;
  title: string;
  /** shown under the title: Steam, Epic, Store, App, File... */
  source: string;
  /** portrait cover art on disk */
  cover: string | null;
  /** wide background art on disk */
  hero: string | null;
  /** icon image on disk (the game's own or downloaded), better than the program's icon */
  icon?: string | null;
  /** transparent title logo on disk */
  logo?: string | null;
  /** path/umid used to show the app or file icon */
  iconPath: string | null;
  iconUmid: string | null;
  launch: LaunchSpec;
  /** made with the disc maker, so it can be removed */
  burned: boolean;
}

export type DrawerId = "apps" | "desktop" | "documents" | "downloads" | "pictures" | "music" | "videos" | "recent";

export interface LibraryEntry {
  id: string;
  title: string;
  iconPath: string | null;
  iconUmid: string | null;
  launch: LaunchSpec;
}

export type Phase = "idle" | "inserting" | "starting";

// ─── Constants ────────────────────────────────────────────────────────────────

const BURNED_STORAGE_KEY = "console-room.burned-discs";
const GAMES_REFRESH_MS = 60_000;

const DRAWER_FOLDERS: Record<Exclude<DrawerId, "apps">, FolderType> = {
  desktop: FolderType.Desktop,
  documents: FolderType.Documents,
  downloads: FolderType.Downloads,
  pictures: FolderType.Pictures,
  music: FolderType.Music,
  videos: FolderType.Videos,
  recent: FolderType.Recent,
};

export const DRAWERS: DrawerId[] = [
  "desktop",
  "apps",
  "documents",
  "downloads",
  "pictures",
  "music",
  "videos",
  "recent",
];

const SOURCE_LABELS: Record<string, string> = {
  Steam: "Steam",
  Epic: "Epic Games",
  Store: "Microsoft Store",
  Shortcut: "Game",
  Gog: "GOG",
  Ubisoft: "Ubisoft",
  Itch: "itch.io",
  Local: "Downloaded game",
};

// ─── Data ─────────────────────────────────────────────────────────────────────

const settings = lazyRune(() => Settings.getAsync());
Settings.onChange((s) => (settings.value = s));

const games = lazyRune(() => invoke(SeelenCommand.GetInstalledGames));

const startMenuItems = lazyRune(() => invoke(SeelenCommand.GetStartMenuItems));
subscribe(SeelenEvent.StartMenuItemsChanged, startMenuItems.setByPayload);

await Promise.all([settings.init(), games.init(), startMenuItems.init(), monitors.init()]);
// load the strings before the first render, so labels never show their raw keys
await locale.set(settings.value.language);

$effect.root(() => {
  $effect(() => {
    locale.set(settings.value.language);
  });
});

// newly installed games show up without restarting
setInterval(async () => {
  try {
    games.value = await invoke(SeelenCommand.GetInstalledGames);
  } catch (e) {
    console.error(e);
  }
}, GAMES_REFRESH_MS);

let burned = $state<Disc[]>(loadBurned());
let selectedId = $state<string | null>(null);
let phase = $state<Phase>("idle");
let playing = $state<Disc | null>(null);
let menuOpen = $state(false);
let libraryOpen = $state(false);
let openDrawer = $state<DrawerId>("desktop");
let drawerEntries = $state<LibraryEntry[]>([]);
let burning = $state<string | null>(null);
let toast = $state<string | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | undefined;

/** what is being dragged: a disc from the row or an entry from the library */
let dragging = $state<{ disc: Disc | null; entry: LibraryEntry | null; x: number; y: number } | null>(null);
let dropTarget = $state<"console" | "maker" | null>(null);

const gameDiscs = $derived(
  (games.value as InstalledGame[]).map(
    (game): Disc => ({
      id: game.id,
      title: game.name,
      source: SOURCE_LABELS[game.source] ?? game.source,
      cover: game.cover ?? null,
      hero: game.hero ?? null,
      icon: game.icon ?? null,
      logo: game.logo ?? null,
      iconPath: game.executable ?? (game.source === "Shortcut" ? game.launch : null),
      iconUmid: game.umid ?? null,
      // downloaded games are started from their own folder, as many need it
      launch: /\.exe$/i.test(game.launch)
        ? { type: "exe", path: game.launch, args: game.launchArgs ?? null }
        : { type: "uri", uri: game.launch },
      burned: false,
    }),
  ),
);

const allDiscs = $derived(
  [...gameDiscs, ...burned].sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" })),
);
const selected = $derived(allDiscs.find((d) => d.id === selectedId) ?? allDiscs[0] ?? null);

// ─── Window ───────────────────────────────────────────────────────────────────

// Desktop preset: the window stays under every other window and covers the desktop of the
// primary monitor, so it works as an interactive desktop background.
$effect.root(() => {
  $effect(() => {
    const monitor = monitors.primaryMonitor;
    if (!monitor) return;
    Widget.self.setPosition(monitor.rect).then(() => Widget.self.show()).catch(console.error);
  });
});

// ─── Helpers ──────────────────────────────────────────────────────────────────

function loadBurned(): Disc[] {
  try {
    const raw = localStorage.getItem(BURNED_STORAGE_KEY);
    const list = raw ? (JSON.parse(raw) as Partial<Disc>[]) : [];
    if (!Array.isArray(list)) return [];
    // discs saved by older versions lack the newer fields
    return list.map((d) => ({
      source: d.launch?.type === "file" ? "File" : "App",
      hero: null,
      cover: null,
      iconPath: null,
      iconUmid: null,
      burned: true,
      ...d,
    })) as Disc[];
  } catch {
    return [];
  }
}

function saveBurned() {
  try {
    localStorage.setItem(BURNED_STORAGE_KEY, JSON.stringify($state.snapshot(burned)));
  } catch (e) {
    console.error("Could not save discs", e);
  }
}

function fileName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return (parts[parts.length - 1] ?? path).replace(/\.lnk$/i, "");
}

const translate = (key: string, vars?: Record<string, string>) => get(t)(key, vars);

function showToast(text: string) {
  toast = text;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast = null), 2600);
}

function sameLaunch(a: LaunchSpec, b: LaunchSpec): boolean {
  if (a.type !== b.type) return false;
  if (a.type === "uri" && b.type === "uri") return a.uri === b.uri;
  if (a.type === "exe" && b.type === "exe") return a.path.toLowerCase() === b.path.toLowerCase();
  if (a.type === "app" && b.type === "app") {
    return (!!a.umid && a.umid === b.umid) || a.path.toLowerCase() === b.path.toLowerCase();
  }
  if (a.type === "file" && b.type === "file") return a.path.toLowerCase() === b.path.toLowerCase();
  return false;
}

function runLaunch(launch: LaunchSpec): Promise<unknown> {
  switch (launch.type) {
    case "uri":
      return invoke(SeelenCommand.OpenFile, { path: launch.uri });
    case "exe":
      return invoke(SeelenCommand.Run, {
        program: launch.path,
        args: launch.args ?? null,
        workingDir: launch.path.replace(/[\\/][^\\/]*$/, ""),
        elevated: false,
      });
    case "file":
      return invoke(SeelenCommand.OpenFile, { path: launch.path });
    case "app":
      // same resolution as the dock: opened through its current start menu entry, so apps
      // that move on every update (e.g. Roblox Studio) keep working
      return invoke(SeelenCommand.WegLaunchItem, {
        item: {
          id: crypto.randomUUID(),
          displayName: launch.displayName,
          umid: launch.umid,
          path: launch.path,
          pinned: false,
          preventPinning: false,
          relaunch: null,
        },
        elevated: false,
      });
  }
}

function entryToDisc(entry: LibraryEntry): Disc {
  return {
    id: entry.id,
    title: entry.title,
    source: entry.launch.type === "file" ? translate("source_file") : translate("source_app"),
    cover: null,
    hero: null,
    iconPath: entry.iconPath,
    iconUmid: entry.iconUmid,
    launch: entry.launch,
    burned: false,
  };
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

// ─── Actions ──────────────────────────────────────────────────────────────────

async function play(disc: Disc, withInsert = false) {
  if (phase !== "idle") return;
  playing = disc;
  if (allDiscs.some((d) => d.id === disc.id)) selectedId = disc.id;

  if (withInsert) {
    phase = "inserting";
    await wait(600);
  }
  phase = "starting";
  try {
    await runLaunch(disc.launch);
  } catch (e) {
    console.error(e);
    showToast(translate("launch_failed", { 0: disc.title }));
    phase = "idle";
    playing = null;
    return;
  }
  // the game opens on top of this window; keep the "starting" screen a moment, then go back to
  // the idle screen
  await wait(5000);
  phase = "idle";
  playing = null;
  menuOpen = false;
  libraryOpen = false;
}

async function burn(entry: LibraryEntry) {
  if (burning) return;
  if (allDiscs.some((d) => sameLaunch(d.launch, entry.launch))) {
    showToast(translate("already_burned", { 0: entry.title }));
    return;
  }

  burning = entry.title;
  await wait(1300);
  const disc: Disc = { ...entryToDisc(entry), id: `burned:${crypto.randomUUID()}`, burned: true };
  burned.push(disc);
  saveBurned();
  burning = null;
  selectedId = disc.id;
  showToast(translate("burned", { 0: entry.title }));
}

function removeDisc(id: string) {
  burned = burned.filter((d) => d.id !== id);
  saveBurned();
  if (selectedId === id) selectedId = null;
}

async function loadDrawer(drawer: DrawerId) {
  openDrawer = drawer;
  drawerEntries = [];

  if (drawer === "apps") {
    drawerEntries = (startMenuItems.value as StartMenuItem[])
      .map((item) => ({
        id: item.umid ?? item.path,
        title: item.display_name,
        iconPath: item.path || null,
        iconUmid: item.umid ?? null,
        launch: {
          type: "app" as const,
          path: item.path,
          umid: item.umid ?? null,
          displayName: item.display_name,
        },
      }))
      .sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" }));
    return;
  }

  const paths = await invoke(SeelenCommand.GetUserFolderContent, { folderType: DRAWER_FOLDERS[drawer] });
  if (openDrawer !== drawer) return;
  drawerEntries = paths.map((path) => ({
    id: path,
    title: fileName(path),
    iconPath: path,
    iconUmid: null,
    launch: { type: "file" as const, path },
  }));
}

function openLibrary() {
  libraryOpen = true;
  loadDrawer(openDrawer);
}

// ─── Drag & drop ──────────────────────────────────────────────────────────────

function startDrag(source: { disc?: Disc; entry?: LibraryEntry }, x: number, y: number) {
  if (phase !== "idle") return;
  dragging = { disc: source.disc ?? null, entry: source.entry ?? null, x, y };
}

function moveDrag(x: number, y: number) {
  if (!dragging) return;
  dragging.x = x;
  dragging.y = y;
  const element = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
  dropTarget = (element?.dataset.drop as "console" | "maker" | undefined) ?? null;
}

function endDrag() {
  const current = dragging;
  const target = dropTarget;
  dragging = null;
  dropTarget = null;
  if (!current) return;

  if (target === "console") {
    const disc = current.disc ?? (current.entry ? entryToDisc(current.entry) : null);
    if (disc) play(disc, true);
  } else if (target === "maker") {
    if (current.entry) {
      burn(current.entry);
    } else if (current.disc) {
      showToast(translate("already_burned", { 0: current.disc.title }));
    }
  }
}

// ─── Exposed state ────────────────────────────────────────────────────────────

class ConsoleHomeState {
  get discs() {
    return allDiscs;
  }
  get selected() {
    return selected;
  }
  get phase() {
    return phase;
  }
  get playing() {
    return playing;
  }
  get menuOpen() {
    return menuOpen;
  }
  get libraryOpen() {
    return libraryOpen;
  }
  get openDrawer() {
    return openDrawer;
  }
  get drawerEntries() {
    return drawerEntries;
  }
  get burning() {
    return burning;
  }
  get toast() {
    return toast;
  }
  get dragging() {
    return dragging;
  }
  get dropTarget() {
    return dropTarget;
  }

  select = (id: string) => (selectedId = id);
  openMenu = () => (menuOpen = true);
  closeMenu = () => {
    menuOpen = false;
    libraryOpen = false;
  };
  play = play;
  burn = burn;
  removeDisc = removeDisc;
  openLibrary = openLibrary;
  closeLibrary = () => (libraryOpen = false);
  loadDrawer = loadDrawer;
  openEntry = (entry: LibraryEntry) => {
    runLaunch(entry.launch).catch(console.error);
    menuOpen = false;
    libraryOpen = false;
  };
  startDrag = startDrag;
  moveDrag = moveDrag;
  endDrag = endDrag;
}

export const home = new ConsoleHomeState();
