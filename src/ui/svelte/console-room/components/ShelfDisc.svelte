<script lang="ts">
  import type { Disc } from "../state.svelte.ts";
  import { room } from "../state.svelte.ts";
  import { t } from "../i18n/index.ts";
  import DiscFace from "./DiscFace.svelte";

  interface Props {
    disc: Disc;
  }

  let { disc }: Props = $props();

  const isDragged = $derived(room.dragging?.disc?.id === disc.id);

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.stopPropagation();
    room.startDrag({ disc }, e.clientX, e.clientY);
  }
</script>

<div class="cr-shelf-slot" class:cr-shelf-slot-empty={isDragged} title={disc.title}>
  <div class="cr-shelf-disc" role="button" tabindex="-1" onpointerdown={onPointerDown}>
    {#if !isDragged}
      <DiscFace cover={disc.cover} iconPath={disc.iconPath} iconUmid={disc.iconUmid} />
    {/if}
  </div>
  <span class="cr-shelf-title">{disc.title}</span>
  {#if disc.burned}
    <button
      class="cr-shelf-remove"
      data-skin="transparent"
      title={$t("remove_disc")}
      onpointerdown={(e) => e.stopPropagation()}
      onclick={(e) => {
        e.stopPropagation();
        room.removeDisc(disc.id);
      }}>✕</button
    >
  {/if}
</div>
