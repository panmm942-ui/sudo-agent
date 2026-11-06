import { invoke } from "@tauri-apps/api/core";

/** Pill rect in logical px relative to the island window's client area. */
export type PillRect = { x: number; y: number; w: number; h: number; r: number };

export type PillRectSink = (rect: PillRect, scale: number) => void;

/** Minimum gap between rect updates while springs move (30 Hz). */
export const PILL_RECT_INTERVAL_MS = 1000 / 30;

/**
 * Throttles pill-rect updates: leading send, then at most one per interval carrying the latest rect.
 * A settled rect is always sent immediately and supersedes any pending one.
 */
export function createPillRectSender(send: PillRectSink, scale: () => number, intervalMs = PILL_RECT_INTERVAL_MS) {
  let lastSent = -Infinity;
  let pending: PillRect | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const cancel = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    pending = null;
  };
  const emit = (rect: PillRect) => {
    lastSent = Date.now();
    send(rect, scale());
  };
  const flush = () => {
    timer = null;
    const rect = pending;
    pending = null;
    if (rect) emit(rect);
  };

  return {
    push(rect: PillRect, settled = false): void {
      const copy = { ...rect };
      const wait = lastSent + intervalMs - Date.now();
      if (settled || wait <= 0) {
        cancel();
        emit(copy);
        return;
      }
      pending = copy;
      timer ??= setTimeout(flush, wait);
    },
    dispose: cancel,
  };
}

const inTauri = (): boolean => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let sender: ReturnType<typeof createPillRectSender> | null = null;

/**
 * Report the drawn pill to Rust for click-through hit-testing. Call every frame the shape changes,
 * with `settled = true` once the springs come to rest. No-op outside Tauri (browser, Playwright).
 */
export function setPillRect(rect: PillRect, settled = false): void {
  if (!inTauri()) return;
  sender ??= createPillRectSender(
    (r, scale) => void invoke("set_pill_rect", { rect: r, scale }).catch(() => {}),
    () => window.devicePixelRatio || 1,
  );
  sender.push(rect, settled);
}
