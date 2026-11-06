<script lang="ts">
  import {onMount, onDestroy} from 'svelte';
  import {invoke} from '@tauri-apps/api/core';
  let {volume, onchange}:{volume:number;onchange:(volume:number)=>void}=$props();
  type Status={phase:string;message?:string;runtimeReady?:boolean;driverReady?:boolean;dataRoot?:string;active?:boolean;latency?:number;gpu?:string};
  type Setup={phase:string;message?:string;busy?:boolean;component?:string};
  type Device={id:number;name:string;kind:'input'|'output'};
  let status=$state<Status>({phase:'checking'}), setup=$state<Setup>({phase:'idle'}), devices=$state<Device[]>([]);
  let busy=$state(false), error=$state(''), input=$state(-1), output=$state(-1);
  let alive=true, polling:ReturnType<typeof setInterval>|undefined, reading=false;
  let readingDevices=$state(false);
  let defaultIsCable=$state(false);
  const active=$derived(!!status.active);
  const ready=$derived(!!status.runtimeReady && input>=0 && output>=0);
  const starting=$derived(['loading','benchmarking','stopping'].includes(status.phase));
  function message(e:unknown){return e instanceof Error?e.message:String(e);}
  function syncPolling(){
    if((active||setup.busy) && !polling) polling=setInterval(()=>void refreshStatus(),750);
    if(!active&&!setup.busy&&polling){clearInterval(polling);polling=undefined;}
  }
  async function refreshStatus(){
    if(reading)return;reading=true;
    try{
      const [next,nextSetup]=await Promise.all([invoke<Status>('vocal_status'),invoke<Setup>('vocal_setup_status')]);
      if(!alive)return;
      const setupFinished=setup.busy&&!nextSetup.busy;
      status=next;setup=nextSetup;
      syncPolling();
      if(setupFinished&&status.runtimeReady)void readDevices();
    }catch(e){if(alive)error=message(e);}
    finally{reading=false;}
  }
  async function readDevices(){
    if(readingDevices||!status.runtimeReady)return;
    readingDevices=true;busy=true;error='';
    try{
      const result=await invoke<{devices:Device[];defaultOutput?:number;defaultIsCable?:boolean}>('vocal_devices');
      if(!alive)return;
      devices=result.devices;
      defaultIsCable=!!result.defaultIsCable;
      if(!devices.some(d=>d.id===input&&d.kind==='input'))input=devices.find(d=>d.kind==='input')?.id??-1;
      if(!devices.some(d=>d.id===output&&d.kind==='output'))output=devices.find(d=>d.kind==='output'&&d.id===result.defaultOutput)?.id??-1;
    }catch(e){if(alive)error=message(e);}
    finally{readingDevices=false;if(alive)busy=false;}
  }
  async function refreshDevices(){await refreshStatus();await readDevices();}
  async function toggle(){
    busy=true;error='';
    try{
      if(active||status.phase==='error')await invoke('vocal_stop');
      else await invoke('vocal_start',{input,output,volume:Math.round(volume)});
    }catch(e){if(alive)error=message(e);}
    finally{await refreshStatus();if(alive)busy=false;}
  }
  async function changeVolume(value:number){
    onchange(value);
    if(active)try{await invoke('vocal_volume',{volume:Math.round(value)});}catch(e){if(alive)error=message(e);}
  }
  async function prepare(component:'driver'|'runtime'|'open-driver'){
    busy=true;error='';
    try{await invoke('vocal_setup_start',{component});await refreshStatus();}
    catch(e){if(alive)error=message(e);}
    finally{if(alive)busy=false;}
  }
  async function cancelSetup(){
    busy=true;error='';
    try{await invoke('vocal_setup_cancel');await refreshStatus();}catch(e){if(alive)error=message(e);}
    finally{if(alive)busy=false;}
  }
  onMount(()=>{
    void (async()=>{await refreshStatus();if(status.runtimeReady)await readDevices();})();
  });
  onDestroy(()=>{alive=false;if(polling)clearInterval(polling);});
</script>

<fieldset class="vocals" aria-label="Optional local vocal reduction">
    <legend>Vocal reduction <span>Optional · local GPU</span></legend>
    <p>Turn down the singer while keeping the music. Separation varies by song and adds about 1.7 seconds of playback delay. Requires Windows x64, a compatible NVIDIA GPU and VB-CABLE.</p>
    {#if status.runtimeReady && !active}<button type="button" class="pill-btn" disabled={busy||setup.busy} onclick={()=>void prepare('runtime')}>Repair runtime</button>{/if}
    <p class="readiness" role="status">Runtime: {status.runtimeReady?'installed':'not set up'} · VB-CABLE: {readingDevices?'checking…':input>=0?'detected':status.runtimeReady?'not detected':'checked after runtime setup'} · Processing: {active?status.phase:'off'}</p>
    <p>Installing the cable driver is only the audio connection. Download the optional runtime, choose your output below, then enable vocal reduction. The microphone button in lyrics controls the vocal level while processing is running.</p>
    {#if defaultIsCable}<p role="status" class="error">Windows’ default output is VB-CABLE. Choose your normal speakers or headphones in Windows Sound settings, then select them under Listen on below. Sudo routes only Spotify to the cable while vocal reduction is enabled.</p>{/if}
    {#if !status.runtimeReady && status.phase!=='checking'}
      <div class="setup"><strong>1. Get the optional local runtime</strong><p>Downloads portable Python, GPU libraries and a 67 MB model into your Sudo data folder. The full download can be several GB. Starts only when you choose this button; normal Spotify controls work without it.</p><button type="button" class="pill-btn" disabled={busy||setup.busy||active} onclick={()=>void prepare('runtime')}>Download optional runtime</button></div>
    {/if}
    {#if !status.runtimeReady || input<0}
      <div class="setup"><strong>{status.runtimeReady?'1':'2'}. Set up VB-CABLE</strong><p>{status.driverReady?'The installer is ready.':'Download the official VB-Audio driver package (about 1.3 MB).'} Open its folder, right-click <b>VBCABLE_Setup_x64.exe</b>, choose <b>Run as administrator</b>, then <b>Install Driver</b>. Restart Windows when requested, return here and refresh devices. Keep your speakers or headphones as Windows’ default output.</p>
        <div class="actions"><button type="button" class="pill-btn" disabled={busy||setup.busy||active} onclick={()=>void prepare(status.driverReady?'open-driver':'driver')}>{status.driverReady?'Show driver installer':'Download driver & open folder'}</button>
        {#if status.runtimeReady}<button type="button" class="pill-btn" disabled={busy||setup.busy||active} onclick={()=>void refreshDevices()}>Refresh devices</button>{/if}</div>
        <small>VB-CABLE is separate donationware from <a href="https://vb-audio.com/Cable/" target="_blank" rel="noreferrer">VB-Audio</a>. Sudo opens its folder; you approve the administrator installation yourself.</small>
      </div>
    {/if}
    {#if setup.busy || setup.phase==='ready' || setup.phase==='error' || setup.phase==='cancelled'}
      <p role="status" aria-live="polite" class:error={setup.phase==='error'}>{setup.message}</p>
      {#if setup.busy}<button type="button" class="pill-btn" disabled={busy||setup.phase==='cancelling'} onclick={()=>void cancelSetup()}>Cancel setup</button>{/if}
    {/if}
    {#if status.runtimeReady}
      <label class="row switch"><span><strong>Enable vocal reduction</strong><small>Starts only when enabled. Stops and restores Spotify’s output when disabled or Sudo closes.</small></span>
        <input type="checkbox" role="switch" aria-label="Enable vocal reduction" checked={active} disabled={busy||setup.busy||(!ready&&!active)} onchange={()=>void toggle()}/>
      </label>
      <label class="row"><span><strong>Vocal level <small class="inline">{volume}%</small></strong><small>0% instrumental · 100% original audio</small></span><input type="range" aria-label="Vocal level" min="0" max="100" step="1" value={volume} oninput={e=>void changeVolume(e.currentTarget.valueAsNumber)}/></label>
      <label class="row"><span><strong>Listen on</strong><small>Choose your speakers or headphones before enabling.</small></span><select aria-label="Vocal mode output" bind:value={output} disabled={active||busy||setup.busy}>
        {#if output<0}<option value={-1}>{devices.some(d=>d.kind==='output')?'Choose speakers or headphones':'No output found'}</option>{/if}
        {#each devices.filter(d=>d.kind==='output') as device (device.id)}<option value={device.id}>{device.name}</option>{/each}
      </select></label>
      {#if devices.filter(d=>d.kind==='input').length>1}<label class="row"><strong>Virtual cable</strong><select aria-label="Vocal mode cable" bind:value={input} disabled={active||busy||setup.busy}>{#each devices.filter(d=>d.kind==='input') as device (device.id)}<option value={device.id}>{device.name}</option>{/each}</select></label>{/if}
      {#if input>=0&&!active}<button type="button" class="pill-btn" disabled={busy||setup.busy} onclick={()=>void refreshDevices()}>Refresh devices</button>{/if}
    {/if}
    {#if starting || status.phase==='running' || status.phase==='error' || error}
      <p role="status" class:error={!!error||status.phase==='error'}>{error||status.message}{#if status.phase==='running'&&status.latency} Estimated delay: {status.latency.toFixed(2)} s.{/if}</p>
    {/if}
    {#if status.phase==='error'}<button type="button" class="pill-btn" disabled={busy||setup.busy} onclick={()=>void toggle()}>Restore normal output</button>{/if}
    {#if status.dataRoot}<details><summary>Storage location</summary><p class="path">{status.dataRoot}</p><small>Setup diagnostics: vocal-setup.log. Runtime and model stay on this computer.</small></details>{/if}
</fieldset>

<style>
 .vocals{margin:14px 0 4px;padding:12px 0 8px;border:0;border-top:1px solid #ffffff20;color:inherit;min-width:0;font:var(--type-body)}
 legend{font-weight:600;padding-right:10px}legend span{font:var(--type-caption);opacity:.65;margin-left:7px}
 p,small{font:var(--type-caption);line-height:18px;opacity:.8}p{margin:7px 0 12px}small{display:block;margin-top:3px}.inline{display:inline}
 .row{display:flex;align-items:center;justify-content:space-between;gap:12px;min-height:48px;padding:9px 0;border-bottom:1px solid #ffffff12}.row>span{min-width:0}strong{font-weight:560}
 input[type=range]{width:112px;flex:none;accent-color:var(--accent)}select{min-width:100px;max-width:170px;min-height:32px;border:1px solid #ffffff25;border-radius:9px;background:var(--surface-control);color:inherit;color-scheme:dark;font:var(--type-caption)}option{background:#24262d;color:#f5f5f5}
 .switch input{appearance:none;width:34px;height:20px;flex:none;margin:0;border:0;border-radius:20px;background:oklch(.55 .01 255);position:relative;cursor:pointer;transition:background-color 160ms ease}
 .switch input::before{content:"";position:absolute;width:14px;height:14px;top:3px;left:3px;border-radius:50%;background:var(--text-primary);transition:transform 160ms ease}
 .switch input:checked{background:var(--accent)}.switch input:checked::before{transform:translateX(14px);background:var(--on-accent)}input:disabled,select:disabled,button:disabled{opacity:.4;cursor:default}
 .actions{display:flex;gap:8px;flex-wrap:wrap}.setup{border-left:2px solid #ffffff36;padding-left:10px;margin:12px 0 16px}.error{color:#ffc9c9;opacity:1}
 a{color:inherit;text-underline-offset:3px}.path{overflow-wrap:anywhere;user-select:text}details{margin:14px 0;font:var(--type-caption)}summary{cursor:pointer}button:focus-visible,summary:focus-visible,a:focus-visible,input:focus-visible,select:focus-visible{outline:2px solid var(--accent);outline-offset:3px}
 @media(max-width:400px){.row select{max-width:145px}}
 @media(prefers-reduced-motion:reduce){.switch input,.switch input::before{transition:none}}
</style>
