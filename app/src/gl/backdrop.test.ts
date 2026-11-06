import { describe, expect, it } from "vitest";
import { HEADER_LEN, LatencyStats, parseFrame, subscribe, captureRate } from "./backdrop";

describe("capture activity budget", () => {
  it("keeps full animation quality, idles at 30, and backs off only after hiding settles", () => {
    expect(captureRate(true, true)).toBe(60);
    expect(captureRate(false, false)).toBe(30);
    expect(captureRate(false, true)).toBe(1);
    expect(captureRate(true, false)).toBe(60);
  });
});

function message(w: number, h: number, tUs: bigint, pixels: number[]): ArrayBuffer {
  const buf = new ArrayBuffer(HEADER_LEN + pixels.length);
  const dv = new DataView(buf);
  dv.setUint16(0, w, true);
  dv.setUint16(2, h, true);
  dv.setBigUint64(4, tUs, true);
  new Uint8Array(buf, HEADER_LEN).set(pixels);
  return buf;
}

describe("parseFrame", () => {
  it("reads the 12-byte little-endian header and a zero-copy pixel view", () => {
    const px = [1, 2, 3, 4, 5, 6, 7, 8];
    const buf = message(2, 1, 1_761_800_000_123_456n, px);
    const f = parseFrame(buf);
    expect(HEADER_LEN).toBe(12);
    expect(f.width).toBe(2);
    expect(f.height).toBe(1);
    expect(f.tCaptureUs).toBe(1_761_800_000_123_456);
    expect(Array.from(f.data)).toEqual(px);
    expect(f.data.buffer).toBe(buf);
  });

  it("accepts a Uint8Array view with a byte offset", () => {
    const inner = new Uint8Array(message(1, 1, 42n, [9, 8, 7, 6]));
    const outer = new Uint8Array(inner.length + 3);
    outer.set(inner, 3);
    const f = parseFrame(outer.subarray(3));
    expect([f.width, f.height, f.tCaptureUs]).toEqual([1, 1, 42]);
    expect(Array.from(f.data)).toEqual([9, 8, 7, 6]);
  });

  it("accepts a plain number array (JSON-serialised bytes)", () => {
    const bytes = Array.from(new Uint8Array(message(1, 1, 7n, [1, 1, 1, 1])));
    expect(parseFrame(bytes).tCaptureUs).toBe(7);
  });

  it("rejects truncated messages", () => {
    expect(() => parseFrame(new ArrayBuffer(8))).toThrow();
    expect(() => parseFrame(message(2, 2, 0n, [0, 0, 0, 0]))).toThrow();
  });
});

describe("LatencyStats", () => {
  it("reports nearest-rank p50/p95 over the samples", () => {
    const s = new LatencyStats(1000);
    for (let i = 1; i <= 100; i++) s.add(i, i * 10);
    const r = s.summary();
    expect(r.n).toBe(100);
    expect(r.p50).toBe(50);
    expect(r.p95).toBe(95);
  });

  it("keeps only the most recent `capacity` samples", () => {
    const s = new LatencyStats(10);
    for (let i = 1; i <= 30; i++) s.add(i, i);
    const r = s.summary();
    expect(r.n).toBe(10);
    expect(r.p50).toBe(25);
  });

  it("computes fps from arrival times over the window", () => {
    const s = new LatencyStats(100);
    for (let i = 0; i <= 30; i++) s.add(5, 1000 + (i * 1000) / 30);
    expect(s.summary().fps).toBeCloseTo(30, 5);
  });

  it("is empty-safe", () => {
    const r = new LatencyStats(4).summary();
    expect(r).toEqual({ n: 0, p50: 0, p95: 0, fps: 0 });
  });
});

describe("subscribe", () => {
  it("no-ops outside Tauri", async () => {
    let calls = 0;
    const stop = await subscribe({ setBackdrop: () => void calls++ });
    stop();
    expect(calls).toBe(0);
  });
});
