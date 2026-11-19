import { invoke, SeelenCommand, SeelenEvent, Settings, subscribe, Widget } from "@seelen-ui/lib";
import { FolderType, type InstalledGame, type StartMenuItem } from "@seelen-ui/lib/types";
import { lazyRune } from "libs/ui/svelte/utils/LazyRune.svelte.ts";
import { rootEffectAsync } from "libs/ui/svelte/utils/RootEffect.svelte.ts";
import { monitors } from "libs/ui/svelte/runes/monitors.svelte.ts";
import { get } from "svelte/store";
import { locale, t } from "./i18n/index.ts";
import { camera } from "./camera.svelte.ts";

// ─── Types ────────────────────────────────────────────────────────────────────

export type LaunchSpec =
  | { type: "uri"; uri: string }
  | { type: "app"; path: string; umid: string | null; displayName: string }
  | { type: "file"; path: string };

export interface Disc {
  id: string;
  title: string;
  kind: "game" | "app" | "file";
  /** cover art image on disk (Steam keeps one per game) */
  cover: string | null;
  /** path/umid used to show the app or file icon on the label */
  iconPath: string | null;
  iconUmid: string | null;
  launch: LaunchSpec;
  /** made with the disc maker, so it can be removed */
  burned: boolean;
}

export type DrawerId = "apps" | "desktop" | "documents" | "downloads" | "pictures" | "music" | "videos" | "recent";

export interface SorterEntry {
  id: string;
  title: string;
  iconPath: string | null;
  iconUmid: string | null;
  launch: LaunchSpec;
}

export type Phase = "idle" | "inserting" | "zooming" | "leaving";

// ─── Constants ────────────────────────────────────────────────────────────────

const BURNED_STORAGE_KEY = "console-room.burned-discs";
export const DISCS_PER_PAGE = 24;

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
  "apps",
  "desktop",
  "documents",
  "downloads",
  "pictures",
  "music",
  "videos",
  "recent",
];

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

let burned = $state<Disc[]>(loadBurned());
let page = $state(0);
let phase = $state<Phase>("idle");
/** disc currently being played (shown on the TV during the launch) */
let playing = $state<Disc | null>(null);
let openDrawer = $state<DrawerId | null>(null);
let drawerEntries = $state<SorterEntry[]>([]);
let burning = $state<string | null>(null);
let toast = $state<string | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | undefined;

/** what is being dragged: a disc from the shelf or an entry from the file sorter */
let dragging = $state<{ disc: Disc | null; entry: SorterEntry | null; x: number; y: number } | null>(null);
let dropTarget = $state<"console" | "maker" | null>(null);

const gameDiscs = $derived(
  (games.value as InstalledGame[]).map(
    (game): Disc => ({
      id: game.id,
      title: game.name,
      kind: "game",
      cover: game.cover ?? null,
      iconPath: game.executable ?? null,
      iconUmid: null,
      launch: { type: "uri", uri: game.launch },
      burned: false,
    }),
  ),
);

const allDiscs = $derived(
  [...gameDiscs, ...burned].sort((a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" })),
);
const pageCount = $derived(Math.max(1, Math.ceil(allDiscs.length / DISCS_PER_PAGE)));
const pageDiscs = $derived(allDiscs.slice(page * DISCS_PER_PAGE, (page + 1) * DISCS_PER_PAGE));

// ─── Window position ──────────────────────────────────────────────────────────

const posSet = rootEffectAsync(() => Widget.self.setPosition(monitors.desktopRect));

Widget.self.onTrigger(async () => {
  resetRoom();
  games.value = await invoke(SeelenCommand.GetInstalledGames);
  await posSet;
  await Widget.self.show();
  await Widget.self.focus();
});

// ─── Helpers ──────────────────────────────────────────────────────────────────

function loadBurned(): Disc[] {
  try {
    const raw = localStorage.getItem(BURNED_STORAGE_KEY);
    const list = raw ? (JSON.parse(raw) as Disc[]) : [];
    return Array.isArray(list) ? list : [];
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

function showToast(text: string) {
  toast = text;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast = null), 2600);
}

function resetRoom() {
  phase = "idle";
  playing = null;
  openDrawer = null;
  dragging = null;
  dropTarget = null;
  camera.reset();
}

function sameLaunch(a: LaunchSpec, b: LaunchSpec): boolean {
  if (a.type !== b.type) return false;
  if (a.type === "uri" && b.type === "uri") return a.uri === b.uri;
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

const translate = (key: string, vars?: Record<string, string>) => get(t)(key, vars);

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

// ─── Actions ──────────────────────────────────────────────────────────────────

async function playDisc(disc: Disc) {
  if (phase !== "idle") return;
  playing = disc;
  openDrawer = null;

  phase = "inserting";
  await wait(500);

  phase = "zooming";
  camera.zoomToTv();
  try {
    await runLaunch(disc.launch);
  } catch (e) {
    console.error(e);
    showToast(translate("launch_failed", { 0: disc.title }));
    await wait(900);
    resetRoom();
    return;
  }

  // let the zoom finish and the app open behind the TV
  await wait(1700);
  phase = "leaving";
  await wait(450);
  await Widget.self.hide();
  resetRoom();
}

async function burn(entry: SorterEntry) {
  if (burning) return;
  const exists = allDiscs.find((d) => sameLaunch(d.launch, entry.launch));
  if (exists) {
    showToast(translate("already_burned", { 0: entry.title }));
    return;
  }

  burning = entry.title;
  await wait(1300);
  burned.push({
    id: `burned:${crypto.randomUUID()}`,
    title: entry.title,
    kind: entry.launch.type === "file" ? "file" : "app",
    cover: null,
    iconPath: entry.iconPath,
    iconUmid: entry.iconUmid,
    launch: entry.launch,
    burned: true,
  });
  saveBurned();
  burning = null;

  // jump to the page where the new disc landed
  const index = allDiscs.findIndex((d) => sameLaunch(d.launch, entry.launch));
  if (index >= 0) page = Math.floor(index / DISCS_PER_PAGE);
  showToast(translate("burned", { 0: entry.title }));
}

function removeDisc(id: string) {
  burned = burned.filter((d) => d.id !== id);
  saveBurned();
  page = Math.min(page, Math.max(0, Math.ceil(allDiscs.length / DISCS_PER_PAGE) - 1));
}

async function openSorterDrawer(drawer: DrawerId) {
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

function openEntry(entry: SorterEntry) {
  runLaunch(entry.launch).catch(console.error);
  Widget.self.hide();
}

// ─── Drag & drop ──────────────────────────────────────────────────────────────

function startDrag(source: { disc?: Disc; entry?: SorterEntry }, x: number, y: number) {
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
    const disc: Disc | null = current.disc ??
      (current.entry
        ? {
          id: current.entry.id,
          title: current.entry.title,
          kind: current.entry.launch.type === "file" ? "file" : "app",
          cover: null,
          iconPath: current.entry.iconPath,
          iconUmid: current.entry.iconUmid,
          launch: current.entry.launch,
          burned: false,
        }
        : null);
    if (disc) playDisc(disc);
  } else if (target === "maker") {
    if (current.entry) {
      burn(current.entry);
    } else if (current.disc) {
      showToast(translate("already_burned", { 0: current.disc.title }));
    }
  }
}

// ─── Exposed state ────────────────────────────────────────────────────────────

class ConsoleRoomState {
  get pageDiscs() {
    return pageDiscs;
  }
  get discCount() {
    return allDiscs.length;
  }
  get page() {
    return page;
  }
  set page(value: number) {
    page = Math.min(Math.max(0, value), pageCount - 1);
  }
  get pageCount() {
    return pageCount;
  }
  get phase() {
    return phase;
  }
  get playing() {
    return playing;
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

  playDisc = playDisc;
  burn = burn;
  removeDisc = removeDisc;
  openSorterDrawer = openSorterDrawer;
  closeDrawer = () => (openDrawer = null);
  openEntry = openEntry;
  startDrag = startDrag;
  moveDrag = moveDrag;
  endDrag = endDrag;
  close = () => {
    resetRoom();
    Widget.self.hide();
  };
}

export const room = new ConsoleRoomState();
