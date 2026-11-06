// Live desktop backdrop for the glass shader: Rust captures the region behind the island (WGC),
// sends BGRA8 frames over a Tauri channel, and we hand them to the renderer for texture upload.

import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Frame message header: w:u16 LE, h:u16 LE, t_capture_us:u64 LE (Unix epoch µs). */
export const HEADER_LEN = 12;

/** Tight BGRA8 pixels (`data` is a view into the message, not a copy). */
export type BackdropFrame = { width: number; height: number; tCaptureUs: number; data: Uint8Array };

/** Window-space rect the backdrop texture covers, logical px relative to the island window. */
export type BackdropRect = { x: number; y: number; w: number; h: number };

export type BackdropStatus = { mode: "capture" | "acrylic" | "frozen"; reason?: string };

/** What `subscribe` needs from GlassRenderer. Raw sources are BGRA8, row 0 = top. */
export interface BackdropTarget {
  /** Upload (texSubImage2D) the frame, or switch the shader to `u_hasBackdrop = 0` on null. */
  setBackdrop(src: { width: number; height: number; data: Uint8Array } | null): void;
  /** Where the texture sits in window space (`u_backdropRect`). */
  setBackdropRect?(rect: BackdropRect): void;
}

export type SubscribeOptions = {
  /** Called after each uploaded frame — wake the on-demand rAF loop here. */
  onFrame?: () => void;
  onStatus?: (s: BackdropStatus) => void;
};

export function parseFrame(msg: ArrayBuffer | Uint8Array | number[]): BackdropFrame {
  const bytes = Array.isArray(msg)
    ? Uint8Array.from(msg)
    : msg instanceof Uint8Array
      ? msg
      : new Uint8Array(msg);
  if (bytes.byteLength < HEADER_LEN) throw new Error(`backdrop frame too short: ${bytes.byteLength} B`);
  const dv = new DataView(bytes.buffer, bytes.byteOffset, HEADER_LEN);
  const width = dv.getUint16(0, true);
  const height = dv.getUint16(2, true);
  const tCaptureUs = Number(dv.getBigUint64(4, true));
  const need = HEADER_LEN + width * height * 4;
  if (bytes.byteLength < need) throw new Error(`backdrop frame truncated: ${bytes.byteLength} < ${need} B`);
  const data = new Uint8Array(bytes.buffer, bytes.byteOffset + HEADER_LEN, width * height * 4);
  return { width, height, tCaptureUs, data };
}

export type LatencySummary = { n: number; p50: number; p95: number; fps: number };

/** Ring buffer of (latency ms, arrival ms) samples. */
export class LatencyStats {
  readonly #lat: Float64Array;
  readonly #at: Float64Array;
  #n = 0;
  #head = 0;

  constructor(readonly capacity = 600) {
    this.#lat = new Float64Array(capacity);
    this.#at = new Float64Array(capacity);
  }

  add(latencyMs: number, arrivalMs: number): void {
    this.#lat[this.#head] = latencyMs;
    this.#at[this.#head] = arrivalMs;
    this.#head = (this.#head + 1) % this.capacity;
    this.#n = Math.min(this.#n + 1, this.capacity);
  }

  summary(): LatencySummary {
    const n = this.#n;
    if (n === 0) return { n: 0, p50: 0, p95: 0, fps: 0 };
    const sorted = Array.from(this.#lat.subarray(0, n)).sort((a, b) => a - b);
    const rank = (p: number) => sorted[Math.max(0, Math.ceil(p * n) - 1)];
    const oldest = this.#at[n < this.capacity ? 0 : this.#head];
    const newest = this.#at[(this.#head - 1 + this.capacity) % this.capacity];
    const span = newest - oldest;
    return { n, p50: rank(0.5), p95: rank(0.95), fps: span > 0 ? ((n - 1) * 1000) / span : 0 };
  }
}

const stats = new LatencyStats();

/** Capture→upload latency over the last 600 frames. */
export function latency(): LatencySummary {
  return stats.summary();
}

/** Wall-clock ms on the same epoch as Rust's `t_capture_us`. */
function nowEpochMs(): number {
  return performance.timeOrigin + performance.now();
}

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let lastFps = 0;

export function captureRate(moving: boolean, hidden: boolean): number { return moving ? 60 : hidden ? 1 : 30; }

/** Preserve visible quality; back off only after the hidden island settles. */
export function setAnimating(on: boolean, hidden = false): void {
  const fps = captureRate(on, hidden);
  if (fps === lastFps || !inTauri()) return;
  lastFps = fps;
  void invoke("set_capture_rate", { fps }).catch(() => (lastFps = 0));
}

/** Start streaming backdrop frames into `target`. Resolves to an unsubscribe function. */
export async function subscribe(target: BackdropTarget, opts: SubscribeOptions = {}): Promise<() => void> {
  if (!inTauri()) return () => {};
  let live = true;
  const channel = new Channel<ArrayBuffer | number[]>((msg) => {
    if (!live) return;
    const f = parseFrame(msg);
    target.setBackdrop(f);
    const now = nowEpochMs();
    stats.add(now - f.tCaptureUs / 1000, now);
    opts.onFrame?.();
  });
  const unlisten = await listen<BackdropStatus>("backdrop_status", (e) => {
    if (e.payload.mode === "acrylic") target.setBackdrop(null);
    opts.onStatus?.(e.payload);
    opts.onFrame?.();
  });
  const unlistenRect = await listen<BackdropRect>("backdrop_rect", (e) => {
    target.setBackdropRect?.(e.payload);
    opts.onFrame?.();
  });
  try {
    const rect = await invoke<BackdropRect>("start_backdrop", { channel });
    target.setBackdropRect?.(rect);
  } catch (err) {
    console.warn("backdrop capture unavailable", err);
  }
  const unmount = new URLSearchParams(location.search).get("debug") === "1" ? mountDebugOverlay() : () => {};
  return () => {
    live = false;
    unlisten();
    unlistenRect();
    unmount();
  };
}

/** `?debug=1`: fixed corner readout of capture→upload p50/p95 and fps. */
function mountDebugOverlay(): () => void {
  const el = document.createElement("div");
  el.style.cssText =
    "position:fixed;left:8px;bottom:8px;font:11px/1.3 ui-monospace,monospace;color:#fff;" +
    "background:rgba(0,0,0,.6);padding:4px 6px;border-radius:4px;pointer-events:none;z-index:9999";
  document.body.appendChild(el);
  const id = setInterval(() => {
    const s = stats.summary();
    el.textContent = `wgc→upload p50 ${s.p50.toFixed(1)} ms · p95 ${s.p95.toFixed(1)} ms · ${s.fps.toFixed(0)} fps · n ${s.n}`;
  }, 500);
  return () => {
    clearInterval(id);
    el.remove();
  };
}
