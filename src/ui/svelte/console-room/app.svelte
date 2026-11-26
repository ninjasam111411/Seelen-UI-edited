<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, SeelenCommand, Widget } from "@seelen-ui/lib";
  import { imageSrc } from "./image.ts";
  import FileIcon from "libs/ui/svelte/components/Icon/FileIcon.svelte";
  import { monitors } from "libs/ui/svelte/runes/monitors.svelte.ts";
  import { t } from "./i18n/index.ts";
  import { DRAWERS, type Disc, home } from "./state.svelte.ts";
  import DiscFace from "./components/DiscFace.svelte";
  import GameCover from "./components/GameCover.svelte";
  import RoomScene from "./components/RoomScene.svelte";
  import { sounds } from "./sounds.ts";
  import { startController, type PadButton } from "./controller.svelte.ts";

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

  /** zoomed into the room's TV: while turning on, and right before zooming back out */
  let roomZoomed = $state(false);

  /** clicking the TV turns the console on: zoom into it, then show the home menu */
  /** clicking the room anywhere but the TV: the windows on its monitor away / back, like a Mac */
  let lastToggle = 0;
  function toggleWindows() {
    if (roomZoomed) return;
    const now = performance.now();
    if (now - lastToggle < 1500) return;
    lastToggle = now;
    invoke(SeelenCommand.ToggleMinimizeAll, { monitorId: monitors.primaryMonitor?.id ?? null }).catch(
      console.error,
    );
  }

  function turnOn(then?: () => void) {
    if (roomZoomed) return;
    roomZoomed = true;
    sounds.boot();
    setTimeout(() => {
      home.openMenu();
      then?.();
    }, 650);
  }


  // leaving the menu shows the room still zoomed into the TV, then zooms back out
  let wasMenuOpen = false;
  $effect(() => {
    const open = home.menuOpen;
    if (wasMenuOpen && !open) {
      // after the room has been drawn zoomed in, so the zoom out is animated
      setTimeout(() => (roomZoomed = false), 40);
    }
    wasMenuOpen = open;
  });

  // only the "Back to room" button leaves the menu; Esc closes the library, arrows pick a game
  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (menu) menu = null;
      else if (home.libraryOpen) home.closeLibrary();
      return;
    }
    if (!home.menuOpen || home.libraryOpen || home.phase !== "idle") return;
    if (e.key === "ArrowRight") home.selectBy(1);
    else if (e.key === "ArrowLeft") home.selectBy(-1);
    else if (e.key === "Enter" && home.selected) home.play(home.selected);
  }

  function backToRoom() {
    menu = null;
    sounds.back();
    home.closeMenu();
  }

  function openLibrary() {
    menu = null;
    sounds.select();
    home.openLibrary();
  }

  // a game controller works like a console's: A plays, Y opens the library, X favorites,
  // View goes back to the room, B closes what is open
  function onPad(button: PadButton) {
    if (home.phase !== "idle") return;
    if (!home.menuOpen) {
      if (button === "a" || button === "start") turnOn();
      return;
    }
    if (menu) {
      if (button === "b") menu = null;
      return;
    }
    if (home.libraryOpen) {
      if (button === "b" || button === "y") {
        sounds.back();
        home.closeLibrary();
      }
      return;
    }
    switch (button) {
      case "lb":
        home.setTab("games");
        break;
      case "rb":
        home.setTab("media");
        break;
      case "left":
        home.selectBy(-1);
        break;
      case "right":
        home.selectBy(1);
        break;
      case "a":
        if (home.selected) home.play(home.selected);
        break;
      case "y":
        openLibrary();
        break;
      case "x":
        if (home.selected) home.toggleFavorite(home.selected.id);
        break;
      case "back":
        backToRoom();
        break;
    }
  }
  onMount(() => startController(onPad));

  // ─── right-click menu of a game ────────────────────────────────────────────
  let menu = $state<{ disc: Disc; x: number; y: number; renaming: boolean; name: string } | null>(null);
  let coverInput: HTMLInputElement | undefined = $state();

  function openGameMenu(e: MouseEvent, disc: Disc) {
    e.preventDefault();
    home.select(disc.id);
    menu = {
      disc,
      x: Math.min(e.clientX, window.innerWidth - 300),
      y: Math.min(e.clientY, window.innerHeight - 320),
      renaming: false,
      name: disc.title,
    };
  }

  function onWindowPointerDown(e: PointerEvent) {
    if (menu && !(e.target as HTMLElement).closest(".ch-game-menu")) menu = null;
  }

  // a desktop background can't take the keyboard, so it is allowed only while renaming a game
  let typing = false;
  $effect(() => {
    const renaming = !!menu?.renaming;
    if (renaming === typing) return;
    typing = renaming;
    const win = Widget.self.window;
    if (renaming) {
      win.setFocusable(true).then(() => Widget.self.focus()).catch(console.error);
    } else {
      win.setFocusable(false).catch(console.error);
    }
  });

  function onCoverPicked(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file && menu) home.setCover(menu.disc.id, file);
    input.value = "";
    menu = null;
  }

  /** clock in the corner, like a console */
  let now = $state(new Date());
  onMount(() => {
    const timer = setInterval(() => (now = new Date()), 10_000);
    return () => clearInterval(timer);
  });
  const clock = $derived(now.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }));

  let failedLogo = $state<Record<string, boolean>>({});
  let failedIcon = $state<Record<string, boolean>>({});

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

<svelte:window
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointerdown={onWindowPointerDown}
  onkeydown={onKeyDown}
/>

<div
  class="ch-home"
  class:ch-dragging={!!home.dragging}
  class:ch-starting={home.phase !== "idle"}
  class:ch-menu-open={home.menuOpen}
  style:--ch-art-hue={artHue}
>
  {#if !home.menuOpen}
    <RoomScene
      tvTitle={$t("title")}
      tvHint={$t("turn_on")}
      zoomed={roomZoomed}
      discs={home.gameDiscs}
      onTurnOn={() => turnOn()}
      drawerLabel={(drawer) => $t(`drawers.${drawer}`)}
      burning={home.burning}
      onBurnPath={(path) => home.burnPath(path)}
      onDiscMenu={openGameMenu}
      onBackground={toggleWindows}
      onPlayDisc={(disc) => home.play(disc, true)}
    />
  {:else}
  <!-- background art of the selected game -->
  <div class="ch-art">
    <!-- slow colored lights in the game's color, seen through and around its art -->
    <div class="ch-aurora">
      <div class="ch-aurora-blob ch-aurora-a"></div>
      <div class="ch-aurora-blob ch-aurora-b"></div>
      <div class="ch-aurora-blob ch-aurora-c"></div>
    </div>
    {#key art?.path}
      {#if art}
        <img
          class="ch-art-image"
          class:ch-art-portrait={!art.wide}
          src={imageSrc(art.path)}
          alt=""
          draggable="false"
          onerror={() => art && (failedArt[art.path] = true)}
        />
      {/if}
    {/key}
    <div class="ch-art-shade"></div>
    <div class="ch-grain"></div>
  </div>

  <!-- light burst when the console turns on -->
  <div class="ch-boot"></div>

  <div class="ch-content">
    <header class="ch-header">
      <button
        class="ch-header-tab"
        class:ch-header-tab-active={home.tab === "games"}
        onclick={() => home.setTab("games")}
      >
        {$t("games")}
      </button>
      <button
        class="ch-header-tab"
        class:ch-header-tab-active={home.tab === "media"}
        onclick={() => home.setTab("media")}
      >
        {$t("media")}
      </button>
      <span class="ch-header-count">{home.discs.length}</span>
      {#if home.hiddenCount > 0}
        <button class="ch-header-chip" onclick={() => home.setShowHidden(!home.showHidden)}>
          {home.showHidden ? $t("hide_hidden") : $t("hidden_count", { 0: String(home.hiddenCount) })}
        </button>
      {/if}
      <div class="ch-spacer"></div>
      <span class="ch-clock">{clock}</span>
    </header>

    <div class="ch-menu">
    <!-- game row -->
    <section class="ch-games">
      {#if home.discs.length === 0}
        <div class="ch-empty">{home.tab === "media" ? $t("no_media") : $t("no_games")}</div>
      {:else}
        <div class="ch-row" bind:this={row} onwheel={onRowWheel}>
          {#each home.discs as disc (disc.id)}
            {@const isSelected = home.selected?.id === disc.id}
            <div class="ch-tile-wrap" class:ch-tile-wrap-selected={isSelected}>
              <button
                class="ch-tile"
                class:ch-tile-selected={isSelected}
                class:ch-tile-held={home.dragging?.disc?.id === disc.id}
                class:ch-tile-hidden={disc.hidden}
                data-disc-id={disc.id}
                aria-label={disc.title}
                onpointerdown={(e) => onTilePointerDown(e, disc)}
                ondblclick={() => home.play(disc)}
                oncontextmenu={(e) => openGameMenu(e, disc)}
              >
                {#key disc.cover}
                  <GameCover {disc} />
                {/key}
                {#if disc.favorite}
                  <span class="ch-tile-star" aria-hidden="true">★</span>
                {/if}
              </button>
              {#if isSelected}
                <span class="ch-tile-name">{disc.title}</span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <!-- selected game -->
    {#if home.selected}
      {@const game = home.selected}
      {#key game.id}
        <section class="ch-details">
          {#if game.logo && !failedLogo[game.logo]}
            <img
              class="ch-logo"
              src={imageSrc(game.logo)}
              alt={game.title}
              draggable="false"
              onerror={() => game.logo && (failedLogo[game.logo] = true)}
            />
          {:else}
            <h1 class="ch-title">{game.title}</h1>
          {/if}
          <div class="ch-meta">
            <span class="ch-meta-icon">
              {#if game.icon && !failedIcon[game.icon]}
                <img
                  src={imageSrc(game.icon)}
                  alt=""
                  draggable="false"
                  onerror={() => game.icon && (failedIcon[game.icon] = true)}
                />
              {:else}
                <FileIcon path={game.iconPath} umid={game.iconUmid} draggable="false" />
              {/if}
            </span>
            <span class="ch-source">{game.source}</span>
          </div>
          <div class="ch-actions">
            <button class="ch-play" onclick={() => home.play(game)}>
              <svg class="ch-play-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l13-7.5z" /></svg>
              {home.tab === "media" ? $t("open") : $t("play")}
            </button>
            {#if game.burned}
              <button class="ch-secondary" onclick={() => home.removeDisc(game.id)}>
                {$t("remove_disc")}
              </button>
            {/if}
          </div>
        </section>
      {/key}
    {/if}

    </div>

    <!-- bottom: back to the room, library, disc maker and console -->
    <section class="ch-bottom">
      <button class="ch-library-button" onclick={backToRoom}>
        <svg class="ch-library-icon" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 3v9" /><path d="M6.3 6.8a8 8 0 1 0 11.4 0" />
        </svg>
        {$t("power_off")}
      </button>
      <button class="ch-library-button" onclick={openLibrary}>
        <svg class="ch-library-icon" viewBox="0 0 24 24" aria-hidden="true">
          <rect x="4" y="4" width="6.5" height="6.5" rx="1.5" /><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5" />
          <rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5" /><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5" />
        </svg>
        {$t("library")}
      </button>

      <div class="ch-spacer"></div>

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
          <div class="ch-card-hint">{$t("insert_here")}</div>
        </div>
      </div>
    </section>
  </div>

  {/if}

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
            <span class="ch-library-item-name">{entry.title}</span>
            {#if home.openDrawer === "apps" && !home.isInMedia(entry.id)}
              <button class="ch-secondary" onclick={() => home.addToMedia(entry)}>{$t("add_to_media")}</button>
            {/if}
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

  <!-- right-click menu of a game -->
  <input
    class="ch-cover-input"
    type="file"
    accept="image/*"
    bind:this={coverInput}
    onchange={onCoverPicked}
  />
  {#if menu}
    {@const disc = menu.disc}
    <div class="ch-game-menu" style:left={menu.x + "px"} style:top={menu.y + "px"} role="menu" tabindex="-1">
      <div class="ch-game-menu-title">{disc.title}</div>
      {#if menu.renaming}
        <form
          class="ch-game-menu-rename"
          onsubmit={(e) => {
            e.preventDefault();
            if (menu) home.rename(disc.id, menu.name);
            menu = null;
          }}
        >
          <!-- svelte-ignore a11y_autofocus -->
          <input class="ch-game-menu-input" bind:value={menu.name} autofocus />
          <button class="ch-small-play" type="submit">{$t("rename_save")}</button>
        </form>
      {:else}
        <button
          class="ch-game-menu-item"
          onclick={() => {
            home.toggleFavorite(disc.id);
            menu = null;
          }}
        >
          {disc.favorite ? "☆ " + $t("unfavorite") : "★ " + $t("favorite")}
        </button>
        <button class="ch-game-menu-item" onclick={() => menu && (menu.renaming = true)}>✎ {$t("rename")}</button>
        <button class="ch-game-menu-item" onclick={() => coverInput?.click()}>🖼 {$t("choose_cover")}</button>
        {#if home.isAddedMedia(disc.id)}
          <button
            class="ch-game-menu-item"
            onclick={() => {
              home.removeFromMedia(disc.id);
              menu = null;
            }}
          >
            ✕ {$t("remove_from_media")}
          </button>
        {/if}
        {#if disc.burned}
          <button
            class="ch-game-menu-item"
            onclick={() => {
              home.moveDisc(disc.id, disc.shelf === "media" ? "games" : "media");
              menu = null;
            }}
          >
            ⇄ {disc.shelf === "media" ? $t("move_to_games") : $t("move_to_media")}
          </button>
          <button
            class="ch-game-menu-item"
            onclick={() => {
              home.removeDisc(disc.id);
              menu = null;
            }}
          >
            ✕ {$t("remove_disc")}
          </button>
        {/if}
        {#if home.hasCustom(disc.id)}
          <button
            class="ch-game-menu-item"
            onclick={() => {
              home.resetCustom(disc.id);
              menu = null;
            }}
          >
            ↺ {$t("reset_custom")}
          </button>
        {/if}
        <button
          class="ch-game-menu-item"
          onclick={() => {
            home.toggleHidden(disc.id);
            menu = null;
          }}
        >
          {disc.hidden ? "👁 " + $t("unhide") : "⊘ " + $t("hide")}
        </button>
      {/if}
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
