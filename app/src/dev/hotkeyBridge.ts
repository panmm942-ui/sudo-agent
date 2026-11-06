/**
 * Maps the Rust hold-Right-Ctrl events and the local Jarvis events (jarvis.rs) onto IslandEvents.
 * Jarvis does the listening, recognition, answering and speaking; this only drives the island.
 */

import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";
import type { IslandEvent } from "../island/events";

export type ListenFn = (name: string, handler: EventCallback<unknown>) => Promise<UnlistenFn>;

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

type Text = { text?: unknown; final?: unknown; done?: unknown };
const text = (payload: unknown): string => {
  const value = (payload as Text | null)?.text;
  return typeof value === "string" ? value : "";
};

/**
 * Event name → IslandEvent (or nothing). Raw `hotkey_down`/`hotkey_up` go to Rust, which may ignore
 * them (the hotkey can be turned off); the island opens only on `jarvis_listening`, i.e. when Sudo AI
 * really listens (hotkey, orb click). "Hey Sudo" arrives as `jarvis_wake`.
 */
export const BRIDGE: Record<string, (payload: unknown) => IslandEvent | null> = {
  jarvis_listening: () => ({ type: "hotkey_down" }),
  hotkey_cancel: () => ({ type: "idle_timeout" }),
  jarvis_wake: () => ({ type: "wake" }),
  jarvis_transcript: (p) => ({ type: "transcript", text: text(p), final: (p as Text | null)?.final !== false }),
  jarvis_thinking: () => ({ type: "thinking" }),
  jarvis_reply: (p) => ({ type: "reply", text: text(p), done: (p as Text | null)?.done === true }),
  jarvis_idle: () => ({ type: "idle_timeout" }),
};

/** Listens for the hotkey and Jarvis events and dispatches them. Returns an uninstall function; a no-op outside Tauri. */
export function installHotkeyBridge(
  dispatch: (e: IslandEvent) => void,
  listenFn: ListenFn | null = inTauri() ? listen : null,
): () => void {
  if (!listenFn) return () => {};
  let disposed = false;
  const unlistens = Object.entries(BRIDGE).map(([name, map]) => {
    const p = listenFn(name, (event) => {
      if (disposed) return;
      const e = map(event.payload);
      if (e) dispatch(e);
    });
    p.catch((err) => console.warn(`hotkey bridge: cannot listen for ${name}`, err));
    return p;
  });
  return () => {
    if (disposed) return;
    disposed = true;
    for (const p of unlistens) p.then((unlisten) => unlisten()).catch(() => {});
  };
}
