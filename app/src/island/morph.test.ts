import { describe, expect, it } from 'vitest';
import { cubicBezier } from './morph';

describe('cubicBezier', () => {
  it('starts at 0, ends at 1 and never goes backwards', () => {
    const ease = cubicBezier(0.2, 0.8, 0.2, 1);
    expect(ease(0)).toBe(0);
    expect(ease(1)).toBe(1);
    expect(ease(-1)).toBe(0);
    expect(ease(2)).toBe(1);
    let previous = 0;
    for (let i = 1; i <= 100; i++) { const v = ease(i / 100); expect(v).toBeGreaterThanOrEqual(previous); previous = v; }
  });
  it('matches CSS curves: linear stays linear, the landing curve front-loads the motion', () => {
    const linear = cubicBezier(1 / 3, 1 / 3, 2 / 3, 2 / 3);
    expect(linear(0.25)).toBeCloseTo(0.25, 3);
    // cubic-bezier(.2,.8,.2,1) is about 70% of the way at a quarter of the time.
    expect(cubicBezier(0.2, 0.8, 0.2, 1)(0.25)).toBeGreaterThan(0.6);
  });
});
