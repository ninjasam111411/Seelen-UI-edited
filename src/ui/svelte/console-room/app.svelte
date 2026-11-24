<script lang="ts">
  import { onMount } from "svelte";
  import { Widget } from "@seelen-ui/lib";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import FileIcon from "libs/ui/svelte/components/Icon/FileIcon.svelte";
  import { t } from "./i18n/index.ts";
  import { DRAWERS, type Disc, home } from "./state.svelte.ts";
  import DiscFace from "./components/DiscFace.svelte";
  import GameCover from "./components/GameCover.svelte";

  onMount(() => {
    Widget.getCurrent().ready();
  });

  /** pixels the pointer must move before a press on a tile becomes a drag */
  const DRAG_THRESHOLD = 6;

  let pressed: { disc: Disc; x: number; y: number } | null = null;
  let row: HTMLDivElement | undefined = $state();
  let consoleSlot: HTMLElement | undefined = $state();
  let inserting = $state<{ disc: Disc; fromX: number; fromY: number; toX: number; toY: number; go: boolean } | null>(
    null,
  );
  let failedArt = $state<Record<string, boolean>>({});
  /** a drag just ended: the click that follows must not open or close the menu */
  let justDragged = false;

  const artDisc = $derived(home.phase !== "idle" && home.playing ? home.playing : home.selected);
  const art = $derived.by(() => {
    if (!artDisc) return null;
    const path = artDisc.hero ?? artDisc.cover;
    return path && !failedArt[path] ? { path, wide: !!artDisc.hero } : null;
  });
  /** background color for games without art, matching their generated cover */
  const artHue = $derived(
    artDisc ? [...artDisc.title].reduce((acc, c) => (acc * 31 + c.charCodeAt(0)) % 360, 7) : 225,
  );

  /** clicking the screen (anything that isn't a control) opens or closes the menu */
  function onScreenClick(e: MouseEvent) {
    if (justDragged || home.phase !== "idle") return;
    const target = e.target as HTMLElement;
    if (target.closest("button, [data-drop], .ch-row, .ch-library, [role='button']")) return;
    if (home.menuOpen) home.closeMenu();
    else home.openMenu();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (home.libraryOpen) home.closeLibrary();
    else if (home.menuOpen) home.closeMenu();
  }

  function onTilePointerDown(e: PointerEvent, disc: Disc) {
    if (e.button !== 0) return;
    pressed = { disc, x: e.clientX, y: e.clientY };
  }

  function onPointerMove(e: PointerEvent) {
    if (pressed && !home.dragging) {
      if (Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) > DRAG_THRESHOLD) {
        home.startDrag({ disc: pressed.disc }, e.clientX, e.clientY);
      }
    }
    home.moveDrag(e.clientX, e.clientY);
  }

  function onPointerUp() {
    const press = pressed;
    pressed = null;
    const drag = home.dragging;
    if (drag) {
      justDragged = true;
      setTimeout(() => (justDragged = false), 50);
    }

    if (!drag) {
      if (press) home.select(press.disc.id);
      return;
    }

    if (home.dropTarget === "console" && consoleSlot) {
      const slot = consoleSlot.getBoundingClientRect();
      const disc = drag.disc;
      if (disc || drag.entry) {
        inserting = {
          disc: disc ?? {
            id: drag.entry!.id,
            title: drag.entry!.title,
            source: "",
            cover: null,
            hero: null,
            iconPath: drag.entry!.iconPath,
            iconUmid: drag.entry!.iconUmid,
            launch: drag.entry!.launch,
            burned: false,
          },
          fromX: drag.x,
          fromY: drag.y,
          toX: slot.left + slot.width / 2,
          toY: slot.top + slot.height / 2,
          go: false,
        };
        requestAnimationFrame(() => requestAnimationFrame(() => inserting && (inserting.go = true)));
        setTimeout(() => (inserting = null), 700);
      }
    }
    home.endDrag();
  }

  /** the mouse wheel scrolls the game row sideways */
  function onRowWheel(e: WheelEvent) {
    if (!row) return;
    e.preventDefault();
    row.scrollLeft += e.deltaY + e.deltaX;
  }

  // keep the selected tile in view
  $effect(() => {
    const id = home.selected?.id;
    if (!row || !id) return;
    row.querySelector<HTMLElement>(`[data-disc-id="${CSS.escape(id)}"]`)?.scrollIntoView({
      behavior: "smooth",
      inline: "nearest",
      block: "nearest",
    });
  });
</script>

<svelte:window onpointermove={onPointerMove} onpointerup={onPointerUp} onkeydown={onKeyDown} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="ch-home"
  class:ch-dragging={!!home.dragging}
  class:ch-starting={home.phase !== "idle"}
  class:ch-menu-open={home.menuOpen}
  style:--ch-art-hue={artHue}
  onclick={onScreenClick}
>
  <!-- background art of the selected game -->
  <div class="ch-art">
    {#key art?.path}
      {#if art}
        <img
          class="ch-art-image"
          class:ch-art-portrait={!art.wide}
          src={convertFileSrc(art.path)}
          alt=""
          draggable="false"
          onerror={() => art && (failedArt[art.path] = true)}
        />
      {/if}
    {/key}
    <div class="ch-art-shade"></div>
  </div>

  <div class="ch-content">
    {#if !home.menuOpen}
      <div class="ch-idle">
        {#if home.selected}
          <div class="ch-idle-title">{home.selected.title}</div>
        {/if}
        <div class="ch-idle-hint">{$t("click_to_open")}</div>
      </div>
    {:else}
    <div class="ch-menu">
    <!-- game row -->
    <section class="ch-games">
      <div class="ch-section-title">{$t("games")}</div>
      {#if home.discs.length === 0}
        <div class="ch-empty">{$t("no_games")}</div>
      {:else}
        <div class="ch-row" bind:this={row} onwheel={onRowWheel}>
          {#each home.discs as disc (disc.id)}
            <button
              class="ch-tile"
              class:ch-tile-selected={home.selected?.id === disc.id}
              class:ch-tile-held={home.dragging?.disc?.id === disc.id}
              data-disc-id={disc.id}
              title={disc.title}
              onpointerdown={(e) => onTilePointerDown(e, disc)}
              ondblclick={() => home.play(disc)}
            >
              <GameCover {disc} />
            </button>
          {/each}
        </div>
      {/if}
    </section>

    <!-- selected game -->
    {#if home.selected}
      <section class="ch-details">
        <h1 class="ch-title">{home.selected.title}</h1>
        <div class="ch-source">{home.selected.source}</div>
        <div class="ch-actions">
          <button class="ch-play" onclick={() => home.selected && home.play(home.selected)}>
            ▶ {$t("play")}
          </button>
          {#if home.selected.burned}
            <button
              class="ch-secondary"
              title={$t("remove_disc")}
              onclick={() => home.selected && home.removeDisc(home.selected.id)}
            >
              {$t("remove_disc")}
            </button>
          {/if}
        </div>
      </section>
    {/if}

    </div>
    {/if}

    <!-- bottom: library button, disc maker and console (the console is always there) -->
    <section class="ch-bottom">
      {#if home.menuOpen}
        <button class="ch-library-button" onclick={() => home.openLibrary()}>
          <span class="ch-library-icon">▤</span>
          {$t("library")}
        </button>
      {/if}

      <div class="ch-spacer"></div>

      {#if home.menuOpen}
      <div
        class="ch-maker"
        class:ch-drop-active={home.dropTarget === "maker"}
        class:ch-maker-burning={!!home.burning}
        data-drop="maker"
      >
        <div class="ch-maker-tray"><div class="ch-maker-blank"></div></div>
        <div class="ch-card-text">
          <div class="ch-card-title">{$t("disc_maker")}</div>
          <div class="ch-card-hint">
            {home.burning ? $t("burning", { 0: home.burning }) : $t("disc_maker_hint")}
          </div>
        </div>
      </div>
      {/if}

      <div
        class="ch-console"
        class:ch-drop-active={home.dropTarget === "console"}
        class:ch-console-reading={home.phase !== "idle"}
        data-drop="console"
      >
        <div class="ch-console-body">
          <div class="ch-console-shell ch-console-shell-left"></div>
          <div class="ch-console-core">
            <div class="ch-console-slot" bind:this={consoleSlot}></div>
            <div class="ch-console-light"></div>
          </div>
          <div class="ch-console-shell ch-console-shell-right"></div>
        </div>
        <div class="ch-card-text">
          <div class="ch-card-title">{$t("console")}</div>
          <div class="ch-card-hint">{home.menuOpen ? $t("insert_here") : $t("console_idle_hint")}</div>
        </div>
      </div>
    </section>
  </div>

  <!-- library: the desktop icons are covered, so files are reachable from here -->
  {#if home.libraryOpen}
    <div class="ch-library" role="dialog" tabindex="-1">
      <div class="ch-library-header">
        <span class="ch-library-title">{$t("library")}</span>
        <button class="ch-secondary" onclick={() => home.closeLibrary()}>✕ {$t("close")}</button>
      </div>
      <div class="ch-library-tabs">
        {#each DRAWERS as drawer (drawer)}
          <button
            class="ch-library-tab"
            class:ch-library-tab-active={home.openDrawer === drawer}
            onclick={() => home.loadDrawer(drawer)}
          >
            {$t(`drawers.${drawer}`)}
          </button>
        {/each}
      </div>
      <div class="ch-library-hint">{$t("library_hint")}</div>
      <div class="ch-library-list">
        {#each home.drawerEntries as entry (entry.id)}
          <div
            class="ch-library-item"
            role="button"
            tabindex="-1"
            onpointerdown={(e) => {
              if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
              home.startDrag({ entry }, e.clientX, e.clientY);
            }}
            ondblclick={() => home.openEntry(entry)}
          >
            <FileIcon class="ch-library-item-icon" path={entry.iconPath} umid={entry.iconUmid} />
            <span class="ch-library-item-name" title={entry.title}>{entry.title}</span>
            <button class="ch-secondary" onclick={() => home.burn(entry)}>{$t("make_disc")}</button>
            <button class="ch-small-play" onclick={() => home.openEntry(entry)}>{$t("open")}</button>
          </div>
        {:else}
          <div class="ch-empty">{$t("empty_folder")}</div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- starting screen -->
  {#if home.phase === "starting" && home.playing}
    <div class="ch-starting-screen">
      <div class="ch-starting-disc">
        <DiscFace
          cover={home.playing.cover}
          iconPath={home.playing.iconPath}
          iconUmid={home.playing.iconUmid}
          spinning
        />
      </div>
      <div class="ch-starting-text">{$t("starting", { 0: home.playing.title })}</div>
    </div>
  {/if}

  {#if home.toast}
    <div class="ch-toast">{home.toast}</div>
  {/if}

  <!-- what is being held -->
  {#if home.dragging}
    <div class="ch-ghost" style:left={home.dragging.x + "px"} style:top={home.dragging.y + "px"}>
      {#if home.dragging.disc}
        <DiscFace
          cover={home.dragging.disc.cover}
          iconPath={home.dragging.disc.iconPath}
          iconUmid={home.dragging.disc.iconUmid}
        />
      {:else if home.dragging.entry}
        <DiscFace iconPath={home.dragging.entry.iconPath} iconUmid={home.dragging.entry.iconUmid} />
      {/if}
    </div>
  {/if}

  <!-- disc flying into the console slot -->
  {#if inserting}
    <div
      class="ch-ghost ch-ghost-inserting"
      class:ch-ghost-go={inserting.go}
      style:left={(inserting.go ? inserting.toX : inserting.fromX) + "px"}
      style:top={(inserting.go ? inserting.toY : inserting.fromY) + "px"}
    >
      <DiscFace cover={inserting.disc.cover} iconPath={inserting.disc.iconPath} iconUmid={inserting.disc.iconUmid} />
    </div>
  {/if}
</div>
