<script lang="ts">
  import { advanceWave, waveHeights, waveScale, type SpotifySpectrum, type WaveStyle } from '../island/waveform';

  let { playing, waves=true, level=null, sensitivity=1.6, style='classic', reactivity=40, smoothness=65, reduceMotion=false }: {
    playing: boolean;
    waves?: boolean;
    level?: SpotifySpectrum | null;
    sensitivity?: number;
    style?: WaveStyle;
    reactivity?: number;
    smoothness?: number;
    reduceMotion?: boolean;
  } = $props();

  const bars = $derived(waveHeights(style));
  const hasInput = $derived(level !== null && level.length >= 7 && level.slice(0,7).every(Number.isFinite));
  let element = $state<HTMLSpanElement>();
  let systemReducedMotion = $state(false);
  const animate = $derived(waves && playing && hasInput && !reduceMotion && !systemReducedMotion && reactivity > 0);

  $effect(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)');
    systemReducedMotion = media.matches;
    const changed = () => { systemReducedMotion = media.matches; };
    media.addEventListener('change', changed);
    return () => media.removeEventListener('change', changed);
  });

  $effect(() => {
    if (!element) return;
    const nodes = Array.from(element.children) as HTMLElement[];
    if (!animate) {
      for (const node of nodes) node.style.setProperty('--level', '.28');
      return;
    }
    const values = new Float64Array(7), previous = new Float64Array(7);
    let raf = 0, last = 0;
    const frame = (time: number) => {
      // The only changing input is the latest measured spectrum; no generated motion.
      advanceWave(values, level, last ? (time-last)/1000 : 1/60, sensitivity, smoothness);
      last = time;
      for (let i=0; i<nodes.length; i++) {
        const value = waveScale(values[i], reactivity);
        if (Math.abs(value-previous[i]) > .0005) {
          nodes[i].style.setProperty('--level', value.toFixed(4));
          previous[i] = value;
        }
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  });
</script>

{#if waves}
  <span bind:this={element} class="spectrum" role="img" aria-label={!hasInput ? 'Spotify audio spectrum unavailable' : playing ? 'Spotify live audio spectrum' : 'Spotify audio spectrum paused'}>
    {#each bars as height,index (index)}<i style:height={`${height}px`}></i>{/each}
  </span>
{/if}

<style>
  .spectrum { display:flex;align-items:center;justify-content:center;gap:2px;width:34px;height:30px; }
  .spectrum i { flex:none;width:3px;border-radius:var(--radius-pill);background:currentColor;transform-origin:center;transform:scaleY(var(--level,.28)); }
  @media (prefers-reduced-motion:reduce) { .spectrum i { transform:scaleY(.28); } }
</style>
