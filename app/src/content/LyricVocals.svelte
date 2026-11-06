<script lang="ts">
 import {onMount,onDestroy} from 'svelte';
 import {invoke} from '@tauri-apps/api/core';
 let {volume,onchange,onlatency}:{volume:number;onchange?:(volume:number)=>void;onlatency:(seconds:number)=>void}=$props();
 type Status={phase:string;active?:boolean;latency?:number;message?:string;runtimeReady?:boolean;driverReady?:boolean};
 type Device={id:number;name:string;kind:'input'|'output'};
 let open=$state(false),status=$state<Status>({phase:'checking'}),error=$state(''),toggling=$state(false);
 let alive=true,reading=false,sending=false,pending:number|undefined;
 const running=$derived(!!status.active&&status.phase==='running');
 const starting=$derived(!!status.active&&!running);
 // Set up once in Settings → Spotify; after that the island can switch vocals on and off itself.
 const prepared=$derived(!!status.runtimeReady&&!!status.driverReady);
 async function refresh(){
  if(reading)return;reading=true;
  try{const next=await invoke<Status>('vocal_status');if(alive){status=next;onlatency(next.active&&next.phase==='running'?Math.max(0,next.latency??0):0);}}
  catch{if(alive){status={phase:'off'};onlatency(0);}}
  finally{reading=false;}
 }
 async function setVolume(value:number){
  onchange?.(value);
  if(!running)return;
  pending=value;if(sending)return;sending=true;error='';
  try{while(pending!==undefined&&alive){const next=pending;pending=undefined;await invoke('vocal_volume',{volume:next});}}
  catch(cause){pending=undefined;if(alive){error=String(cause);void refresh();}}
  finally{sending=false;}
 }
 async function toggle(){
  if(toggling)return;toggling=true;error='';
  try{
   if(status.active||status.phase==='error'){await invoke('vocal_stop');}
   else{
    const result=await invoke<{devices:Device[];defaultOutput?:number;defaultIsCable?:boolean}>('vocal_devices');
    const input=result.devices.find(d=>d.kind==='input')?.id;
    const output=result.devices.find(d=>d.kind==='output'&&d.id===result.defaultOutput)?.id??result.devices.find(d=>d.kind==='output')?.id;
    if(input===undefined)throw new Error('The virtual cable is missing. Repair it in Settings → Spotify.');
    if(output===undefined||result.defaultIsCable)throw new Error('Choose your speakers or headphones as the Windows output, then try again.');
    await invoke('vocal_start',{input,output,volume:Math.round(volume)});
   }
  }catch(cause){if(alive)error=cause instanceof Error?cause.message:String(cause);}
  finally{await refresh();if(alive)toggling=false;}
 }
 onMount(()=>{
  void refresh();const timer=setInterval(()=>void refresh(),300);
  const close=(event:KeyboardEvent)=>{if(open&&event.key==='Escape'){event.stopImmediatePropagation();open=false;}};
  window.addEventListener('keydown',close,true);
  return()=>{clearInterval(timer);window.removeEventListener('keydown',close,true);};
 });
 onDestroy(()=>{alive=false;pending=undefined;onlatency(0);});
 const note=$derived(error||(running?'':starting?(status.message||'Preparing vocal control…'):status.phase==='error'?(status.message||'Vocal control stopped.'):prepared?'':'Set up vocal control once in Settings → Spotify.'));
</script>
<div class="vocal-control">
 <button type="button" class="mic" aria-label="Vocal level" aria-expanded={open} aria-controls="lyric-vocal-panel" class:enabled={running} onclick={()=>open=!open}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3" stroke="currentColor" stroke-width="1.6"/><path d="M6 11v1a6 6 0 0012 0v-1M12 18v3M9 21h6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg></button>
 {#if open}<div class="panel" id="lyric-vocal-panel" role="region" aria-label="Vocal controls">
  {#if prepared||status.active}
   <div class="head"><span>Vocals <output>{volume}%</output></span>
    <button type="button" class="power" class:on={status.active} role="switch" aria-checked={!!status.active} aria-label="Vocal control" disabled={toggling} onclick={toggle}><span></span></button>
   </div>
   <input id="lyric-vocal-level" type="range" min="0" max="100" step="1" value={volume} aria-label="Lyrics vocal level" oninput={event=>void setVolume(event.currentTarget.valueAsNumber)}/>
   <small role="status">{note||(running?'0 % instrumental · 100 % original':'Turn on to change the vocal level live.')}</small>
  {:else}
   <strong>Vocals</strong>
   <small role="status">{note}</small>
  {/if}
 </div>{/if}
</div>
<style>
 .vocal-control{position:relative;flex:none;margin-left:auto}
 .mic{width:28px;height:28px;display:grid;place-items:center;padding:4px;color:inherit;background:transparent;border:0;border-radius:8px;cursor:pointer}
 .mic svg{width:19px;height:19px}.mic:hover,.mic[aria-expanded=true]{background:var(--surface-control-hover)}
 .mic.enabled{color:var(--on-accent);background:var(--accent)}
 .panel{position:absolute;right:0;top:34px;z-index:20;width:210px;box-sizing:border-box;padding:12px;border:1px solid var(--border-subtle,#ffffff30);border-radius:12px;background:var(--surface-solid,#24262d);box-shadow:0 8px 24px #0006;color:var(--text-primary);font:var(--type-caption)}
 .head{display:flex;align-items:center;justify-content:space-between;gap:10px;font-weight:600}
 .head output{margin-left:6px;font-variant-numeric:tabular-nums;opacity:.8}
 .power{position:relative;flex:none;width:34px;height:20px;padding:0;border:0;border-radius:10px;background:var(--indicator-idle,#ffffff30);cursor:pointer;transition:background .2s}
 .power span{position:absolute;top:2px;left:2px;width:16px;height:16px;border-radius:50%;background:#fff;transition:transform .2s}
 .power.on{background:var(--accent)}.power.on span{transform:translateX(14px)}
 .power:disabled{opacity:.6;cursor:default}
 .panel input{width:100%;min-height:28px;margin:6px 0 2px;accent-color:var(--accent)}
 strong{display:block;font-weight:600;margin-bottom:4px}
 small{display:block;line-height:1.4;opacity:.8}
 .mic:focus-visible,.panel input:focus-visible,.power:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
</style>
