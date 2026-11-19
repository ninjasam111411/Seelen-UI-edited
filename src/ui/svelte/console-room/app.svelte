<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import { Widget } from "@seelen-ui/lib";
  import { monitors } from "libs/ui/svelte/runes/monitors.svelte";
  import FileIcon from "libs/ui/svelte/components/Icon/FileIcon.svelte";
  import { t } from "./i18n/index.ts";
  import { room, DRAWERS, type Disc } from "./state.svelte.ts";
  import { camera, ROOM, TV } from "./camera.svelte.ts";
  import DiscFace from "./components/DiscFace.svelte";
  import ShelfDisc from "./components/ShelfDisc.svelte";

  onMount(() => {
    Widget.getCurrent().ready();
  });

  /** The room is drawn on the primary monitor, like the other full screen widgets. */
  const screen = $derived.by(() => {
    if (!monitors.primaryMonitor) return null;
    const { rect, scaleFactor } = monitors.primaryMonitor;
    const relative = monitors.calculateDesktopRelativeRect(rect);
    return {
      x: relative.left / scaleFactor,
      y: relative.top / scaleFactor,
      width: (rect.right - rect.left) / scaleFactor,
      height: (rect.bottom - rect.top) / scaleFactor,
    };
  });

  /** ~90° horizontal field of view. */
  const perspective = $derived((screen?.width ?? window.innerWidth) / 2);

  // ─── Looking around / dragging ──────────────────────────────────────────────

  let looking: { x: number; y: number } | null = null;
  let consoleSlot: HTMLElement | undefined = $state();
  let inserting = $state<{ disc: Disc; fromX: number; fromY: number; toX: number; toY: number; go: boolean } | null>(
    null,
  );

  function onBackgroundPointerDown(e: PointerEvent) {
    if (e.button !== 0 || room.phase !== "idle") return;
    looking = { x: e.clientX, y: e.clientY };
  }

  function onPointerMove(e: PointerEvent) {
    if (room.dragging) {
      room.moveDrag(e.clientX, e.clientY);
      return;
    }
    if (looking) {
      camera.look(e.clientX - looking.x, e.clientY - looking.y);
      looking = { x: e.clientX, y: e.clientY };
    }
  }

  function onPointerUp() {
    looking = null;
    const drag = room.dragging;
    if (!drag) return;

    if (room.dropTarget === "console" && consoleSlot) {
      const slot = consoleSlot.getBoundingClientRect();
      const disc: Disc | null = drag.disc ??
        (drag.entry
          ? {
            id: drag.entry.id,
            title: drag.entry.title,
            kind: "app",
            cover: null,
            iconPath: drag.entry.iconPath,
            iconUmid: drag.entry.iconUmid,
            launch: drag.entry.launch,
            burned: false,
          }
          : null);
      if (disc) {
        inserting = {
          disc,
          fromX: drag.x,
          fromY: drag.y,
          toX: slot.left + slot.width / 2,
          toY: slot.top + slot.height / 2,
          go: false,
        };
        // next frame: start the flight into the slot
        requestAnimationFrame(() => requestAnimationFrame(() => inserting && (inserting.go = true)));
        setTimeout(() => (inserting = null), 650);
      }
    }
    room.endDrag();
  }

  function onWheel(e: WheelEvent) {
    if (room.openDrawer) return;
    e.preventDefault();
    camera.walk(e.deltaY);
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (room.openDrawer) {
      room.closeDrawer();
    } else if (room.phase === "idle") {
      room.close();
    }
  }

  // turn the camera while a disc is held near the left/right screen edge
  $effect(() => {
    if (!room.dragging || !screen) return;
    let frame = 0;
    const tick = () => {
      const drag = room.dragging;
      if (drag && screen) {
        const edge = screen.width * 0.08;
        const localX = drag.x - screen.x;
        if (localX < edge) camera.turn(-1.2 * (1 - localX / edge));
        else if (localX > screen.width - edge) camera.turn(1.2 * (1 - (screen.width - localX) / edge));
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  });

  /**
   * Places a centered element on a wall, in the wall's own coordinates: `x`/`y` along the wall from
   * its center, `offset` towards the room. Mounting it inside the wall keeps Chrome from drawing
   * the wall tiles on top of it.
   */
  function placeOnWall(x: number, y: number, offset: number) {
    return `translate3d(${x}px, ${y}px, ${offset}px) translate(-50%, -50%)`;
  }

  /** CSS transform placing a centered element in the room. */
  function place(x: number, y: number, z: number, rotation = "") {
    return `translate3d(${x}px, ${y}px, ${z}px) ${rotation} translate(-50%, -50%)`;
  }

  /**
   * Large surfaces are split in tiles: Chrome drops a whole 3D layer when part of it is behind the
   * camera, so one big floor would vanish as soon as you walk over it.
   */
  const TILE = 400;
  function tiles(width: number, height: number) {
    const columns = Math.ceil(width / TILE);
    const rows = Math.ceil(height / TILE);
    const tileWidth = width / columns;
    const tileHeight = height / rows;
    const list: { key: string; x: number; y: number; width: number; height: number }[] = [];
    for (let row = 0; row < rows; row++) {
      for (let col = 0; col < columns; col++) {
        list.push({ key: `${col}-${row}`, x: col * tileWidth, y: row * tileHeight, width: tileWidth, height: tileHeight });
      }
    }
    return list;
  }

  const roomWidth = ROOM.halfWidth * 2;
  const roomDepth = ROOM.halfDepth * 2;
  const wallHeight = ROOM.floorY - ROOM.ceilingY;
  const wallCenterY = (ROOM.floorY + ROOM.ceilingY) / 2;

  const surfaces: {
    name: string;
    width: number;
    height: number;
    transform: string;
    content?: Snippet;
  }[] = [
    {
      name: "cr-wall-back",
      content: backWall,
      width: roomWidth,
      height: wallHeight,
      transform: `translate3d(0px, ${wallCenterY}px, ${-ROOM.halfDepth}px)`,
    },
    {
      name: "cr-wall-left",
      content: leftWall,
      width: roomDepth,
      height: wallHeight,
      transform: `translate3d(${-ROOM.halfWidth}px, ${wallCenterY}px, 0px) rotateY(90deg)`,
    },
    {
      name: "cr-wall-right",
      content: rightWall,
      width: roomDepth,
      height: wallHeight,
      transform: `translate3d(${ROOM.halfWidth}px, ${wallCenterY}px, 0px) rotateY(-90deg)`,
    },
    {
      name: "cr-wall-front",
      width: roomWidth,
      height: wallHeight,
      transform: `translate3d(0px, ${wallCenterY}px, ${ROOM.halfDepth}px) rotateY(180deg)`,
    },
    {
      name: "cr-floor",
      width: roomWidth,
      height: roomDepth,
      transform: `translate3d(0px, ${ROOM.floorY}px, 0px) rotateX(90deg)`,
    },
    {
      name: "cr-ceiling",
      width: roomWidth,
      height: roomDepth,
      transform: `translate3d(0px, ${ROOM.ceilingY}px, 0px) rotateX(-90deg)`,
    },
  ];

  // TV stand box: from the back wall to 300px into the room, top at y = 210
  const STAND_TOP = 210;
  const STAND_FRONT_Z = -ROOM.halfDepth + 300;
  // console standing on the TV stand, to the right of the TV
  const CONSOLE = { x: 640, width: 150, height: 360, depth: 150, frontZ: STAND_FRONT_Z - 60 };
</script>

<svelte:window onpointermove={onPointerMove} onpointerup={onPointerUp} onkeydown={onKeyDown} />

{#if screen}
  <div
    class="cr-screen"
    class:cr-dragging={!!room.dragging}
    style:left={screen.x + "px"}
    style:top={screen.y + "px"}
    style:width={screen.width + "px"}
    style:height={screen.height + "px"}
  >
    <!-- 3D viewport -->
    <div
      class="cr-viewport"
      role="application"
      style:perspective={perspective + "px"}
      onpointerdown={onBackgroundPointerDown}
      onwheel={onWheel}
    >
      <div
        class="cr-world"
        class:cr-world-animated={camera.animated}
        style:transform={camera.worldTransform(perspective)}
      >
        <!-- Walls, floor and ceiling (tiled, see `tiles`) -->
        {#each surfaces as surface (surface.name)}
          <div class="cr-surface" style:transform={surface.transform}>
            {#each tiles(surface.width, surface.height) as tile (tile.key)}
              <div
                class="cr-plane cr-tile {surface.name}"
                style:width={tile.width + 1 + "px"}
                style:height={tile.height + 1 + "px"}
                style:transform={`translate3d(${tile.x - surface.width / 2}px, ${tile.y - surface.height / 2}px, 0)`}
                style:background-size={`${surface.width}px ${surface.height}px`}
                style:background-position={`${-tile.x}px ${-tile.y}px`}
              ></div>
            {/each}
            {@render surface.content?.()}
          </div>
        {/each}

        <!-- TV stand -->
        <div
          class="cr-plane cr-stand-front"
          style:width="1500px"
          style:height={ROOM.floorY - STAND_TOP + "px"}
          style:transform={place(0, (ROOM.floorY + STAND_TOP) / 2, STAND_FRONT_Z)}
        ></div>
        <div
          class="cr-plane cr-stand-top"
          style:width="1500px"
          style:height="300px"
          style:transform={place(0, STAND_TOP, STAND_FRONT_Z - 150, "rotateX(90deg)")}
        ></div>

        <!-- Console: front, side and top faces -->
        <div
          class="cr-plane cr-console-side"
          style:width={CONSOLE.depth + "px"}
          style:height={CONSOLE.height + "px"}
          style:transform={place(
            CONSOLE.x - CONSOLE.width / 2,
            STAND_TOP - CONSOLE.height / 2,
            CONSOLE.frontZ - CONSOLE.depth / 2,
            "rotateY(-90deg)",
          )}
        ></div>
        <div
          class="cr-plane cr-console-top"
          style:width={CONSOLE.width + "px"}
          style:height={CONSOLE.depth + "px"}
          style:transform={place(
            CONSOLE.x,
            STAND_TOP - CONSOLE.height,
            CONSOLE.frontZ - CONSOLE.depth / 2,
            "rotateX(90deg)",
          )}
        ></div>
        <div
          class="cr-plane cr-console"
          class:cr-drop-active={room.dropTarget === "console"}
          class:cr-console-reading={room.phase !== "idle"}
          data-drop="console"
          title={$t("insert_here")}
          style:width={CONSOLE.width + "px"}
          style:height={CONSOLE.height + "px"}
          style:transform={place(CONSOLE.x, STAND_TOP - CONSOLE.height / 2, CONSOLE.frontZ)}
        >
          <div class="cr-console-shell cr-console-shell-left"></div>
          <div class="cr-console-core">
            <div class="cr-console-slot" bind:this={consoleSlot}></div>
            <div class="cr-console-light"></div>
          </div>
          <div class="cr-console-shell cr-console-shell-right"></div>
        </div>
        {#if room.dragging}
          <div
            class="cr-plane cr-console-hint"
            style:transform={place(CONSOLE.x, STAND_TOP - CONSOLE.height - 50, CONSOLE.frontZ)}
          >
            {$t("insert_here")}
          </div>
        {/if}

      </div>
    </div>

    <!-- 2D overlays -->
    {#if room.phase === "idle"}
      <div class="cr-hint">{$t("hint")}</div>
      <button class="cr-close" data-skin="default" title={$t("close")} onclick={() => room.close()}>✕</button>
    {/if}

    {#if room.openDrawer}
      <div class="cr-folder" role="dialog" tabindex="-1">
        <div class="cr-folder-header">
          <button data-skin="transparent" onclick={() => room.closeDrawer()}>← {$t("back")}</button>
          <span class="cr-folder-title">{$t(`drawers.${room.openDrawer}`)}</span>
        </div>
        <div class="cr-folder-list">
          {#each room.drawerEntries as entry (entry.id)}
            <div
              class="cr-folder-item"
              role="button"
              tabindex="-1"
              onpointerdown={(e) => {
                if (e.button !== 0) return;
                if ((e.target as HTMLElement).closest("button")) return;
                room.startDrag({ entry }, e.clientX, e.clientY);
              }}
            >
              <FileIcon class="cr-folder-icon" path={entry.iconPath} umid={entry.iconUmid} />
              <span class="cr-folder-name" title={entry.title}>{entry.title}</span>
              <button data-skin="transparent" onclick={() => room.burn(entry)}>{$t("make_disc")}</button>
              <button data-skin="default" onclick={() => room.openEntry(entry)}>{$t("open")}</button>
            </div>
          {:else}
            <div class="cr-folder-empty">{$t("empty_folder")}</div>
          {/each}
        </div>
      </div>
    {/if}

    {#if room.toast}
      <div class="cr-toast">{room.toast}</div>
    {/if}

    <!-- what is being held -->
    {#if room.dragging}
      <div class="cr-ghost" style:left={room.dragging.x + "px"} style:top={room.dragging.y + "px"}>
        {#if room.dragging.disc}
          <DiscFace
            cover={room.dragging.disc.cover}
            iconPath={room.dragging.disc.iconPath}
            iconUmid={room.dragging.disc.iconUmid}
          />
        {:else if room.dragging.entry}
          <DiscFace iconPath={room.dragging.entry.iconPath} iconUmid={room.dragging.entry.iconUmid} />
        {/if}
      </div>
    {/if}

    <!-- disc flying into the console slot -->
    {#if inserting}
      <div
        class="cr-ghost cr-ghost-inserting"
        class:cr-ghost-go={inserting.go}
        style:left={(inserting.go ? inserting.toX : inserting.fromX) + "px"}
        style:top={(inserting.go ? inserting.toY : inserting.fromY) + "px"}
      >
        <DiscFace cover={inserting.disc.cover} iconPath={inserting.disc.iconPath} iconUmid={inserting.disc.iconUmid} />
      </div>
    {/if}

    <div class="cr-fade" class:cr-fade-on={room.phase === "leaving"}></div>
  </div>
{/if}

{#snippet backWall()}
  <!-- TV -->
  <div
    class="cr-plane cr-tv"
    class:cr-tv-on={room.phase !== "idle"}
    style:width={TV.width + 40 + "px"}
    style:height={TV.height + 40 + "px"}
    style:transform={placeOnWall(TV.x, TV.y - wallCenterY, TV.z + ROOM.halfDepth)}
  >
    <div class="cr-tv-screen">
      {#if room.playing}
        <div class="cr-tv-playing">
          <div class="cr-tv-disc">
            <DiscFace
              cover={room.playing.cover}
              iconPath={room.playing.iconPath}
              iconUmid={room.playing.iconUmid}
              spinning
            />
          </div>
          <span class="cr-tv-loading">{$t("loading", { 0: room.playing.title })}</span>
        </div>
      {:else}
        <span class="cr-tv-idle">{$t("title")}</span>
      {/if}
    </div>
  </div>
{/snippet}

{#snippet leftWall()}
  <!-- Game shelf (left wall) -->
  <div
    class="cr-plane cr-shelf"
    style:transform={placeOnWall(150, -80 - wallCenterY, 30)}
  >
    <div class="cr-panel-title">{$t("shelf")}</div>
    {#if room.discCount === 0}
      <div class="cr-shelf-empty">{$t("shelf_empty")}</div>
    {:else}
      <div class="cr-shelf-grid">
        {#each room.pageDiscs as disc (disc.id)}
          <div ondblclick={() => room.playDisc(disc)} role="presentation">
            <ShelfDisc {disc} />
          </div>
        {/each}
      </div>
    {/if}
    {#if room.pageCount > 1}
      <div class="cr-shelf-pager" role="presentation" onpointerdown={(e) => e.stopPropagation()}>
        <button data-skin="default" disabled={room.page === 0} onclick={() => (room.page = room.page - 1)}>
          ◀ {$t("prev")}
        </button>
        <span>{$t("page", { 0: String(room.page + 1), 1: String(room.pageCount) })}</span>
        <button
          data-skin="default"
          disabled={room.page >= room.pageCount - 1}
          onclick={() => (room.page = room.page + 1)}
        >
          {$t("next")} ▶
        </button>
      </div>
    {/if}
  </div>
{/snippet}

{#snippet rightWall()}
  <!-- Disc maker (right wall, near the back) -->
  <div
    class="cr-plane cr-maker"
    class:cr-drop-active={room.dropTarget === "maker"}
    class:cr-maker-burning={!!room.burning}
    data-drop="maker"
    style:transform={placeOnWall(-820, -20 - wallCenterY, 30)}
  >
    <div class="cr-panel-title">{$t("disc_maker")}</div>
    <div class="cr-maker-tray">
      <div class="cr-maker-blank"></div>
    </div>
    <div class="cr-maker-status">
      {room.burning ? $t("burning", { 0: room.burning }) : $t("disc_maker_hint")}
    </div>
  </div>
  <!-- File sorter (right wall, near the front) -->
  <div class="cr-plane cr-sorter" style:transform={placeOnWall(250, -60 - wallCenterY, 30)}>
    <div class="cr-panel-title">{$t("file_sorter")}</div>
    <div class="cr-sorter-drawers" role="presentation" onpointerdown={(e) => e.stopPropagation()}>
      {#each DRAWERS as drawer (drawer)}
        <button class="cr-sorter-drawer" onclick={() => room.openSorterDrawer(drawer)}>
          <span class="cr-sorter-handle"></span>
          <span class="cr-sorter-label">{$t(`drawers.${drawer}`)}</span>
        </button>
      {/each}
    </div>
  </div>
{/snippet}
