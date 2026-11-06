import { expect, it } from 'vitest';
import { advanceWave, waveHeights, waveScale } from './waveform';

it('restores the classic contour and makes motion depth independently adjustable',()=>{
 expect(waveHeights('classic')).toEqual([8,14,19,22,19,14,8]);
 expect(waveHeights('spectrum')).toEqual([22,22,22,22,22,22,22]);
 expect(waveScale(1,40)).toBeLessThan(.6);
 expect(waveScale(1,100)).toBe(1);
 expect(waveScale(1,0)).toBe(waveScale(0,0));
});
it('lets the user soften attack without replacing the measured frequencies',()=>{
 const calm=new Float64Array(7),fast=new Float64Array(7);
 for(let i=0;i<5;i++){advanceWave(calm,[1,0,0,0,0,0,0],.016,1.6,100);advanceWave(fast,[1,0,0,0,0,0,0],.016,1.6,0);}
 expect(calm[0]).toBeLessThan(fast[0]-.25);
 expect(calm[1]).toBe(0);
});

it('preserves independent frequency bands so the tallest bar follows the measured spectrum', () => {
  const values = new Float64Array(7);
  for (let frame = 0; frame < 30; frame++) advanceWave(values, [.00001, .00002, .9, .00004, .00003, .00002, .00001], 1 / 60);
  expect(values[2]).toBeGreaterThan(.8);
  expect(values[0]).toBeLessThan(.2);
  for (let frame = 0; frame < 30; frame++) advanceWave(values, [.00001, .00002, .00001, .00004, .00003, .00002, .9], 1 / 60);
  expect(values[6]).toBeGreaterThan(.8);
  expect(values[2]).toBeLessThan(.2);
});

it('shows quiet measured music clearly without normalizing away volume differences', () => {
  const values = new Float64Array(7);
  for (let frame = 0; frame < 60; frame++) advanceWave(values, [.02, .01, .008, .01, .004, .002, .0007], 1 / 60);
  expect(values[0]).toBeGreaterThan(.6);
  expect(values[6]).toBeLessThan(.15);
  expect(values[0]).toBeGreaterThan(values[3]);
});

it('responds within 80ms, releases to silence, and reuses its storage', () => {
  const values = new Float64Array(7);
  const loud = [1, 1, 1, 1, 1, 1, 1];
  expect(advanceWave(values, loud, 1 / 60,1.6,0)).toBe(values);
  for (let frame = 0; frame < 4; frame++) advanceWave(values, loud, .016,1.6,0);
  expect(Math.min(...values)).toBeGreaterThan(.95);
  for (let frame = 0; frame < 100; frame++) advanceWave(values, null, .016,1.6,0);
  expect(Math.max(...values)).toBeLessThan(.00001);
});

it('keeps absolute volume differences, clamps invalid values, and freezes at zero elapsed time', () => {
  const quiet = new Float64Array(7), loud = new Float64Array(7);
  for (let frame = 0; frame < 60; frame++) {
    advanceWave(quiet, [.05, .05, .05, .05, .05, .05, .05], 1 / 60);
    advanceWave(loud, [.5, .5, .5, .5, .5, .5, .5], 1 / 60);
  }
  expect(loud[0]).toBeGreaterThan(quiet[0] + .2);
  const before = [...quiet]; advanceWave(quiet, [1, 1, 1, 1, 1, 1, 1], 0);
  expect([...quiet]).toEqual(before);
  for (let frame = 0; frame < 100; frame++) advanceWave(loud, [NaN, Infinity, -1, 3, 0, 0, 0], 1 / 60);
  expect(loud.every(Number.isFinite)).toBe(true);
  expect(Math.min(...loud)).toBeGreaterThanOrEqual(0);
  expect(Math.max(...loud)).toBeLessThanOrEqual(1);
});
