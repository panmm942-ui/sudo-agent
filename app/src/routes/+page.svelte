<script lang="ts">
  import { applyDesign, startGlisseProgress } from '../theme/glisse';
  import { onMount, untrack } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Island from "../Island.svelte";
  import { installDevKeys, jumpTo } from "../dev/devKeys";
  import { installHotkeyBridge } from "../dev/hotkeyBridge";
  import { SCRIPTS, run, type MockScriptName } from "../dev/mockDriver";
  import type { AlarmInfo, IslandEvent } from "../island/events";
  import { ISLAND_STATES, type IslandState } from "../island/machine";
  import { watchSpotify, type MediaAction, type SpotifyTrack } from "../island/spotify";
  import { loadPreferences, PAGE_KEY, SETTINGS_KEY, type Preferences } from "../island/preferences";
  import { readMedia, writeMedia } from "../island/localMedia";
  import { createRoutine } from "../routine/scheduler";
  import {iphoneConnection} from '../routine/iphoneConnection';
  import {cloudSchedule,createCompanionSync,type CloudState} from '../island/onlineCompanion';
  import {STORAGE_KEY as ROUTINE_KEY} from '../routine/scheduler';
  import { createAlarmAudio } from "../sound/alarmAudio";
  import {FOCUS_KEY,focusRemaining,freshFocus,loadFocus,pauseFocus,startFocus} from '../island/homeWidgets';
  import {latestRelease,newerVersion,currentVersion,downloadUpdate,installUpdate,type DownloadedUpdate,type Release} from '../island/updates';

  // Test/dev URL params: ?state=<IslandState>&frozen=1&backdrop=fixture&mock=voice&demo=1
  const params = new URLSearchParams(location.search);
  const stateParam = params.get("state");
  const initialState: IslandState = ISLAND_STATES.find((s) => s === stateParam) ?? "dormant";
  // Jumped-to states carry sample payload so their content renders (goldens, dev).
  const initial = jumpTo(initialState);
  const frozen = params.get("frozen") === "1";
  const useFixture = params.get("backdrop") === "fixture";
  const mockVoice = params.get("mock") === "voice";
  const demo = params.get("demo") === "1";
  // Outside Tauri the window is a plain page: paint the fixture behind the canvas so the
  // screenshot shows what the glass refracts.
  const showWallpaper = useFixture && !isTauri();
  // In Tauri (without the fixture) the glass refracts the live desktop.
  const live = isTauri() && !useFixture;
  const routineEnabled = !frozen && (live || params.get("routine") === "1");

  let island = $state<Island>();
  let backdrop = $state.raw<ImageBitmap | null>(null);
  let wallpaperUrl = $state<string | null>(null);
  let ready = $state(!useFixture);
  let upcoming = $state.raw<AlarmInfo[]>([]);
  let track = $state.raw<SpotifyTrack | null>(null);
  let preferences = $state.raw(loadPreferences(localStorage));
  $effect(()=>{
    applyDesign(preferences.islandDesign);
    if(preferences.islandDesign!=='glisse')return;
    return startGlisseProgress();
  });
  $effect(()=>{if(live)void invoke('set_spotify_bridge_enabled',{enabled:preferences.spotifyBridge}).catch(error=>console.warn('Local Spotify connection:',error));});
  $effect(()=>{
    if(!live||frozen)return;
    const enabled=preferences.iphoneAlarms;
    const schedule={alarms:preferences.alarms,program:preferences.program,reminders:preferences.reminders,gradualSleep:preferences.gradualSleep};
    void iphoneConnection.update(enabled,schedule);
  });
  onMount(()=>{
    if(!live||frozen)return;
    const timer=setInterval(()=>void iphoneConnection.refresh(),30_000);
    return()=>{clearInterval(timer);void iphoneConnection.update(false,preferences);};
  });
  let optimistic = $state.raw<{ key: string; playing: boolean } | null>(null);
  const shownTrack = $derived(track && optimistic?.key === track.key ? { ...track, playing: optimistic.playing } : track);
  const trackKey = $derived(track?.key);
  let mediaWatcher: ReturnType<typeof watchSpotify> | undefined;
  let extrasVisible=$state(false);
  let reconcileTimer: ReturnType<typeof setTimeout> | undefined;
  let soundRevision = $state(0);
  let videoRevision = $state(0);
  let sound = $state.raw<{ url: string; name: string } | undefined>();
  let clip = $state.raw<{ url: string; name: string; key: string } | undefined>();
  const alarmAudio = createAlarmAudio();
  let pauseTask: { key?: string; result: Promise<boolean> } | undefined;
  let resumeTimer: ReturnType<typeof setTimeout> | undefined;
  let routine: ReturnType<typeof createRoutine> | undefined;
  let startupEnabled = $state<boolean | null>(null);
  let startupBusy = $state(false);
  let startupError = $state("");
  // svelte-ignore state_referenced_locally
  let focus=$state.raw(localStorage.getItem(FOCUS_KEY)?loadFocus(localStorage.getItem(FOCUS_KEY)):freshFocus(preferences.focusMinutes));
  let focusNow=$state(Date.now());
  let focusPending=$state(false);
  let companionState=$state.raw<CloudState|null>(null),companionMessage=$state('');
  let companionSync:ReturnType<typeof createCompanionSync>|undefined,companionBusy=false,companionTimer:ReturnType<typeof setTimeout>|undefined;
  async function syncCompanion(){
    if(!live||frozen||companionBusy)return;
    companionBusy=true;
    try{
      const status=await invoke<{configured:boolean}>('companion_status');
      if(!status.configured){companionSync?.stop();companionSync=undefined;companionState=null;companionMessage='';return;}
      companionSync??=createCompanionSync((path,method,body)=>invoke('companion_request',{path,method,body}),()=>{
        const raw=localStorage.getItem(ROUTINE_KEY),saved=raw?JSON.parse(raw):null;
        if(saved&&typeof saved.start!=='string')throw new Error('The routine start date is invalid.');
        return {note:preferences.homeNote,focus,minutes:preferences.focusMinutes,schedule:cloudSchedule(preferences,saved?.start??null)};
      },state=>{
        companionState=state;
        const next={remaining:state.focus.remaining,deadline:state.focus.deadline};
        if(JSON.stringify(focus)!==JSON.stringify(next)){focus=next;focusPending=false;focusNow=Date.now();saveFocus();}
        if(preferences.homeNote!==state.note.text||preferences.focusMinutes!==state.focus.minutes){preferences={...preferences,homeNote:state.note.text,focusMinutes:state.focus.minutes};localStorage.setItem(SETTINGS_KEY,JSON.stringify(preferences));}
      });
      await companionSync.refresh();companionMessage='Synced with your online companion.';
    }catch(error){companionMessage=String(error);}finally{companionBusy=false;}
  }
  function queueCompanion(){if(!live||frozen)return;clearTimeout(companionTimer);companionTimer=setTimeout(()=>void syncCompanion(),600);}
  onMount(()=>{
    if(!live||frozen)return;
    void syncCompanion();const timer=setInterval(()=>void syncCompanion(),15_000);
    return()=>{clearInterval(timer);clearTimeout(companionTimer);companionSync?.stop();};
  });
  const remaining=$derived(focusRemaining(focus,focusNow));
  let updateBusy=$state(false);
  let updateMessage=$state('');
  let release=$state.raw<Release|null>(null);
  let downloadedUpdate=$state<DownloadedUpdate|null>(null);
  let updateDownloading=$state(false),updateInstalling=$state(false);
  async function downloadNewVersion(){
    if(!release||updateDownloading)return;
    updateDownloading=true;updateMessage='Downloading and verifying update…';
    try{downloadedUpdate=await downloadUpdate(release);updateMessage=`${downloadedUpdate.version} is ready. Click Install update to open setup.`;}
    catch(error){updateMessage=String(error);}
    finally{updateDownloading=false;}
  }
  async function installNewVersion(){
    if(!downloadedUpdate||updateInstalling)return;
    updateInstalling=true;
    try{await installUpdate(downloadedUpdate);downloadedUpdate=null;release=null;updateMessage='Windows setup opened. Follow its steps; close Sudo if setup asks you to.';}
    catch(error){updateMessage=String(error);}
    finally{updateInstalling=false;}
  }
  function saveFocus(){localStorage.setItem(FOCUS_KEY,JSON.stringify(focus));}
  function controlFocus(action:'toggle'|'reset'){
    focusNow=Date.now();focusPending=false;
    focus=action==='reset'?freshFocus(preferences.focusMinutes):focus.deadline===null?startFocus(focus,focusNow,preferences.focusMinutes):pauseFocus(focus,focusNow);
    saveFocus();
    queueCompanion();
  }
  async function checkUpdates(){
    if(updateBusy||updateDownloading||updateInstalling)return;
    downloadedUpdate=null;updateBusy=true;updateMessage='Checking GitHub…';release=null;
    try{const latest=await latestRelease();release=newerVersion(latest.version,currentVersion)?latest:null;updateMessage=release?`Update available: ${latest.version}. Installed: ${currentVersion}. ${latest.downloadable?'Click Download update below.':'Open GitHub releases to download it.'}`:`No update needed. Installed: ${currentVersion} · Latest release: ${latest.version}.`;}
    catch(error){updateMessage=error instanceof Error?error.message:'Could not reach GitHub.';}
    finally{updateBusy=false;}
  }
  async function openReleases(){
    try{if(!await invoke<boolean>('open_release_page'))throw new Error();}
    catch{updateMessage='Could not open your browser. Visit this project’s GitHub Releases page.';}
  }
  $effect(()=>{
    if(frozen || (focus.deadline===null&&!focusPending))return;
    const tick=()=>{
      focusNow=Date.now();
      if(focus.deadline!==null&&focusRemaining(focus,focusNow)===0){focus={remaining:0,deadline:null};focusPending=true;saveFocus();}
      if(focusPending&&island&&['dormant','compact','expanded'].includes(island.getState().state)){
        focusPending=false;
        island.dispatch({type:'alarm_ring',id:'focus-finished',label:'Focus complete',time:new Date().toLocaleTimeString([],{hour:'2-digit',minute:'2-digit'})});
      }
    };
    untrack(tick);
    const timer=setInterval(tick,1000);
    return()=>clearInterval(timer);
  });
  onMount(()=>{if(live&&!frozen&&preferences.checkUpdates)void checkUpdates();});

  async function updateStartup(enabled: boolean): Promise<void> {
    if (startupBusy) return;
    startupBusy = true;
    startupError = "";
    try { startupEnabled = await invoke<boolean>("set_startup_enabled", { enabled }); }
    catch (error) {
      startupError = typeof error === "string" && error.includes("Windows Startup Apps") ? error : "Windows couldn’t save the startup change. Check Windows Startup Apps and try again.";
      try { startupEnabled = await invoke<boolean>("startup_enabled"); } catch { /* Keep last confirmed state. */ }
    }
    finally { startupBusy = false; }
  }

  function updatePreferences(patch: Partial<Preferences>): void {
    if (patch.lyricRomanization === true) patch = {...patch,lyricTranslation:false};
    else if (patch.lyricTranslation === true) patch = {...patch,lyricRomanization:false};
    if(patch.focusMinutes!==undefined){patch={...patch,focusMinutes:Math.max(1,Math.min(180,Math.round(patch.focusMinutes)||25))};if(focus.deadline===null){focus=freshFocus(patch.focusMinutes);saveFocus();}}
    if(patch.tabs&&preferences.compactTab.startsWith('tab:')&&!patch.tabs.some(t=>'tab:'+t.id===preferences.compactTab))patch={...patch,compactTab:'auto'};
    preferences = { ...preferences, ...patch };
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(preferences));
    queueCompanion();
    if (!preferences.rememberPage) localStorage.removeItem(PAGE_KEY);
  }

  $effect(() => {
    if (live) {
      void soundRevision;
      sound = undefined;
      return loadAsset("alarm-sound", (file) => (sound = file));
    }
  });

  $effect(() => {
    const key = trackKey;
    void videoRevision;
    clip = undefined;
  });

  function loadAsset(key: string, update: (asset: { url: string; name: string }) => void) {
    let obsolete = false;
    let url: string | undefined;
    void readMedia(key).then((file) => {
      if (obsolete || !file) return;
      url = URL.createObjectURL(file);
      update({ url, name: file.name });
    }).catch(console.warn);
    return () => { obsolete = true; if (url) URL.revokeObjectURL(url); };
  }

  async function saveFile(kind: "sound" | "video", file?: File): Promise<void> {
    const isSound = kind === "sound";
    if (!isSound && !trackKey) throw new Error("Select a track in Spotify first.");
    if (file && (!file.size || file.size > (isSound ? 20 : 120) * 1024 * 1024)) {
      throw new Error(isSound ? "Choose an audio file up to 20 MB." : "Choose a video up to 120 MB.");
    }
    if (file && !(isSound ? /\.(mp3|wav|ogg|flac|m4a|aac|opus)$/i : /\.(mp4|webm)$/i).test(file.name)) {
      throw new Error(isSound ? "Choose a supported audio file." : "Choose MP4 or WebM.");
    }
    await writeMedia(isSound ? "alarm-sound" : `video:${trackKey}`, file);
    if (isSound) { soundRevision++; if (file) updatePreferences({ tone: "custom" }); }
    else {
      videoRevision++;
      if (file) updatePreferences({ video: true });
    }
  }

  function testAlarm(): void {
    if (!island || island.getState().state === "alarm") return;
    const time = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
    island.dispatch({ type: "alarm_ring", id: "routine-preview", label: "Test alarm", time });
  }

  function acknowledgeAlarm(id: string, snooze: boolean): void {
    if(id==='focus-finished'){
      if(snooze){focusNow=Date.now();focus={remaining:preferences.snooze*60_000,deadline:focusNow+preferences.snooze*60_000};saveFocus();}
      return;
    }
    routine?.acknowledge(id, snooze, Date.now(), preferences.snooze);
  }

  async function mediaControl(action: MediaAction): Promise<boolean> {
    const pending = action === "toggle" && shownTrack ? { key: shownTrack.key, playing: !shownTrack.playing } : null;
    // In vocal mode ~1 s of processed audio is queued; drop it so the press is heard at once.
    if (action === "next" || action === "previous" || (action === "toggle" && shownTrack?.playing)) void invoke("vocal_flush").catch(() => {});
    if (pending) optimistic = pending;
    try {
      const accepted = await invoke<boolean>("spotify_control", { action,trackKey:shownTrack?.key });
      if (!accepted && optimistic === pending) optimistic = null;
      return accepted;
    } catch (error) {
      if (optimistic === pending) optimistic = null;
      throw error;
    } finally {
      mediaWatcher?.refresh();
      if (pending && optimistic === pending) {
        clearTimeout(reconcileTimer);
        reconcileTimer = setTimeout(() => { if (optimistic === pending) optimistic = null; }, 2100);
      }
    }
  }

  function alarmChanged(ringing: boolean): void {
    clearTimeout(resumeTimer);
    if (ringing) {
      alarmAudio.play(preferences, sound?.url);
      if (live && preferences.pauseSpotify && !pauseTask) {
        pauseTask = {
          key: track?.key,
          result: invoke<boolean>("spotify_control", { action: "pause" })
            .catch(() => false).finally(() => mediaWatcher?.refresh()),
        };
      }
      return;
    }
    alarmAudio.stop();
    resumeTimer = setTimeout(async () => {
      const paused = pauseTask;
      if (!paused) return;
      const accepted = await paused.result;
      if (island?.getState().state === "alarm" || pauseTask !== paused) return;
      pauseTask = undefined;
      if (accepted && paused.key && preferences.resumeSpotify) {
        await invoke("spotify_control", { action: "play", trackKey: paused.key }).catch(console.warn);
        mediaWatcher?.refresh();
      }
    }, 1100);
  }

  onMount(() => {
    if (!live || frozen) return;
    void invoke<boolean>("startup_enabled").then((enabled) => (startupEnabled = enabled))
      .catch(() => (startupError = "Couldn’t read Windows startup settings. Restart Sudo to try again."));
    mediaWatcher = watchSpotify((next) => {
      track = next;
      if (optimistic && (next?.key !== optimistic.key || next.playing === optimistic.playing)) optimistic = null;
    },()=>({like:false,repeat:extrasVisible&&preferences.spotifyShowRepeat,shuffle:extrasVisible&&preferences.spotifyShowShuffle}));
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenAvoid: (() => void) | undefined;
    let unlistenPointer:(()=>void)|undefined;
    let unlistenJarvis:(()=>void)|undefined;
    let unlistenLevels:(()=>void)|undefined;
    let unlistenCycle:(()=>void)|undefined;
    listen<boolean>('island:reveal-zone',event=>island?.setRevealZone(event.payload)).catch(console.warn);
    listen<boolean>('island:pointer-inside',event=>island?.setPointerInside(event.payload)).then(off=>{if(disposed)off();else unlistenPointer=off;}).catch(console.warn);
    listen<boolean | {active:boolean;offset:number;overIsland?:boolean}>("island:edge-avoid", (event) => {
      if(typeof event.payload === "boolean") island?.setAvoiding(event.payload);
      else island?.setAvoiding(event.payload.active,event.payload.offset,event.payload.overIsland);
    }).then((off) => {
      if (disposed) off(); else unlistenAvoid = off;
    }).catch(console.warn);
    listen("jarvis_setup_needed", () => island?.openJarvisSettings()).then((off) => {
      if (disposed) off(); else unlistenJarvis = off;
    }).catch(console.warn);
    listen<{ source?: "mic" | "tts"; rms?: number; centroid?: number; pitchHz?: number; onset?: boolean }>("jarvis_level", (event) => {
      const p = event.payload;
      island?.pushVoiceLevel({ rms: p.rms ?? 0, centroid: p.centroid ?? 0, pitchHz: p.pitchHz ?? 0, onset: !!p.onset, source: p.source === "tts" ? "tts" : "mic" });
    }).then((off) => {
      if (disposed) off(); else unlistenLevels = off;
    }).catch(console.warn);
    listen("video:cycle-background", () => island?.cycleBackgroundSource()).then((off) => {
      if (disposed) off(); else unlistenCycle = off;
    }).catch(console.warn);
    listen("island:edge-reveal", () => island?.reveal()).then((off) => {
      if (disposed) off();
      else unlisten = off;
    }).catch(console.warn);
    return () => {
      disposed = true;
      mediaWatcher?.stop();
      clearTimeout(reconcileTimer);
      clearTimeout(resumeTimer);
      alarmAudio.stop();
      unlisten?.();
      unlistenAvoid?.();
      unlistenPointer?.();
      unlistenJarvis?.();
      unlistenLevels?.();
      unlistenCycle?.();
    };
  });

  onMount(() => {
    if (!useFixture) return;
    loadFixture()
      .catch((err) => console.error("island: fixture backdrop failed", err))
      .finally(() => (ready = true));
  });

  // Event sources: the Right-Ctrl hotkey (Tauri only), dev shortcuts and the mock driver.
  $effect(() => {
    const target = island;
    if (!target) return;
    const dispatch = (e: IslandEvent) => target.dispatch(e);
    const ringing = () => target.getState().state === "alarm";
    const scheduledAlarm = () => ringing() && (target.getState().payload.alarm as AlarmInfo)?.id.startsWith("routine:");
    let stopScript: (() => void) | null = null;
    const script = (name: MockScriptName) => {
      stopScript?.();
      stopScript = run(SCRIPTS[name], (e) => { if (!scheduledAlarm()) dispatch(e); });
    };
    const offHotkey = installHotkeyBridge(dispatch);
    // Developer shortcuts (1–9, D/C/A) run fake scripts such as "Delete 3 backups?"; never in a release.
    const offKeys = frozen || !import.meta.env.DEV ? () => {} : installDevKeys({
      jump: (s) => { if (!scheduledAlarm()) target.jump(jumpTo(s)); },
      script,
    });
    if (demo && !frozen) script("demo");
    let timer: ReturnType<typeof setInterval> | undefined;
    if (routineEnabled) {
      routine = createRoutine(localStorage, new Date());
      let last = "";
      const tick = () => {
        const result = routine?.tick(new Date(), preferences);
        if (!result) return;
        const key = JSON.stringify(result.next);
        if (key !== last) {
          upcoming = result.next;
          last = key;
        }
        if (result.due && !ringing()) dispatch({ type: "alarm_ring", ...result.due });
      };
      untrack(tick);
      timer = setInterval(tick, 1000);
    }
    return () => {
      clearInterval(timer);
      stopScript?.();
      offHotkey();
      offKeys();
    };
  });

  async function loadFixture(): Promise<void> {
    const { default: url } = await import("../../tests/visual/fixtures/wallpaper.png?url");
    const blob = await (await fetch(url)).blob();
    if (showWallpaper) {
      const img = new Image();
      img.src = url;
      await img.decode();
      wallpaperUrl = url;
    }
    backdrop = await createImageBitmap(blob);
  }

  onMount(()=>{
    const changed=(event:StorageEvent)=>{if(event.key===SETTINGS_KEY)preferences=loadPreferences(localStorage);};
    window.addEventListener('storage',changed);
    return ()=>window.removeEventListener('storage',changed);
  });

  async function onready(): Promise<void> {
    await document.fonts?.ready;
    document.body.dataset.islandReady = "1";
    // Shows the (initially hidden) Tauri window only once the first frame has been painted.
    if (isTauri()) invoke("island_ready").catch((err) => console.error("island_ready failed", err));
  }
</script>

{#if wallpaperUrl}
  <img class="wallpaper" src={wallpaperUrl} alt="" />
{/if}

{#if ready}
  <Island bind:this={island} {initial} {frozen} {backdrop} {mockVoice} {live} {onready}
    desktopControls={live && !frozen} track={shownTrack} onmediacontrol={mediaControl}
    onextrasactive={active=>extrasVisible=active}
    onspotifyrefresh={()=>mediaWatcher?.refresh()}
    {preferences} onpreferences={updatePreferences} onalarmchange={alarmChanged}
    focusRemaining={remaining} focusRunning={focus.deadline!==null} onfocus={controlFocus}
    settingsProps={{ soundName: sound?.name, videoName: clip?.name, trackTitle: track?.title,
      companionState,companionMessage,oncompanionsync:()=>void syncCompanion(),
      startupEnabled, startupBusy, startupError, onstartup: updateStartup,
      updateBusy,updateMessage,oncheckupdates:checkUpdates,onreleases:openReleases,
      updateAvailable:!!release?.downloadable,updateDownloading,updateInstalling,updateReady:!!downloadedUpdate,
      ondownloadupdate:downloadNewVersion,oninstallupdate:installNewVersion,
      onupdate: updatePreferences, onfile: saveFile, ontest: testAlarm,
      onpreview: (ended) => alarmAudio.play(preferences, sound?.url, ended),
      onstoppreview: () => { if (island?.getState().state !== "alarm") alarmAudio.stop(); } }}
    {upcoming} ontestalarm={routineEnabled ? testAlarm : undefined} onalarmdismiss={acknowledgeAlarm} />
{/if}

<style>
  :global(html, body) {
    margin: 0;
    background: transparent;
    overflow: hidden;
  }
  .wallpaper {
    position: fixed;
    inset: 0;
    width: 100vw;
    height: 100vh;
    display: block;
  }
</style>
