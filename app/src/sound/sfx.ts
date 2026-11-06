/** Synthesized UI sounds (Web Audio). Recipes are pure data; `play` renders them. */

export type SfxName = "open" | "close" | "confirm" | "done" | "error" | "alarm";

export const SFX_NAMES: readonly SfxName[] = ["open", "close", "confirm", "done", "error", "alarm"];

/** Applied once on the shared output bus. */
export const MASTER_GAIN = 0.25;

export interface SfxPartial {
  freq: number;
  /** Peak gain before the master bus. */
  gain: number;
  /** Onset relative to the start of the sound; default 0. */
  startMs?: number;
  /** Oscillator wave; default "sine". */
  wave?: OscillatorType;
}

export interface SfxRecipe {
  partials: SfxPartial[];
  /** Linear rise from silence to peak. */
  attackMs: number;
  /** Length of each partial from its onset to silence (attack included). */
  decayMs: number;
  /** Final frequency of the first partial; the others glide by the same ratio. */
  glideTo?: number;
  /** Lowpass cutoff applied to the whole sound. */
  lowpassHz?: number;
  /** Period after which the sound repeats until stopped. */
  loopMs?: number;
}

const RECIPES: Record<SfxName, SfxRecipe> = {
  open: {
    partials: [
      { freq: 660, gain: 0.18 },
      { freq: 990, gain: 0.08 },
    ],
    attackMs: 4,
    decayMs: 140,
  },
  close: { partials: [{ freq: 990, gain: 0.18 }], attackMs: 4, decayMs: 120, glideTo: 660 },
  confirm: {
    partials: [
      { freq: 784, gain: 0.16 },
      { freq: 1175, gain: 0.12, startMs: 70 },
    ],
    attackMs: 4,
    decayMs: 90,
  },
  done: { partials: [{ freq: 1046, gain: 0.14, wave: "sawtooth" }], attackMs: 4, decayMs: 160, lowpassHz: 2400 },
  error: { partials: [{ freq: 220, gain: 0.22, wave: "triangle" }], attackMs: 6, decayMs: 160 },
  alarm: {
    partials: [
      { freq: 523, gain: 0.2 },
      { freq: 659, gain: 0.2, startMs: 180 },
      { freq: 784, gain: 0.2, startMs: 360 },
    ],
    attackMs: 6,
    decayMs: 180,
    loopMs: 3 * 180 + 600,
  },
};

/** The synthesis recipe for `name`; a fresh copy on every call. */
export function recipe(name: SfxName): SfxRecipe {
  const r = RECIPES[name];
  return { ...r, partials: r.partials.map((p) => ({ ...p })) };
}

/** States in which the island is at rest (no active voice turn). */
const RESTING = new Set(["dormant", "compact", "expanded"]);

/**
 * Which sound a state change should trigger: `open` when listening starts from rest,
 * `close` when an active turn returns to compact/dormant, `confirm` on entering confirm,
 * `alarm` (looping) on entering alarm and `stop: "alarm"` on leaving it.
 */
export function sfxForTransition(from: string, to: string): { play?: SfxName; stop?: SfxName } {
  if (from === to) return {};
  if (from === "alarm") return { stop: "alarm" };
  if (to === "alarm") return { play: "alarm" };
  if (to === "confirm") return { play: "confirm" };
  if (to === "listening" && RESTING.has(from)) return { play: "open" };
  if ((to === "compact" || to === "dormant") && !RESTING.has(from)) return { play: "close" };
  return {};
}

export interface Sfx {
  /** Plays `name` now. `alarm` loops until `stop("alarm")`; replaying a running loop is a no-op. */
  play(name: SfxName, alarm?: { volume: number; tone: "chime" | "bell" | "pulse"; fadeSeconds?: number }): void;
  /** Stops every voice of `name` (immediately, with a 15 ms release) and cancels its loop. */
  stop(name: SfxName): void;
}

const SILENCE = 0.0001;
const RELEASE_S = 0.015;
const LOOKAHEAD_S = 0.1;
/** How often a loop re-checks a suspended context before queueing its next repeat. */
const SUSPENDED_RETRY_MS = 250;

const wakeDelayMs = (at: number, now: number) => Math.max(0, (at - now - LOOKAHEAD_S) * 1000);

/** Builds a player around one lazily created AudioContext. */
export function createSfx(makeContext: () => AudioContext = () => new AudioContext()): Sfx {
  let ctx: AudioContext | null = null;
  let master: GainNode | null = null;
  let unavailable = false;
  const voices = new Map<SfxName, Set<{ osc: OscillatorNode; env: GainNode }>>();
  const loops = new Map<SfxName, ReturnType<typeof setTimeout>>();
  let alarmEnvelope: GainNode | null = null;

  function bus(): { ctx: AudioContext; master: GainNode } | null {
    if (unavailable) return null;
    if (!ctx || !master) {
      try {
        ctx = makeContext();
        master = ctx.createGain();
        master.gain.value = MASTER_GAIN;
        master.connect(ctx.destination);
      } catch {
        unavailable = true;
        return null;
      }
    }
    wake(ctx);
    return { ctx, master };
  }

  /** Asks a suspended context to start (autoplay policy may refuse; that is not an error here). */
  function wake(c: AudioContext) {
    if (c.state !== "suspended") return;
    try {
      c.resume().catch(() => {});
    } catch {
      /* resume() unsupported or threw synchronously */
    }
  }

  function schedule(name: SfxName, r: SfxRecipe, at: number, out: { ctx: AudioContext; master: GainNode }) {
    const { ctx: c } = out;
    let dest: AudioNode = out.master;
    if (r.lowpassHz) {
      const lp = c.createBiquadFilter();
      lp.type = "lowpass";
      lp.frequency.value = r.lowpassHz;
      lp.connect(out.master);
      dest = lp;
    }
    const set = voices.get(name) ?? new Set();
    voices.set(name, set);
    const ratio = r.glideTo ? r.glideTo / r.partials[0].freq : 1;
    const attack = r.attackMs / 1000;
    const len = r.decayMs / 1000;
    for (const p of r.partials) {
      const t0 = at + (p.startMs ?? 0) / 1000;
      const osc = c.createOscillator();
      const env = c.createGain();
      osc.type = p.wave ?? "sine";
      osc.frequency.setValueAtTime(p.freq, t0);
      if (ratio !== 1) osc.frequency.exponentialRampToValueAtTime(p.freq * ratio, t0 + len);
      env.gain.setValueAtTime(0, t0);
      env.gain.linearRampToValueAtTime(p.gain, t0 + attack);
      env.gain.exponentialRampToValueAtTime(SILENCE, t0 + len);
      osc.connect(env);
      env.connect(dest);
      const voice = { osc, env };
      set.add(voice);
      osc.onended = () => {
        set.delete(voice);
        env.disconnect();
      };
      osc.start(t0);
      osc.stop(t0 + len + RELEASE_S);
    }
  }

  return {
    play(name, alarm) {
      if (loops.has(name)) return;
      let out = bus();
      if (!out) return;
      if (name === "alarm" && alarm?.fadeSeconds) {
        const envelope = out.ctx.createGain();
        envelope.gain.setValueAtTime(0, out.ctx.currentTime);
        envelope.gain.linearRampToValueAtTime(1, out.ctx.currentTime + alarm.fadeSeconds);
        envelope.connect(out.master);
        alarmEnvelope = envelope;
        out = { ctx: out.ctx, master: envelope };
      }
      const r = recipe(name);
      if (name === "alarm" && alarm) {
        const frequencies = alarm.tone === "bell" ? [880, 1175, 1568] : alarm.tone === "pulse" ? [440, 440, 660] : null;
        r.partials.forEach((part, index) => {
          part.gain *= alarm.volume / 70;
          if (frequencies) part.freq = frequencies[index];
          if (alarm.tone === "pulse") part.wave = "triangle";
        });
      }
      const start = out.ctx.currentTime + 0.005;
      schedule(name, r, start, out);
      if (r.loopMs) {
        const period = r.loopMs / 1000;
        let next = start + period;
        // Repeats are placed on the audio clock; the timer wakes LOOKAHEAD_S early to queue each one.
        // While the context is not running its clock is frozen, so nothing is queued (no node pile-up).
        const tick = () => {
          if (out.ctx.state !== "running") {
            wake(out.ctx);
            loops.set(name, setTimeout(tick, SUSPENDED_RETRY_MS));
            return;
          }
          next = Math.max(next, out.ctx.currentTime + 0.005);
          schedule(name, r, next, out);
          next += period;
          loops.set(name, setTimeout(tick, wakeDelayMs(next, out.ctx.currentTime)));
        };
        loops.set(name, setTimeout(tick, wakeDelayMs(next, out.ctx.currentTime)));
      }
    },
    stop(name) {
      const loop = loops.get(name);
      if (loop !== undefined) clearTimeout(loop);
      loops.delete(name);
      if (name === "alarm" && alarmEnvelope) { alarmEnvelope.disconnect(); alarmEnvelope = null; }
      if (!ctx) return;
      const now = ctx.currentTime;
      for (const { osc, env } of voices.get(name) ?? []) {
        env.gain.cancelScheduledValues(now);
        env.gain.setValueAtTime(env.gain.value, now);
        env.gain.linearRampToValueAtTime(0, now + RELEASE_S);
        try {
          osc.stop(now + RELEASE_S);
        } catch {
          /* already stopped */
        }
      }
    },
  };
}

/** App-wide player; the AudioContext is created on the first `play`. */
export const sfx: Sfx = createSfx();
