/** Seven measured bass-to-treble RMS amplitudes; no generated oscillation or temporal contours. */
export type SpotifySpectrum = readonly number[];
export type WaveStyle='classic'|'spectrum'|'fine';
const CLASSIC=[8,14,19,22,19,14,8] as const;
const SPECTRUM=[22,22,22,22,22,22,22] as const;
const FINE=[12,12,12,12,12,12,12] as const;
export function waveHeights(style:WaveStyle):readonly number[]{return style==='spectrum'?SPECTRUM:style==='fine'?FINE:CLASSIC;}
export function waveScale(energy:number,reactivity:number):number{return .28+Math.max(0,Math.min(1,energy))*.72*Math.max(0,Math.min(100,reactivity))/100;}
export function advanceWave(values: Float64Array, bands: SpotifySpectrum | null, dt: number, sensitivity = 1.6,smoothness=65): Float64Array {
  const seconds = Math.min(.05, Math.max(0, Number.isFinite(dt) ? dt : 0));
  const gain = Math.min(4, Math.max(.25, Number.isFinite(sensitivity) ? sensitivity : 1.6));
  for (let i = 0; i < values.length; i++) {
    const raw = bands?.[i] ?? 0;
    const amplitude = Math.min(1, Math.max(0, Number.isFinite(raw) ? raw : 0));
    // Fixed -65..-10dBFS display range makes quiet music readable while preserving absolute
    // volume and band ordering. A common fixed gain never normalizes each bar to itself.
    const target = amplitude > 0 ? Math.min(1, Math.max(0, (20 * Math.log10(amplitude * gain) + 65) / 55)) : 0;
    const softness=Math.max(0,Math.min(100,Number.isFinite(smoothness)?smoothness:65))/100;
    const tau = target > values[i] ? .018+softness*.10 : .11+softness*.16;
    values[i] += (target - values[i]) * (1 - Math.exp(-seconds / tau));
  }
  return values;
}
