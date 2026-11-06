<script lang="ts">
  import {onDestroy, untrack} from 'svelte';
  import {attachArtworkVideo} from '../island/artworkPlayback';
  import {validArtworkUrl} from '../island/artwork';
  import {attachVideoSync, type VideoSyncClock} from '../island/videoSync';
  import type {VideoQuality} from '../island/videoQuality';
  let { src, playing, clock, active=true,quality='best', onerror,onready }: { src: string; playing: boolean;clock?:VideoSyncClock;active?:boolean;quality?:VideoQuality; onerror?: () => void;onready?:(ready:boolean)=>void } = $props();
  let video = $state<HTMLVideoElement>();
  let ready = $state(false);
  let synchronizer = $state<ReturnType<typeof attachVideoSync>>();
  let disposed=false;
  let sourceRevision=0;
  $effect(()=>{
    const element=video, source=src, requestedQuality=quality;
    if(!element)return;
    const revision=++sourceRevision;
    let attached=true,failed=false;
    const publishReady=(value:boolean)=>{
      if(!attached||disposed||revision!==sourceRevision)return;
      ready=value;untrack(()=>onready?.(value));
    };
    const fail=()=>{
      if(!attached||disposed||failed||revision!==sourceRevision)return;
      failed=true;publishReady(false);untrack(()=>onerror?.());
    };
    const loaded=()=>{if(!failed){sync.update();if(!element.seeking&&element.readyState>=2)publishReady(true);}};
    publishReady(false);
    element.addEventListener('loadeddata',loaded);
    element.addEventListener('seeked',loaded);
    element.addEventListener('error',fail);
    const sync=untrack(()=>attachVideoSync(element,()=>clock??{key:source,playing}, {active,onerror:fail}));
    synchronizer=sync;
    let release:()=>void;
    if(validArtworkUrl(source))release=attachArtworkVideo(element,source,fail,requestedQuality);
    else {
      element.src=source;element.load();
      release=()=>{element.pause();element.removeAttribute('src');element.load();};
    }
    return()=>{
      attached=false;++sourceRevision;
      element.removeEventListener('loadeddata',loaded);element.removeEventListener('seeked',loaded);element.removeEventListener('error',fail);
      sync.stop();release();
    };
  });
  onDestroy(()=>{disposed=true;++sourceRevision;synchronizer?.stop();onready?.(false);});
  $effect(() => {
    clock;playing;active;synchronizer?.setActive(active);synchronizer?.update();
  });
</script>

<div class="backdrop" class:ready={ready&&active} aria-hidden="true">
  {#key `${src}|${quality}`}
    <video bind:this={video} muted loop playsinline preload="auto"></video>
  {/key}
</div>

<style>
  .backdrop { position: absolute; inset: 1px; border-radius: inherit; overflow: hidden; pointer-events: none; opacity: 0; }
  .backdrop.ready { opacity: 1; transition: opacity 240ms ease; }
  .backdrop::after { content: ""; position: absolute; inset: 0; background: linear-gradient(0deg, oklch(0.12 0.015 255 / 0.86), oklch(0.13 0.015 255 / 0.30)); }
  video { display: block; width: 100%; height: 100%; object-fit: cover; }
</style>
