import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PILL_RECT_INTERVAL_MS, createPillRectSender, setPillRect, type PillRect } from "./bridge";

const rect = (w: number): PillRect => ({ x: (640 - w) / 2, y: 10, w, h: 84, r: 42 });

describe("createPillRectSender", () => {
  let send: ReturnType<typeof vi.fn<(rect: PillRect, scale: number) => void>>;

  beforeEach(() => {
    vi.useFakeTimers();
    send = vi.fn<(rect: PillRect, scale: number) => void>();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("sends the first rect immediately with the current scale", () => {
    const s = createPillRectSender(send, () => 1.25);
    s.push(rect(126));
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenLastCalledWith(rect(126), 1.25);
  });

  it("throttles a 60 Hz stream to 30 Hz, trailing with the latest rect", () => {
    const s = createPillRectSender(send, () => 1);
    for (let i = 0; i < 60; i++) {
      s.push(rect(126 + i));
      vi.advanceTimersByTime(1000 / 60);
    }
    vi.advanceTimersByTime(PILL_RECT_INTERVAL_MS);
    expect(send.mock.calls.length).toBeGreaterThanOrEqual(29);
    expect(send.mock.calls.length).toBeLessThanOrEqual(31);
    expect(send).toHaveBeenLastCalledWith(rect(185), 1);
  });

  it("never sends two rects closer than the interval", () => {
    const s = createPillRectSender(send, () => 1);
    const times: number[] = [];
    send.mockImplementation(() => times.push(Date.now()));
    for (let i = 0; i < 100; i++) {
      s.push(rect(126 + i));
      vi.advanceTimersByTime(7);
    }
    for (let i = 1; i < times.length; i++) {
      expect(times[i] - times[i - 1]).toBeGreaterThanOrEqual(Math.floor(PILL_RECT_INTERVAL_MS));
    }
  });

  it("sends a settled rect immediately even inside the throttle window", () => {
    const s = createPillRectSender(send, () => 1);
    s.push(rect(126));
    vi.advanceTimersByTime(5);
    s.push(rect(300), true);
    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenLastCalledWith(rect(300), 1);
  });

  it("drops the pending trailing rect once a settled rect is sent", () => {
    const s = createPillRectSender(send, () => 1);
    s.push(rect(126));
    s.push(rect(200));
    s.push(rect(360), true);
    vi.advanceTimersByTime(PILL_RECT_INTERVAL_MS * 3);
    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenLastCalledWith(rect(360), 1);
  });

  it("re-sends a settled rect even if identical to the last one", () => {
    const s = createPillRectSender(send, () => 1);
    s.push(rect(360), true);
    s.push(rect(360), true);
    expect(send).toHaveBeenCalledTimes(2);
  });

  it("copies the rect so later caller mutation does not leak into a pending send", () => {
    const s = createPillRectSender(send, () => 1);
    s.push(rect(126));
    const r = rect(200);
    s.push(r);
    r.w = 999;
    vi.advanceTimersByTime(PILL_RECT_INTERVAL_MS);
    expect(send).toHaveBeenLastCalledWith(rect(200), 1);
  });

  it("dispose cancels the pending trailing send", () => {
    const s = createPillRectSender(send, () => 1);
    s.push(rect(126));
    s.push(rect(200));
    s.dispose();
    vi.advanceTimersByTime(PILL_RECT_INTERVAL_MS * 2);
    expect(send).toHaveBeenCalledTimes(1);
  });
});

describe("setPillRect", () => {
  it("is a no-op outside Tauri", () => {
    expect(() => setPillRect(rect(126))).not.toThrow();
    expect(() => setPillRect(rect(126), true)).not.toThrow();
  });
});

describe("setPillRect inside Tauri", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.doUnmock("@tauri-apps/api/core");
    vi.resetModules();
  });

  it("invokes set_pill_rect with the rect and devicePixelRatio", async () => {
    vi.resetModules();
    const invoke = vi.fn(() => Promise.resolve());
    vi.doMock("@tauri-apps/api/core", () => ({ invoke }));
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {}, devicePixelRatio: 1.5 });
    const bridge = await import("./bridge");
    bridge.setPillRect(rect(360), true);
    expect(invoke).toHaveBeenCalledWith("set_pill_rect", { rect: rect(360), scale: 1.5 });
  });
});
