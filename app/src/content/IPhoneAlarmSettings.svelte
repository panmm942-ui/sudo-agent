<script lang="ts">
  import {onMount} from 'svelte';
  import {invoke,isTauri} from '@tauri-apps/api/core';
  import qrcode from 'qrcode-generator';
  import type {Preferences} from '../island/preferences';
  import type {IPhoneStatus} from '../routine/iphoneAlarms';
  import {iphoneConnection} from '../routine/iphoneConnection';
  let {preferences,onchange}:{preferences:Preferences;onchange:(enabled:boolean)=>void}=$props();
  let status=$state<IPhoneStatus|null>(null),busy=$state(false),reading=false,error=$state(''),copied=$state(''),showKey=$state(false),selectedUrl=$state('');
  let connectionError=$state(''),readError=$state('');
  const setupUrl=$derived(status?.setupUrls.includes(selectedUrl)?selectedUrl:status?.setupUrls[0]??'');
  const feedUrl=$derived(setupUrl?new URL('/v1/alarms',setupUrl).href:'');
  const qr=$derived.by(()=>{
    if(!setupUrl)return '';
    const code=qrcode(0,'M');code.addData(setupUrl);code.make();
    // Locally generated image; no pairing data is sent to a QR service.
    return code.createDataURL(5,20);
  });
  function time(at:number|null|undefined){return at?new Date(at).toLocaleString([], {dateStyle:'short',timeStyle:'short'}):'Not yet';}
  async function read(){
    if(reading||!isTauri())return;
    reading=true;
    try{status=await invoke<IPhoneStatus>('iphone_alarms_status');connectionError=iphoneConnection.error();readError='';}
    catch(cause){readError=String(cause);}finally{reading=false;}
  }
  async function toggle(next:boolean){
    if(!isTauri()){error='Open the Windows Sudo app to connect your iPhone.';return;}
    busy=true;error='';copied='';showKey=false;
    try{
      const success=await iphoneConnection.update(next,preferences);
      if(!success){const reason=iphoneConnection.error();await iphoneConnection.update(false,preferences);throw new Error(reason);}
      onchange(next);await read();
    }catch(cause){error=String(cause);}finally{busy=false;}
  }
  async function copy(value:string,label:string){
    try{await navigator.clipboard.writeText(value);copied=`${label} copied`;error='';}
    catch{error='Clipboard access failed. Select and copy the value below.';}
  }
  onMount(()=>{void read();const timer=setInterval(read,3000);return()=>clearInterval(timer);});
</script>

<div class="iphone-connection">
  <label class="toggle"><span><strong>Local Wi-Fi connection</strong><small>Optional alternative to the online companion. Copy upcoming alarms using a Shortcut on your trusted home Wi-Fi.</small></span>
    <input type="checkbox" role="switch" aria-label="Connect to iPhone Clock" checked={preferences.iphoneAlarms} disabled={busy} onchange={event=>{const next=event.currentTarget.checked;event.currentTarget.checked=preferences.iphoneAlarms;void toggle(next);}}/>
  </label>
  {#if preferences.iphoneAlarms&&status?.enabled}
    <dl class="status" aria-label="iPhone alarm connection status">
      <div><dt>Schedule fetched</dt><dd>{time(status.lastFetchedAt)}</dd></div>
      <div><dt>Imported into Clock</dt><dd>{time(status.lastImportedAt)}{#if status.lastImportedAt&&status.importedCount!==null} · {status.importedCount} alarms{/if}</dd></div>
    </dl>
    {#if setupUrl}
      <div class="pairing"><img src={qr} alt="Scan with your iPhone Camera to open the private Sudo alarm setup instructions" width="156" height="156"/>
        <div><strong>Set up once on your iPhone</strong><p>Scan this code with Camera. Open the setup page in Safari and follow the Shortcuts steps. Keep both devices on the same Wi-Fi.</p>
          <button type="button" class="pill-btn" onclick={()=>copy(setupUrl,'Setup link')}>Copy setup link</button>
        </div>
      </div>
      <input class="value" aria-label="Private iPhone setup link" readonly value={setupUrl}/>
      {#if status.setupUrls.length>1}
        <p class="help">PC has more than one network. If the first link cannot open on your phone, try another address:</p>
        <div class="addresses">{#each status.setupUrls as url}<button type="button" class="pill-btn" aria-pressed={setupUrl===url} onclick={()=>selectedUrl=url}>{new URL(url).hostname}</button>{/each}</div>
      {/if}
      <details><summary>Connection values & troubleshooting</summary>
        <p>If the QR page cannot open, allow Sudo on <strong>Private networks</strong> in Windows Firewall. Guest Wi-Fi or client isolation can block the connection. Reconnect this option after changing networks.</p>
        <label class="field">Schedule URL<input class="value" aria-label="iPhone schedule URL" readonly value={feedUrl}/></label>
        <label class="field">Pairing key<div class="key"><input class="value" aria-label="iPhone pairing key" type={showKey?'text':'password'} readonly value={status.pairingKey??''}/><button class="pill-btn" type="button" onclick={()=>showKey=!showKey}>{showKey?'Hide':'Show'}</button></div></label>
        <button class="pill-btn" type="button" onclick={()=>copy(status?.pairingKey??'','Pairing key')}>Copy key</button>
        <p>This key grants access to your alarm schedule. Keep the setup link private. The local HTTP connection is intended for a trusted home network.</p>
      </details>
    {:else}<p class="help">No private IPv4 address was found. Connect this PC to your home network, then turn this option off and on.</p>{/if}
    <p class="help">The Shortcut imports the next 24 hours. Run it at least daily, after schedule changes, and before leaving home. Only alarms with this Sudo connection’s label are managed. PC edits reach the phone on the next run.</p>
    <p class="help">Once imported, Clock can ring without the PC. Phone sounds, volume, snooze and dismiss are separate. Turning this connection off leaves imported alarms on your iPhone; remove them in Clock if you no longer want them.</p>
    <button type="button" class="pill-btn" disabled={busy} onclick={()=>void read()}>Refresh status</button>
  {/if}
  {#if copied}<p class="feedback" role="status">{copied}</p>{/if}
  {#if error||connectionError||readError}<p class="feedback error" role="alert">{error||connectionError||readError}</p>{/if}
</div>

<style>
  .iphone-connection{padding:12px 0;border-bottom:1px solid #ffffff15}.toggle{display:flex;align-items:center;gap:16px;justify-content:space-between}.toggle span{min-width:0}strong{font:560 13px/18px var(--font-sans)}small,.help,p,.field{display:block;font:var(--type-caption);line-height:1.55;color:#e7e7e7;margin:6px 0}.toggle input{width:18px;height:18px;accent-color:var(--accent);flex:none}.status{font:var(--type-caption);margin:12px 0;display:grid;gap:5px}.status div{display:flex;flex-wrap:wrap;gap:4px 12px;justify-content:space-between}dt{color:#d8d8d8}dd{margin:0;color:#fff}.pairing{display:flex;align-items:center;gap:16px;margin:15px 0}.pairing img{border-radius:10px;flex:none;max-width:40%;height:auto}.pairing div{min-width:0}.value{box-sizing:border-box;width:100%;border:1px solid #ffffff30;border-radius:8px;background:#ffffff0a;color:#fff;padding:9px 10px;font:11px/1.5 var(--font-sans);min-width:0}.value:focus-visible,summary:focus-visible{outline:2px solid #ffffffa0;outline-offset:3px}.key{display:flex;gap:6px;margin:4px 0}.key .value{flex:1}.pill-btn{margin:4px 0}.help{margin-top:10px}details{margin:12px 0;font:var(--type-caption)}summary{cursor:pointer;padding:6px 0;color:#fff}.field{margin:10px 0}.feedback{font:var(--type-caption);line-height:1.5;overflow-wrap:anywhere}.error{color:#ffcbcb}@media(max-width:420px){.pairing{align-items:flex-start;gap:12px}.pairing img{max-width:34%}}
</style>
