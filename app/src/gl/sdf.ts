/** CPU mirror of the glass shader's signed distance functions (logical px). */

/**
 * Signed distance from point (px, py), relative to the rect centre, to a w×h rounded rect
 * with corner radius r (clamped to half the shorter side). Negative inside.
 */
export function roundedRect(px: number, py: number, w: number, h: number, r: number): number {
  const rr = Math.max(0, Math.min(r, w / 2, h / 2));
  const qx = Math.abs(px) - (w / 2 - rr);
  const qy = Math.abs(py) - (h / 2 - rr);
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  const inside = Math.min(Math.max(qx, qy), 0);
  return outside + inside - rr;
}
