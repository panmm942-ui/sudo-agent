/** Seconds moved per mouse-wheel notch when scrolling over fullscreen lyrics. */
export const SEEK_STEP_SECONDS = 5;
const NOTCH_PIXELS = 100;

/**
 * Turns wheel input into whole seek steps. Scrolling down moves forward, up moves back.
 * Touchpads send many small deltas, so the remainder carries over between events.
 */
export function createWheelSeek(step = SEEK_STEP_SECONDS) {
  let carry = 0;
  return {
    add(deltaY: number, deltaMode = 0): number {
      if (!Number.isFinite(deltaY)) return 0;
      // deltaMode 1 = lines, 2 = pages; 0 = pixels.
      carry += deltaMode === 1 ? deltaY * 33 : deltaMode === 2 ? deltaY * 800 : deltaY;
      const notches = Math.trunc(carry / NOTCH_PIXELS);
      carry -= notches * NOTCH_PIXELS;
      return notches * step;
    },
    reset(): void { carry = 0; },
  };
}

/** Where to land, kept inside the song; the last second is avoided so Spotify doesn't skip to the next track. */
export function seekTarget(from: number, offset: number, duration: number | null | undefined): number {
  const end = typeof duration === 'number' && Number.isFinite(duration) && duration > 0 ? Math.max(0, duration - 1) : Infinity;
  return Math.min(end, Math.max(0, from + offset));
}
