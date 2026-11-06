/** Adaptive glass tint: denser smoke over bright backdrops so content stays legible. */

/** Tint alpha over a dark backdrop (the Global Constraints value). */
export const TINT_ALPHA_DARK = 0.55;
/** Tint alpha over a bright (near-white) backdrop. */
export const TINT_ALPHA_BRIGHT = 0.72;
/** Mean luminance at or below which the dark alpha applies, and at or above which the bright one does. */
export const LUMA_DARK = 0.15;
export const LUMA_BRIGHT = 0.75;
/** Smoothing time constant for tint changes (s). */
export const TINT_TAU = 0.3;

const GRID_X = 32;
const GRID_Y = 16;

/** Relative luminance (sRGB-encoded inputs, 0..1) — perceptual enough for a tint choice. */
const luma = (r: number, g: number, b: number) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;

/** Mean luminance (0..1) of an 8-bit RGBA/BGRA image, sampled on a 32×16 grid. */
export function meanLuminance(data: Uint8Array | Uint8ClampedArray, width: number, height: number, bgra = false): number {
  if (width <= 0 || height <= 0 || data.length < width * height * 4) return 0;
  const [ri, bi] = bgra ? [2, 0] : [0, 2];
  let sum = 0;
  let n = 0;
  for (let j = 0; j < GRID_Y; j++) {
    const y = Math.min(height - 1, Math.floor(((j + 0.5) * height) / GRID_Y));
    for (let i = 0; i < GRID_X; i++) {
      const x = Math.min(width - 1, Math.floor(((i + 0.5) * width) / GRID_X));
      const o = (y * width + x) * 4;
      sum += luma(data[o + ri], data[o + 1], data[o + bi]);
      n++;
    }
  }
  return sum / n;
}

/** Tint alpha for a backdrop luminance: 0.55 when dark rising smoothly to 0.72 when bright. */
export function tintAlphaFor(luminance: number): number {
  if (!Number.isFinite(luminance)) return TINT_ALPHA_DARK;
  const t = Math.min(1, Math.max(0, (luminance - LUMA_DARK) / (LUMA_BRIGHT - LUMA_DARK)));
  return TINT_ALPHA_DARK + (TINT_ALPHA_BRIGHT - TINT_ALPHA_DARK) * t * t * (3 - 2 * t);
}

/** One exponential smoothing step toward `target` over `dt` seconds. */
export function smoothTint(current: number, target: number, dt: number): number {
  if (!(dt > 0)) return current;
  return current + (target - current) * (1 - Math.exp(-dt / TINT_TAU));
}

/** Mean luminance of an ImageBitmap (downsampled through a small 2D canvas); null if unavailable. */
export function bitmapLuminance(bmp: ImageBitmap): number | null {
  if (typeof OffscreenCanvas === "undefined") return null;
  const c = new OffscreenCanvas(GRID_X, GRID_Y);
  const ctx = c.getContext("2d");
  if (!ctx) return null;
  ctx.drawImage(bmp, 0, 0, GRID_X, GRID_Y);
  return meanLuminance(ctx.getImageData(0, 0, GRID_X, GRID_Y).data, GRID_X, GRID_Y);
}
