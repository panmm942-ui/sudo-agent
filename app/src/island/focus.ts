import { invoke } from "@tauri-apps/api/core";

const inTauri = (): boolean => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Takes keyboard focus for the island (confirm needs Enter/Esc) or hands it back to the window
 * that had it. The island window is created with `focus: false`; no-op outside Tauri.
 */
export function setIslandFocus(on: boolean): void {
  if (!inTauri()) return;
  invoke("island_focus", { on }).catch((err) => console.warn("island_focus failed", err));
}
