import type { OrbFeatures } from "./features";

/** Small deterministic PRNG (mulberry32). */
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const smoothstep = (e0: number, e1: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - e0) / (e1 - e0)));
  return t * t * (3 - 2 * t);
};

/**
 * Speech-like feature stream: phrases of 1.2-2.8 s separated by 0.35-0.9 s pauses;
 * syllables at 4-6 Hz with a fast-attack envelope; a declining pitch contour inside
 * 110-220 Hz with stress bumps; onsets on phrase starts and stressed syllables.
 */
export class MockVoice {
  /** True while inside a phrase (between pauses). */
  inPhrase = false;
  /** Syllables started so far. */
  syllableCount = 0;

  private readonly rnd: () => number;
  private t = 0;
  private segEnd: number;

  private phraseStart = 0;
  private phraseDur = 0;
  private f0Start = 190;
  private f0End = 125;
  private gain = 1;

  private sylStart = 0;
  private sylDur = 0.2;
  private sylAmp = 0;
  private sylCentroid = 0.4;
  private sylFricative = false;
  private sylStressed = false;
  private sylPending = false;
  private firstSyllableOfPhrase = 0;

  constructor(seed = 1) {
    this.rnd = mulberry32(seed);
    this.segEnd = 0.4;
  }

  next(dt: number): OrbFeatures {
    this.t += Math.max(0, Number.isFinite(dt) ? dt : 0);
    let onset = false;

    if (this.t >= this.segEnd) this.advanceSegment();
    if (this.inPhrase && this.t >= this.sylStart + this.sylDur) this.nextSyllable();
    if (this.sylPending) {
      onset = this.sylStressed || this.syllableCount === this.firstSyllableOfPhrase;
      this.sylPending = false;
    }

    const noise = (this.rnd() - 0.5) * 0.01;
    if (!this.inPhrase) {
      return { rms: Math.max(0, 0.006 + noise), pitchHz: 0, centroid: 0.12, onset: false };
    }

    const u = (this.t - this.sylStart) / this.sylDur;
    const env = u < 0.22 ? smoothstep(0, 0.22, u) : 1 - smoothstep(0.35, 1, u);
    const rms = Math.min(1, Math.max(0, this.sylAmp * this.gain * env + 0.01 + noise));

    const p = Math.min(1, (this.t - this.phraseStart) / this.phraseDur);
    const stress = this.sylStressed ? 22 * Math.sin(Math.PI * Math.min(1, u)) : 0;
    const jitter = (this.rnd() - 0.5) * 3;
    const f0 = this.f0Start + (this.f0End - this.f0Start) * p + stress + jitter;
    const voiced = env > 0.2 && !(this.sylFricative && u < 0.18);
    const pitchHz = voiced ? Math.min(220, Math.max(110, f0)) : 0;

    const centroid = this.sylFricative && u < 0.18 ? 0.82 : this.sylCentroid * (0.85 + 0.3 * env);
    return { rms, pitchHz, centroid: Math.min(1, Math.max(0, centroid)), onset };
  }

  /** Ends the pause and starts a phrase (phrases end themselves in `nextSyllable`). */
  private advanceSegment() {
    this.inPhrase = true;
    this.phraseStart = this.t;
    this.phraseDur = 1.2 + this.rnd() * 1.6;
    this.f0Start = 175 + this.rnd() * 30;
    this.f0End = 118 + this.rnd() * 18;
    this.gain = 0.75 + this.rnd() * 0.25;
    this.segEnd = Infinity;
    this.firstSyllableOfPhrase = this.syllableCount + 1;
    this.sylStart = this.t;
    this.sylDur = 0;
    this.nextSyllable();
  }

  /** Starts the next syllable, or ends the phrase if it would overrun (phrases end on syllable boundaries). */
  private nextSyllable() {
    const start = this.sylStart + this.sylDur;
    if (start - this.phraseStart >= this.phraseDur && this.syllableCount >= this.firstSyllableOfPhrase) {
      this.inPhrase = false;
      this.segEnd = start + 0.35 + this.rnd() * 0.55;
      return;
    }
    this.sylStart = start;
    this.sylDur = 1 / (4 + this.rnd() * 2);
    this.sylStressed = this.rnd() < 0.28;
    this.sylAmp = this.sylStressed ? 0.75 + this.rnd() * 0.2 : 0.35 + this.rnd() * 0.35;
    this.sylCentroid = 0.25 + this.rnd() * 0.3;
    this.sylFricative = this.rnd() < 0.3;
    this.sylPending = true;
    this.syllableCount++;
  }
}
