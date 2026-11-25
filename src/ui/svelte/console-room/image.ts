import { convertFileSrc } from "@tauri-apps/api/core";

/** Image source for a picture on disk, or a cover the user picked (kept as a data url). */
export function imageSrc(path: string): string {
  return path.startsWith("data:") ? path : convertFileSrc(path);
}
