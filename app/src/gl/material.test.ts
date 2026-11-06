import { describe, expect, it } from "vitest";
import { stepMaterial } from "./material";
describe("glass material transitions", () => {
  it("crossfades color and blend continuously when leaving Spotify", () => {
    const current = { color: [0.5, 0.1, 0.1] as [number, number, number], blend: 1 };
    const next = stepMaterial(current, [0.1, 0.1, 0.1], 0, 1 / 60);
    expect(next.color[0]).toBeLessThan(0.5);
    expect(next.color[0]).toBeGreaterThan(0.45);
    expect(next.blend).toBeGreaterThan(0.9);
    expect(next.settled).toBe(false);
  });
  it("snaps frozen frames and settles exactly to avoid a permanent render loop", () => {
    let current = { color: [0.5, 0.1, 0.1] as [number, number, number], blend: 1 };
    for (let i = 0; i < 240; i++) current = stepMaterial(current, [0.1, 0.1, 0.1], 0, 1 / 60);
    expect(current).toMatchObject({ color: [0.1, 0.1, 0.1], blend: 0, settled: true });
    expect(stepMaterial(current, [0.3, 0.2, 0.1], 1, 0, true)).toMatchObject({ color: [0.3, 0.2, 0.1], blend: 1, settled: true });
  });
});
