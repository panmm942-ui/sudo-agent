<script lang="ts">
  import spotifyIcon from "../assets/spotify-icon.svg";
  import type {TabPackage} from '../island/extensions';
  import ExtensionIcon from './ExtensionIcon.svelte';
  let { spotify, selected, onselect, onsettings,tabs=[],oncustom,customId,top=10,clipWidth=440,clipHeight=210,radius=48 }: {
    top?:number;clipWidth?:number;clipHeight?:number;radius?:number;
    customId?:string;
    tabs?:TabPackage[];oncustom?:(id:string)=>void;
    spotify: boolean;
    selected: "home" | "spotify" | "sudo";
    onselect: (page: "home" | "spotify" | "sudo") => void;
    onsettings: () => void;
  } = $props();
  // One height for every page, so the tab icons never shift when scrolling between tabs.
  const panelHeight=210;
  const horizontalClip=$derived(Math.max(0,(440-clipWidth)/2));
</script>

<div class="chrome island-content" style:top={`${top}px`} style:height={`${panelHeight}px`} style:clip-path={`inset(0px ${horizontalClip}px ${Math.max(0,panelHeight-clipHeight)}px round ${radius}px)`}>
  <button type="button" class="icon settings" aria-label="Settings" onclick={onsettings}>
    <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="m9.2 3-.6 2.1-1.7 1-2.2-.5-2 3.5 1.5 1.6v2.1l-1.5 1.6 2 3.5 2.2-.5 1.7 1 .6 2.1h4l.6-2.1 1.7-1 2.2.5 2-3.5-1.5-1.6v-2.1l1.5-1.6-2-3.5-2.2.5-1.7-1-.6-2.1Z"/><circle cx="11.2" cy="12" r="3"/></svg>
  </button>
    <nav class="rail" aria-label="Island pages">
      <button type="button" class="icon" aria-label="Home" aria-pressed={selected === "home"&&!customId} onclick={() => onselect("home")}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1z" fill="currentColor" /></svg>
      </button>
      <button type="button" class="icon" aria-label="Spotify" aria-pressed={selected === "spotify"} onclick={() => onselect("spotify")}>
        <img class="spotify-mark" src={spotifyIcon} width="21" height="21" alt="" />
      </button>
      <button type="button" class="icon" aria-label="Sudo AI" aria-pressed={selected === "sudo"} onclick={() => onselect("sudo")}>
        <span class="sudo-mark" aria-hidden="true"></span>
      </button>
    </nav>
  {#if selected==='home'&&tabs.length}<nav class="addons" aria-label="Your extensions" onwheel={event=>event.stopPropagation()}>
    {#each tabs as tab (tab.id)}<button type="button" class="icon" aria-label={'Open '+tab.name} aria-pressed={customId===tab.id} onclick={()=>oncustom?.(tab.id)}><ExtensionIcon name={tab.icon}/></button>{/each}
  </nav>{/if}
</div>

<style>
  .chrome { position: fixed; left: 50vw; top: 10px; width: 440px; height: 210px; transform: translateX(-50%); pointer-events: none; }
  .icon { display: grid; place-items: center; width: 32px; height: 32px; padding: 0; border: 0; border-radius: var(--radius-pill); color: var(--text-primary); background: transparent; cursor: pointer; pointer-events: auto; }
  .icon svg { width: 18px; height: 18px; }
  .spotify-mark { display: block; width: 21px; height: 21px; }
  /* A plain white orb glyph, like the other tab icons. */
  .sudo-mark { display: block; width: 17px; height: 17px; border-radius: 50%; box-shadow: inset 0 0 0 1.7px currentColor; background: radial-gradient(circle, currentColor 0 3.2px, transparent 3.7px); }
  .icon[aria-pressed="false"] .sudo-mark { opacity: .76; }
  .icon:hover, .icon[aria-pressed="true"] { background: var(--surface-control-hover); }
  .icon[aria-pressed="false"] svg { opacity: 0.76; }
  .settings { position: absolute; top: 14px; right: 12px; background: transparent; }
  .settings:hover { background: transparent; }
  .rail { position: absolute; top: 50%; right: 12px; display: grid; gap: var(--space-1); transform: translateY(-50%);max-height:122px;overflow-y:auto;scrollbar-width:none; }
  .addons{position:absolute;left:20px;top:50%;transform:translateY(-50%);display:grid;gap:4px;max-height:128px;overflow-y:auto;scrollbar-width:none;pointer-events:auto;}
</style>
