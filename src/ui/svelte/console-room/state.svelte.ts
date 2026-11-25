import { invoke, SeelenCommand, SeelenEvent, Settings, subscribe, Widget } from "@seelen-ui/lib";
import { FolderType, type InstalledGame, type StartMenuItem } from "@seelen-ui/lib/types";
import { lazyRune } from "libs/ui/svelte/utils/LazyRune.svelte.ts";
import { monitors } from "libs/ui/svelte/runes/monitors.svelte.ts";
import { get } from "svelte/store";
import { locale, t } from "./i18n/index.ts";
import { sounds } from "./sounds.ts";

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
  /** pinned to the front of the row */
  favorite?: boolean;
  /** hidden from the row (shown only while "show hidden" is on) */
  hidden?: boolean;
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
const CUSTOM_STORAGE_KEY = "console-room.customizations";
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

/** apps the Media tab picks up by themselves (lower case letters and digits of their name) */
const MEDIA_APP_NAMES = new Set([
  "discord",
  "googlechrome",
  "chrome",
  "microsoftedge",
  "firefox",
  "mozillafirefox",
  "brave",
  "opera",
  "operagx",
  "operagxbrowser",
  "operabrowser",
  "spotify",
  "youtube",
  "youtubemusic",
  "netflix",
  "disney",
  "primevideo",
  "amazonprimevideo",
  "hulu",
  "max",
  "twitch",
  "crunchyroll",
  "plex",
  "vlc",
  "vlcmediaplayer",
  "mediaplayer",
  "windowsmediaplayer",
  "applemusic",
  "appletv",
  "itunes",
  "amazonmusic",
  "tiktok",
  "whatsapp",
  "telegram",
  "telegramdesktop",
  "messenger",
  "instagram",
  "facebook",
  "snapchat",
  "reddit",
  "obsstudio",
  "medal",
]);

export type HomeTab = "games" | "media";

function normalizeName(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]/g, "");
}

// ─── Data ─────────────────────────────────────────────────────────────────────

const settings = lazyRune(() => Settings.getAsync());
Settings.onChange((s) => (settings.value = s));

const games = lazyRune(() => invoke(SeelenCommand.GetInstalledGames));

/** apps and shortcuts in Media folders (e.g. C:\Media), shown on the Media tab by themselves */
const mediaFolderItems = lazyRune(() => invoke(SeelenCommand.GetMediaFolderItems));

const startMenuItems = lazyRune(() => invoke(SeelenCommand.GetStartMenuItems));
subscribe(SeelenEvent.StartMenuItemsChanged, startMenuItems.setByPayload);

await Promise.all([
  settings.init(),
  games.init(),
  mediaFolderItems.init(),
  startMenuItems.init(),
  monitors.init(),
]);
// load the strings before the first render, so labels never show their raw keys
await locale.set(settings.value.language);

$effect.root(() => {
  $effect(() => {
    locale.set(settings.value.language);
  });
});

// newly installed games (and apps put in a Media folder) show up without restarting
setInterval(async () => {
  try {
    games.value = await invoke(SeelenCommand.GetInstalledGames);
    mediaFolderItems.value = await invoke(SeelenCommand.GetMediaFolderItems);
  } catch (e) {
    console.error(e);
  }
}, GAMES_REFRESH_MS);

/** what the user changed about the games: kept in this widget's storage */
interface Customizations {
  favorites: string[];
  hidden: string[];
  names: Record<string, string>;
  /** image data urls */
  covers: Record<string, string>;
  /** game id -> time it was last started (ms) */
  lastPlayed: Record<string, number>;
  /** apps added to the Media tab by hand, as library entries */
  media: LibraryEntry[];
}

let burned = $state<Disc[]>(loadBurned());
let custom = $state<Customizations>(loadCustom());
let showHidden = $state(false);
let tab = $state<HomeTab>("games");
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

/** apps like Discord and Chrome from the start menu, and the ones added by hand */
const mediaDiscs = $derived.by((): Disc[] => {
  const seen = new Set<string>();
  const found: Disc[] = [];
  const add = (entry: LibraryEntry) => {
    const key = normalizeName(entry.title);
    if (seen.has(key)) return;
    seen.add(key);
    found.push({ ...entryToDisc(entry), id: `media:${entry.id}`, source: translate("app") });
  };
  // things put in a Media folder come first: they were put there on purpose
  for (const path of mediaFolderItems.value as string[]) add(mediaFolderEntry(path));
  for (const item of startMenuItems.value as StartMenuItem[]) {
    if (MEDIA_APP_NAMES.has(normalizeName(item.display_name))) add(startMenuEntry(item));
  }
  custom.media.forEach(add);
  return found;
});

/** every disc with the user's changes applied, hidden ones included */
const everyDisc = $derived(
  [...(tab === "media" ? mediaDiscs : [...gameDiscs, ...burned])].map((d): Disc => ({
    ...d,
    title: custom.names[d.id] ?? d.title,
    cover: custom.covers[d.id] ?? d.cover,
    favorite: custom.favorites.includes(d.id),
    hidden: custom.hidden.includes(d.id),
  })),
);

/** like a console: favorites first, then the most recently played, then the rest by name */
const allDiscs = $derived(
  everyDisc
    .filter((d) => showHidden || !d.hidden)
    .sort((a, b) => {
      if (a.favorite !== b.favorite) return a.favorite ? -1 : 1;
      const played = (custom.lastPlayed[b.id] ?? 0) - (custom.lastPlayed[a.id] ?? 0);
      if (played !== 0) return played;
      return a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
    }),
);
const hiddenCount = $derived(everyDisc.filter((d) => d.hidden).length);
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

function loadCustom(): Customizations {
  const empty: Customizations = { favorites: [], hidden: [], names: {}, covers: {}, lastPlayed: {}, media: [] };
  try {
    const raw = localStorage.getItem(CUSTOM_STORAGE_KEY);
    return raw ? { ...empty, ...(JSON.parse(raw) as Partial<Customizations>) } : empty;
  } catch {
    return empty;
  }
}

function saveCustom() {
  try {
    localStorage.setItem(CUSTOM_STORAGE_KEY, JSON.stringify($state.snapshot(custom)));
  } catch (e) {
    console.error("Could not save game changes", e);
    showToast(translate("save_failed"));
  }
}

function toggleIn(list: string[], id: string): string[] {
  return list.includes(id) ? list.filter((x) => x !== id) : [...list, id];
}

/** shrinks a picture the user picked to a small square jpeg, so it fits in the widget storage */
async function fileToCover(file: File): Promise<string> {
  const bitmap = await createImageBitmap(file);
  const size = 480;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no canvas");
  // fills the square like the tiles do (centered crop)
  const scale = Math.max(size / bitmap.width, size / bitmap.height);
  const w = bitmap.width * scale;
  const h = bitmap.height * scale;
  ctx.drawImage(bitmap, (size - w) / 2, (size - h) / 2, w, h);
  bitmap.close();
  return canvas.toDataURL("image/jpeg", 0.85);
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

/** A Media folder item: a shortcut or program, or the program found in a folder in it. */
function mediaFolderEntry(path: string): LibraryEntry {
  const parts = path.split(/[\\/]/).filter(Boolean);
  const file = parts[parts.length - 1] ?? path;
  const isExe = /\.exe$/i.test(file);
  // a program found inside a folder is named after its folder
  const inFolder = isExe && parts.length >= 3 && !/^media$/i.test(parts[parts.length - 2] ?? "");
  const title = inFolder ? parts[parts.length - 2]! : file.replace(/\.(lnk|url|exe|appref-ms)$/i, "");
  return {
    id: path,
    title,
    iconPath: path,
    iconUmid: null,
    launch: isExe ? { type: "exe" as const, path } : { type: "file" as const, path },
  };
}

function startMenuEntry(item: StartMenuItem): LibraryEntry {
  return {
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
  };
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
    sounds.insert();
    await wait(600);
  }
  phase = "starting";
  sounds.start();
  custom.lastPlayed[disc.id] = Date.now();
  saveCustom();
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
      .map(startMenuEntry)
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
  // from the Media tab it opens on the apps, where they can be added to it
  loadDrawer(tab === "media" ? "apps" : openDrawer);
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

  get hiddenCount() {
    return hiddenCount;
  }
  get tab() {
    return tab;
  }

  setTab = (value: HomeTab) => {
    if (tab === value) return;
    tab = value;
    selectedId = null;
    sounds.select();
  };
  /** puts an app from the library on the Media tab */
  addToMedia = (entry: LibraryEntry) => {
    if (!custom.media.some((m) => m.id === entry.id)) {
      custom.media = [...custom.media, $state.snapshot(entry) as LibraryEntry];
      saveCustom();
    }
    showToast(translate("added_to_media", { 0: entry.title }));
    sounds.select();
  };
  isInMedia = (entryId: string) => custom.media.some((m) => m.id === entryId);
  /** whether a Media tab disc was added by hand (and so can be removed) */
  isAddedMedia = (discId: string) => custom.media.some((m) => `media:${m.id}` === discId);
  removeFromMedia = (discId: string) => {
    custom.media = custom.media.filter((m) => `media:${m.id}` !== discId);
    saveCustom();
    sounds.select();
  };
  get showHidden() {
    return showHidden;
  }

  select = (id: string) => {
    if (selectedId !== id) sounds.move();
    selectedId = id;
  };
  /** moves the selection by `step` games along the row */
  selectBy = (step: number) => {
    if (allDiscs.length === 0) return;
    const index = Math.max(
      0,
      allDiscs.findIndex((d) => d.id === selected?.id),
    );
    const next = allDiscs[Math.min(allDiscs.length - 1, Math.max(0, index + step))];
    if (next) this.select(next.id);
  };
  toggleFavorite = (id: string) => {
    custom.favorites = toggleIn(custom.favorites, id);
    saveCustom();
    sounds.select();
  };
  toggleHidden = (id: string) => {
    custom.hidden = toggleIn(custom.hidden, id);
    saveCustom();
    sounds.select();
  };
  setShowHidden = (value: boolean) => (showHidden = value);
  rename = (id: string, name: string) => {
    const trimmed = name.trim();
    if (trimmed) custom.names[id] = trimmed;
    else delete custom.names[id];
    saveCustom();
  };
  setCover = async (id: string, file: File) => {
    try {
      custom.covers[id] = await fileToCover(file);
      saveCustom();
    } catch (e) {
      console.error(e);
      showToast(translate("cover_failed"));
    }
  };
  resetCustom = (id: string) => {
    delete custom.names[id];
    delete custom.covers[id];
    saveCustom();
  };
  hasCustom = (id: string) => id in custom.names || id in custom.covers;
  openMenu = () => {
    menuOpen = true;
  };
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
