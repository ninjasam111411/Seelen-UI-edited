<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import FileIcon from "libs/ui/svelte/components/Icon/FileIcon.svelte";

  interface Props {
    cover?: string | null;
    iconPath?: string | null;
    iconUmid?: string | null;
    spinning?: boolean;
  }

  let { cover = null, iconPath = null, iconUmid = null, spinning = false }: Props = $props();

  let coverFailed = $state(false);
</script>

<!-- The disc itself: rainbow sheen, a label with the cover art or the app icon, and the hole. -->
<div class="cr-disc-face" class:cr-disc-spinning={spinning}>
  <div class="cr-disc-label">
    {#if cover && !coverFailed}
      <img class="cr-disc-cover" src={convertFileSrc(cover)} alt="" draggable="false" onerror={() => (coverFailed = true)} />
    {:else}
      <FileIcon class="cr-disc-icon" path={iconPath} umid={iconUmid} draggable="false" />
    {/if}
  </div>
  <div class="cr-disc-hole"></div>
</div>
