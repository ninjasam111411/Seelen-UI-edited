<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import FileIcon from "libs/ui/svelte/components/Icon/FileIcon.svelte";
  import type { Disc } from "../state.svelte.ts";

  interface Props {
    disc: Disc;
    /** show the title on the generated cover */
    showTitle?: boolean;
  }

  let { disc, showTitle = true }: Props = $props();

  let failed = $state(false);
  let iconFailed = $state(false);

  /** a stable color for each game, so its generated cover always looks the same */
  const hue = $derived([...disc.title].reduce((acc, c) => (acc * 31 + c.charCodeAt(0)) % 360, 7));
</script>

{#if disc.cover && !failed}
  <img class="ch-cover-image" src={convertFileSrc(disc.cover)} alt="" draggable="false" onerror={() => (failed = true)} />
{:else}
  <!-- generated cover for games without art: color from the title, the game's icon and the name -->
  <div class="ch-cover-generated" style:--ch-cover-hue={hue}>
    {#if disc.icon && !iconFailed}
      <img
        class="ch-cover-generated-icon"
        src={convertFileSrc(disc.icon)}
        alt=""
        draggable="false"
        onerror={() => (iconFailed = true)}
      />
    {:else}
      <FileIcon class="ch-cover-generated-icon" path={disc.iconPath} umid={disc.iconUmid} draggable="false" />
    {/if}
    {#if showTitle}
      <span class="ch-cover-generated-title">{disc.title}</span>
    {/if}
  </div>
{/if}
