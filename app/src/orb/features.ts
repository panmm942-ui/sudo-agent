/** Per-frame voice features (mic in Phase 1/2, TTS output while speaking). */
export type OrbFeatures = {
  /** Loudness 0..1. */
  rms: number;
  /** Fundamental frequency; 0 = unvoiced. */
  pitchHz: number;
  /** Normalised spectral centroid 0..1 (brightness / noisiness). */
  centroid: number;
  /** A syllable / transient started this frame. */
  onset: boolean;
  /** Who produced the audio; absent means mic. */
  source?: "mic" | "tts";
};

/** Smoothed values the orb shader consumes. */
export type OrbUniforms = {
  /** Disk scale, 1..1.35. */
  scale: number;
  /** OKLCH hue in degrees, 70 (amber) .. 230 (ice). */
  hue: number;
  /** Surface noise amount, 0.2..1. */
  turbulence: number;
  /** Onset ripple, 1 on onset decaying to 0. */
  ripple: number;
};

/** `listening` follows mic features, `speaking` follows TTS features, `thinking` ignores both and swirls. */
export type OrbMode = "listening" | "thinking" | "speaking";

export const ORB_SCALE_GAIN = 0.35;
export const ORB_SCALE_MAX = 1 + ORB_SCALE_GAIN;
export const ORB_SPRING_K = 180;
export const ORB_RIPPLE_TAU = 0.25;
export const ORB_PITCH_LO = 90;
export const ORB_PITCH_HI = 320;
export const ORB_HUE_LO = 70;
export const ORB_HUE_HI = 230;
/** Fixed lightness / chroma of the orb accent (matches the default accent token). */
export const ORB_L = 0.8;
export const ORB_C = 0.11;

const SPRING_C = 2 * Math.sqrt(ORB_SPRING_K);
const SUBSTEP = 1 / 240;
const MAX_DT = 0.5;
/** Short exponential smoothing so pitch / centroid jitter never reads as flicker. */
const HUE_TAU = 0.12;
const TURBULENCE_TAU = 0.1;

const clamp = (v: number, lo: number, hi: number) => (v < lo ? lo : v > hi ? hi : v);
const finiteOr = (v: number, fallback: number) => (Number.isFinite(v) ? v : fallback);

/** Linear map of pitch 90..320 Hz onto OKLCH hue 70..230, clamped. */
export function pitchToHue(pitchHz: number): number {
  const t = clamp((pitchHz - ORB_PITCH_LO) / (ORB_PITCH_HI - ORB_PITCH_LO), 0, 1);
  return ORB_HUE_LO + t * (ORB_HUE_HI - ORB_HUE_LO);
}

/** Turns raw features into stable shader uniforms. */
export class OrbSmoother {
  mode: OrbMode = "listening";

  private scale = 1;
  private scaleVel = 0;
  private hueTarget = ORB_HUE_LO;
  private hue = ORB_HUE_LO;
  private turbulence = 0.2;
  private ripple = 0;

  push(f: OrbFeatures, dt: number): OrbUniforms {
    const step = clamp(finiteOr(dt, 0), 0, MAX_DT);
    const accepted = this.accepts(f);
    const rms = accepted ? clamp(finiteOr(f.rms, 0), 0, 1) : 0;
    const centroid = accepted ? clamp(finiteOr(f.centroid, 0), 0, 1) : 0;
    const pitch = accepted ? finiteOr(f.pitchHz, 0) : 0;

    if (pitch > 0) this.hueTarget = pitchToHue(pitch);
    this.stepScale(1 + ORB_SCALE_GAIN * rms, step);
    this.hue += (this.hueTarget - this.hue) * (1 - Math.exp(-step / HUE_TAU));
    this.turbulence += (0.2 + 0.8 * centroid - this.turbulence) * (1 - Math.exp(-step / TURBULENCE_TAU));
    this.ripple = accepted && f.onset ? 1 : this.ripple * Math.exp(-step / ORB_RIPPLE_TAU);

    return { scale: this.scale, hue: this.hue, turbulence: this.turbulence, ripple: this.ripple };
  }

  /** True once nothing visibly moves any more (lets the render loop sleep). */
  get settled(): boolean {
    return (
      Math.abs(this.scale - 1) < 1e-3 &&
      Math.abs(this.scaleVel) < 1e-3 &&
      Math.abs(this.hue - this.hueTarget) < 0.05 &&
      Math.abs(this.turbulence - 0.2) < 1e-3 &&
      this.ripple < 1e-3
    );
  }

  private accepts(f: OrbFeatures): boolean {
    if (this.mode === "thinking") return false;
    const source = f.source ?? "mic";
    return this.mode === "speaking" ? source === "tts" : source === "mic";
  }

  /** Critically damped spring (semi-implicit Euler, 240 Hz substeps), clamped to 1..1.35. */
  private stepScale(target: number, dt: number) {
    const n = Math.ceil(dt / SUBSTEP);
    const h = n > 0 ? dt / n : 0;
    for (let i = 0; i < n; i++) {
      this.scaleVel += (ORB_SPRING_K * (target - this.scale) - SPRING_C * this.scaleVel) * h;
      this.scale += this.scaleVel * h;
      if (this.scale > ORB_SCALE_MAX) {
        this.scale = ORB_SCALE_MAX;
        this.scaleVel = Math.min(this.scaleVel, 0);
      } else if (this.scale < 1) {
        this.scale = 1;
        this.scaleVel = Math.max(this.scaleVel, 0);
      }
    }
  }
}

/** OKLab (L, a, b) to linear-light sRGB, unclamped (Ottosson's matrices). */
export function oklabToLinearSrgb(L: number, a: number, b: number): [number, number, number] {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

/** OKLCH (L 0..1, C, hue degrees) to linear-light sRGB, unclamped. */
export function oklchToLinearSrgb(L: number, C: number, hueDeg: number): [number, number, number] {
  const h = (hueDeg * Math.PI) / 180;
  return oklabToLinearSrgb(L, C * Math.cos(h), C * Math.sin(h));
}

const rad = (deg: number) => (deg * Math.PI) / 180;
const AMBER_AB = [ORB_C * Math.cos(rad(ORB_HUE_LO)), ORB_C * Math.sin(rad(ORB_HUE_LO))];
const ICE_AB = [ORB_C * Math.cos(rad(ORB_HUE_HI)), ORB_C * Math.sin(rad(ORB_HUE_HI))];

/**
 * The orb accent (linear sRGB, clamped) for a hue on the 70..230 scale at L 0.80.
 * The endpoints are exactly OKLCH(0.80 0.11 70) amber and OKLCH(0.80 0.11 230) ice; in between
 * the colour desaturates through neutral silver in OKLab (amber, champagne, silver, frost, ice)
 * instead of rotating the hue angle, which would sweep through lime and green like a rainbow.
 */
/** The orb's second, deeper colour: the accent hue turned ~70°, so the inner current reads as two tones. */
export function orbSecondary(hueDeg: number): [number, number, number] {
  const [r, g, b] = oklchToLinearSrgb(0.66, 0.14, hueDeg + 70);
  return [clamp(r, 0, 1), clamp(g, 0, 1), clamp(b, 0, 1)];
}

export function orbAccent(hueDeg: number): [number, number, number] {
  const t = clamp((hueDeg - ORB_HUE_LO) / (ORB_HUE_HI - ORB_HUE_LO), 0, 1);
  const [end, k] = t < 0.5 ? [AMBER_AB, 1 - 2 * t] : [ICE_AB, 2 * t - 1];
  const a = end[0] * k;
  const b = end[1] * k;
  const [r, g, bl] = oklabToLinearSrgb(ORB_L, a, b);
  return [clamp(r, 0, 1), clamp(g, 0, 1), clamp(bl, 0, 1)];
}
