import { describe, expect, it } from "vitest";
import { MockVoice } from "./mock";
import type { OrbFeatures } from "./features";

function record(seconds: number, seed = 7): OrbFeatures[] {
  const v = new MockVoice(seed);
  return Array.from({ length: Math.round(seconds * 100) }, () => v.next(0.01));
}

describe("MockVoice", () => {
  it("is deterministic per seed", () => {
    expect(record(3, 1)).toEqual(record(3, 1));
    expect(record(3, 1)).not.toEqual(record(3, 2));
  });

  it("stays in range with pitch 110-220 Hz when voiced", () => {
    for (const f of record(30)) {
      expect(f.rms).toBeGreaterThanOrEqual(0);
      expect(f.rms).toBeLessThanOrEqual(1);
      expect(f.centroid).toBeGreaterThanOrEqual(0);
      expect(f.centroid).toBeLessThanOrEqual(1);
      if (f.pitchHz !== 0) {
        expect(f.pitchHz).toBeGreaterThanOrEqual(110);
        expect(f.pitchHz).toBeLessThanOrEqual(220);
      }
    }
  });

  it("produces syllables at 4-6 Hz inside phrases, with pauses and onsets", () => {
    const v = new MockVoice(3);
    let speech = 0;
    let onsets = 0;
    let silentFrames = 0;
    for (let i = 0; i < 6000; i++) {
      const f = v.next(0.01);
      if (v.inPhrase) speech += 0.01;
      else if (f.rms < 0.05) silentFrames++;
      if (f.onset) {
        onsets++;
        expect(v.inPhrase).toBe(true);
      }
    }
    const rate = v.syllableCount / speech;
    expect(rate).toBeGreaterThanOrEqual(4);
    expect(rate).toBeLessThanOrEqual(6);
    expect(silentFrames).toBeGreaterThan(300);
    expect(onsets).toBeGreaterThan(10);
  });
});
