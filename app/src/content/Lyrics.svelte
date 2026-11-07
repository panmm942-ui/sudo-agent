<script lang="ts">
  import { tick, onMount, onDestroy } from "svelte";
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import {fitLyrics} from '../island/lyricFit';
  import LyricRow from './LyricRow.svelte';
  import LyricVocals from './LyricVocals.svelte';
  import type { LyricsResult } from "../island/lyrics";
  import {createLyricsSource,importedLyricsResult} from '../island/lyricSource';
  import {importedLyrics,saveImportedLyrics} from '../island/importedLyrics';
  import { estimateWords, lyricWindow, playbackSeconds } from "../island/lyricTiming";
  import { romanize, targetLanguage, translateLine } from "../island/lyricLanguage";
  import type { Preferences } from "../island/preferences";
  import type { SpotifyTrack } from "../island/spotify";
  let { track, preferences, onupdate, onclose, onfullscreen, fullscreen=false, minimal=false }: { track: SpotifyTrack; preferences: Preferences; onupdate?: (patch: Partial<Preferences>) => void; onclose: () => void; onfullscreen?:()=>void; fullscreen?:boolean; minimal?:boolean } = $props();
  let result = $state.raw<LyricsResult | null>(null);
  let error = $state("");
  let retry = $state(0);
  // Your own lyrics for this song (an .lrc or .txt file) replace what the sources found.
  let importNote = $state('');
  let importRevision = $state(0);
  const hasImported = $derived.by(() => { void importRevision; try { return !!importedLyrics(track.key); } catch { return false; } });
  async function importFile(event: Event & { currentTarget: HTMLInputElement }): Promise<void> {
    const input = event.currentTarget, file = input.files?.[0];
    input.value = '';
    if (!file) return;
    if (file.size > 300_000) { importNote = 'That file is too large for lyrics.'; return; }
    const text = await file.text();
    if (!importedLyricsResult(text)) { importNote = 'That file has no lyrics in it.'; return; }
    try { saveImportedLyrics(track.key, text); } catch { importNote = 'Couldn’t save the lyrics.'; return; }
    importNote = ''; importRevision++; retry++;
  }
  function removeImported(): void { saveImportedLyrics(track.key, null); importRevision++; retry++; }
  let now = $state(Date.now());
  let translated = $state.raw<Record<string, string>>({});
  let romanized = $state.raw<Record<string, string>>({});
  let languageError = $state("");
  let plainPage=$state(0);
  let plainContainer=$state<HTMLDivElement>();
  let lineContainer=$state<HTMLDivElement>();
  let systemReduced=$state(false);
  let vocalLatency=$state(0);
  const reduced=$derived(preferences.reduceMotion||systemReduced);
  const flowDuration=$derived(preferences.lyricFlow&&!reduced?preferences.lyricFlowDuration:0);
  onMount(()=>{
    const media=matchMedia('(prefers-reduced-motion: reduce)');
    const update=()=>systemReduced=media.matches;
    update();media.addEventListener('change',update);
    return()=>media.removeEventListener('change',update);
  });
  $effect(()=>{
    if(reduced||!preferences.lyricFlow)lineContainer?.getAnimations({subtree:true}).forEach(animation=>animation.cancel());
  });
  const lookupKey = $derived(JSON.stringify({ key:track.key, title: track.title, artist: track.artist, album: track.album, duration: track.duration }));
  // Per-song nudge for sources whose timestamps run early or late; remembered on this device.
  const OFFSETS_KEY='sudo.lyricSongOffsets';
  const readOffsets=():Record<string,number>=>{try{const value=JSON.parse(localStorage.getItem(OFFSETS_KEY)??'{}');return value&&typeof value==='object'&&!Array.isArray(value)?value:{};}catch{return {};}};
  let songOffsets=$state<Record<string,number>>(readOffsets());
  const songOffset=$derived(Number.isFinite(songOffsets[track.key])?Math.max(-5,Math.min(5,songOffsets[track.key])):0);
  function nudge(delta:number){
    const next=Math.round((songOffset+delta)*10)/10;
    const entries=Object.entries({...songOffsets}).filter(([key])=>key!==track.key);
    if(Math.abs(next)>=.05)entries.push([track.key,Math.max(-5,Math.min(5,next))]);
    songOffsets=Object.fromEntries(entries.slice(-200));
    try{localStorage.setItem(OFFSETS_KEY,JSON.stringify(songOffsets));}catch{/* Optional convenience only. */}
  }
  const position = $derived(Math.max(0,playbackSeconds(track, now)-vocalLatency+preferences.lyricOffset+songOffset));
  const fontFamily=$derived(preferences.lyricFont==='serif'?'Georgia, "Times New Roman", serif':preferences.lyricFont==='rounded'?'"Segoe UI Variable Text", "Arial Rounded MT Bold", sans-serif':'var(--font-sans)');
  const rows = $derived(lyricWindow(result?.kind === "lyrics" ? result.lines ?? [] : [], position));
  const display=(text:string)=>preferences.lyricRomanization?romanized[text]||text:text;
  // Real word timing when the source has it for the shown text; otherwise an estimate across the line.
  const currentWords=$derived.by(()=>{
    const row=rows[1];
    if(!preferences.lyricWordSync||!row?.text)return undefined;
    const shown=display(row.text);
    return row.words&&shown===row.text?row.words:estimateWords(shown,row.time,rows[2]?.time);
  });
  // Move each line between slots by its centre only (FLIP). Svelte's flip also
  // scales to the new slot size, which stretched the text; LyricRow grows the font itself.
  const rowKey=$derived(rows.map(row=>row?`${row.time}:${row.text}`:'').join('|'));
  let slotCentres=new Map<string,number>();
  const centres=()=>new Map([...(lineContainer?.querySelectorAll<HTMLElement>('.line-slot[data-key]')??[])].map(node=>{const r=node.getBoundingClientRect();return [node.dataset.key!,r.top+r.height/2] as const;}));
  $effect.pre(()=>{void rowKey;slotCentres=centres();});
  $effect(()=>{
    void rowKey;
    const before=slotCentres,ms=flowDuration;
    if(!ms||!lineContainer)return;
    for(const node of lineContainer.querySelectorAll<HTMLElement>('.line-slot[data-key]')){
      const from=before.get(node.dataset.key!);if(from===undefined)continue;
      const r=node.getBoundingClientRect(),dy=from-(r.top+r.height/2);
      if(Math.abs(dy)>=1)node.animate([{transform:`translateY(${dy}px)`},{transform:'none'}],{duration:ms,easing:'cubic-bezier(.22,1,.36,1)'});
    }
  });
  const plainText=$derived(result?.kind==='lyrics'&&!result.lines?.length?result.text:'');
  const plainRows=$derived(plainText.split(/\r?\n/).filter(line=>line.trim()));
  const pageCount=$derived(Math.max(1,Math.ceil(plainRows.length/3)));
  const shownPlain=$derived(plainRows.slice(plainPage*3,plainPage*3+3));
  const visibleKey = $derived(JSON.stringify(plainText?shownPlain:rows.map(row => row?.text ?? "")));
  const language = $derived(targetLanguage(preferences.lyricLanguage));
  $effect(()=>{
    void visibleKey;void romanized;void translated;void preferences.lyricRomanization;void preferences.lyricTranslation;
    void preferences.lyricFont;void preferences.lyricWeight;void preferences.lyricSpacing;void preferences.lyricScale;
    const container=plainText?plainContainer:undefined;
    if(!container)return;
    let disposed=false;
    const fit=()=>{
      if(disposed)return;
      const lines=plainText?shownPlain.map(text=>({main:preferences.lyricRomanization?romanized[text]||text:text})):rows.map(row=>({main:row?.text?(preferences.lyricRomanization?romanized[row.text]||row.text:row.text):' ',sub:row?.text&&preferences.lyricTranslation?translated[`${language}:${row.text}`]||'…':undefined}));
      fitLyrics(container,lines,!plainText,42*preferences.lyricScale*(fullscreen?1.4:1));
    };
    void tick().then(fit);
    const observer=new ResizeObserver(fit);observer.observe(container);
    return()=>{disposed=true;observer.disconnect();};
  });
  $effect(() => {
    if(!track.playing||result?.kind!=='lyrics'||!result.lines?.length)return;
    now=Date.now();
    const timer=setInterval(()=>now=Date.now(),100);
    return ()=>clearInterval(timer);
  });
  const source=createLyricsSource((value,message)=>{result=value;error=message??'';});
  let previousLookup:string|undefined;
  onDestroy(()=>source.stop());
  $effect(() => {
    const query=JSON.parse(lookupKey);
    if(previousLookup!==lookupKey){previousLookup=lookupKey;plainPage=0;translated={};romanized={};}
    source.request(query,retry>0);
  });
  $effect(() => {
    void lookupKey;
    const texts: string[] = JSON.parse(visibleKey);
    const doTranslate = preferences.lyricTranslation&&!plainText;
    const doRomanize = preferences.lyricRomanization;
    const target = language;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 10_000);
    let disposed = false;
    languageError = "";
    void Promise.all([...new Set(texts.filter(Boolean))].map(async text => {
      if (doRomanize) { const value = await romanize(text,controller.signal); if (!disposed&&!controller.signal.aborted) romanized = { ...romanized, [text]: value }; }
      if (doTranslate) {
        try { const value = await translateLine(text, target, controller.signal); if (!disposed&&!controller.signal.aborted) translated = { ...translated, [`${target}:${text}`]: value }; }
        catch (failure) { if (!disposed) languageError = failure instanceof Error ? failure.message : "Translation unavailable."; }
      }
    })).catch(() => { if (!disposed) languageError = "Romanization unavailable."; }).finally(() => clearTimeout(timeout));
    return () => { disposed = true; clearTimeout(timeout); controller.abort(); };
  });
</script>

<section class="lyrics" class:fullscreen class:minimal class:balanced={preferences.lyricLayout==='balanced'} class:focus={preferences.lyricLayout==='focus'} class:has-footer={!!plainText||!!languageError} class:reduced aria-label="Song lyrics" style:--lyric-font={fontFamily} style:--lyric-flow-ms={`${flowDuration}ms`} style:--lyric-origin={preferences.lyricAlign==='center'?'center':preferences.lyricAlign==='right'?'right center':'left center'} style:--lyric-align={preferences.lyricAlign} style:--lyric-weight={preferences.lyricWeight} style:--lyric-leading={preferences.lyricSpacing==='airy'?1.25:preferences.lyricSpacing==='compact'?1.08:1.16} style:--lyric-gap={preferences.lyricSpacing==='airy'?'9px':preferences.lyricSpacing==='compact'?'3px':'5px'} style:--lyric-prev-opacity={preferences.lyricContext==='clear'?'.75':preferences.lyricContext==='soft'?'.48':'.32'} style:--lyric-next-opacity={preferences.lyricContext==='clear'?'.85':preferences.lyricContext==='soft'?'.68':'.56'}>
  <header>
    {#if !fullscreen}<button type="button" class="back" aria-label="Close lyrics" onclick={onclose}>‹</button>{/if}
    <div class="options" aria-label="Lyrics display options">
      <button type="button" aria-pressed={preferences.lyricRomanization} onclick={() => onupdate?.({lyricRomanization:!preferences.lyricRomanization,lyricTranslation:false})}>Romanize</button>
      <button type="button" aria-pressed={preferences.lyricTranslation} disabled={!!plainText} aria-label={plainText?'Translation is available for timed lyric lines.':undefined} onclick={() => onupdate?.({lyricTranslation:!preferences.lyricTranslation,lyricRomanization:false})}>Translate</button>
    </div>
    {#if result?.kind==='lyrics'&&result.lines?.length}<div class="sync" role="group" aria-label="Lyric timing for this song">
      <button type="button" aria-label="Show lyrics later" onclick={()=>nudge(-.2)}>−</button>
      <button type="button" class="sync-value" aria-label="Reset lyric timing for this song" disabled={!songOffset} onclick={()=>nudge(-songOffset)}>{songOffset?`${songOffset>0?'+':''}${songOffset.toFixed(1)}s`:'Sync'}</button>
      <button type="button" aria-label="Show lyrics earlier" onclick={()=>nudge(.2)}>+</button>
    </div>{/if}
    <LyricVocals volume={preferences.vocalVolume} onchange={value=>onupdate?.({vocalVolume:value})} onlatency={seconds=>vocalLatency=seconds}/>
    {#if hasImported&&!fullscreen}<button type="button" class="mine" title="Stop using your lyrics file for this song" onclick={removeImported}>My lyrics ✕</button>{/if}
    {#if !fullscreen}<label class="import" title={hasImported?'Your lyrics file is shown. Import another .lrc or .txt':'Import lyrics for this song (.lrc or .txt)'}><input type="file" accept=".lrc,.txt,text/plain" onchange={importFile} /><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M7 3h7l5 5v12a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z M14 3v5h5 M12 11v6m-3-3 3 3 3-3" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg><span class="sr">Import lyrics</span></label>{/if}
    {#if !fullscreen&&onfullscreen}<button type="button" class="expand" aria-label="Open fullscreen lyrics" onclick={onfullscreen}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M9 4H4v5M15 4h5v5M4 15v5h5M20 15v5h-5" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg></button>{/if}
  </header>
  {#if importNote}<div class="notice" role="status">{importNote}<button type="button" class="pill-btn" onclick={() => importNote = ''}>OK</button></div>
  {:else if error}<div class="notice" role="status">{error}<button type="button" class="pill-btn" onclick={() => retry++}>Retry</button></div>
  {:else if !result}<div class="notice" role="status">Finding timed lyrics…</div>
  {:else if result.kind !== "lyrics"}<div class="notice" role="status">{result.kind === "instrumental" ? "Instrumental track" : "Lyrics unavailable for this track"}{#if result.kind!=='instrumental'&&!fullscreen}<label class="pill-btn import-cta">Import .lrc<input type="file" accept=".lrc,.txt,text/plain" onchange={importFile} /></label>{/if}</div>
  {:else if !result.lines?.length}
    <div class="plain" bind:this={plainContainer} aria-label="Unsynced lyrics">{#each shownPlain as text,i (plainPage+':'+i)}<p>{preferences.lyricRomanization?romanized[text]||text:text}</p>{/each}</div>
  {:else if track.position === null || track.position === undefined}<div class="notice" role="status">Spotify hasn’t supplied a playback position yet.</div>
  {:else}
    <div class="lines" bind:this={lineContainer} role="region" aria-label="Previous, current and next lyric">
      {#each rows as row,index (row??`empty:${index}`)}
        <div class="line-slot" data-key={row?`${row.time}:${row.text}`:undefined} style:grid-row={index+1} in:fly={{y:flowDuration?18:0,duration:flowDuration,easing:cubicOut,opacity:0}} out:fly={{y:flowDuration?-18:0,duration:flowDuration*.7,easing:cubicOut,opacity:0}}>
          <LyricRow {fullscreen} {position} words={index===1?currentWords:undefined} wordSync={preferences.lyricWordSync} duration={flowDuration} clock={()=>Math.max(0,playbackSeconds(track,Date.now())-vocalLatency+preferences.lyricOffset+songOffset)} identity={row?`${row.time}:${row.text}`:`empty:${index}`} kind={index===0?'previous':index===1?'current':'next'} original={row?.text??''} main={row?.text?display(row.text):''} sub={row?.text&&preferences.lyricTranslation?(translated[`${language}:${row.text}`]||(languageError?'Translation unavailable':'…')):''} {reduced} flow={preferences.lyricFlow} scale={preferences.lyricScale*(fullscreen?1.4:1)} fitKey={`${preferences.lyricFont}:${preferences.lyricWeight}:${preferences.lyricSpacing}:${preferences.lyricLayout}`}/>
        </div>
      {/each}
    </div>
  {/if}
  {#if plainText||languageError}<footer>{languageError||'Unsynced lyrics'}{#if plainText&&!preferences.spotifyBridge&&!minimal}<button type="button" class="timed" onclick={()=>onupdate?.({spotifyBridge:true})}>Get timed lyrics from Spotify</button>{/if}{#if plainText}<span class="pages"><button type="button" aria-label="Previous lyric page" disabled={plainPage===0} onclick={()=>plainPage--}>‹</button><span>{plainPage+1}/{pageCount}</span><button type="button" aria-label="Next lyric page" disabled={plainPage+1>=pageCount} onclick={()=>plainPage++}>›</button></span>{/if}</footer>{/if}
</section>

<style>
  .lyrics.fullscreen {padding:0;gap:24px;grid-template-rows:30px minmax(0,1fr);}
  .fullscreen header {justify-content:flex-end;}
  .fullscreen .lines {gap:24px;grid-template-rows:repeat(3,minmax(0,1fr));}
  .lyrics { height: 100%; padding: 12px 62px 10px 24px; display: grid; grid-template-rows: 24px minmax(0, 1fr); gap: 4px; }
  .lyrics.has-footer {grid-template-rows:24px minmax(0,1fr) 20px;}
  header { display: flex; align-items: center; gap: 8px; }
  /* Right-click: lyrics only. The header buttons hide and the lines use the width the tab rail had. */
  .lyrics.minimal{padding-right:24px;grid-template-rows:0 minmax(0,1fr);gap:0}
  .lyrics.minimal.has-footer{grid-template-rows:0 minmax(0,1fr) 20px}
  .minimal header{visibility:hidden;overflow:hidden}
  button { color: inherit; background: transparent; border: 0; cursor: pointer; font: var(--type-caption); border-radius: 8px; }
  .back { width: 24px; height: 24px; font-size: 25px; line-height: 22px; padding: 0; }
  .options { display: flex; gap: 4px; }
  .mine{flex:none;height:22px;padding:0 8px;border:0;border-radius:11px;background:var(--surface-control);color:inherit;font:600 10.5px/1 var(--font-sans);cursor:pointer;white-space:nowrap}.mine:hover{background:var(--surface-control-hover)}.minimal .mine{display:none}
    .import{position:relative;width:24px;height:24px;display:grid;place-items:center;flex:none;border-radius:50%;cursor:pointer;opacity:.75}.import:hover{opacity:1;background:var(--surface-control-hover)}.import svg{width:17px;height:17px}.import input,.import-cta input{position:absolute;width:1px;height:1px;opacity:0;pointer-events:none}.import-cta{position:relative;margin-left:8px;cursor:pointer}.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%)}.minimal .import{display:none}
  .expand{width:24px;height:24px;display:grid;place-items:center;margin-left:0;padding:3px;flex:none}.expand svg{width:18px;height:18px}
  .options button { min-height: 24px; padding: 2px 7px; }
  .sync{display:flex;align-items:center;margin-left:auto;gap:1px;flex:none}.sync button{min-width:22px;height:24px;padding:0 5px;font-variant-numeric:tabular-nums}.sync .sync-value{min-width:38px;opacity:.75}.sync .sync-value:disabled{cursor:default;background:transparent}.sync+:global(.vocal-control){margin-left:0}
  button:hover, button[aria-pressed="true"] { background: var(--surface-control-hover); }
  .lines { position: relative; display:grid;grid-template-rows:minmax(0,1fr) minmax(0,2.8fr) minmax(0,1fr);gap:var(--lyric-gap,5px);min-height:0;overflow:hidden; }
  .line-slot{min-height:0;min-width:0;grid-column:1;}
  .balanced .lines{grid-template-rows:minmax(0,1fr) minmax(0,1.6fr) minmax(0,1fr)}
  .focus .lines{grid-template-rows:0 minmax(0,1fr) 0;gap:0}.focus .lines :global(.lyric-row:not(.current)){visibility:hidden}
  p { margin: 0; white-space: normal; overflow-wrap: anywhere; }
  .plain{overflow:hidden;min-height:0;display:flex;flex-direction:column;justify-content:center;gap:var(--lyric-gap,4px);}.plain p{white-space:pre-wrap;font:var(--lyric-weight,600) var(--lyric-size,20px)/var(--lyric-leading,1.16) var(--lyric-font,var(--font-sans));text-align:var(--lyric-align,left);}
  .timed{margin-right:auto;margin-left:6px;padding:0 6px;height:18px;font:500 10px/18px var(--font-sans);background:var(--surface-control-hover);border-radius:6px}
  .pages{display:inline-flex;align-items:center;gap:4px;flex:none}.pages button{width:24px;height:20px;font-size:20px;line-height:18px}.pages button:disabled{opacity:.3}
  .notice { display: grid; align-content: center; gap: 8px; font: var(--type-body); }
  .notice button { justify-self: start; }
  footer { display:flex;align-items:center;justify-content:space-between;gap:6px;font: 400 10px/14px var(--font-sans); opacity: .65; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
