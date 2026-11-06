import { describe, expect, it } from 'vitest';
import { selectVideoLevel } from './videoQuality';

describe('supported artwork rendition selection', () => {
  const levels = [
    { width: 1080, height: 1080, bitrate: 5_000_000 },
    { width: 480, height: 480, bitrate: 500_000 },
    { width: 2160, height: 2160, bitrate: 20_000_000 },
    { width: 720, height: 720, bitrate: 2_000_000 },
    { width: 2160, height: 2160, bitrate: 28_000_000 },
  ];
  it('chooses the highest available resolution and bitrate from unsorted levels by default', () => {
    expect(selectVideoLevel(levels)).toBe(4);
    expect(selectVideoLevel(levels, '2160')).toBe(4);
  });
  it('honors each cap without depending on playlist order', () => {
    expect(selectVideoLevel(levels, '1080')).toBe(0);
    expect(selectVideoLevel(levels, '720')).toBe(3);
    expect(selectVideoLevel(levels, '480')).toBe(1);
  });
  it('uses the highest available lower rendition when the exact cap is absent', () => {
    expect(selectVideoLevel([levels[2], levels[1], levels[3]], '1080')).toBe(2);
  });
  it('uses the lowest available long edge, then higher bitrate when all renditions exceed a cap', () => {
    expect(selectVideoLevel([levels[2], levels[0], { ...levels[0], bitrate: 8_000_000 }], '480')).toBe(2);
  });
  it('measures portrait and landscape streams by their longer edge', () => {
    expect(selectVideoLevel([
      { width: 720, height: 1280, bitrate: 5_000_000 },
      { width: 1080, height: 600, bitrate: 3_000_000 },
      { width: 720, height: 720, bitrate: 2_000_000 },
    ], '1080')).toBe(1);
  });
  it('reports no playable variant for an empty supported manifest', () => {
    expect(selectVideoLevel([], 'best')).toBe(-1);
  });
});
