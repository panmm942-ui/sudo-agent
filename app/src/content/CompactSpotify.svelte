<script lang="ts">
  import {advanceWave,waveHeights,waveScale, type SpotifySpectrum,type WaveStyle} from '../island/waveform';
  import type { SpotifyTrack } from "../island/spotify";
  import Marquee from "./Marquee.svelte";
  let { orb = false, marquee = true, track, waves, reduceMotion, onclick, reactive = false, level = null,sensitivity=1.6,style='classic',reactivity=40,smoothness=65 }: {
    style?:WaveStyle;reactivity?:number;smoothness?:number;
    sensitivity?:number;
    marquee?: boolean;
    track: SpotifyTrack; waves: boolean; orb?: boolean; reduceMotion: boolean; onclick: () => void;
    reactive?: boolean; level?: SpotifySpectrum | null;
  } = $props();
  const bars = $derived(waveHeights(style));
  let waveElement=$state<HTMLSpanElement>();
  const animateReactive=$derived(waves&&reactive&&!reduceMotion&&track.playing&&reactivity>0);
  $effect(()=>{
    const element=waveElement;
    if(!element)return;
    const nodes=Array.from(element.children) as HTMLElement[];
    if(!animateReactive){for(const node of nodes)node.style.setProperty('--level','.28');return;}
    const values=new Float64Array(7),previous=new Float64Array(7);
    let raf=0,last=0;
    const frame=(t:number)=>{
      advanceWave(values,level,last?(t-last)/1000:1/60,sensitivity,smoothness);last=t;
      for(let i=0;i<nodes.length;i++){
        const value=waveScale(values[i],reactivity);
        if(Math.abs(value-previous[i])>.0005){nodes[i].style.setProperty('--level',value.toFixed(4));previous[i]=value;}
      }
      raf=requestAnimationFrame(frame);
    };
    raf=requestAnimationFrame(frame);return()=>cancelAnimationFrame(raf);
  });
</script>

<button type="button" class="mini" class:with-orb={orb} aria-label={`Expand Spotify · ${track.title || "No track selected"}`} {onclick}>
  {#if track.artwork}<img src={track.artwork} width="32" height="32" alt="" />
  {:else}<span class="empty" aria-hidden="true">♪</span>{/if}
  <span class="copy"><strong><Marquee text={track.title || "Spotify"} enabled={marquee} /></strong><small><Marquee text={track.artist || "No track selected"} enabled={marquee} /></small></span>
  {#if waves && !orb}
    <span bind:this={waveElement} class="wave" class:playing={track.playing} class:still={reduceMotion} class:reactive aria-label={reactive && level===null ? "Spotify audio level unavailable" : undefined} aria-hidden="true">
      {#each bars as height,index (index)}<i style:height={`${height}px`}></i>{/each}
    </span>
  {/if}
</button>

<style>
  /* The Sudo orb sits at the right end while the song is paused. */
  .mini.with-orb { padding-right: 34px; }
  .mini { display: flex; align-items: center; gap: 9px; width: 100%; height: 100%; padding: 0 13px 0 7px; border: 0; border-radius: var(--radius-pill); background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .mini:focus-visible { outline-offset: -3px; }
  img, .empty { flex: none; width: 32px; height: 32px; object-fit: cover; border-radius: 10px; }
  .empty { display: grid; place-items: center; background: var(--surface-control); }
  .copy { display: grid; flex: 1; min-width: 0; }
  strong, small { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  strong { font: 620 13px/16px var(--font-sans); }
  small { font: 500 11px/13px var(--font-sans); opacity: 0.82; }
  .wave { display: flex; align-items: center; justify-content: center; width: 34px; gap: 2px; height: 22px; flex: none; }
  .wave i { width: 2px; height: 22px; background: currentColor; border-radius: var(--radius-pill); opacity: 0.8; transform: scaleY(0.14) translateZ(0); transform-origin: center; will-change: transform; }
  .wave i:nth-child(2n) { opacity: 0.58; }
  .wave.reactive i { animation: none; transform: translateZ(0) scaleY(var(--level)); transition: none; }
  .wave.reactive.still i { transform: scaleY(0.34); transition: none; }
  @media (prefers-reduced-motion: reduce) { .wave i { animation: none; } .wave.reactive i { transform: scaleY(0.34); transition: none; } }
</style>
