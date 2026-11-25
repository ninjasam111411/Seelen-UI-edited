<script lang="ts">
  import { Widget } from "@seelen-ui/lib";
  import { onMount } from "svelte";
  import { debounce } from "lodash";
  import Dock from "./components/Dock.svelte";
  import { windowsState, focused, widgetStatuses } from "./state/windows.svelte.ts";
  import { settingsState } from "./state/settings.svelte.ts";

  const startMenuExes = ["SearchHost.exe", "StartMenuExperienceHost.exe"];

  const topWindowIsFullscreen = $derived(windowsState.topInteractableWindow?.isFullscreen);

  const focusedIsAppsMenu = $derived(
    startMenuExes.some((program) => (focused.value?.exe || "").endsWith(program)) ||
      widgetStatuses.value.some(
        (w) => w.widgetId === "@seelen/apps-menu" && w.webviewWindowId === focused.value?.hwnd,
      ),
  );

  const showWidget = $derived(!topWindowIsFullscreen || focusedIsAppsMenu);
  const setWidgetVisibility = debounce((value: boolean) => {
    if (value) {
      Widget.self.show();
    } else {
      Widget.self.hide();
    }
  }, 100);
  // a full screen app (e.g. a video in the browser) can report itself as not full screen for a
  // moment now and then: the dock comes back only once that lasts, so it doesn't flicker over it
  const showAfterFullscreen = debounce(() => Widget.self.show(), 1500);

  $effect(() => {
    if (!settingsState.isReady) return;
    if (showWidget && !focusedIsAppsMenu) {
      setWidgetVisibility.cancel();
      showAfterFullscreen();
    } else {
      showAfterFullscreen.cancel();
      setWidgetVisibility(showWidget);
    }
  });

  onMount(() => {
    Widget.self.ready({ show: false }).then(() => {
      settingsState.isReady = true;
    });
  });
</script>

<Dock />
