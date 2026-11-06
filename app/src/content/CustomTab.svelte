<script lang="ts">
 import type {TabPackage} from '../island/extensions';
 import {attachExtension,extensionDocument} from '../island/extensionRuntime';
 let {tab,remaining=0}:{tab:TabPackage;remaining?:number}=$props();let now=$state(new Date());let frame:HTMLIFrameElement|undefined=$state();
 const session=crypto.randomUUID();let source=$derived(tab.version===2?extensionDocument(tab,session):'');
 $effect(()=>{if(tab.version===2&&frame)return attachExtension(frame,tab,session);});
 $effect(()=>{if(tab.version!==1||!tab.blocks.some(b=>b.type==='clock'))return;const t=setInterval(()=>now=new Date(),60000);return()=>clearInterval(t);});
</script>
{#if tab.version===2}
 <iframe bind:this={frame} title={tab.name} sandbox="allow-scripts" referrerpolicy="no-referrer" srcdoc={source}></iframe>
{:else}
<section aria-label={tab.name}><h2>{tab.name}</h2><!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="blocks" tabindex="0" role="region" aria-label="Custom tab content">
 {#each tab.blocks as block,i (i)}{#if block.type==='text'}<p>{block.text}</p>{:else if block.type==='clock'}<p class="large tabular">{now.toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'})}</p>{:else}<p class="large tabular">{Math.floor(remaining/60000)}:{String(Math.ceil(remaining/1000)%60).padStart(2,'0')} <small>Focus</small></p>{/if}{/each}
</div></section>
{/if}
<style>iframe{display:block;border:0;width:calc(100% - 122px);margin-left:60px;height:100%;background:transparent}section{height:100%;padding:20px 68px 18px 60px;display:flex;flex-direction:column;gap:10px;}h2{font:600 17px/22px var(--font-sans);margin:0}.blocks{overflow:auto;min-height:0;scrollbar-width:thin;}p{white-space:pre-wrap;overflow-wrap:anywhere;font:var(--type-body);margin:0 0 10px}.large{font:500 25px/30px var(--font-sans)}small{font:var(--type-caption);opacity:.65}</style>
