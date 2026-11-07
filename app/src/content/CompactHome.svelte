<script lang="ts">
 import {onMount} from 'svelte';import type {Preferences} from '../island/preferences';import type {TabPackage} from '../island/extensions';
 import ExtensionIcon from './ExtensionIcon.svelte';
 import {systemLine,watchSystem,type SystemStats} from '../island/system';
 let {preferences,remaining,item,tab,onclick}:{preferences:Preferences;remaining:number;item?:{label:string;time:string};tab?:TabPackage;onclick:()=>void}=$props();let now=$state(new Date());
 onMount(()=>{if(preferences.compactWidget!=='clock'||tab)return;const t=setInterval(()=>now=new Date(),60000);return()=>clearInterval(t);});
 let stats=$state<SystemStats|null>(null);
 $effect(()=>{if(preferences.compactWidget!=='system'||tab)return;return watchSystem(s=>stats=s);});
 const label=$derived(tab?tab.compact:preferences.compactWidget==='clock'?now.toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'}):preferences.compactWidget==='focus'?`${Math.floor(remaining/60000)}:${String(Math.ceil(remaining/1000)%60).padStart(2,'0')} · Focus`:preferences.compactWidget==='note'?preferences.homeNote||'Add a note in Settings':preferences.compactWidget==='system'?systemLine(stats):item?.label||'Nothing up next');
</script>
<button type="button" aria-label={`Expand ${tab?.name??'Home'}`} {onclick}><span class="mark" aria-hidden="true">{#if tab}<ExtensionIcon name={tab.icon}/>{:else}{preferences.compactWidget==='focus'?'◷':preferences.compactWidget==='note'?'≡':preferences.compactWidget==='system'?'▤':'·'}{/if}</span><span class="copy">{label}</span>{#if !tab&&preferences.compactWidget==='reminders'&&item}<small>{item.time}</small>{/if}</button>
<style>button{display:flex;align-items:center;gap:9px;width:100%;height:100%;padding:0 15px;border:0;border-radius:24px;background:transparent;color:inherit;text-align:left;cursor:pointer;font:var(--type-label)}.mark{opacity:.65;flex:none}.copy{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1}small{opacity:.7;font:var(--type-caption);flex:none}</style>
