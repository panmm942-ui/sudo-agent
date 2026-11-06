import { describe, expect, it } from "vitest";
import { TINT_ALPHA_BRIGHT, TINT_ALPHA_DARK, meanLuminance, smoothTint, tintAlphaFor } from "./tint";

const solid = (w: number, h: number, px: [number, number, number, number]) => {
  const d = new Uint8Array(w * h * 4);
  for (let i = 0; i < w * h; i++) d.set(px, i * 4);
  return d;
};

describe("adaptive tint", () => {
  it("measures mean luminance of solid images", () => {
    expect(meanLuminance(solid(64, 32, [0, 0, 0, 255]), 64, 32)).toBe(0);
    expect(meanLuminance(solid(64, 32, [255, 255, 255, 255]), 64, 32)).toBeCloseTo(1, 5);
  });

  it("honours BGRA channel order", () => {
    const blueRgba = solid(8, 8, [0, 0, 255, 255]);
    expect(meanLuminance(blueRgba, 8, 8)).toBeCloseTo(0.0722, 3);
    expect(meanLuminance(blueRgba, 8, 8, true)).toBeCloseTo(0.2126, 3);
  });

  it("guards against short buffers", () => {
    expect(meanLuminance(new Uint8Array(4), 8, 8)).toBe(0);
  });

  it("maps dark → 0.55 and bright → 0.72, monotonic in between", () => {
    expect(tintAlphaFor(0)).toBe(TINT_ALPHA_DARK);
    expect(tintAlphaFor(1)).toBe(TINT_ALPHA_BRIGHT);
    expect(tintAlphaFor(NaN)).toBe(TINT_ALPHA_DARK);
    let prev = 0;
    for (let l = 0; l <= 1; l += 0.05) {
      const a = tintAlphaFor(l);
      expect(a).toBeGreaterThanOrEqual(prev);
      prev = a;
    }
  });

  it("smooths toward the target and never overshoots", () => {
    const a = smoothTint(0.55, 0.72, 0.3);
    expect(a).toBeGreaterThan(0.55);
    expect(a).toBeLessThan(0.72);
    expect(smoothTint(0.55, 0.72, 0)).toBe(0.55);
    expect(smoothTint(0.55, 0.72, 100)).toBeCloseTo(0.72, 6);
  });
});
