import { describe, expect, it } from "vitest";
import {
  OrbSmoother,
  oklchToLinearSrgb,
  orbAccent,
  pitchToHue,
  type OrbFeatures,
  type OrbUniforms,
} from "./features";

const SILENT: OrbFeatures = { rms: 0, pitchHz: 0, centroid: 0, onset: false };
const voiced = (pitchHz: number): OrbFeatures => ({ rms: 0.5, pitchHz, centroid: 0.4, onset: false });

/** Pushes `f` at 60 fps for `seconds`, returning the last output. */
function run(s: OrbSmoother, f: OrbFeatures, seconds: number): OrbUniforms {
  let out = s.push(f, 0);
  const steps = Math.round(seconds * 60);
  for (let i = 0; i < steps; i++) out = s.push(f, 1 / 60);
  return out;
}

describe("pitchToHue", () => {
  it("maps 90 Hz to amber 70 and 320 Hz to ice 230, clamping outside", () => {
    expect(pitchToHue(90)).toBe(70);
    expect(pitchToHue(320)).toBe(230);
    expect(pitchToHue(205)).toBeCloseTo(150, 9);
    expect(pitchToHue(40)).toBe(70);
    expect(pitchToHue(900)).toBe(230);
  });
});

describe("OrbSmoother", () => {
  it("settles hue on the pitch map", () => {
    expect(run(new OrbSmoother(), voiced(90), 1.5).hue).toBeCloseTo(70, 2);
    expect(run(new OrbSmoother(), voiced(320), 1.5).hue).toBeCloseTo(230, 2);
  });

  it("holds the last voiced hue while unvoiced", () => {
    const s = new OrbSmoother();
    run(s, voiced(200), 1.5);
    const held = run(s, { ...SILENT, rms: 0.3 }, 2).hue;
    expect(held).toBeCloseTo(pitchToHue(200), 2);
  });

  it("fires ripple 1 on onset, decaying with tau 250 ms", () => {
    const s = new OrbSmoother();
    expect(s.push({ ...SILENT, onset: true }, 1 / 60).ripple).toBe(1);
    let out = s.push(SILENT, 0);
    for (let i = 0; i < 15; i++) out = s.push(SILENT, 1 / 60);
    expect(out.ripple).toBeCloseTo(Math.exp(-1), 3);
    expect(new OrbSmoother().push({ ...SILENT, onset: true }, 0).ripple).toBe(1);
  });

  it("maps centroid to turbulence 0.2 + 0.8 c", () => {
    expect(run(new OrbSmoother(), { ...SILENT, centroid: 0 }, 1.5).turbulence).toBeCloseTo(0.2, 3);
    expect(run(new OrbSmoother(), { ...SILENT, centroid: 1 }, 1.5).turbulence).toBeCloseTo(1, 3);
    expect(run(new OrbSmoother(), { ...SILENT, centroid: 0.5 }, 1.5).turbulence).toBeCloseTo(0.6, 3);
  });

  it("drives scale toward 1 + 0.35 rms without ever exceeding 1.35", () => {
    const s = new OrbSmoother();
    expect(run(s, { ...SILENT, rms: 0.4 }, 1.5).scale).toBeCloseTo(1.14, 3);
    let max = 0;
    for (let i = 0; i < 600; i++) {
      const spike = i % 7 === 0 ? 50 : i % 3 === 0 ? 1 : 0;
      const dt = [0, 1 / 240, 1 / 60, 1 / 8, 0.4][i % 5];
      max = Math.max(max, s.push({ ...SILENT, rms: spike }, dt).scale);
    }
    expect(max).toBeLessThanOrEqual(1.35);
    expect(run(s, { ...SILENT, rms: 1 }, 2).scale).toBeCloseTo(1.35, 3);
  });

  it("stays finite for dt = 0 and garbage input", () => {
    const s = new OrbSmoother();
    const outs = [
      s.push(SILENT, 0),
      s.push({ rms: 1, pitchHz: 150, centroid: 1, onset: true }, 0),
      s.push({ rms: NaN, pitchHz: Infinity, centroid: -3, onset: false }, 0),
      s.push(SILENT, NaN),
      s.push(SILENT, -1),
    ];
    for (const o of outs) for (const v of Object.values(o)) expect(Number.isFinite(v)).toBe(true);
  });

  it("ignores features in thinking mode and mic features while speaking", () => {
    const thinking = new OrbSmoother();
    thinking.mode = "thinking";
    const t = run(thinking, { rms: 1, pitchHz: 320, centroid: 1, onset: true }, 1.5);
    expect(t.scale).toBeCloseTo(1, 3);
    expect(t.hue).toBeCloseTo(70, 3);
    expect(t.ripple).toBe(0);

    const speaking = new OrbSmoother();
    speaking.mode = "speaking";
    expect(run(speaking, { rms: 1, pitchHz: 320, centroid: 1, onset: false }, 1.5).scale).toBeCloseTo(1, 3);
    expect(run(speaking, { rms: 1, pitchHz: 320, centroid: 1, onset: false, source: "tts" }, 1.5).scale).toBeCloseTo(1.35, 3);

    const listening = new OrbSmoother();
    expect(run(listening, { rms: 1, pitchHz: 0, centroid: 0, onset: false, source: "tts" }, 1.5).scale).toBeCloseTo(1, 3);
  });
});

describe("oklchToLinearSrgb", () => {
  it("matches known reference colours", () => {
    const white = oklchToLinearSrgb(1, 0, 0);
    white.forEach((c) => expect(c).toBeCloseTo(1, 3));
    oklchToLinearSrgb(0, 0, 0).forEach((c) => expect(c).toBeCloseTo(0, 6));

    const red = oklchToLinearSrgb(0.627955, 0.257683, 29.2339);
    expect(red[0]).toBeCloseTo(1, 3);
    expect(red[1]).toBeCloseTo(0, 3);
    expect(red[2]).toBeCloseTo(0, 3);

    // #008000: sRGB 128/255 -> linear 0.2158605
    const green = oklchToLinearSrgb(0.519752, 0.176858, 142.4953);
    expect(green[0]).toBeCloseTo(0, 3);
    expect(green[1]).toBeCloseTo(0.2158605, 3);
    expect(green[2]).toBeCloseTo(0, 3);
  });

  it("keeps the accent ring L=0.80 C=0.11 inside the sRGB gamut", () => {
    for (let h = 70; h <= 230; h += 5) {
      for (const c of oklchToLinearSrgb(0.8, 0.11, h)) {
        expect(c).toBeGreaterThanOrEqual(0);
        expect(c).toBeLessThanOrEqual(1);
      }
    }
  });
});

describe("orbAccent", () => {
  it("hits the amber and ice endpoints exactly", () => {
    orbAccent(70).forEach((c, i) => expect(c).toBeCloseTo(oklchToLinearSrgb(0.8, 0.11, 70)[i], 9));
    orbAccent(230).forEach((c, i) => expect(c).toBeCloseTo(oklchToLinearSrgb(0.8, 0.11, 230)[i], 9));
  });

  it("passes through neutral silver, never through green", () => {
    const [r, g, b] = orbAccent(150);
    expect(Math.max(r, g, b) - Math.min(r, g, b)).toBeLessThan(1e-3);
    for (let h = 70; h <= 230; h += 2) {
      const [r, g, b] = orbAccent(h);
      expect(g - Math.max(r, b)).toBeLessThan(1e-3);
    }
  });
});
