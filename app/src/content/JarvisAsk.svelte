<script lang="ts">
  import {onDestroy, onMount} from 'svelte';
  import {invoke} from '@tauri-apps/api/core';
  import MiniOrb from './MiniOrb.svelte';

  /** One-line typed question for Sudo AI on the expanded Home view; the reply shows in the island's responding view. */
  let {onfocuschange}:{onfocuschange:(focused:boolean)=>void}=$props();
  let value=$state(''), error=$state(''), orbClick=$state(false);
  onMount(()=>{invoke<{orbClick?:boolean}>('jarvis_status').then(s=>orbClick=s.orbClick!==false).catch(()=>{});});

  async function submit(event:SubmitEvent){
    event.preventDefault();
    const text=value.trim();
    if(!text)return;
    value='';error='';
    try{await invoke('jarvis_ask',{text});}catch(e){error=e instanceof Error?e.message:String(e);}
  }
  onDestroy(()=>onfocuschange(false));
</script>

<form class="ask" class:with-orb={orbClick} onsubmit={submit}>
  {#if orbClick}<MiniOrb onerror={message=>error=message}/>{/if}
  <input aria-label="Ask Sudo AI" placeholder={error||'Ask Sudo AI…'} maxlength="4000" autocomplete="off" spellcheck="false" bind:value
    onpointerdown={()=>onfocuschange(true)} onfocus={()=>onfocuschange(true)} onblur={()=>onfocuschange(false)}
    onkeydown={e=>{if(e.key==='Escape'){value='';e.currentTarget.blur();}}}/>
</form>

<style>
  .ask{position:absolute;left:30px;right:66px;bottom:12px;z-index:3;pointer-events:auto;margin:0;display:flex;gap:8px;align-items:center}
  input{flex:1}
  input{width:100%;box-sizing:border-box;height:30px;padding:0 12px;border:1px solid #ffffff22;border-radius:15px;background:#ffffff10;color:inherit;font:var(--type-caption);outline:none;transition:background-color 120ms ease,border-color 120ms ease}
  input::placeholder{color:var(--text-secondary)}
  input:focus{background:#ffffff1a;border-color:#ffffff55}
  @media(prefers-reduced-motion:reduce){input{transition:none}}
</style>
