<script lang="ts">
  import {onMount, onDestroy} from 'svelte';
  import {invoke} from '@tauri-apps/api/core';
  type Last={coldStart?:boolean;source?:string;lang?:string;asrMs?:number;ttftMs?:number;firstChunkMs?:number;eosToAudioMs?:number;firstSynthMs?:number;llmMs?:number;tokens?:number};
  type Status={phase:string;message?:string;installed?:boolean;enabled?:boolean;wake?:boolean;orbClick?:boolean;hotkey?:boolean;keepLoaded?:boolean;aiLoaded?:boolean;llmReady?:boolean;voiceReady?:boolean;dataRoot?:string;last?:Last|null;routerInstalled?:boolean;routerRunning?:boolean;routerError?:string|null};
  let {hoverExpand=true,onhoverexpand}:{hoverExpand?:boolean;onhoverexpand?:(value:boolean)=>void}=$props();
  type Setup={phase:string;message?:string;busy?:boolean;step?:number;steps?:number;done?:number;total?:number};
  type Check={id:string;ok:boolean;blocking:boolean;label:string;detail:string};
  type Preflight={ready:boolean;checks:Check[]};
  let status=$state<Status>({phase:'checking'}), setup=$state<Setup>({phase:'idle'}), preflight=$state<Preflight|null>(null);
  let microphones=$state<string[]>([]), microphone=$state(''), defaultInput=$state('');
  let busy=$state(false), checking=$state(false), error=$state('');
  let alive=true, timer:ReturnType<typeof setInterval>|undefined;
  const on=$derived(!!status.enabled);
  const last=$derived(status.last && status.last.ttftMs!==undefined ? status.last : null);
  const percent=$derived(setup.total ? Math.min(100, Math.round((setup.done ?? 0) / setup.total * 100)) : null);
  function message(e:unknown){return e instanceof Error?e.message:String(e);}
  async function refresh(){
    try{
      const [next,nextSetup]=await Promise.all([invoke<Status>('jarvis_status'),invoke<Setup>('jarvis_setup_status')]);
      if(!alive)return;
      const justInstalled=!status.installed&&next.installed;
      status=next;setup=nextSetup;
      if(justInstalled)void loadMicrophones();
    }catch(e){if(alive)error=message(e);}
  }
  async function check(){
    checking=true;
    try{const result=await invoke<Preflight>('jarvis_preflight');if(alive)preflight=result;}
    catch(e){if(alive)error=message(e);}
    finally{if(alive)checking=false;}
  }
  async function loadMicrophones(){
    try{
      const result=await invoke<{microphones:string[];defaultInput:string;selected:string|null}>('jarvis_microphones');
      if(!alive)return;
      microphones=result.microphones;defaultInput=result.defaultInput;microphone=result.selected??'';
    }catch{/* listed again after setup */}
  }
  async function act(run:()=>Promise<unknown>){
    busy=true;error='';
    try{await run();}catch(e){if(alive)error=message(e);}
    finally{await refresh();if(alive)busy=false;}
  }
  const enable=(enabled:boolean,wake=!!status.wake)=>act(()=>invoke('jarvis_set_enabled',{enabled,wake}));
  /** Ways to start Sudo AI; any combination, but never none. */
  const triggers=$derived({wake:!!status.wake,orbClick:status.orbClick!==false,hotkey:status.hotkey!==false});
  const onlyOne=$derived([triggers.wake,triggers.orbClick,triggers.hotkey].filter(Boolean).length===1);
  const setTrigger=(name:'wake'|'orbClick'|'hotkey',value:boolean)=>act(()=>invoke('jarvis_set_triggers',{...triggers,[name]:value}));
  onMount(()=>{
    void (async()=>{await refresh();if(status.installed)await loadMicrophones();else await check();})();
    timer=setInterval(()=>void refresh(),800);
  });
  onDestroy(()=>{alive=false;clearInterval(timer);});
  const ms=(v?:number)=>v===undefined||v===null?'–':v>=1000?`${(v/1000).toFixed(2)} s`:`${v} ms`;
  const gb=(v?:number)=>((v??0)/1e9).toFixed(2);
</script>

<section class="jarvis" aria-label="Jarvis local assistant">
  <p class="intro">Talk to your computer in Greek or English. Hold <b>Right Ctrl</b>, speak, and Sudo AI answers aloud in the same language. Everything runs on this computer; nothing you say is sent online.</p>

  {#if setup.busy}
    <div class="progress" role="status" aria-live="polite">
      <strong>{#if setup.step}Step {setup.step} of {setup.steps}: {/if}{setup.message}</strong>
      <div class="bar" class:indeterminate={percent===null}><span style:width={percent===null?undefined:`${percent}%`}></span></div>
      <small>{#if percent!==null}{gb(setup.done)} of {gb(setup.total)} GB · {percent}%{:else}Working… this step shows no byte count{/if}. You can keep using Sudo; closing it pauses setup and the next try resumes.</small>
      <button type="button" class="pill-btn" disabled={busy||setup.phase==='cancelling'} onclick={()=>void act(()=>invoke('jarvis_setup_cancel'))}>Cancel</button>
    </div>
  {:else if status.phase!=='checking' && !status.installed}
    <div class="setup">
      <strong>Set up Sudo AI</strong>
      <p>One click downloads and installs everything Sudo AI needs (about 7 GB), checks every file, and turns Sudo AI on. It takes 5–20 minutes depending on your internet.</p>
      {#if preflight}
        <ul class="checks" aria-label="Computer check">
          {#each preflight.checks as item (item.id)}
            <li class:fail={!item.ok} class:warn={!item.ok&&!item.blocking}><span aria-hidden="true">{item.ok?'✓':item.blocking?'✕':'!'}</span><span>{item.label}{#if item.detail}<small>{item.detail}</small>{/if}</span></li>
          {/each}
        </ul>
      {:else}<p class="muted">Checking this computer…</p>{/if}
      <div class="actions">
        <button type="button" class="pill-btn primary" disabled={busy||checking||!preflight?.ready} onclick={()=>void act(()=>invoke('jarvis_setup_start'))}>Set up Sudo AI</button>
        {#if preflight && !preflight.ready}<button type="button" class="pill-btn" disabled={checking} onclick={()=>void check()}>{checking?'Checking…':'Check again'}</button>{/if}
      </div>
      {#if setup.phase==='error'||setup.phase==='cancelled'}<p role="status" class:error={setup.phase==='error'}>{setup.message} Press Set up Sudo AI to continue where it stopped.</p>{/if}
    </div>
  {/if}

  {#if status.installed}
    <label class="row switch"><span><strong>Sudo AI</strong><small>Starts with Sudo and listens for you. The AI itself loads when you call it and unloads after 5 quiet minutes.</small></span>
      <input type="checkbox" role="switch" aria-label="Enable Sudo AI" checked={on} disabled={busy||setup.busy} onchange={e=>void enable(e.currentTarget.checked)}/></label>
    <p role="status" class="state" class:error={status.phase==='error'||!!error} class:ready={status.phase==='ready'}>{error||status.message}</p>
    <label class="row"><span><strong>Microphone</strong><small>Pick the one you speak into.</small></span>
      <select aria-label="Jarvis microphone" value={microphone} disabled={busy} onchange={e=>{microphone=e.currentTarget.value;void act(()=>invoke('jarvis_set_microphone',{name:microphone||null}));}}>
        <option value="">Windows default{defaultInput?` (${defaultInput.trim()})`:''}</option>
        {#each microphones as name (name)}<option value={name}>{name}</option>{/each}
      </select></label>
    <h4 class="group">Start Sudo AI by</h4>
    {#snippet trigger(name:'wake'|'orbClick'|'hotkey',title:string,help:string)}
      <label class="row switch"><span><strong>{title}</strong><small>{help}</small></span>
        <input type="checkbox" role="switch" aria-label={title} checked={triggers[name]} disabled={busy||!on||(triggers[name]&&onlyOne)} onchange={e=>void setTrigger(name,e.currentTarget.checked)}/></label>
    {/snippet}
    {@render trigger('wake','Saying “Hey Sudo”','Hands-free. A tiny detector listens on the CPU; the AI loads only when it hears you.')}
    {@render trigger('orbClick','Clicking the orb','Hover the island and click the orb on Home, then speak.')}
    {@render trigger('hotkey','Holding Right Ctrl','Push to talk: hold while you speak, release to send.')}
    {#if onlyOne}<p class="muted">At least one way to start Sudo AI stays on.</p>{/if}
    {#if onhoverexpand}
      <label class="row switch"><span><strong>Open the small island only when clicked</strong><small>Hovering no longer opens it. With orb clicking on, a small orb appears on the island: click it to talk without opening Home. Same as Island → Expand on hover.</small></span>
        <input type="checkbox" role="switch" aria-label="Open the small island only when clicked" checked={!hoverExpand} onchange={e=>onhoverexpand(!e.currentTarget.checked)}/></label>
    {/if}
    <label class="row switch"><span><strong>Keep AI loaded</strong><small>Fastest replies, but about 3.5 GB of video memory stays in use. Off: the first answer after a pause takes a few seconds longer.</small></span>
      <input type="checkbox" role="switch" aria-label="Keep AI loaded" checked={!!status.keepLoaded} disabled={busy||!on} onchange={e=>void act(()=>invoke('jarvis_set_keep_loaded',{keep:e.currentTarget.checked}))}/></label>
    <h4 class="group">Fast commands</h4>
    <p class="muted">“Take a screenshot”, “open Spotify”, “type …”, “next song”, “volume up” (and the same in Greek) run at once on this computer, without waiting for the AI.</p>
    {#if status.routerInstalled}
      {#if status.routerError}<p class="state error" role="status">{status.routerError}</p>{/if}
      <p class="state ready">Any wording works too, such as “could you grab my screen” or “θέλω να ανοίξεις το Steam”: GLiNER on the CPU recognises the command in about 0.2 s. It loads and unloads with the AI (about 1.5 GB of memory while loaded).</p>
    {:else if !setup.busy}
      <div class="row"><span><strong>Understand any wording</strong><small>Adds GLiNER, a small classifier that runs on the CPU, so commands work however you phrase them. About 1.4 GB download, checked like the rest; about 1.5 GB of memory while Sudo AI is awake. Without it, the exact phrases above still work.</small></span>
        <button type="button" class="pill-btn" disabled={busy} onclick={()=>void act(()=>invoke('jarvis_setup_start',{component:'router'}))}>Add</button></div>
      {#if setup.phase==='error'||setup.phase==='cancelled'}<p role="status" class:error={setup.phase==='error'}>{setup.message}</p>{/if}
    {/if}
    {#if on}<div class="actions"><button type="button" class="pill-btn" disabled={busy} onclick={()=>void act(()=>invoke('jarvis_clear_history'))}>Forget this conversation</button></div>{/if}
    {#if last}
      <p class="metrics tabular" aria-label="Last answer timing">Last {last.source==='voice'?'spoken':'typed'} answer ({last.lang==='el'?'Greek':'English'}): {#if last.source==='voice'}speech end → first audio {ms(last.eosToAudioMs)}{#if last.coldStart} (AI was loading){/if} · recognition {ms(last.asrMs)} · {/if}first token {ms(last.ttftMs)} · first voice chunk {ms(last.firstSynthMs)}</p>
    {/if}
  {/if}
  {#if status.dataRoot}<details><summary>Storage and logs</summary><p class="path">{status.dataRoot}</p><small>jarvis-setup.log, jarvis-llm.log, jarvis-voice.log, jarvis-router.log, jarvis-metrics.jsonl (timing of every answer) and sudo-crash.log (any unexpected error, to send when reporting a problem).</small></details>{/if}
</section>

<style>
 .jarvis{padding:4px 0 8px;font:var(--type-body)}
 p,small{font:var(--type-caption);line-height:18px;opacity:.8}p{margin:7px 0 12px}small{display:block;margin-top:3px}.intro{opacity:.9}
 .row{display:flex;align-items:center;justify-content:space-between;gap:12px;min-height:48px;padding:9px 0;border-bottom:1px solid #ffffff12}.row>span{min-width:0}strong{font-weight:560}
 select{min-width:100px;max-width:190px;min-height:32px;border:1px solid #ffffff25;border-radius:9px;background:var(--surface-control);color:inherit;color-scheme:dark;font:var(--type-caption)}option{background:#24262d;color:#f5f5f5}
 .switch input{appearance:none;width:34px;height:20px;flex:none;margin:0;border:0;border-radius:20px;background:oklch(.55 .01 255);position:relative;cursor:pointer;transition:background-color 160ms ease}
 .switch input::before{content:"";position:absolute;width:14px;height:14px;top:3px;left:3px;border-radius:50%;background:var(--text-primary);transition:transform 160ms ease}
 .switch input:checked{background:var(--accent)}.switch input:checked::before{transform:translateX(14px);background:var(--on-accent)}input:disabled,button:disabled,select:disabled{opacity:.4;cursor:default}
 .setup,.progress{border-left:2px solid #ffffff36;padding-left:10px;margin:12px 0 16px}.progress{display:grid;gap:8px}.progress button{justify-self:start}
 .checks{list-style:none;margin:8px 0 12px;padding:0;display:grid;gap:6px}.checks li{display:flex;gap:8px;font:var(--type-caption)}.checks li>span:first-child{width:14px;flex:none;color:oklch(.8 .15 150)}
 .checks li.fail>span:first-child{color:#ffb4b4}.checks li.warn>span:first-child{color:oklch(.85 .13 85)}.checks small{opacity:.75}
 .bar{height:6px;border-radius:3px;background:#ffffff1c;overflow:hidden}.bar span{display:block;height:100%;background:var(--accent);border-radius:3px;transition:width 600ms ease}
 .bar.indeterminate span{width:30%;animation:slide 1.4s ease-in-out infinite}@keyframes slide{from{transform:translateX(-100%)}to{transform:translateX(340%)}}
 .actions{display:flex;gap:8px;flex-wrap:wrap}.primary{font-weight:600}.muted{opacity:.6}.error{color:#ffc9c9;opacity:1}.state.ready{opacity:.9}.metrics{opacity:.7}.group{margin:18px 0 2px;font:600 11px/16px var(--font-sans);letter-spacing:.065em;text-transform:uppercase;opacity:.62}
 .path{overflow-wrap:anywhere;user-select:text}details{margin:14px 0;font:var(--type-caption)}summary{cursor:pointer}button:focus-visible,summary:focus-visible,input:focus-visible,select:focus-visible{outline:2px solid var(--accent);outline-offset:3px}
 @media(prefers-reduced-motion:reduce){.switch input,.switch input::before,.bar span{transition:none}.bar.indeterminate span{animation:none;width:100%;opacity:.5}}
</style>
