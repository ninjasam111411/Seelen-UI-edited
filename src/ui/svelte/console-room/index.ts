import { getRootContainer } from "libs/ui/react/utils/index.ts";
import { mount } from "svelte";
import App from "./app.svelte";
import { Widget } from "@seelen-ui/lib";

import "@seelen-ui/lib/styles/reset.css";

// Desktop preset: always at the bottom of the z-order, over the desktop icons and under every
// app window. The state module positions it over the primary monitor and shows it.
const widget = Widget.getCurrent();
await widget.init({ normalizeDevicePixelRatio: true, saveAndRestoreLastRect: false });
await widget.window.setResizable(false);

mount(App, {
  target: getRootContainer(),
});
