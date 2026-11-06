import { describe, expect, it } from 'vitest';
import { createWheelSeek, seekTarget } from './wheelSeek';

describe('wheel seek', () => {
  it('moves forward when scrolling down and back when scrolling up, one step per notch', () => {
    const wheel = createWheelSeek(5);
    expect(wheel.add(100)).toBe(5);
    expect(wheel.add(300)).toBe(15);
    expect(wheel.add(-100)).toBe(-5);
  });
  it('adds up small touchpad deltas instead of dropping them', () => {
    const wheel = createWheelSeek(5);
    expect([wheel.add(40), wheel.add(40), wheel.add(40)]).toEqual([0, 0, 5]);
    wheel.reset();
    expect(wheel.add(40)).toBe(0);
  });
  it('understands line and page wheel modes and ignores bad input', () => {
    const wheel = createWheelSeek(5);
    expect(wheel.add(3, 1)).toBe(0);
    expect(wheel.add(1, 1)).toBe(5);
    expect(wheel.add(1, 2)).toBe(40);
    expect(wheel.add(Number.NaN)).toBe(0);
  });
  it('keeps the target inside the song', () => {
    expect(seekTarget(3, -10, 200)).toBe(0);
    expect(seekTarget(195, 10, 200)).toBe(199);
    expect(seekTarget(60, 15, 200)).toBe(75);
    expect(seekTarget(60, 15, null)).toBe(75);
  });
});
