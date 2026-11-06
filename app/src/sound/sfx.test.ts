import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MASTER_GAIN, SFX_NAMES, createSfx, recipe, sfxForTransition, type SfxName } from "./sfx";

const freqs = (n: SfxName) => recipe(n).partials.map((p) => p.freq);

describe("sfx.recipe", () => {
  it("open: 660 + 990 Hz, gains 0.18/0.08, attack 4 ms, decay 140 ms", () => {
    const r = recipe("open");
    expect(r.partials).toEqual([
      { freq: 660, gain: 0.18 },
      { freq: 990, gain: 0.08 },
    ]);
    expect(r.attackMs).toBe(4);
    expect(r.decayMs).toBe(140);
    expect(r.glideTo).toBeUndefined();
  });

  it("close: 990 Hz gliding to 660 Hz over 120 ms", () => {
    const r = recipe("close");
    expect(freqs("close")).toEqual([990]);
    expect(r.glideTo).toBe(660);
    expect(r.decayMs).toBe(120);
  });

  it("confirm: 784 Hz then 1175 Hz 70 ms later, 90 ms each", () => {
    const r = recipe("confirm");
    expect(freqs("confirm")).toEqual([784, 1175]);
    expect(r.partials.map((p) => p.startMs ?? 0)).toEqual([0, 70]);
    expect(r.decayMs).toBe(90);
  });

  it("done: 1046 Hz through a 2.4 kHz lowpass, 160 ms", () => {
    const r = recipe("done");
    expect(freqs("done")).toEqual([1046]);
    expect(r.lowpassHz).toBe(2400);
    expect(r.decayMs).toBe(160);
  });

  it("error: 220 Hz triangle, 160 ms", () => {
    const r = recipe("error");
    expect(r.partials).toHaveLength(1);
    expect(r.partials[0]).toMatchObject({ freq: 220, wave: "triangle" });
    expect(r.decayMs).toBe(160);
  });

  it("alarm: rising 523/659/784 Hz motif, 180 ms each, 600 ms gap, looping", () => {
    const r = recipe("alarm");
    expect(freqs("alarm")).toEqual([523, 659, 784]);
    expect(r.partials.map((p) => p.startMs ?? 0)).toEqual([0, 180, 360]);
    expect(r.decayMs).toBe(180);
    expect(r.loopMs).toBe(3 * 180 + 600);
  });

  it("keeps every gain, and every simultaneous sum, at or under 0.25 after master", () => {
    expect(MASTER_GAIN).toBe(0.25);
    for (const n of SFX_NAMES) {
      const r = recipe(n);
      for (const p of r.partials) expect(p.gain * MASTER_GAIN).toBeLessThanOrEqual(0.25);
      const byStart = new Map<number, number>();
      for (const p of r.partials) byStart.set(p.startMs ?? 0, (byStart.get(p.startMs ?? 0) ?? 0) + p.gain);
      for (const sum of byStart.values()) expect(sum * MASTER_GAIN).toBeLessThanOrEqual(0.25);
    }
  });

  it("is pure: mutating a result does not affect the next call", () => {
    const a = recipe("open");
    a.partials[0].freq = 1;
    a.decayMs = 1;
    expect(recipe("open").partials[0].freq).toBe(660);
    expect(recipe("open").decayMs).toBe(140);
  });
});

describe("sfxForTransition", () => {
  it("opens when listening starts from the resting states", () => {
    expect(sfxForTransition("dormant", "listening")).toEqual({ play: "open" });
    expect(sfxForTransition("compact", "listening")).toEqual({ play: "open" });
    expect(sfxForTransition("expanded", "listening")).toEqual({ play: "open" });
  });

  it("closes when an active state returns to rest, but not on hover collapse", () => {
    expect(sfxForTransition("responding", "compact")).toEqual({ play: "close" });
    expect(sfxForTransition("listening", "dormant")).toEqual({ play: "close" });
    expect(sfxForTransition("expanded", "compact")).toEqual({});
    expect(sfxForTransition("compact", "dormant")).toEqual({});
  });

  it("chimes on entering confirm and loops/stops the alarm", () => {
    expect(sfxForTransition("responding", "confirm")).toEqual({ play: "confirm" });
    expect(sfxForTransition("thinking", "alarm")).toEqual({ play: "alarm" });
    expect(sfxForTransition("alarm", "thinking")).toEqual({ stop: "alarm" });
    expect(sfxForTransition("alarm", "compact")).toEqual({ stop: "alarm" });
  });

  it("is silent for same-state and unrelated transitions", () => {
    expect(sfxForTransition("thinking", "thinking")).toEqual({});
    expect(sfxForTransition("listening", "thinking")).toEqual({});
  });
});

/** Minimal Web Audio stand-in: records created nodes and stop() calls. */
function fakeAudio() {
  const param = () => ({
    value: 0,
    setValueAtTime: vi.fn(),
    linearRampToValueAtTime: vi.fn(),
    exponentialRampToValueAtTime: vi.fn(),
    cancelScheduledValues: vi.fn(),
  });
  const oscs: { start: ReturnType<typeof vi.fn>; stop: ReturnType<typeof vi.fn>; type: string; frequency: ReturnType<typeof param> }[] = [];
  const node = () => ({ connect: vi.fn(), disconnect: vi.fn() });
  const t0 = Date.now();
  const ctx = {
    get currentTime() {
      return (Date.now() - t0) / 1000;
    },
    state: "running" as AudioContextState,
    destination: node(),
    resume: vi.fn(() => Promise.resolve()),
    createGain: () => ({ ...node(), gain: param() }),
    createBiquadFilter: () => ({ ...node(), type: "", frequency: param(), Q: param() }),
    createOscillator: () => {
      const o = { ...node(), type: "sine", frequency: param(), start: vi.fn(), stop: vi.fn(), onended: null };
      oscs.push(o);
      return o;
    },
  };
  return { ctx, oscs };
}

type NodeEvents = {
  on(event: "unhandledRejection", fn: (reason: unknown) => void): void;
  off(event: "unhandledRejection", fn: (reason: unknown) => void): void;
};

describe("createSfx", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("creates a single AudioContext lazily", () => {
    const { ctx, oscs } = fakeAudio();
    const factory = vi.fn(() => ctx as unknown as AudioContext);
    const sfx = createSfx(factory);
    expect(factory).not.toHaveBeenCalled();
    sfx.play("open");
    sfx.play("confirm");
    expect(factory).toHaveBeenCalledTimes(1);
    expect(oscs.map((o) => o.frequency.setValueAtTime.mock.calls[0][0])).toEqual([660, 990, 784, 1175]);
  });

  it("loops the alarm until stop('alarm'), and a second play does not double it", () => {
    const { ctx, oscs } = fakeAudio();
    const sfx = createSfx(() => ctx as unknown as AudioContext);
    sfx.play("alarm");
    sfx.play("alarm");
    expect(oscs).toHaveLength(3);
    vi.advanceTimersByTime(1140 * 2);
    expect(oscs.length).toBeGreaterThanOrEqual(9);
    sfx.stop("alarm");
    const n = oscs.length;
    for (const o of oscs) expect(o.stop).toHaveBeenCalled();
    vi.advanceTimersByTime(1140 * 3);
    expect(oscs).toHaveLength(n);
  });

  it("swallows a rejected resume() while the context is suspended", async () => {
    vi.useRealTimers();
    const unhandled: unknown[] = [];
    const onUnhandled = (reason: unknown) => unhandled.push(reason);
    // No @types/node in this project; vitest runs under Node, so `process` exists at runtime.
    const proc = (globalThis as unknown as { process: NodeEvents }).process;
    proc.on("unhandledRejection", onUnhandled);
    try {
      const { ctx } = fakeAudio();
      ctx.state = "suspended";
      // A plain function: vi.fn would attach its own handler to record the settled result.
      let resumes = 0;
      ctx.resume = (() => {
        resumes++;
        return Promise.reject(new Error("autoplay blocked"));
      }) as typeof ctx.resume;
      const sfx = createSfx(() => ctx as unknown as AudioContext);
      sfx.play("open");
      expect(resumes).toBe(1);
      await new Promise((r) => setTimeout(r, 10));
      expect(unhandled).toEqual([]);
    } finally {
      proc.off("unhandledRejection", onUnhandled);
    }
  });

  it("does not pile up alarm repeats while suspended, and resumes the loop once running", () => {
    const { ctx, oscs } = fakeAudio();
    ctx.state = "suspended";
    const sfx = createSfx(() => ctx as unknown as AudioContext);
    sfx.play("alarm");
    const first = oscs.length;
    vi.advanceTimersByTime(1140 * 5);
    expect(oscs).toHaveLength(first);
    ctx.state = "running";
    vi.advanceTimersByTime(1140 * 2);
    expect(oscs.length).toBeGreaterThan(first);
    sfx.stop("alarm");
    const n = oscs.length;
    vi.advanceTimersByTime(1140 * 3);
    expect(oscs).toHaveLength(n);
  });

  it("is a no-op when Web Audio is unavailable", () => {
    const sfx = createSfx(() => {
      throw new Error("no audio");
    });
    expect(() => sfx.play("open")).not.toThrow();
    expect(() => sfx.stop("alarm")).not.toThrow();
  });
});
