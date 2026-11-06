import { describe, expect, it } from "vitest";
import { SPRINGS, Spring, type SpringCfg } from "./spring";

const FRAME = 1 / 60;

function run(s: Spring, seconds: number, onFrame?: (s: Spring) => void) {
  for (let t = 0; t < seconds; t += FRAME) {
    s.step(FRAME);
    onFrame?.(s);
  }
}

describe("Spring", () => {
  it.each(Object.entries(SPRINGS) as [string, SpringCfg][])(
    "%s converges to its target within 1.5 s",
    (_name, cfg) => {
      const s = new Spring(0, cfg);
      s.setTarget(100);
      run(s, 1.5);
      expect(s.atRest).toBe(true);
      expect(s.x).toBeCloseTo(100, 1);
    },
  );

  it("bouncy overshoots its target", () => {
    const s = new Spring(0, SPRINGS.bouncy);
    s.setTarget(100);
    let max = 0;
    run(s, 1.5, (sp) => (max = Math.max(max, sp.x)));
    expect(max).toBeGreaterThan(100);
  });

  it.each([30,60,120,144,240])('panel opens and closes without a rebound at %i Hz', hz => {
    for (const [from,to] of [[246,440],[44,420],[440,246],[420,44]]) {
      const spring = new Spring(from, SPRINGS.panel);
      spring.setTarget(to);
      let previous = from;
      for(let frame=0;frame<hz*1.5;frame++) {
        spring.step(1/hz);
        expect(spring.x).toBeGreaterThanOrEqual(Math.min(from,to));
        expect(spring.x).toBeLessThanOrEqual(Math.max(from,to));
        expect((spring.x-previous)*Math.sign(to-from)).toBeGreaterThanOrEqual(0);
        previous=spring.x;
      }
      expect(spring.atRest).toBe(true);
    }
  });

  it("gentle overshoots by less than 1 %", () => {
    const s = new Spring(0, SPRINGS.gentle);
    s.setTarget(100);
    let max = 0;
    run(s, 1.5, (sp) => (max = Math.max(max, sp.x)));
    expect(max).toBeLessThan(101);
  });

  it('keeps animation time after an 80 ms dropped frame',()=>{
    const dropped=new Spring(0,SPRINGS.panel),regular=new Spring(0,SPRINGS.panel);
    dropped.setTarget(100);regular.setTarget(100);
    dropped.step(.08);for(let i=0;i<8;i++)regular.step(.01);
    expect(Math.abs(dropped.x-regular.x)).toBeLessThan(1.5);
  });

  it("stays finite for a huge frame dt", () => {
    const s = new Spring(0, SPRINGS.snappy);
    s.setTarget(1000);
    for (let i = 0; i < 10; i++) s.step(0.5);
    expect(Number.isFinite(s.x)).toBe(true);
    expect(Number.isFinite(s.v)).toBe(true);
    expect(Math.abs(s.x)).toBeLessThan(2000);
  });

  it("bounds a suspended frame to 100 ms", () => {
    const a = new Spring(0, SPRINGS.snappy);
    const b = new Spring(0, SPRINGS.snappy);
    a.setTarget(100);
    b.setTarget(100);
    a.step(0.5);
    b.step(1 / 10);
    expect(a.x).toBeCloseTo(b.x, 10);
  });

  it("advances on every short frame instead of holding then jumping a whole substep", () => {
    const s = new Spring(246, SPRINGS.snappy);
    s.setTarget(440);
    let previous = s.x;
    for (let frame = 0; frame < 8; frame++) {
      s.step(0.002);
      expect(s.x).toBeGreaterThan(previous);
      previous = s.x;
    }
  });

  it("includes the fractional time in a high-refresh display frame", () => {
    const full = new Spring(246, SPRINGS.snappy);
    const short = new Spring(246, SPRINGS.snappy);
    full.setTarget(440);
    short.setTarget(440);
    full.step(1 / 144);
    short.step(1 / 240);
    expect(full.x).toBeGreaterThan(short.x);
  });

  it.each([60, 120])("preserves the existing whole-substep trajectory at %i Hz", (hz) => {
    const s = new Spring(246, SPRINGS.snappy);
    s.setTarget(440);
    let x = 246, v = 0;
    const substep = 1 / 240;
    for (let frame = 0; frame < hz / 2; frame++) {
      for (let step = 0; step < 240 / hz; step++) {
        v += (-SPRINGS.snappy.k * (x - 440) - SPRINGS.snappy.c * v) * substep;
        x += v * substep;
      }
      s.step(1 / hz);
      expect(s.x).toBeCloseTo(x, 10);
      expect(s.v).toBeCloseTo(v, 10);
    }
  });

  it("decelerates smoothly after the opening speed peaks on a 144 Hz display", () => {
    const s = new Spring(246, SPRINGS.snappy);
    s.setTarget(440);
    const dt = 1 / 144;
    let speed = Infinity;
    for (let frame = 0; frame < 18; frame++) {
      const previous = s.x;
      s.step(dt);
      const nextSpeed = (s.x - previous) / dt;
      if (frame >= 8) expect(nextSpeed).toBeLessThan(speed);
      speed = nextSpeed;
    }
  });

  it("keeps x and v continuous when retargeted mid-flight", () => {
    const s = new Spring(0, SPRINGS.snappy);
    s.setTarget(100);
    run(s, 0.1);
    const { x, v } = s;
    expect(v).not.toBe(0);
    s.setTarget(-50, SPRINGS.gentle);
    expect(s.x).toBe(x);
    expect(s.v).toBe(v);
    s.step(FRAME);
    // One frame later x has moved smoothly, not jumped toward the new target.
    expect(Math.abs(s.x - x)).toBeLessThan(0.1 * Math.abs(-50 - x));
    run(s, 1.5);
    expect(s.x).toBeCloseTo(-50, 1);
  });

  it("is at rest when constructed and moving after a retarget", () => {
    const s = new Spring(5, SPRINGS.snappy);
    expect(s.atRest).toBe(true);
    s.setTarget(6);
    expect(s.atRest).toBe(false);
  });

  it("snap() jumps to the target with zero velocity", () => {
    const s = new Spring(0, SPRINGS.bouncy);
    s.setTarget(42);
    s.step(FRAME);
    s.snap();
    expect(s.x).toBe(42);
    expect(s.v).toBe(0);
    expect(s.atRest).toBe(true);
  });
});

describe("Spring non-finite guard", () => {
  it.each([NaN, Infinity, -Infinity])("ignores a %s target and keeps settling to the last finite one", (bad) => {
    const s = new Spring(0, SPRINGS.snappy);
    s.setTarget(100);
    s.step(FRAME);
    s.setTarget(bad);
    expect(s.target).toBe(100);
    run(s, 1.5);
    expect(Number.isFinite(s.x)).toBe(true);
    expect(s.atRest).toBe(true);
    expect(s.x).toBeCloseTo(100, 1);
  });

  it("still applies the config when the target is ignored", () => {
    const s = new Spring(0, SPRINGS.snappy);
    s.setTarget(NaN, SPRINGS.gentle);
    s.setTarget(100);
    let max = 0;
    run(s, 1.5, (sp) => (max = Math.max(max, sp.x)));
    expect(max).toBeLessThan(101);
  });
});
