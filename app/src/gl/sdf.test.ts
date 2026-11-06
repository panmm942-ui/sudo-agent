import { describe, expect, it } from "vitest";
import { roundedRect } from "./sdf";

describe("roundedRect", () => {
  it("is minus the half-height at the centre of a 100×40 r20 pill", () => {
    expect(roundedRect(0, 0, 100, 40, 20)).toBe(-20);
  });

  it("is zero on a straight edge", () => {
    expect(roundedRect(0, 20, 100, 40, 20)).toBe(0);
    expect(roundedRect(10, -20, 100, 40, 20)).toBe(0);
    expect(roundedRect(50, 0, 100, 40, 10)).toBe(0);
  });

  it("is the Euclidean distance to the arc outside a rounded corner", () => {
    // Corner arc centre of a 100×40 r20 rect sits at (30, 0); radius 20.
    const d = roundedRect(60, 30, 100, 40, 20);
    expect(d).toBeGreaterThan(0);
    expect(d).toBeCloseTo(Math.hypot(60 - 30, 30 - 0) - 20, 10);
  });

  it("is negative inside and positive outside, symmetric in all quadrants", () => {
    for (const [x, y] of [[45, 15], [-45, 15], [45, -15], [-45, -15]]) {
      expect(roundedRect(x, y, 120, 40, 12)).toBeLessThan(0);
      expect(roundedRect(x * 2, y * 2, 120, 40, 12)).toBeGreaterThan(0);
    }
  });

  it("clamps the radius to half the shorter side", () => {
    expect(roundedRect(0, 0, 126, 34, 999)).toBe(roundedRect(0, 0, 126, 34, 17));
  });
});
