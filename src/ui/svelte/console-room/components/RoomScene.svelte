<script lang="ts">
  // The room shown while the console is "off": drawn with CSS 3D on a 1920x1080 stage that is
  // scaled to cover the monitor. Drag to look around and scroll to walk. The shelf on the left
  // wall holds the game discs: drag one into the console to play it. Clicking the TV turns the
  // console on: the view zooms into the TV and the home menu opens. A quick, still click anywhere
  // else minimizes the windows (and brings them back on the next one), like a Mac's wallpaper.

  import { DRAWERS, type Disc, type DrawerId } from "../state.svelte.ts";
  import DiscFace from "./DiscFace.svelte";

  interface Props {
    tvTitle: string;
    tvHint: string;
    /** zoomed into the TV (while turning on, or just before the room shows again) */
    zoomed: boolean;
    /** games for the shelf */
    discs: Disc[];
    onTurnOn: () => void;
    onBackground: () => void;
    /** a disc was dropped in the console */
    onPlayDisc: (disc: Disc) => void;
    /** the file sorter (one of its drawers) or the disc maker was clicked */
    onOpenLibrary: (drawer: DrawerId) => void;
    drawerLabel: (drawer: DrawerId) => string;
  }

  let { tvTitle, tvHint, zoomed, discs, onTurnOn, onBackground, onPlayDisc, onOpenLibrary, drawerLabel }: Props =
    $props();

  /** like the TV: walk back to the start, then turn on and open the library */
  function openLibrary(e: MouseEvent, drawer: DrawerId) {
    e.stopPropagation();
    if (dragged) return;
    returnToStart(() => onOpenLibrary(drawer));
  }

  let pressAt: { x: number; y: number; t: number } | null = null;

  // ─── camera: drag to look, scroll to walk ──────────────────────────────────
  const MAX_PITCH = 30;
  const WALK_LIMIT_X = 1050;
  const WALK_LIMIT_Z_BACK = -950;
  const WALK_LIMIT_Z_FRONT = 1150;
  /** movement before a press counts as a drag (and not as a click) */
  const DRAG_THRESHOLD = 6;
  const SHELF_SIZE = 16;

  let yaw = $state(0);
  let pitch = $state(0);
  let cameraX = $state(0);
  // starts where the room was designed to be seen from
  let cameraZ = $state(900);
  /** going back to the start position before zooming into the TV */
  let returning = $state(false);
  let looking = $state<{ x: number; y: number; moved: boolean } | null>(null);

  const shelfDiscs = $derived(discs.slice(0, SHELF_SIZE));

  function clamp(value: number, min: number, max: number) {
    return Math.min(Math.max(value, min), max);
  }

  function onLookMove(e: PointerEvent) {
    if (disc) {
      moveDisc(e);
      return;
    }
    if (!looking || zoomed) return;
    const dx = e.clientX - looking.x;
    const dy = e.clientY - looking.y;
    if (!looking.moved && Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
    looking = { x: e.clientX, y: e.clientY, moved: true };
    // grabbing the room: dragging right turns the view left
    yaw -= dx * 0.12;
    pitch = clamp(pitch + dy * 0.1, -MAX_PITCH, MAX_PITCH);
  }

  function onWheel(e: WheelEvent) {
    if (zoomed || returning) return;
    e.preventDefault();
    const step = -e.deltaY * 1.2;
    const radians = (yaw * Math.PI) / 180;
    cameraX = clamp(cameraX + Math.sin(radians) * step, -WALK_LIMIT_X, WALK_LIMIT_X);
    cameraZ = clamp(cameraZ - Math.cos(radians) * step, WALK_LIMIT_Z_BACK, WALK_LIMIT_Z_FRONT);
  }

  /** Walks back to the start position (facing the TV and console), then calls `then`. */
  function returnToStart(then?: () => void) {
    const moved = yaw !== 0 || pitch !== 0 || cameraX !== 0 || cameraZ !== CAMERA_Z;
    if (!moved) {
      then?.();
      return;
    }
    returning = true;
    yaw = 0;
    pitch = 0;
    cameraX = 0;
    cameraZ = CAMERA_Z;
    setTimeout(() => {
      returning = false;
      then?.();
    }, 520);
  }

  /** the zoom into the TV is made for the start position, so walk back there first */
  function turnOn() {
    returnToStart(onTurnOn);
  }

  // ─── dragging a disc from the shelf into the console ───────────────────────
  let disc = $state<{ disc: Disc; x: number; y: number; overConsole: boolean } | null>(null);
  let consoleEl: HTMLDivElement | undefined = $state();

  function grabDisc(e: PointerEvent, item: Disc) {
    if (e.button !== 0 || zoomed) return;
    e.stopPropagation();
    e.preventDefault();
    pressAt = null;
    looking = null;
    disc = { disc: item, x: e.clientX, y: e.clientY, overConsole: false };
    // turn to face the console, so the disc can be carried to it
    returnToStart();
  }

  function moveDisc(e: PointerEvent) {
    if (!disc) return;
    const rect = consoleEl?.getBoundingClientRect();
    // a console behind the camera gets a huge, meaningless box: only a visible one counts
    const visible = !!rect &&
      rect.width > 0 &&
      rect.width < window.innerWidth / 2 &&
      rect.right > 0 &&
      rect.left < window.innerWidth &&
      rect.bottom > 0 &&
      rect.top < window.innerHeight;
    // generous target: the console is small from across the room
    const pad = 70;
    const over = visible && !!rect &&
      e.clientX > rect.left - pad &&
      e.clientX < rect.right + pad &&
      e.clientY > rect.top - pad &&
      e.clientY < rect.bottom + pad;
    disc = { ...disc, x: e.clientX, y: e.clientY, overConsole: over };
  }

  /** the last press was a drag to look around, so its click is ignored */
  let dragged = false;

  function onPointerUp() {
    dragged = !!looking?.moved;
    looking = null;
    if (!disc) return;
    const dropped = disc;
    disc = null;
    if (dropped.overConsole) onPlayDisc(dropped.disc);
  }

  /** TV center in the stage, from its 3D position (camera at z 900, perspective 960) */
  const TV_SCREEN_Y = -59;
  /** zoom that makes the TV fill the screen */
  const TV_ZOOM = 4.6;

  const STAGE_WIDTH = 1920;
  const STAGE_HEIGHT = 1080;
  const PERSPECTIVE = STAGE_WIDTH / 2;

  // room in CSS px: +x right, +y down, +z towards the viewer; camera at z = 900 looking at -z
  const HALF_WIDTH = 1400;
  const BACK_Z = -1400;
  const FRONT_Z = 1400;
  const FLOOR_Y = 460;
  const CEILING_Y = -700;
  const CAMERA_Z = 900;
  const WALL_HEIGHT = FLOOR_Y - CEILING_Y;
  const WALL_CENTER_Y = (FLOOR_Y + CEILING_Y) / 2;
  const ROOM_DEPTH = FRONT_Z - BACK_Z;
  const STAND_TOP = 210;
  const STAND_FRONT_Z = BACK_Z + 300;

  const DISC_COLORS = [
    "#e5484d",
    "#3e63dd",
    "#30a46c",
    "#f76b15",
    "#0090ff",
    "#8e4ec6",
    "#ffc53d",
    "#12a594",
    "#d6409f",
    "#6e56cf",
    "#e54666",
    "#46a758",
    "#ab4aba",
    "#ffb224",
    "#00a2c7",
    "#978365",
  ];

  let container: HTMLDivElement | undefined = $state();
  let scale = $state(1);

  $effect(() => {
    if (!container) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry!.contentRect;
      scale = Math.max(width / STAGE_WIDTH, height / STAGE_HEIGHT);
    });
    observer.observe(container);
    return () => observer.disconnect();
  });

  /** Chrome drops big 3D layers that reach behind the camera, so surfaces are split in tiles. */
  const TILE = 400;
  function tiles(width: number, height: number) {
    const columns = Math.ceil(width / TILE);
    const rows = Math.ceil(height / TILE);
    const list: { key: string; x: number; y: number; w: number; h: number }[] = [];
    for (let row = 0; row < rows; row++) {
      for (let col = 0; col < columns; col++) {
        list.push({
          key: `${col}-${row}`,
          x: (col * width) / columns,
          y: (row * height) / rows,
          w: width / columns,
          h: height / rows,
        });
      }
    }
    return list;
  }

  const surfaces = [
    { name: "back", width: HALF_WIDTH * 2, height: WALL_HEIGHT, transform: `translate3d(0px, ${WALL_CENTER_Y}px, ${BACK_Z}px)` },
    { name: "left", width: ROOM_DEPTH, height: WALL_HEIGHT, transform: `translate3d(${-HALF_WIDTH}px, ${WALL_CENTER_Y}px, 0px) rotateY(90deg)` },
    { name: "right", width: ROOM_DEPTH, height: WALL_HEIGHT, transform: `translate3d(${HALF_WIDTH}px, ${WALL_CENTER_Y}px, 0px) rotateY(-90deg)` },
    { name: "floor", width: HALF_WIDTH * 2, height: ROOM_DEPTH, transform: `translate3d(0px, ${FLOOR_Y}px, 0px) rotateX(90deg)` },
    { name: "ceiling", width: HALF_WIDTH * 2, height: ROOM_DEPTH, transform: `translate3d(0px, ${CEILING_Y}px, 0px) rotateX(-90deg)` },
  ];

  /** On a wall, in the wall's own coordinates; `offset` is towards the room. */
  function onWall(x: number, y: number, offset: number) {
    return `translate3d(${x}px, ${y}px, ${offset}px) translate(-50%, -50%)`;
  }

  function place(x: number, y: number, z: number, rotation = "") {
    return `translate3d(${x}px, ${y}px, ${z}px) ${rotation} translate(-50%, -50%)`;
  }
</script>

<svelte:window onpointermove={onLookMove} onpointerup={onPointerUp} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="room-wallpaper"
  class:room-looking={!!looking?.moved}
  bind:this={container}
  onwheel={onWheel}
  onpointerdown={(e) => {
    pressAt = e.button === 0 && e.isTrusted ? { x: e.clientX, y: e.clientY, t: e.timeStamp } : null;
    if (e.button === 0) looking = { x: e.clientX, y: e.clientY, moved: false };
  }}
  onclick={(e) => {
    // only a real, quick, still click: not a drag, a double click or a synthetic click
    const press = pressAt;
    pressAt = null;
    if (!press || !e.isTrusted || e.detail !== 1) return;
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) > 6 || e.timeStamp - press.t > 600) return;
    onBackground();
  }}
>
  <div
    class="zoom"
    class:zoomed
    style:transform-origin={`50% calc(50% + ${TV_SCREEN_Y * scale}px)`}
    style:--tv-zoom={TV_ZOOM}
  >
  <div
    class="stage"
    style:width={STAGE_WIDTH + "px"}
    style:height={STAGE_HEIGHT + "px"}
    style:transform={`translate(-50%, -50%) scale(${scale})`}
    style:perspective={PERSPECTIVE + "px"}
  >
    <div
      class="world"
      class:world-returning={returning}
      style:transform={`translateZ(${PERSPECTIVE}px) rotateX(${pitch}deg) rotateY(${yaw}deg) translate3d(${-cameraX}px, 0px, ${-cameraZ}px)`}
    >
      {#each surfaces as surface (surface.name)}
        <div class="surface" style:transform={surface.transform}>
          {#each tiles(surface.width, surface.height) as tile (tile.key)}
            <div
              class="plane tile {surface.name}"
              style:width={tile.w + 1 + "px"}
              style:height={tile.h + 1 + "px"}
              style:transform={`translate3d(${tile.x - surface.width / 2}px, ${tile.y - surface.height / 2}px, 0px)`}
              style:background-size={`${surface.width}px ${surface.height}px`}
              style:background-position={`${-tile.x}px ${-tile.y}px`}
            ></div>
          {/each}
          {#if surface.name === "back"}
            <button
              class="plane tv"
              style:transform={onWall(0, -140 - WALL_CENTER_Y, 20)}
              onclick={(e) => {
                e.stopPropagation();
                turnOn();
              }}
            >
              <div class="tv-screen">
                <span class="tv-title">{tvTitle}</span>
                <span class="tv-hint">{tvHint}</span>
              </div>
            </button>
          {:else if surface.name === "left"}
            <div class="plane shelf" style:transform={onWall(150, -80 - WALL_CENTER_Y, 30)}>
              <div class="panel-title">Game Shelf</div>
              <div class="shelf-grid">
                <!-- the real games first (drag one into the console), empty slots after -->
                {#each shelfDiscs as item (item.id)}
                  <div
                    class="disc disc-real"
                    class:disc-taken={disc?.disc.id === item.id}
                    role="button"
                    tabindex="-1"
                    aria-label={item.title}
                    onpointerdown={(e) => grabDisc(e, item)}
                  >
                    <DiscFace cover={item.cover} iconPath={item.iconPath} iconUmid={item.iconUmid} />
                  </div>
                {/each}
                {#each DISC_COLORS.slice(shelfDiscs.length) as color, i (i)}
                  <div class="disc">
                    <div class="disc-label" style:background={color}></div>
                    <div class="disc-hole"></div>
                  </div>
                {/each}
              </div>
            </div>
          {:else if surface.name === "right"}
            <button
              class="plane maker"
              style:transform={onWall(-820, -20 - WALL_CENTER_Y, 30)}
              onclick={(e) => openLibrary(e, "apps")}
            >
              <div class="panel-title dark">Disc Maker</div>
              <div class="maker-tray"><div class="maker-blank"></div></div>
              <div class="panel-hint dark">Click to burn a disc</div>
            </button>
            <div class="plane sorter" style:transform={onWall(250, -60 - WALL_CENTER_Y, 30)}>
              <div class="panel-title">File Sorter</div>
              <div class="sorter-drawers">
                {#each DRAWERS as drawer (drawer)}
                  <button class="drawer" onclick={(e) => openLibrary(e, drawer)}>
                    <span class="drawer-label">{drawerLabel(drawer)}</span>
                    <span class="handle"></span>
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {/each}

      <!-- TV stand and console -->
      <div
        class="plane stand-front"
        style:height={FLOOR_Y - STAND_TOP + "px"}
        style:transform={place(0, (FLOOR_Y + STAND_TOP) / 2, STAND_FRONT_Z)}
      ></div>
      <div class="plane stand-top" style:transform={place(0, STAND_TOP, STAND_FRONT_Z - 150, "rotateX(90deg)")}></div>
      <div
        class="plane console"
        class:console-target={!!disc}
        class:console-over={disc?.overConsole}
        bind:this={consoleEl}
        style:transform={place(640, STAND_TOP - 180, STAND_FRONT_Z - 60)}
      >
        <div class="console-shell left"></div>
        <div class="console-core">
          <div class="console-slot"></div>
          <div class="console-light"></div>
        </div>
        <div class="console-shell right"></div>
      </div>

    </div>
  </div>
</div>

  <!-- the disc being carried to the console -->
  {#if disc}
    <div class="carried-disc" class:carried-over={disc.overConsole} style:left={disc.x + "px"} style:top={disc.y + "px"}>
      <DiscFace cover={disc.disc.cover} iconPath={disc.disc.iconPath} iconUmid={disc.disc.iconUmid} spinning />
      <span class="carried-title">{disc.disc.title}</span>
    </div>
  {/if}
</div>

<style>
  .room-wallpaper {
    position: absolute;
    inset: 0;
    overflow: hidden;
    /* matches the ceiling, so tiles Chrome skips right above the camera don't show as holes */
    background: linear-gradient(180deg, #232733 0%, #13151b 45%, #2a2219 100%);
  }

  .room-looking {
    cursor: grabbing;
  }

  .world-returning {
    transition: transform 0.5s cubic-bezier(0.65, 0, 0.35, 1);
  }

  .disc-real {
    background: none;
    cursor: grab;
    transition: transform 0.15s ease;
  }

  .disc-real:hover {
    transform: translateZ(20px) scale(1.06);
  }

  .disc-taken {
    opacity: 0.25;
  }

  .disc-real :global(.cr-disc-face) {
    width: 100%;
    height: 100%;
  }

  /* the console glows while a disc is carried, brighter when it would go in */
  .console-target {
    filter: drop-shadow(0 0 30px rgb(120 170 255 / 0.55));
  }

  .console-over {
    filter: drop-shadow(0 0 60px rgb(150 200 255 / 0.95));
  }

  .carried-disc {
    position: fixed;
    z-index: 20;
    width: 130px;
    height: 130px;
    transform: translate(-50%, -50%);
    pointer-events: none;
    transition: transform 0.15s ease;
  }

  .carried-disc :global(.cr-disc-face) {
    width: 100%;
    height: 100%;
  }

  .carried-over {
    transform: translate(-50%, -50%) scale(0.75);
  }

  .carried-title {
    position: absolute;
    left: 50%;
    top: 100%;
    margin-top: 8px;
    transform: translateX(-50%);
    white-space: nowrap;
    font-size: 15px;
    font-weight: 600;
    color: #fff;
    text-shadow: 0 2px 8px rgb(0 0 0 / 0.8);
  }

  .zoom {
    position: absolute;
    inset: 0;
    transition: transform 0.7s cubic-bezier(0.65, 0, 0.35, 1);
  }

  .zoom.zoomed {
    transform: scale(var(--tv-zoom));
  }

  .stage {
    position: absolute;
    left: 50%;
    top: 50%;
    transform-origin: center;
  }

  .world {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 0;
    height: 0;
    transform-style: preserve-3d;
  }

  .plane {
    position: absolute;
    left: 0;
    top: 0;
  }

  .surface {
    position: absolute;
    left: 0;
    top: 0;
    transform-style: preserve-3d;
  }

  .tile {
    background-repeat: no-repeat;
  }

  .left,
  .right {
    background-image: radial-gradient(ellipse at 50% 35%, #3a4152 0%, #232733 60%, #161922 100%);
  }

  .back {
    background-image: radial-gradient(ellipse at 50% 40%, #46506a 0%, #262b39 55%, #171a23 100%);
  }

  .floor {
    background-image:
      repeating-linear-gradient(90deg, rgb(255 255 255 / 0.04) 0 2px, transparent 2px 200px),
      repeating-linear-gradient(0deg, rgb(255 255 255 / 0.04) 0 2px, transparent 2px 200px),
      radial-gradient(ellipse at 50% 30%, #5a4a3b 0%, #3a3027 55%, #241e19 100%);
  }

  .ceiling {
    background-image: radial-gradient(ellipse at 50% 50%, #2a2e3a 0%, #13151b 100%);
  }

  .tv {
    width: 1000px;
    height: 580px;
    padding: 20px;
    border: none;
    border-radius: 18px;
    background: #07080b;
    box-shadow: 0 30px 80px rgb(0 0 0 / 0.6);
    cursor: pointer;
    font: inherit;
    color: inherit;
    transition: box-shadow 0.2s ease;
  }

  .tv:hover {
    box-shadow:
      0 30px 80px rgb(0 0 0 / 0.6),
      0 0 60px rgb(120 160 255 / 0.35);
  }

  .tv-screen {
    width: 100%;
    height: 100%;
    border-radius: 6px;
    background: radial-gradient(ellipse at 50% 50%, #16203a 0%, #070a12 100%);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 24px;
  }

  .tv-title {
    font-size: 46px;
    font-weight: 300;
    letter-spacing: 0.3em;
    text-transform: uppercase;
    color: rgb(255 255 255 / 0.35);
  }

  .tv-hint {
    animation: tv-breathe 2.4s ease-in-out infinite alternate;
    font-size: 26px;
    padding: 8px 22px;
    border-radius: 999px;
    color: rgb(255 255 255 / 0.7);
    background: rgb(255 255 255 / 0.08);
  }

  .stand-front {
    width: 1500px;
    background: linear-gradient(180deg, #2b241d 0%, #1d1813 100%);
    border-radius: 6px 6px 0 0;
  }

  .stand-top {
    width: 1500px;
    height: 300px;
    background: linear-gradient(0deg, #3a3027 0%, #2e261f 100%);
  }

  .console {
    width: 150px;
    height: 360px;
    display: flex;
  }

  .console-shell {
    flex: 1;
    background: linear-gradient(180deg, #ffffff 0%, #e6e8ee 100%);
  }

  .console-shell.left {
    border-radius: 46px 0 0 10px;
  }

  .console-shell.right {
    border-radius: 0 46px 10px 0;
  }

  .console-core {
    width: 38%;
    background: linear-gradient(180deg, #16181e 0%, #0b0c10 100%);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding-top: 70px;
    gap: 18px;
  }

  .console-slot {
    width: 70%;
    height: 6px;
    border-radius: 3px;
    background: #000;
  }

  .console-light {
    width: 6px;
    height: 120px;
    border-radius: 3px;
    background: var(--system-accent-color, #0a84ff);
    opacity: 0.6;
  }

  .panel-title {
    font-size: 34px;
    font-weight: 600;
    color: #e9ecf3;
    text-align: center;
    margin-bottom: 20px;
  }

  .panel-title.dark {
    color: #1a1d24;
  }

  .shelf {
    width: 1560px;
    padding: 30px 40px;
    border-radius: 20px;
    background: linear-gradient(180deg, #3b3027 0%, #2a221c 100%);
  }

  .shelf-grid {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 24px 10px;
  }

  .disc {
    position: relative;
    width: 150px;
    height: 150px;
    margin: 0 auto;
    border-radius: 50%;
    background: conic-gradient(from 20deg, #cfd4dc, #a6d8ff, #ecd9ff, #cfd4dc, #ffe2bd, #bfffe8, #cfd4dc);
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.45);
  }

  .disc-label {
    position: absolute;
    inset: 22%;
    border-radius: 50%;
  }

  .disc-hole {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 11%;
    height: 11%;
    transform: translate(-50%, -50%);
    border-radius: 50%;
    background: #05060a;
  }

  .maker,
  .drawer {
    border: none;
    font: inherit;
    cursor: pointer;
    transition:
      transform 0.15s ease,
      filter 0.15s ease;
  }

  .maker:hover {
    filter: brightness(1.08) drop-shadow(0 0 40px rgb(150 200 255 / 0.6));
  }

  .drawer:hover {
    transform: translateZ(24px);
    filter: brightness(1.2);
  }

  .panel-hint {
    font-size: 26px;
    color: rgb(255 255 255 / 0.7);
  }

  .panel-hint.dark {
    color: rgb(20 24 32 / 0.6);
  }

  .drawer-label {
    font-size: 30px;
    font-weight: 600;
    color: #eef1f6;
  }

  .maker {
    width: 560px;
    padding: 30px;
    border-radius: 24px;
    background: linear-gradient(180deg, #e9ebf0 0%, #c9cdd6 100%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 20px;
  }

  .maker-tray {
    width: 280px;
    height: 280px;
    border-radius: 50%;
    background: #1a1d24;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .maker-blank {
    width: 84%;
    height: 84%;
    border-radius: 50%;
    background: conic-gradient(from 0deg, #8a8f99, #d9dde4, #8a8f99, #d9dde4, #8a8f99);
    opacity: 0.35;
  }

  .sorter {
    width: 760px;
    padding: 30px;
    border-radius: 20px;
    background: linear-gradient(180deg, #4a5160 0%, #343a46 100%);
  }

  .sorter-drawers {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }

  .drawer {
    height: 110px;
    border-radius: 10px;
    background: linear-gradient(180deg, #5d6577 0%, #4b5262 100%);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
  }

  .handle {
    width: 90px;
    height: 12px;
    border-radius: 6px;
    background: #c9ced8;
  }

  @keyframes tv-breathe {
    from {
      opacity: 0.55;
    }
    to {
      opacity: 1;
    }
  }
</style>
