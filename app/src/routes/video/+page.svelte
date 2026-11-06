<script lang="ts">
  import { onMount,untrack } from 'svelte';
  import { invoke, isTauri } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { emit } from '@tauri-apps/api/event';
  import Lyrics from '../../content/Lyrics.svelte';
  import {watchSpotify,type SpotifyTrack,type MediaAction} from '../../island/spotify';
  import {loadPreferences,SETTINGS_KEY,type Preferences} from '../../island/preferences';
  import { attachArtworkVideo } from '../../island/artworkPlayback';
  import { validArtworkImageUrl, validArtworkUrl } from '../../island/artwork';
  import {attachVideoSync} from '../../island/videoSync';
  import {playbackSeconds} from '../../island/lyricTiming';
  import {createWheelSeek,seekTarget} from '../../island/wheelSeek';
  import { closeVideoView, getVideoView, listenVideoView, validVideoInput, validVideoViewUrl, type VideoViewState } from '../../island/videoWindow';
  import '../../theme/tokens.css';

  let view = $state<VideoViewState | null>(null);
  let video = $state<HTMLVideoElement>();
  let ready = $state(false);
  let ambient = $state<HTMLCanvasElement>();
  let ratio = $state(1);
  let failed = $state(false);
  let reader = $state(false);
  let lyricsOnly=$state(false);
  let track = $state<SpotifyTrack|null>(null);
  let preferences = $state(loadPreferences(localStorage));
  let actionPending = $state(false);
  let actionError = $state('');
  let refresh: (()=>void)|undefined;
  async function control(action:MediaAction) {
    if(actionPending||!track)return;
    actionPending=true;actionError='';
    try { if(!await invoke<boolean>('spotify_control',{action,trackKey:track.key}))throw Error('Spotify could not complete that action.'); }
    catch(error){actionError=String(error);}
    finally {actionPending=false;refresh?.();revealControls();}
  }
  function updatePreferences(patch:Partial<Preferences>) {
    preferences={...loadPreferences(localStorage),...patch};
    localStorage.setItem(SETTINGS_KEY,JSON.stringify(preferences));
  }
  let controlsVisible = $state(true);
  let controlTimer: ReturnType<typeof setTimeout> | undefined;
  let sourceHintVisible = $state(false);
  let sourceHintTimer: ReturnType<typeof setTimeout> | undefined;
  let playRevision = 0;
  let disposed = false;
  let myMode: 'fullscreen' | 'desktop' = 'fullscreen';
  let synchronizer:ReturnType<typeof attachVideoSync>|undefined;
  const sourceUrl = $derived(view?.url);
  const sourceQuality = $derived(view?.quality ?? 'best');
  /** Apple Music's 1200×1200 cover for this song; Spotify's own thumbnail is small. */
  let appleStill=$state<{key:string;url:string}|null>(null);
  let stillRequested='';
  $effect(()=>{
    const t=track;
    if(!t?.title||!t.artist||!t.album)return;
    const key=t.key;
    if(stillRequested===key)return;
    stillRequested=key;
    requestStill({key,title:t.title,artist:t.artist,album:t.album,duration:t.duration??null},1);
  });
  /** Asks again (three tries, 4 s apart) when Apple was busy or the connection was slow. */
  function requestStill(song:{key:string;title:string;artist:string;album:string;duration:number|null},attempt:number){
    const retry=()=>{if(attempt<3&&stillRequested===song.key)setTimeout(()=>{if(stillRequested===song.key)requestStill(song,attempt+1);},4000);};
    void invoke<string|null>('artwork_still',{track:song})
      .then(url=>{if(stillRequested!==song.key)return;if(validArtworkImageUrl(url))appleStill={key:song.key,url};else if(url===null&&attempt===1)retry();})
      .catch(retry);
  }
  const showing=$derived(!sourceUrl||lyricsOnly?'Cover':validArtworkUrl(sourceUrl)?'Apple Music video':'Spotify video');
  const artwork=$derived(track&&appleStill?.key===track.key?appleStill.url:track?.artwork??null);
  const videoClock=$derived(view?.clock??(track?{...track,pauseWithSong:view?.pauseWithSong}:null));
  const lyricTrack=$derived(track&&(!view?.clock||view.clock.key===track.key)?{...track,...(view?.clock??{})}:null);

  function revealControls() {
    controlsVisible = true;
    clearTimeout(controlTimer);
    controlTimer = setTimeout(() => { controlsVisible = false; }, 2400);
  }
  function fail() {
    failed = true;
    ready = false;
    if (myMode === 'desktop') void closeVideoView().catch(() => {});
  }
  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') { event.preventDefault(); void closeVideoView(); }
    const interactive=event.target instanceof Element&&!!event.target.closest('button,input,textarea,select,[contenteditable="true"]');
    if(event.code==='Space'&&!interactive&&!event.repeat){event.preventDefault();void control('toggle');}
    if(event.code==='KeyB'&&!interactive&&!event.repeat&&!event.ctrlKey&&!event.altKey&&!event.metaKey){event.preventDefault();cycleSource();}
    revealControls();
  }
  // What plays behind: the cover, Apple Music's video or Spotify's video. The island skips
  // sources this song doesn't have; with neither, a tap changes nothing.
  function cycleSource(){
    if(lyricsOnly)return;
    void emit('video:cycle-background');
    sourceHintVisible=true;clearTimeout(sourceHintTimer);
    sourceHintTimer=setTimeout(()=>{sourceHintVisible=false;},1400);
  }
  // Scrolling over the lyrics moves through the song: down = forward, up = back.
  // A burst of scrolling becomes one seek, sent once the wheel rests.
  const wheelSeek=createWheelSeek();
  let seekPending=$state<{key:string;seconds:number;offset:number}|null>(null);
  let seekTimer:ReturnType<typeof setTimeout>|undefined;
  let seekHintTimer:ReturnType<typeof setTimeout>|undefined;
  let seekHint=$state('');
  const clockText=(seconds:number)=>`${Math.floor(seconds/60)}:${String(Math.floor(seconds%60)).padStart(2,'0')}`;
  function lyricWheel(node:HTMLElement){
    const listener=(event:WheelEvent)=>{
      if(event.ctrlKey||Math.abs(event.deltaX)>Math.abs(event.deltaY))return;
      event.preventDefault();
      const song=track;if(!song?.key||typeof song.duration!=='number')return;
      const step=wheelSeek.add(event.deltaY,event.deltaMode);if(!step)return;
      const base=seekPending?.key===song.key?seekPending:{key:song.key,seconds:playbackSeconds(song,Date.now()),offset:0};
      const seconds=seekTarget(base.seconds,step,song.duration);
      seekPending={key:song.key,seconds,offset:base.offset+(seconds-base.seconds)};
      const offset=Math.round(seekPending.offset);
      seekHint=`${offset>0?'+':offset<0?'−':''}${Math.abs(offset)} s · ${clockText(seconds)}`;
      clearTimeout(seekHintTimer);clearTimeout(seekTimer);
      seekTimer=setTimeout(()=>void commitSeek(),300);
    };
    node.addEventListener('wheel',listener,{passive:false});
    return {destroy(){node.removeEventListener('wheel',listener);}};
  }
  async function commitSeek(){
    const target=seekPending;seekPending=null;wheelSeek.reset();
    seekHintTimer=setTimeout(()=>{seekHint='';},900);
    if(!target||track?.key!==target.key)return;
    try{
      void invoke('vocal_flush').catch(()=>{});
      if(!await invoke<boolean>('spotify_seek',{seconds:target.seconds,trackKey:target.key}))throw Error('Spotify couldn’t jump to that moment.');
    }catch(error){actionError=error instanceof Error?error.message:String(error);}
    finally{refresh?.();}
  }
  let pressAt:{x:number;y:number}|null=null;
  function pointerdown(event:PointerEvent){pressAt=event.button===0?{x:event.clientX,y:event.clientY}:null;}
  /** A plain tap on the picture switches the source; controls, lyrics, drags and text selection don't. */
  function tap(event:MouseEvent){
    const start=pressAt;pressAt=null;
    if(event.button!==0||!start||Math.hypot(event.clientX-start.x,event.clientY-start.y)>8)return;
    if(event.target instanceof Element&&event.target.closest('button,a,input,textarea,select,label,[contenteditable="true"],[role="slider"],.controls,.reader-lyrics,.song-info,.action-error'))return;
    if(String(window.getSelection()??'').length)return;
    cycleSource();revealControls();
  }

  onMount(() => {
    if (!isTauri()) { failed = true; return; }
    myMode = getCurrentWindow().label === 'video-desktop' ? 'desktop' : 'fullscreen';
    let off: (() => void) | undefined;
    let eventRevision=0;
    const update = (next: VideoViewState | null) => {
      if (!disposed && next?.mode === myMode && validVideoInput(next)) {
        if(!view||next.presentation==='lyrics'&&view.presentation!=='lyrics'){
          lyricsOnly=next.presentation==='lyrics';reader=false;
        }
        view = next;
        failed = false; // the window is reused between opens: forget a previous error
      }
      else if (!disposed) view = null;
    };
    void listenVideoView(next=>{++eventRevision;update(next);}).then(unlisten => {
      if (disposed) unlisten();
      else { off = unlisten; const initialRevision=eventRevision;void getVideoView().then(next=>{if(eventRevision===initialRevision)update(next);}).catch(fail); }
    }).catch(fail);
    const watcher=watchSpotify(value=>track=value);
    refresh=()=>watcher.refresh();
    const storage=(event:StorageEvent)=>{if(event.key===SETTINGS_KEY)preferences=loadPreferences(localStorage);};
    window.addEventListener('storage',storage);
    revealControls();
    return () => { watcher.stop();window.removeEventListener('storage',storage);disposed = true; ++playRevision; clearTimeout(controlTimer); clearTimeout(sourceHintTimer); clearTimeout(seekTimer); clearTimeout(seekHintTimer); off?.(); };
  });

  $effect(() => {
    const source = sourceUrl;
    const quality = sourceQuality;
    const element = video;
    ready = false;
    failed = false;
    if (!element || !source || !validVideoViewUrl(source)) return;
    let cancelled = false;
    const sourceFailed = () => { if (!cancelled) fail(); };
    const timeout = setTimeout(() => { if (!cancelled && element.readyState < 2) sourceFailed(); }, 15000);
    const cleanup = validArtworkUrl(source) ? attachArtworkVideo(element, source, sourceFailed, quality) : (() => {
      element.src = source;
      element.load();
      return () => { element.pause(); element.removeAttribute('src'); element.load(); };
    })();
    const sync=attachVideoSync(element,()=>untrack(()=>videoClock),{onerror:sourceFailed});synchronizer=sync;
    return () => { cancelled = true; clearTimeout(timeout); sync.stop();if(synchronizer===sync)synchronizer=undefined;cleanup(); };
  });
  $effect(() => {
    void videoClock;synchronizer?.update();
  });
  // Reuse decoded frames, keeping the ambient copy on the same playback clock.
  $effect(() => {
    const element=video, canvas=ambient;
    if(!element||!canvas||!ready||lyricsOnly)return;
    const context=canvas.getContext('2d',{alpha:false});
    if(!context)return;
    const scale=Math.min(1,640/Math.max(element.videoWidth,element.videoHeight));
    canvas.width=Math.max(1,Math.round(element.videoWidth*scale));
    canvas.height=Math.max(1,Math.round(element.videoHeight*scale));
    let stopped=false,callback=0;
    const paint=()=>{
      if(stopped)return;
      context.drawImage(element,0,0,canvas.width,canvas.height);
      callback=element.requestVideoFrameCallback(paint);
    };
    paint();
    return ()=>{stopped=true;element.cancelVideoFrameCallback(callback);};
  });
  function loaded() {
    synchronizer?.update();
    if(!video||video.seeking||video.readyState<2)return;
    ratio=video&&video.videoHeight?video.videoWidth/video.videoHeight:1;
    ready = true;
    failed = false;
    if (isTauri() && view?.url) void invoke('video_view_ready', { url: view.url }).catch(fail);
  }
</script>

<svelte:head><title>Sudo video</title></svelte:head>
<svelte:window onkeydown={keydown} onpointermove={revealControls} />

<!-- Pointer-only shortcut: the B key does the same for keyboard users. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<main class:reader class:lyrics-only={lyricsOnly} aria-label="Sudo video. Tap the picture or press B to switch between cover, Apple Music video and Spotify video" style:--video-ratio={ratio} onpointerdown={pointerdown} onclick={tap}>
  {#if sourceHintVisible&&!lyricsOnly}<p class="source-hint" role="status">{showing}</p>{/if}
  {#if artwork}<img class="ambient-art" src={artwork} alt="" aria-hidden="true"/>{/if}
  <canvas class="ambient" class:ready={ready&&!failed&&!lyricsOnly} bind:this={ambient} aria-hidden="true"></canvas>
  {#if sourceUrl&&!lyricsOnly}
    {#key `${sourceUrl}:${sourceQuality}`}<video class="sharp" bind:this={video} class:ready={ready&&!failed} muted loop playsinline crossorigin="anonymous" preload="auto" onloadeddata={loaded} onseeked={loaded} onerror={event=>{if(event.currentTarget===video)fail();}}></video>{/key}
  {/if}
  {#if (!sourceUrl||failed)&&!lyricsOnly}
    {#if artwork}<img class="sharp artwork ready" src={artwork} alt={`${track?.title??'Song'} artwork`}/>{:else}<div class="sharp artwork placeholder ready" aria-label="No song artwork"><svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M9 5v11a3 3 0 1 1-2-2.83V6l13-3v11a3 3 0 1 1-2-2.83V3.46Z"/></svg></div>{/if}
  {/if}
  {#if sourceUrl&&!ready&&!failed&&!lyricsOnly}<p class="status" role="status">Loading video…</p>{/if}
  {#snippet transport()}
    <div class="transport" aria-label="Fullscreen playback controls">
      <button type="button" aria-label="Previous track" disabled={actionPending||!track?.canPrevious} onclick={()=>control('previous')}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5.75a1.25 1.25 0 0 1 2.5 0v12.5a1.25 1.25 0 0 1-2.5 0zm13.2-.2a1.2 1.2 0 0 1 1.8 1.04v10.82a1.2 1.2 0 0 1-1.8 1.04l-8.7-5.41a1.22 1.22 0 0 1 0-2.08z"/></svg></button>
      <button type="button" class="play" aria-label={track?.playing?'Pause':'Play'} disabled={actionPending||!track?.canToggle} onclick={()=>control('toggle')}><svg viewBox="0 0 24 24" aria-hidden="true">{#if track?.playing}<rect x="6" y="4.5" width="4.5" height="15" rx="1.4"/><rect x="13.5" y="4.5" width="4.5" height="15" rx="1.4"/>{:else}<path d="M7.5 4.9a1.3 1.3 0 0 1 1.95-1.12l12 7.1a1.3 1.3 0 0 1 0 2.24l-12 7.1a1.3 1.3 0 0 1-1.95-1.12z"/>{/if}</svg></button>
      <button type="button" aria-label="Next track" disabled={actionPending||!track?.canNext} onclick={()=>control('next')}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M19 5.75a1.25 1.25 0 0 0-2.5 0v12.5a1.25 1.25 0 0 0 2.5 0zm-13.2-.2A1.2 1.2 0 0 0 4 6.59v10.82a1.2 1.2 0 0 0 1.8 1.04l8.7-5.41a1.22 1.22 0 0 0 0-2.08z"/></svg></button>
    </div>
  {/snippet}
  <div class="controls" class:visible={controlsVisible||reader} aria-hidden={!controlsVisible&&!reader} inert={!controlsVisible&&!reader}>{@render transport()}</div>
  {#if actionError}<p class="action-error" role="status">{actionError}</p>{/if}
  <button type="button" class="edge-switch" class:right={reader} class:visible={controlsVisible} aria-label={reader?'Return to full view':'Show artwork and lyrics'} onclick={()=>{reader=!reader;lyricsOnly=false;revealControls();}}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d={reader?'m14 6-6 6 6 6':'m10 6 6 6-6 6'} fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg></button>
  {#if reader}
    <div class="song-info"><strong>{track?.title||'Waiting for Spotify'}</strong><span>{track?.artist||''}</span></div>
  {/if}
  {#if seekHint}<p class="seek-hint" role="status">{seekHint}</p>{/if}
  {#if reader||lyricsOnly}<div class="reader-lyrics" use:lyricWheel>{#if lyricTrack?.title}<Lyrics track={lyricTrack} {preferences} onupdate={updatePreferences} onclose={()=>{reader=false;lyricsOnly=false;}} fullscreen />{:else}<p>{track?'Loading this song…':'Play a song in Spotify to see its lyrics.'}</p>{/if}</div>{/if}
</main>

<style>
  :global(html, body) { margin: 0; width: 100%; height: 100%; background: #000; overflow: hidden; }
  main { position: fixed; inset: 0; background: #000; color: var(--text-primary); font: var(--type-text); }
  .ambient,.ambient-art {position:absolute;inset:-140px;width:calc(100% + 280px);height:calc(100% + 280px);object-fit:cover;filter:blur(100px) brightness(.35);pointer-events:none;}
  .ambient{opacity:0;}
  .sharp.artwork{width:min(72vw,76vh);height:min(72vw,76vh);border-radius:18px;mask-image:none;box-shadow:0 16px 80px #0004;}
  .placeholder{display:grid;place-items:center;background:#ffffff0b;color:#ffffff65;}.placeholder svg{width:25%;height:25%;}
  .sharp {position:absolute;left:50%;top:50%;transform:translate(-50%,-50%);width:min(100vw,calc(100vh * var(--video-ratio)));height:min(100vh,calc(100vw / var(--video-ratio)));display:block;object-fit:contain;opacity:0;mask-image:linear-gradient(to right,transparent,black 3%,black 97%,transparent);}
  .ready {opacity:1;}
  .status { position: absolute; inset: 45% 24px auto; margin: 0; text-align: center; }
  .controls {position:absolute;left:50%;bottom:max(36px,6vh);transform:translateX(-50%);opacity:0;pointer-events:none;transition:opacity 220ms ease;}
  .controls.visible {opacity:1;pointer-events:auto;}
  .transport {display:flex;gap:20px;align-items:center;padding:6px 12px;border:1px solid #ffffff1c;border-radius:100px;background:#151b222e;backdrop-filter:blur(20px);box-shadow:0 8px 28px #00000018;}
  button {display:grid;place-items:center;width:44px;height:44px;padding:9px;border:0;border-radius:50%;background:transparent;color:#fff;cursor:pointer;transition:background-color 140ms ease,opacity 140ms ease;}
  button:hover {background:#ffffff14;}
  button:focus-visible {outline:2px solid #fff;outline-offset:4px;}
  button:disabled {cursor:default;opacity:.4;}
  button svg {width:23px;height:23px;fill:currentColor;filter:drop-shadow(0 1px 3px #0004);}
  .play {width:52px;height:52px;}
  .play svg {width:30px;height:30px;}
  .source-hint {position:absolute;z-index:3;left:50%;top:28px;transform:translateX(-50%);margin:0;padding:6px 14px;border-radius:999px;background:#0000006b;color:#fff;font:600 13px/18px var(--font-sans, system-ui);backdrop-filter:blur(12px);pointer-events:none;animation:source-hint 1400ms ease forwards;}
  @keyframes source-hint {0%{opacity:0}12%{opacity:1}75%{opacity:1}100%{opacity:0}}
  @media (prefers-reduced-motion: reduce) {.source-hint{animation:none}}
  .seek-hint {position:absolute;z-index:3;left:50%;bottom:96px;transform:translateX(-50%);margin:0;padding:6px 14px;border-radius:999px;background:#0000006b;color:#fff;font:600 13px/18px var(--font-sans, system-ui);font-variant-numeric:tabular-nums;backdrop-filter:blur(12px);pointer-events:none;}
  .edge-switch {position:absolute;left:8px;top:50%;transform:translateY(-50%);width:40px;height:70px;border-radius:18px;opacity:0;transition:opacity 220ms ease;background:#0b102330;}
  .edge-switch.visible,.edge-switch:focus-visible {opacity:.9;}
  .edge-switch.right {left:auto;right:8px;}
  .edge-switch svg {fill:none;}
  .song-info {position:absolute;left:10%;top:calc(50% + min(20vw,30vh) + 96px);width:40%;text-align:center;display:grid;gap:5px;}
  .song-info strong {font-size:clamp(17px,1.5vw,25px);white-space:nowrap;text-overflow:ellipsis;overflow:hidden;}
  .song-info span {font-size:clamp(14px,1.2vw,20px);opacity:.62;}
  .reader .sharp {left:30%;top:calc(50% - 40px);width:min(40vw,60vh);height:min(40vw,60vh);object-fit:cover;border-radius:14px;mask-image:none;box-shadow:0 16px 80px #0004;}
  .reader .controls {left:30%;top:calc(50% + min(20vw,30vh) + 6px);bottom:auto;transform:translate(-50%,-50%);}
  .reader .transport {padding:0;background:transparent;backdrop-filter:none;border-color:transparent;box-shadow:none;}
  .reader-lyrics {position:absolute;left:54%;right:9%;top:15%;bottom:15%;min-height:0;}
  .lyrics-only .reader-lyrics{left:15%;right:15%;top:10%;bottom:22%;}
  .action-error {position:absolute;bottom:24px;left:15%;right:15%;text-align:center;font-size:14px;}
  @media(max-width:700px) {.reader .sharp{left:50%;top:24%;width:min(64vw,32vh);height:min(64vw,32vh)}.reader .controls{left:50%;top:calc(24% + min(32vw,16vh) + 30px)}.reader-lyrics{left:9%;right:9%;top:55%;bottom:6%}.song-info{display:none}}
  @media(max-width:700px){.lyrics-only .reader-lyrics{left:8%;right:8%;top:9%;bottom:23%;}}
  @media(prefers-reduced-motion:reduce){.controls,.edge-switch{transition:none;}}
</style>
