<script lang="ts">
 import {tick,onDestroy} from 'svelte';
 import {fitLyrics,measureLyrics} from '../island/lyricFit';
 import type {TimedWord} from '../island/lyricTiming';
 let {identity,main,original='',sub='',kind,reduced=false,flow=false,duration=0,scale=1,fitKey='',fullscreen=false,words,position=0,wordSync=false,clock}:{identity:string;main:string;original?:string;sub?:string;kind:'previous'|'current'|'next';reduced?:boolean;flow?:boolean;duration?:number;scale?:number;fitKey?:string;fullscreen?:boolean;words?:TimedWord[];position?:number;wordSync?:boolean;clock?:()=>number}=$props();
 let box=$state<HTMLDivElement>(),body=$state<HTMLDivElement>(),text=$state<HTMLParagraphElement>();
 let lastIdentity:string|undefined,lastKind:string|undefined;
 let animation:Animation|undefined,grow:Animation|undefined;
 onDestroy(()=>{animation?.cancel();grow?.cancel();});
 const motionOff=()=>matchMedia('(prefers-reduced-motion: reduce)').matches;
 $effect(()=>{
  const container=box,content=body,line=text,key=identity,role=kind,lyric=main,translation=sub,quiet=reduced,moving=flow,size=scale,ms=duration;
  void fitKey;
  if(!container||!content||!line)return;
  let disposed=false;
  const fit=()=>{if(!disposed)fitLyrics(container,[{main:lyric,sub:translation}],false,(fullscreen||role==='current'?42:18)*size);};
  // Measure before the refit so a line changing role can grow/shrink smoothly
  // instead of snapping between the small and spotlight sizes.
  const before=parseFloat(getComputedStyle(line).fontSize)||0;
  void tick().then(()=>{
   if(disposed)return;
   fit();
   if(quiet||motionOff()){animation?.cancel();grow?.cancel();}
   if(key!==lastIdentity){
    animation?.cancel();
    if(lastIdentity!==undefined&&!quiet&&!moving&&!motionOff()){
     animation=content.animate([{opacity:.65},{opacity:1}],{duration:120,easing:'cubic-bezier(.2,.75,.25,1)'});
    }
   }else if(role!==lastKind&&lastKind!==undefined&&moving&&ms>0&&!quiet&&!motionOff()){
    const after=parseFloat(getComputedStyle(line).fontSize)||0;
    if(before>0&&after>0&&Math.abs(before/after-1)>.02){
     grow?.cancel();
     grow=line.animate([{transform:`scale(${before/after})`},{transform:'none'}],{duration:ms,easing:'cubic-bezier(.22,1,.36,1)'});
    }
   }
   lastIdentity=key;lastKind=role;
  });
  // Measure the size this line will need in its next slot while the browser is idle,
  // so the line change itself needs no synchronous layout work (it caused dropped frames).
  const nextRole=role==='next'?'current':role==='current'?'previous':null;
  const idle=(callback:()=>void)=>typeof requestIdleCallback==='function'?requestIdleCallback(callback,{timeout:600}):setTimeout(callback,150);
  const cancelIdle=(handle:number)=>typeof cancelIdleCallback==='function'?cancelIdleCallback(handle):clearTimeout(handle);
  const ahead=nextRole?idle(()=>{
   if(disposed)return;
   const slot=container.closest('.lines')?.querySelector<HTMLElement>(`.lyric-row[data-line="${nextRole}"]`);
   if(slot)measureLyrics(slot,[{main:lyric,sub:translation}],false,(fullscreen||nextRole==='current'?42:18)*size);
  }) as number:undefined;
  const observer=new ResizeObserver(fit);observer.observe(container);
  return()=>{disposed=true;observer.disconnect();if(ahead!==undefined)cancelIdle(ahead);};
 });
 const karaoke=$derived(wordSync&&kind==='current'&&!!words?.length);
 const progress=(word:TimedWord,at:number)=>Math.max(0,Math.min(1,(at-word.time)/(word.end-word.time)));
 // Drive the sweep every frame straight on the DOM, so it moves smoothly at the
 // display's refresh rate instead of in 40 ms steps through component state.
 $effect(()=>{
  const line=text,list=words,read=clock;
  if(!karaoke||!line||!list)return;
  let frame=0;
  const paint=()=>{
   const at=read?read():position;
   const spans=line.querySelectorAll<HTMLElement>('.w');
   // Touch a word only when it starts or stops being lit; no per-frame style writes.
   spans.forEach((span,i)=>{const word=list[i];if(!word)return;const lit=progress(word,at)>0;if(span.classList.contains('lit')!==lit)span.classList.toggle('lit',lit);});
   frame=requestAnimationFrame(paint);
  };
  paint();
  return()=>cancelAnimationFrame(frame);
 });
</script>
<div bind:this={box} class="lyric-row" class:flow class:current={kind==='current'} class:previous={kind==='previous'} data-line={kind} aria-current={kind==='current'?'true':undefined}>
 <div bind:this={body} class="body"><p bind:this={text} class="original" class:word-sync={karaoke}>{#if karaoke&&words}{#each words as word,i (i)}<span class="w">{word.text}</span>{/each}{:else}{main||' '}{/if}</p>{#if sub}<p class="translation">{sub}</p>{/if}</div>
</div>
<style>
 .lyric-row{position:relative;height:100%;box-sizing:border-box;min-height:0;min-width:0;overflow:hidden;display:flex;align-items:center;padding-left:10px;opacity:.56;transition:opacity var(--lyric-flow-ms,0ms) cubic-bezier(.22,1,.36,1);}
 .lyric-row.flow{overflow:visible;}
 .previous{opacity:var(--lyric-prev-opacity,.32);}.lyric-row:not(.previous):not(.current){opacity:var(--lyric-next-opacity,.56);}.current{opacity:1;}
 .body{width:100%;min-width:0;}p{margin:0;white-space:normal;overflow-wrap:anywhere;}
 .original{font:500 var(--lyric-size,18px)/var(--lyric-leading,1.16) var(--lyric-font,var(--font-sans));letter-spacing:-.012em;text-align:var(--lyric-align,left);transform-origin:var(--lyric-origin,left center);will-change:transform;}
 .current .original{color:#fff;font-weight:var(--lyric-weight,650);letter-spacing:-.025em;}
 /* Karaoke: every word on the current line is solid white. As a word is sung it
    lifts slightly and picks up a soft light glow; nothing is ever dimmed or grey. */
 .word-sync .w{display:inline-block;white-space:pre-wrap;color:#fff;transform:translateY(.03em);text-shadow:0 0 0 rgb(255 255 255/0);transition:transform .4s cubic-bezier(.22,1,.36,1),text-shadow .4s ease;}
 .word-sync .w:global(.lit){transform:none;text-shadow:0 0 .32em rgb(255 255 255/.5);}
 @media (prefers-reduced-motion: reduce){.word-sync .w{transition:none;transform:none}}
 .translation{font:450 max(10px,calc(var(--lyric-size,20px)*.45))/1.2 var(--lyric-font,var(--font-sans));opacity:.75;margin-top:2px;text-align:var(--lyric-align,left);}
</style>
