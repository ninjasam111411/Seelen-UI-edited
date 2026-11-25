// Game controller navigation for the console home, like a console's own menu.
//
// The backend answers only while the desktop or a Seelen UI widget is in front, so playing a game
// with the controller never moves the menu behind it. It reads Xbox (XInput) controllers itself;
// other controllers (e.g. PlayStation ones) are read through the browser's Gamepad API, and count
// only while the backend says the desktop is in front.

import { invoke, SeelenCommand } from "@seelen-ui/lib";

/** XInput button flags */
export const PAD = {
  up: 0x0001,
  down: 0x0002,
  left: 0x0004,
  right: 0x0008,
  start: 0x0010,
  back: 0x0020,
  a: 0x1000,
  b: 0x2000,
  x: 0x4000,
  y: 0x8000,
} as const;

export type PadButton = keyof typeof PAD;

const POLL_MS = 60;
/** polling while no controller answers, to stay cheap */
const IDLE_POLL_MS = 1000;
const STICK_DEADZONE = 16000;
/** held left/right repeats after this delay, then every REPEAT_MS */
const REPEAT_DELAY_MS = 380;
const REPEAT_MS = 110;

/** the browser's standard gamepad mapping, in XInput flags */
const STANDARD_BUTTONS: [number, number][] = [
  [0, PAD.a],
  [1, PAD.b],
  [2, PAD.x],
  [3, PAD.y],
  [8, PAD.back],
  [9, PAD.start],
  [12, PAD.up],
  [13, PAD.down],
  [14, PAD.left],
  [15, PAD.right],
];

function browserPad(): number {
  let buttons = 0;
  try {
    for (const pad of navigator.getGamepads?.() ?? []) {
      if (!pad) continue;
      for (const [index, flag] of STANDARD_BUTTONS) {
        if (pad.buttons[index]?.pressed) buttons |= flag;
      }
      const [x = 0, y = 0] = pad.axes;
      if (x < -0.5) buttons |= PAD.left;
      if (x > 0.5) buttons |= PAD.right;
      if (y < -0.5) buttons |= PAD.up;
      if (y > 0.5) buttons |= PAD.down;
    }
  } catch {
    // no gamepad support
  }
  return buttons;
}

/** Calls `onPress` once per button press, and repeatedly while left/right are held. */
export function startController(onPress: (button: PadButton) => void): () => void {
  let previous = 0;
  let heldSince = 0;
  let lastRepeat = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;

  const tick = async () => {
    let buttons = 0;
    let answered = false;
    try {
      const state = await invoke(SeelenCommand.GetGamepadState);
      if (state) {
        answered = true;
        buttons = state.buttons;
        if (state.leftX < -STICK_DEADZONE) buttons |= PAD.left;
        if (state.leftX > STICK_DEADZONE) buttons |= PAD.right;
        if (state.leftY > STICK_DEADZONE) buttons |= PAD.up;
        if (state.leftY < -STICK_DEADZONE) buttons |= PAD.down;
      }
    } catch {
      // backend not available
    }
    // other controllers count only while the desktop is in front too (the widget is visible
    // under other windows, so the browser would report them during games)
    if (answered) buttons |= browserPad();

    const pressed = buttons & ~previous;
    const now = performance.now();
    for (const [name, flag] of Object.entries(PAD) as [PadButton, number][]) {
      if (pressed & flag) onPress(name);
    }
    // held left/right keeps moving, like scrolling through a console's game row
    const horizontal = buttons & (PAD.left | PAD.right);
    if (horizontal && (previous & horizontal) === horizontal) {
      if (now - heldSince > REPEAT_DELAY_MS && now - lastRepeat > REPEAT_MS) {
        lastRepeat = now;
        onPress(horizontal & PAD.left ? "left" : "right");
      }
    } else if (horizontal) {
      heldSince = now;
      lastRepeat = now;
    }
    previous = buttons;

    if (!stopped) timer = setTimeout(tick, answered || buttons ? POLL_MS : IDLE_POLL_MS);
  };

  tick();
  return () => {
    stopped = true;
    clearTimeout(timer);
  };
}
