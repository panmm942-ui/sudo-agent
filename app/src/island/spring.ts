/** Damped harmonic spring, integrated with semi-implicit Euler at bounded substeps. */

export type SpringCfg = { k: number; c: number; m?: number };

export const SPRINGS = {
  /** ζ≈0.92 — default; settles without a visible wobble. */
  snappy: { k: 380, c: 36 },
  /** Critically damped (ζ≈1.0, ~0.3 s): panel geometry glides in without a rim rebound. */
  panel: { k: 560, c: 48 },
  /** ζ≈0.75 — for growing, with a soft settle. */
  bouncy: { k: 300, c: 26 },
  /** ζ≈0.93 — for shrinking. */
  gentle: { k: 260, c: 30 },
} as const satisfies Record<string, SpringCfg>;

export type SpringName = keyof typeof SPRINGS;

const SUBSTEP = 1 / 240;
const MAX_DT = 1 / 10;
const REST_EPS = 0.01;

export class Spring {
  x: number;
  v = 0;
  target: number;
  private cfg: SpringCfg;

  constructor(x: number, cfg: SpringCfg = SPRINGS.snappy) {
    this.x = x;
    this.target = x;
    this.cfg = cfg;
  }

  /** Retargets from the current position and velocity (never jumps). Non-finite targets are ignored. */
  setTarget(t: number, cfg?: SpringCfg): void {
    if (Number.isFinite(t)) this.target = t;
    if (cfg) this.cfg = cfg;
  }

  step(dt: number): void {
    if (!(dt > 0)) return;
    let remaining = Math.min(dt, MAX_DT);
    const { k, c, m = 1 } = this.cfg;
    // Include the fractional end of this display frame. Carrying it into the next frame
    // quantizes geometry to 240 Hz, making the rim hold/jump at other refresh rates.
    while (remaining > 0) {
      const step = Math.min(remaining, SUBSTEP);
      remaining -= step;
      const a = (-k * (this.x - this.target) - c * this.v) / m;
      this.v += a * step;
      this.x += this.v * step;
    }
    if (this.atRest) this.snap();
  }

  /** Jumps to the target with zero velocity. */
  snap(): void {
    this.x = this.target;
    this.v = 0;
  }

  get atRest(): boolean {
    return Math.abs(this.x - this.target) < REST_EPS && Math.abs(this.v) < REST_EPS;
  }
}
