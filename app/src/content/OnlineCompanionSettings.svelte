<script lang="ts">
  import {onMount} from 'svelte';
  import {invoke,isTauri} from '@tauri-apps/api/core';
  import qrcode from 'qrcode-generator';
  import type {CloudState} from '../island/onlineCompanion';
  let {cloud=null,message='',onrefresh}:{cloud?:CloudState|null;message?:string;onrefresh?:()=>void}=$props();
  let configured=$state(false),url=$state(''),key=$state(''),busy=$state(false),error=$state(''),pairUrl=$state(''),expires=$state(0),copied=$state('');
  const qr=$derived.by(()=>{if(!pairUrl)return '';const code=qrcode(0,'M');code.addData(pairUrl);code.make();return code.createDataURL(4,16);});
  function stamp(at:number|null|undefined){return at?new Date(at).toLocaleString([], {dateStyle:'short',timeStyle:'short'}):'Not yet';}
  async function read(){if(!isTauri())return;try{const s=await invoke<{configured:boolean;origin:string|null}>('companion_status');configured=s.configured;url=s.origin??url;}catch(e){error=String(e);}}
  async function action(path:string,body:Record<string,unknown>={}){return invoke<any>('companion_request',{path,method:'POST',body});}
  async function connect(){busy=true;error='';try{await invoke('companion_connect',{url,key});key='';await read();onrefresh?.();}catch(e){error=String(e);}finally{busy=false;}}
  async function pair(){busy=true;error='';try{const result=await action('/v1/pair/new');pairUrl=result.pairUrl;expires=result.expiresAt;}catch(e){error=String(e);}finally{busy=false;}}
  async function revoke(){busy=true;error='';try{await action('/v1/revoke');pairUrl='';onrefresh?.();}catch(e){error=String(e);}finally{busy=false;}}
  async function disconnect(){busy=true;error='';try{await invoke('companion_disconnect');configured=false;pairUrl='';onrefresh?.();}catch(e){error=String(e);}finally{busy=false;}}
  async function copy(value:string){try{await navigator.clipboard.writeText(value);copied='Copied';}catch{error='Could not copy. Select the text to copy manually.';}}
  async function dismiss(id:string){busy=true;error='';try{await action('/v1/action',{type:'dismiss',id});onrefresh?.();}catch(e){error=String(e);}finally{busy=false;}}
  onMount(()=>{void read();});
</script>
<div class="online">
  <div class="heading"><strong>Online iPhone companion</strong><span>{configured?'Connected':'Not connected'}</span></div>
  <p>Reach your alarms, shared Home note and focus timer over mobile data or any Wi-Fi. Send text or links back to Sudo, and optionally report your iPhone battery with Shortcuts.</p>
  {#if !configured}
    <label>Companion HTTPS address<input class="value" type="url" bind:value={url} placeholder="https://your-companion.workers.dev"/></label>
    <label>Desktop connection key<input class="value" type="password" bind:value={key} autocomplete="off" placeholder="Private desktop key"/></label>
    <button type="button" class="pill-btn" disabled={busy||!url||!key} onclick={connect}>{busy?'Connecting…':'Connect online companion'}</button>
    <p>Use your own deployed service and desktop key. The key is kept in Windows Credential Manager and is never sent to the phone. <a href="https://github.com/panmm942-ui/sudo-agent/blob/master/docs/online-companion.md" target="_blank" rel="noreferrer">Setup guide</a></p>
  {:else}
    <dl><div><dt>Service</dt><dd>{url}</dd></div><div><dt>Phone</dt><dd>{cloud?.phonePaired?'Paired':'Not paired yet'}</dd></div><div><dt>Schedule uploaded</dt><dd>{stamp(cloud?.sourceUpdatedAt)}</dd></div><div><dt>Available until</dt><dd>{stamp(cloud?.coverageUntil)}</dd></div><div><dt>Clock import reported</dt><dd>{stamp(cloud?.lastImportedAt)}</dd></div></dl>
    <div class="buttons"><button type="button" class="pill-btn" disabled={busy} onclick={pair}>Pair iPhone</button><button type="button" class="pill-btn" disabled={busy} onclick={()=>onrefresh?.()}>Sync now</button><button type="button" class="pill-btn" disabled={busy} onclick={revoke}>Revoke phone access</button></div>
    {#if pairUrl}<div class="pair"><img src={qr} alt="Private one-use iPhone pairing QR" width="140" height="140"/><div><strong>Scan with iPhone Camera</strong><p>Open in Safari. This pairing code expires at {new Date(expires).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'})} and can be used once.</p><button type="button" class="pill-btn" onclick={()=>copy(pairUrl)}>Copy pairing link</button></div></div>{/if}
    {#if cloud?.battery}<p>iPhone battery: <strong>{cloud.battery.percent}%{cloud.battery.charging?' · Charging':''}</strong> · reported {stamp(cloud.battery.updatedAt)}. This is the last Shortcut report.</p>{/if}
    {#if cloud?.inbox.length}<h4>Sent from your iPhone</h4>{#each cloud.inbox as item(item.id)}<div class="inbox"><p>{item.text}</p><small>{stamp(item.createdAt)}</small><div class="buttons"><button class="pill-btn" type="button" onclick={()=>copy(item.text)}>Copy</button><button class="pill-btn" type="button" disabled={busy} onclick={()=>dismiss(item.id)}>Dismiss</button></div></div>{/each}{/if}
    <p>Cloud coverage lasts seven days after the last PC upload. Clock still needs a Shortcut run to import the next 24 hours. PC edits arrive after upload and the next phone refresh. Notes and focus sync while both are online.</p>
    <button type="button" class="pill-btn" disabled={busy} onclick={disconnect}>Disconnect this PC</button>
    <p>Disconnecting stops PC sync. To block the paired phone, revoke its access first. Existing iPhone Clock alarms stay on the phone.</p>
  {/if}
  {#if message}<p role="status">{message}</p>{/if}{#if copied}<p role="status">{copied}</p>{/if}{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
<style>
.online{padding:12px 0;border-bottom:1px solid #ffffff15}.heading{display:flex;justify-content:space-between;gap:12px}.heading strong,h4{font:560 13px/18px var(--font-sans)}.heading span{font:var(--type-caption);color:#deded9}p,label,dl,small{font:var(--type-caption);line-height:1.55;color:#e7e7e7}p{margin:8px 0}label{display:block;margin:10px 0}.value{box-sizing:border-box;width:100%;border:1px solid #ffffff30;border-radius:8px;background:#ffffff0a;color:#fff;padding:9px 10px;font:var(--type-caption);margin-top:4px;min-width:0}.value:focus-visible{outline:2px solid var(--focus-ring);outline-offset:2px}dl{display:grid;gap:7px}dl div{display:flex;flex-wrap:wrap;gap:3px 10px;justify-content:space-between}dd{margin:0;overflow-wrap:anywhere}dt{color:#c5c5c5}.buttons{display:flex;flex-wrap:wrap;gap:6px}.pair{display:flex;align-items:center;gap:12px;margin:14px 0}.pair img{flex:none;max-width:37%;height:auto;border-radius:10px}.inbox{padding:10px 0;border-top:1px solid #ffffff22}.inbox p{white-space:pre-wrap;overflow-wrap:anywhere}.error{color:#ffcbcb}a{color:inherit;text-underline-offset:3px}
</style>
