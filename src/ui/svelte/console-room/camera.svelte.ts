// First person camera of the room, rendered with CSS 3D transforms.
//
// World units are CSS pixels: +x right, +y down, +z towards the viewer. The camera starts in front
// of the TV wall looking at -z. `yaw` turns left/right and `pitch` looks up/down (degrees).

/** Room bounds the camera can walk inside (keeps it away from the walls). */
export const ROOM = {
  halfWidth: 1400,
  halfDepth: 1400,
  floorY: 460,
  ceilingY: -700,
};

const WALK_LIMIT_X = 1050;
const WALK_LIMIT_Z_BACK = -950;
const WALK_LIMIT_Z_FRONT = 1150;
const MAX_PITCH = 35;

/** The TV on the back wall: center and size, used for the zoom-in on launch. */
export const TV = { x: 0, y: -140, z: -ROOM.halfDepth + 6, width: 960, height: 540 };

const START = { x: 0, y: 0, z: 900, yaw: 0, pitch: 0 };

let x = $state(START.x);
let y = $state(START.y);
let z = $state(START.z);
let yaw = $state(START.yaw);
let pitch = $state(START.pitch);
/** animated moves (reset, zoom to TV) use a CSS transition, manual moves don't */
let animated = $state(false);
let animationTimer: ReturnType<typeof setTimeout> | undefined;

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

function animateTo(target: { x: number; y: number; z: number; yaw: number; pitch: number }, ms: number) {
  animated = true;
  // turn the short way around
  const turn = ((target.yaw - yaw + 540) % 360) - 180;
  yaw = yaw + turn;
  x = target.x;
  y = target.y;
  z = target.z;
  pitch = target.pitch;
  clearTimeout(animationTimer);
  animationTimer = setTimeout(() => (animated = false), ms);
}

class Camera {
  get animated() {
    return animated;
  }
  get yaw() {
    return yaw;
  }

  /**
   * CSS transform of the world element. The parent has `perspective: <p>px`, so translating by
   * `p` puts the eye at the camera position.
   */
  worldTransform(perspective: number): string {
    return `translateZ(${perspective}px) rotateX(${pitch}deg) rotateY(${yaw}deg) translate3d(${-x}px, ${-y}px, ${-z}px)`;
  }

  /** Mouse drag: grabbing the scene, so dragging right turns the view left. */
  look(dx: number, dy: number) {
    if (animated) return;
    yaw -= dx * 0.15;
    pitch = clamp(pitch + dy * 0.12, -MAX_PITCH, MAX_PITCH);
  }

  /** Turns slowly, used while dragging a disc near the screen edges. */
  turn(degrees: number) {
    if (animated) return;
    yaw += degrees;
  }

  /** Mouse wheel: walk forward (scroll up) or back where the camera is looking. */
  walk(deltaY: number) {
    if (animated) return;
    const step = -deltaY * 1.4;
    const radians = (yaw * Math.PI) / 180;
    x = clamp(x + Math.sin(radians) * step, -WALK_LIMIT_X, WALK_LIMIT_X);
    z = clamp(z - Math.cos(radians) * step, WALK_LIMIT_Z_BACK, WALK_LIMIT_Z_FRONT);
  }

  /** Moves the camera straight in front of the TV, close enough for it to fill the screen. */
  zoomToTv(viewportWidth = window.innerWidth, perspective = viewportWidth / 2) {
    const distance = (TV.width * perspective) / viewportWidth;
    animateTo({ x: TV.x, y: TV.y, z: TV.z + distance, yaw: 0, pitch: 0 }, 1300);
  }

  reset() {
    clearTimeout(animationTimer);
    animated = false;
    x = START.x;
    y = START.y;
    z = START.z;
    yaw = START.yaw;
    pitch = START.pitch;
  }
}

export const camera = new Camera();
