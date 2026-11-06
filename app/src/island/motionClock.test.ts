import { describe, expect, it } from 'vitest';
import { MotionClock } from './motionClock';
import { Spring, SPRINGS } from './spring';

/** Exercise event times against real panel springs, independently of display callback cadence. */
function panel() {
  const clock = new MotionClock();
  const spring = new Spring(246, SPRINGS.panel);
  return {
    clock, spring,
    frame(now: number) { spring.step(clock.step(now)); },
    target(now: number, value: number) {
      // Time before the input belongs to the previous trajectory.
      spring.step(clock.retarget(now));
      spring.setTarget(value);
    },
  };
}

describe('MotionClock input timing', () => {
  it('starts the same trajectory after idle or ongoing backdrop/tint frames', () => {
    const sleeping = panel(), painting = panel();
    painting.clock.wake(0);
    for (const now of [14, 28, 42, 56, 70, 84, 98]) painting.frame(now);
    sleeping.target(111, 440);
    painting.target(111, 440);
    for (const now of [112, 126, 140, 154, 168]) {
      sleeping.frame(now); painting.frame(now);
      expect(painting.spring.x).toBeCloseTo(sleeping.spring.x, 10);
      expect(painting.spring.v).toBeCloseTo(sleeping.spring.v, 10);
    }
  });

  it('does not charge the 13 ms before an input to its new target', () => {
    const actual = panel(), reference = new Spring(246, SPRINGS.panel);
    actual.clock.wake(100);
    actual.target(113, 440);
    actual.frame(114);
    reference.setTarget(440); reference.step(.001);
    expect(actual.spring.x).toBeCloseTo(reference.x, 10);
    expect(actual.spring.x - 246).toBeLessThan(1);
  });

  it('advances a moving old target to the retarget time before changing direction', () => {
    const actual = panel(), reference = new Spring(246, SPRINGS.panel);
    actual.target(0, 440); reference.setTarget(440);
    actual.frame(70); reference.step(.07);
    const before = actual.spring.x;
    actual.target(83, 180); reference.step(.013); reference.setTarget(180);
    expect(actual.spring.x).toBeGreaterThan(before);
    expect(actual.spring.x).toBeCloseTo(reference.x, 10);
    expect(actual.spring.v).toBeCloseTo(reference.v, 10);
    actual.frame(84); reference.step(.001);
    expect(actual.spring.x).toBeCloseTo(reference.x, 10);
    expect(actual.spring.v).toBeCloseTo(reference.v, 10);
  });

  it('preserves a retarget at a frame boundary without integrating twice', () => {
    const actual = panel(), reference = new Spring(246, SPRINGS.panel);
    actual.target(0, 440); reference.setTarget(440);
    actual.frame(14); reference.step(.014);
    actual.target(14, 420); reference.setTarget(420);
    actual.frame(28); reference.step(.014);
    expect(actual.spring.x).toBeCloseTo(reference.x, 10);
    expect(actual.spring.v).toBeCloseTo(reference.v, 10);
  });

  it('does not let backdrop wakes reset an already running animation clock', () => {
    const actual = panel(), reference = new Spring(246, SPRINGS.panel);
    actual.target(0, 440); reference.setTarget(440);
    actual.clock.wake(7);
    actual.clock.wake(12);
    actual.frame(14); reference.step(.014);
    expect(actual.spring.x).toBeCloseTo(reference.x, 10);
  });

  it('drops a sleeping gap and caps suspended-frame catchup without retaining a backlog', () => {
    const clock = new MotionClock();
    clock.wake(0);
    expect(clock.step(1000)).toBe(.1);
    expect(clock.step(1014)).toBeCloseTo(.014);
    clock.sleep();
    expect(clock.retarget(60_000)).toBe(0);
    expect(clock.step(60_014)).toBeCloseTo(.014);
  });

});
