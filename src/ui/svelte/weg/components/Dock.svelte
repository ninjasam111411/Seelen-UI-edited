<script lang="ts">
  import { invoke, SeelenCommand } from "@seelen-ui/lib";
  import { SeelenWegMode, WegPinnedItemsVisibility } from "@seelen-ui/lib/types";
  import { DragDropProvider, DragOverlay } from "@dnd-kit/svelte";
  import { move } from "@dnd-kit/helpers";
  import { onDestroy } from "svelte";
  import { BackgroundByLayers } from "libs/ui/svelte/components/BackgroundByLayers";
  import { t } from "../i18n/index.ts";
  import { dockState, listToGroups } from "../state/items.svelte.ts";
  import { settingsState } from "../state/settings.svelte.ts";
  import { systemState } from "../state/system.svelte.ts";
  import { getWindowsForItem, windowsState } from "../state/windows.svelte.ts";
  import { dockShouldBeHidden, dockTab, setDockIsDraggingItem } from "../state/hidden.svelte.ts";
  import { getSeelenWegMenu } from "../dockMenu.ts";
  import { createDragDropManager } from "libs/ui/dnd.ts";
  import type { SwItem } from "../types.ts";
  import DockItemsGroup from "./DockItemsGroup.svelte";
  import WegItemSwitch from "./WegItemSwitch.svelte";

  // Workaround for https://github.com/clauderic/dnd-kit/issues/2112
  const manager = createDragDropManager();
  onDestroy(() => manager.destroy());

  const settings = $derived(settingsState.value as any);
  const isHorizontal = $derived(settings?.position === "Top" || settings?.position === "Bottom");

  function isItemVisible(item: SwItem): boolean {
    const pinnedVisibility = settings?.pinnedItemsVisibility as WegPinnedItemsVisibility;
    const monitor = systemState.currentMonitor;
    const showPinned = pinnedVisibility === WegPinnedItemsVisibility.Always || monitor.isPrimary;

    if (item.type !== "AppOrFile") {
      return showPinned;
    }
    if (item.pinned && showPinned) {
      return true;
    }
    return getWindowsForItem(item as any, windowsState.dockWindows).length > 0;
  }

  // splits the flat items array (left..., left-separator, center..., right-separator, ...right)
  // into their three groups, same as the toolbar does
  const groupedItems = $derived(listToGroups(dockState.items, true));
  const visibleGroupedItems = $derived.by(() => ({
    left: groupedItems.left.filter(isItemVisible),
    center: groupedItems.center.filter(isItemVisible),
    right: groupedItems.right.filter(isItemVisible),
  }));

  const isEmpty = $derived(
    [
      ...visibleGroupedItems.left,
      ...visibleGroupedItems.center,
      ...visibleGroupedItems.right,
    ].filter((c) => c.type !== "Separator").length === 0,
  );

  const itemIndexById = $derived.by(() => {
    const map = new Map<string, number>();
    dockState.items.forEach((item, i) => map.set(item.id, i));
    return map;
  });

  function onContextMenu(e: MouseEvent) {
    const alignX = settingsState.popupAlignX;
    const alignY = settingsState.popupAlignY;
    const cursor = { x: e.clientX, y: e.clientY };
    invoke(SeelenCommand.TriggerContextMenu, {
      menu: { ...getSeelenWegMenu($t, cursor), alignX, alignY },
      forwardTo: null,
    });
  }

  function handleDragOver(event: any) {
    const temp = dockState.items.map((item) => item.id);
    const newIds = move(temp, event);
    dockState.items = newIds.map((id) => dockState.items.find((item) => item.id === id)!);
  }

  // the dock opened from its tab folds back once the mouse has left it for a moment
  const CLOSE_AFTER_LEAVE_MS = 900;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  function onDockPointerEnter() {
    clearTimeout(closeTimer);
  }

  function onDockPointerLeave() {
    if (!dockTab.enabled || !dockTab.open) return;
    clearTimeout(closeTimer);
    closeTimer = setTimeout(() => dockTab.close(), CLOSE_AFTER_LEAVE_MS);
  }

  /** opening an app from the dock folds it back too */
  function onDockClick(e: MouseEvent) {
    if (!dockTab.enabled || !dockTab.open) return;
    if ((e.target as HTMLElement).closest(".weg-item")) {
      setTimeout(() => dockTab.close(), 250);
    }
  }

  function handleDragStart() {
    setDockIsDraggingItem(true);
  }

  function handleDragEnd() {
    setDockIsDraggingItem(false);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events (the click only folds the dock back) -->
<div
  role="toolbar"
  tabindex="0"
  data-has-margin={!!settings?.margin}
  data-size={settings?.mode === SeelenWegMode.FullWidth ? "full-width" : "min-content"}
  class="taskbar {settingsState.position.toLowerCase()}"
  class:horizontal={isHorizontal}
  class:vertical={!isHorizontal}
  class:hidden={dockShouldBeHidden.value}
  class:tab-mode={dockTab.enabled}
  oncontextmenu={onContextMenu}
  onpointerenter={onDockPointerEnter}
  onpointerleave={onDockPointerLeave}
  onclick={onDockClick}
>
  <BackgroundByLayers />
  <div class="weg-items-container">
    <DragDropProvider
      {manager}
      onDragStart={handleDragStart}
      onDragOver={handleDragOver}
      onDragEnd={handleDragEnd}
    >
      <div class="weg-items">
        {#if isEmpty}
          <span class="weg-empty-state-label">{$t("weg.empty")}</span>
        {:else}
          <DockItemsGroup id="left" items={visibleGroupedItems.left} {itemIndexById} />
          <DockItemsGroup id="center" items={visibleGroupedItems.center} {itemIndexById} />
          <DockItemsGroup id="right" items={visibleGroupedItems.right} {itemIndexById} />
        {/if}
      </div>

      <DragOverlay>
        {#snippet children(source)}
          {@const overlayItem = dockState.items.find((c) => c.id === source.id)}
          {#if overlayItem}
            <WegItemSwitch item={overlayItem} isOverlay />
          {/if}
        {/snippet}
      </DragOverlay>
    </DragDropProvider>
  </div>
</div>

{#if dockTab.enabled}
  <!-- the tab the dock folds into: press it to open the dock -->
  <button
    class="weg-tab {settingsState.position.toLowerCase()}"
    class:weg-tab-open={dockTab.open}
    aria-label="Dock"
    aria-expanded={dockTab.open}
    onclick={() => dockTab.toggle()}
    onpointerenter={onDockPointerEnter}
    onpointerleave={onDockPointerLeave}
  >
    <span class="weg-tab-grip"></span>
  </button>
{/if}
