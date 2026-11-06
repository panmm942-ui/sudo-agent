import type { Tint } from "../island/spotify";
import { smoothTint } from "./tint";

export type Material = { color: Tint; blend: number };
/** Interpolate the material itself, including entry/exit, rather than switching shader modes. */
export function stepMaterial(current: Material, color: Tint, blend: number, dt: number, snap = false): Material & { settled: boolean } {
  const nextColor = color.map((target, i) => snap ? target : smoothTint(current.color[i], target, dt)) as Tint;
  const nextBlend = snap ? blend : smoothTint(current.blend, blend, dt);
  const settled = Math.abs(nextBlend - blend) < 0.001 && color.every((v, i) => Math.abs(v - nextColor[i]) < 0.001);
  return settled ? { color, blend, settled } : { color: nextColor, blend: nextBlend, settled };
}
