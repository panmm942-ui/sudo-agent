export type VideoQuality = 'best' | '2160' | '1080' | '720' | '480';

export type VideoLevel = { width: number; height: number; bitrate: number };

/** Select from codec-supported HLS levels. Caps apply to the longer edge so
 * portrait and square artwork obey the same requested maximum as landscape.
 * If every rendition exceeds the cap, use the smallest available rendition.
 */
export function selectVideoLevel(levels: readonly VideoLevel[], quality: VideoQuality = 'best'): number {
  if (!levels.length) return -1;
  const cap = quality === 'best' ? Infinity : Number(quality);
  const edge = (level: VideoLevel) => Math.max(level.width, level.height);
  const withinCap = levels.some(level => edge(level) <= cap);
  let selected = -1;
  for (let index = 0; index < levels.length; index++) {
    const level = levels[index];
    if (withinCap && edge(level) > cap) continue;
    const previous = selected >= 0 ? levels[selected] : undefined;
    if (!previous) { selected = index; continue; }
    // Retain the original best-quality pixel/bitrate ordering. When no level
    // meets a cap, first minimize the amount by which the longer edge exceeds it.
    if (!withinCap && edge(level) !== edge(previous)) {
      if (edge(level) < edge(previous)) selected = index;
      continue;
    }
    const pixels = level.width * level.height;
    const previousPixels = previous.width * previous.height;
    if (pixels > previousPixels || (pixels === previousPixels && level.bitrate > previous.bitrate)) selected = index;
  }
  return selected;
}
