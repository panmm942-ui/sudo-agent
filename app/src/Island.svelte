<script lang="ts">
  import "./theme/tokens.css";
  import { flushSync, onMount, tick, untrack, type ComponentProps, type Snippet } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { CONTENT, Crossfade, contentProps, type ContentHandlers } from "./content";
  import IslandNavigation from "./content/IslandNavigation.svelte";
  import CustomTab from './content/CustomTab.svelte';
  import CompactHome from './content/CompactHome.svelte';
  import HomeWidgets from "./content/HomeWidgets.svelte";
  import MiniOrb from "./content/MiniOrb.svelte";
  import SudoChat, { type ArtSpot, type ChatMessage, type SudoState } from "./content/SudoChat.svelte";
  import { finishMorph, morph, toCompact, type MorphShape } from "./island/morph";
  import type { IslandPage } from "./island/wheel";
  import FpsMeter from './content/FpsMeter.svelte';
  import {AnimationTiming,type FrameTiming} from './island/frameTiming';
  import Spotify from "./content/Spotify.svelte";
  import SpotifyEmpty from './content/SpotifyEmpty.svelte';
  import CompactSpotify from "./content/CompactSpotify.svelte";
  import Settings from "./content/Settings.svelte";
  import Lyrics from "./content/Lyrics.svelte";
  import TrackVideo from './content/TrackVideo.svelte';
  import {MotionClock} from './island/motionClock';
  import {createVideoTimeline} from './island/videoTimeline';
  import {createVideoLookup,type VideoResult} from './island/videoSource';
  import {openVideoView,syncVideoView,closeVideoView,getVideoView,listenVideoView,prepareVideoView,type VideoViewState,type VideoViewMode} from './island/videoWindow';
  import { watchSpotifyLevels } from "./island/levels";
  import { GlassRenderer, PILL_TOP, TINT_RGB, type BackdropSource, type PillRect } from "./gl/renderer";
  import { stepMaterial, type Material } from "./gl/material";
  import { setAnimating, subscribe as subscribeBackdrop } from "./gl/backdrop";
  import { TINT_ALPHA_BRIGHT, TINT_ALPHA_DARK, smoothTint, tintAlphaFor } from "./gl/tint";
  import { setPillRect } from "./island/bridge";
  import type { IslandEvent } from "./island/events";
  import { setIslandFocus } from "./island/focus";
  import { NEUTRAL_COVER_TINT, tintFromCover, type MediaAction, type SpotifyTrack, type Tint } from "./island/spotify";
  import { createWheelNavigation } from "./island/wheel";
  import { DEFAULTS, PAGE_KEY, type Preferences, type SettingsTab } from "./island/preferences";
  import {
    SIZES,
    heightFor,
    initialState,
    reduce,
    springFor,
    type IslandState,
    type MachineState,
  } from "./island/machine";
  import { SPRINGS, Spring, type SpringCfg } from "./island/spring";
  import { OrbSmoother, type OrbFeatures, type OrbMode } from "./orb/features";
  import { MockVoice } from "./orb/mock";
  import { OrbPass } from "./orb/orbPass";
  import { sfx, sfxForTransition } from "./sound/sfx";

  type Props = {
    preferences?: Preferences;
    focusRemaining?: number;
    focusRunning?: boolean;
    onfocus?: (action:'toggle'|'reset')=>void;
    onpreferences?: (patch: Partial<Preferences>) => void;
    settingsProps?: Omit<ComponentProps<typeof Settings>, "preferences" | "tab" | "onclose">;
    onalarmchange?: (ringing: boolean) => void;
    desktopControls?: boolean;
    track?: SpotifyTrack | null;
    onmediacontrol?: (action: MediaAction) => Promise<boolean>;
    onextrasactive?: (active:boolean)=>void;
    onspotifyrefresh?:()=>void;
    upcoming?: { label: string; time: string }[];
    ontestalarm?: () => void;
    onalarmdismiss?: (id: string, snooze: boolean) => void;
    /** State (or full machine state, e.g. with sample payload) to start in. */
    initial?: IslandState | MachineState;
    /** Springs snap to their targets and shader time stays at 0 (visual goldens). */
    frozen?: boolean;
    /** Image behind the island; null → tinted glass only. */
    backdrop?: BackdropSource;
    /** Feed the orb from the deterministic mock voice (`?mock=voice`). */
    mockVoice?: boolean;
    /** Stream the live desktop backdrop (WGC, Tauri only) instead of `backdrop`. */
    live?: boolean;
    /** Replaces the built-in state content; positioned over the pill and clipped to its shape. */
    children?: Snippet<[MachineState, PillRect]>;
    /** Called after every rendered frame with the pill rect (window logical px). */
    onframe?: (pill: PillRect) => void;
    /** Called once, after the first frame has been painted. */
    onready?: () => void;
  };

  let {
    preferences = DEFAULTS,
    focusRemaining = 1500000, focusRunning = false, onfocus,
    onpreferences,
    settingsProps,
    onalarmchange,
    desktopControls = false,
    track = null,
    onmediacontrol,
    onextrasactive,
    onspotifyrefresh,
    upcoming,
    ontestalarm,
    onalarmdismiss,
    initial = "dormant",
    frozen = false,
    backdrop = null,
    mockVoice = false,
    live = false,
    children,
    onframe,
    onready,
  }: Props = $props();

  const REDUCED_MS = 120;
  /** Orb fade in/out durations (s), matching the content crossfade. */
  const ORB_IN_S = 0.22;
  const ORB_OUT_S = 0.14;
  const ORB_MODE: Partial<Record<IslandState, OrbMode>> = {
    listening: "listening",
    thinking: "thinking",
    responding: "speaking",
  };
  const SILENT: OrbFeatures = { rms: 0, pitchHz: 0, centroid: 0, onset: false };
  /** Latest real voice features from Sudo AI (mic while listening, its own voice while speaking). */
  let heard: { f: OrbFeatures; at: number } | null = null;
  /** Features older than this are stale: the orb relaxes instead of freezing mid-gesture. */
  const HEARD_FRESH_MS = 160;
  /** A finished reply stays this long before the island settles back to compact. */
  const IDLE_AFTER_REPLY_MS = 6000;
  /** After Sudo stops talking (and nobody follows up), the reply stays this long. */
  const IDLE_AFTER_SPEECH_MS = 2500;
  /** Sudo AI is speaking its answer: the reply view never folds away mid-sentence. */
  let sudoSpeaking = false;
  /** Fixed clock for the expanded state in goldens. */
  const FROZEN_NOW = new Date(2026, 9, 30, 9, 41);

  // svelte-ignore state_referenced_locally
  const first: MachineState = typeof initial === "string" ? { ...initialState(), state: initial } : initial;
  let machine = $state.raw<MachineState>(first);
  let renderer = $state.raw<GlassRenderer | null>(null);
  let canvas: HTMLCanvasElement;
  let contentEl = $state<HTMLDivElement>();

  let contentHeight = $state<number | undefined>(undefined);
  const start = { ...SIZES[first.state], h: heightFor(first.state) };
  const springs = [new Spring(start.w), new Spring(start.h), new Spring(start.r), new Spring(PILL_TOP)] as const;
  const SPOTIFY_SHAPE = { w: 440, h: 210, r: 48 };
  const COVER_SHAPE={w:440,h:420,r:48};
  const WIDE_SHAPE={w:440,h:240,r:48};
  const SETTINGS_SHAPE = { w: 480, h: 420, r: 40 };
  const MINI_SHAPE = { w: 246, h: 44, r: 22 };
  let hidden = $state(untrack(()=>desktopControls&&!preferences.compactAlwaysVisible));
  let revealing = false;
  let pointerInside=$state(false);
  /** Cursor is where the small island sits (reported by the native hover watcher). */
  let inRevealZone=$state(false);
  export function setRevealZone(inside:boolean):void{inRevealZone=inside;}
  let revealHold=$state(false);
  let revealTimer:ReturnType<typeof setTimeout>|undefined;
  let nativeLeaveTimer:ReturnType<typeof setTimeout>|undefined;
  const wheel = createWheelNavigation();
  let settingsTab = $state<SettingsTab | null>(null);
  let selection = $state<IslandPage | null>(null);
  let pinnedPages = $state({ home: false, spotify: false, sudo: false });
  /** The Sudo tab's conversation (typed chat with Sudo AI). */
  let chat = $state<ChatMessage[]>([]);
  let chatBusy = $state(false);
  /** Sudo AI as the Sudo tab should describe it (not set up / off / starting / ready). */
  let sudoState = $state<SudoState>("missing");
  /**
   * Where the album art sits inside the Spotify panel (measured on screen, per layout). Home's
   * logo and the Sudo orb sit exactly there, so all three share one spot and morph in place.
   */
  const ART_SPOT_KEY = "sudo.artspot.v2";
  const defaultSpot = (layout: string): ArtSpot => (layout === "centered" ? { x: 174, y: 14, w: 92, h: 92, r: 12 } : { x: 26, y: 24, w: 92, h: 92, r: 12 });
  function storedSpots(): Record<string, ArtSpot> {
    try { return JSON.parse(localStorage.getItem(ART_SPOT_KEY) ?? "{}") ?? {}; } catch { return {}; }
  }
  let artSpots = $state<Record<string, ArtSpot>>(storedSpots());
  /** The art box depends on the layout and on what the player shows around it. */
  const spotKey = $derived(`${preferences.spotifyLayout}|${preferences.spotifySeekBar}|${preferences.spotifyShowRepeat}|${preferences.spotifyShowShuffle}`);
  const artSpot = $derived(artSpots[spotKey] ?? defaultSpot(preferences.spotifyLayout));
  /** Sudo AI (jarvis.rs) is enabled: Home shows a one-line typed question. */
  let jarvisOn = $state(false);
  /** Starting Sudo AI by clicking the orb is allowed (Settings → Sudo AI). */
  let orbClickOn = $state(false);
  let askFocused = $state(false);
  function togglePin(): void { pinnedPages[page] = !pinnedPages[page]; }
  function clearScrollPins(): void { pinnedPages = { home: false, spotify: false, sudo: false }; }
  let lyricsOpen = $state(false);
  let videoMenu = $state(false);
  // Right-click hides the extra controls on Lyrics and on the Spotify player; right-click again shows them.
  let lyricsMinimal = $state(false);
  let spotifyMinimal = $state(false);
  let searchOpen = $state(false);
  let customId=$state<string|null>(null);
  const activeTabs=$derived(preferences.tabs.filter(t=>!preferences.disabledTabs.includes(t.id)));
  const customTab=$derived(activeTabs.find(t=>t.id===customId));
  const compactPackage=$derived(preferences.compactTab==='auto'?customTab:activeTabs.find(t=>'tab:'+t.id===preferences.compactTab));
  let avoiding = $state(false);
  let avoidOffset = $state(0);
  let audioLevel = $state<readonly number[]|null>(null);
  const page = $derived<IslandPage>(selection==='sudo' ? "sudo" : selection==='spotify'||(track&&selection!=='home') ? "spotify" : "home");
  const sudoExpanded = $derived(machine.state === "expanded" && page === "sudo" && !settingsTab && !customId);
  const pinned = $derived(pinnedPages[page]);
  const resting = $derived(machine.state === "dormant" || machine.state === "compact");
  const spotifyExpanded = $derived(machine.state === "expanded" && page === "spotify" && !settingsTab);
  $effect(() => {
    if (!spotifyExpanded || !track || searchOpen || lyricsOpen) return;
    void track.artwork;
    const layout = spotKey;
    const timer = setTimeout(() => {
      const art = contentEl?.querySelector<HTMLElement>(".artwork-button");
      const frame = art?.closest<HTMLElement>(".frame");
      if (!art || !frame) return;
      const a = art.getBoundingClientRect(), f = frame.getBoundingClientRect();
      // Only a plain square cover is shared (the full-width cover card keeps the default spot).
      if (a.width < 40 || a.width > 140 || Math.abs(a.width - a.height) > 1) return;
      const radius = parseFloat(getComputedStyle(art.querySelector("img.artwork") ?? art).borderTopLeftRadius) || 12;
      const spot = { x: Math.round(a.left - f.left), y: Math.round(a.top - f.top), w: Math.round(a.width), h: Math.round(a.height), r: radius };
      const known = artSpots[layout];
      if (known && known.x === spot.x && known.y === spot.y && known.w === spot.w && known.r === spot.r) return;
      artSpots = { ...artSpots, [layout]: spot };
      try { localStorage.setItem(ART_SPOT_KEY, JSON.stringify(artSpots)); } catch { /* optional */ }
    }, 700);
    return () => clearTimeout(timer);
  });
  // Prepare the fullscreen view the first time the Spotify panel opens, so fullscreen opens instantly.
  let fullscreenPrepared = false;
  $effect(() => {
    if (!spotifyExpanded || fullscreenPrepared || !live) return;
    fullscreenPrepared = true;
    const timer = setTimeout(() => void prepareVideoView().catch(() => { fullscreenPrepared = false; }), 1500);
    return () => clearTimeout(timer);
  });
  // Home uses the Spotify player's size, so switching tabs never resizes the island.
  const homeSameAsSpotify = $derived(desktopControls && machine.state === "expanded" && page === "home" && !settingsTab && !customTab);
  const playerShape=$derived(preferences.spotifyLayout==='cover-card'?COVER_SHAPE:preferences.spotifyLayout==='wide'?WIDE_SHAPE:SPOTIFY_SHAPE);
  const coverExpanded=$derived(spotifyExpanded&&preferences.spotifyLayout==='cover-card');
  const spotifyCompact = $derived(resting && !compactPackage && !!track && (preferences.compactTab==='spotify'||(preferences.compactTab==='auto'&&page==='spotify')));
  /** The orb shows on the small island while Sudo AI is on, whenever it isn't showing a song cover. */
  const smallOrb = $derived(resting && jarvisOn && !settingsTab && !frozen && !(spotifyCompact && !!track?.artwork));
  const coverArt = $derived(track?.artwork);
  const homeArt = $derived(preferences.homeArtwork && !!track && page === "home" && (resting || machine.state === "expanded") && !settingsTab);
  // Song color, where the user chose it (Settings → Look). Settings itself keeps the default look.
  const songColorOn=$derived((place:'spotify'|'home'|'compact')=>{
    const scope=preferences.songColor;
    if(scope==='off')return false;
    if(place==='spotify')return true;
    if(place==='home')return scope==='spotify-home'||scope==='all';
    return scope==='spotify-compact'||scope==='all';
  });
  const homeTint=$derived(songColorOn('home') && !!track && page==='home' && machine.state==='expanded' && !settingsTab);
  const compactTint=$derived(songColorOn('compact') && !!track && resting && !settingsTab);
  let coverColor = $state.raw<Tint | undefined>();
  const startX = typeof innerWidth === "number" ? (innerWidth - start.w) / 2 : 0;
  let pill = $state.raw<PillRect>({ x: startX, y: PILL_TOP, ...start });
  let tween: { from: number[]; t0: number } | null = null;
  let raf = 0;
  let last = 0;
  const motionClock=new MotionClock();
  let painted = false;
  let wasMoving = true;
  let sentRect = "";
  const animationTiming=new AnimationTiming();
  let frameTiming=$state<FrameTiming>({active:false,fps:0,meanMs:0,worstMs:0,frames:0});

  let tintAlpha = TINT_ALPHA_DARK;
  let darkness = 0;
  let surfaceOpacity = 1;
  let material: Material = { color: TINT_RGB, blend: 0 };
  let backgroundResult=$state<VideoResult|null>(null);
  let canvasReady=$state(false);
  let canvasError=$state('');
  let canvasRevision=$state(0);
  let videoLookup:ReturnType<typeof createVideoLookup>|undefined;
  let artworkError=$state('');
  let videoView=$state<VideoViewState|null>(null);
  let videoBusy=$state(false);
  let videoViewError=$state('');
  const videoMode=$derived(videoView?.mode??null);
  const videoPresentation=$derived(videoView?.presentation);
  const videoGeneration=$derived(videoView?.generation);
  const updateVideoTimeline=createVideoTimeline();
  const songVideoClock=$derived(updateVideoTimeline(track,preferences.videoPauseWithSong));
  async function showVideo(mode:VideoViewMode,lyricsOnly=false):Promise<void> {
    if(videoBusy)return;
    videoBusy=true;videoViewError='';
    try {
      if(mode===videoMode&&!lyricsOnly) {await closeVideoView();videoView=null;}
      else if(track)videoView=await openVideoView({mode,url:backgroundUrl??null,presentation:lyricsOnly?'lyrics':backgroundUrl?'video':'artwork',quality:preferences.videoQuality,playing:!!track.playing,pauseWithSong:preferences.videoPauseWithSong,clock:songVideoClock});
    } catch(error) {videoViewError=String(error);}
    finally {videoBusy=false;}
  }
  let nativeWindowHeight=280;
  let nativeHeightTarget=280;
  let settingsResizeTimer: ReturnType<typeof setTimeout> | undefined;
  /** Live capture is unavailable: tinted glass only, at the legible (bright-end) tint. */
  let fallback = false;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  const orb = new OrbPass();
  const smoother = new OrbSmoother();
  const voice = new MockVoice(7);

  const reducedMotion = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;
  const lowMotion = $derived(preferences.reduceMotion || !!reducedMotion?.matches);

  const handlers: ContentHandlers = {
    onconfirm: (id) => dispatch({ type: "confirm_result", id, ok: true }),
    oncancel: (id) => dispatch({ type: "confirm_result", id, ok: false }),
    onsnooze: (id) => dismissAlarm(id, true),
    onstop: (id) => dismissAlarm(id, false),
    onresize: (h) => setContentHeight(h),
  };

  function dismissAlarm(id: string, snooze: boolean): void {
    if (machine.state !== "alarm" || (machine.payload.alarm as { id: string })?.id !== id) return;
    onalarmdismiss?.(id, snooze);
    dispatch({ type: "alarm_stop", id });
  }

  function nativeHidden(value: boolean): void {
    if (live) void invoke("set_island_hidden", { hidden: value }).catch(console.warn);
  }

  function setHidden(value: boolean, edge = false): void {
    if (hidden === value) {
      if (!value && !edge && revealing) { revealing = false; nativeHidden(false); }
      return;
    }
    hidden = value;
    wheel.reset();
    revealing = !value && edge;
    if (!revealing) nativeHidden(value);
    if (value) dispatch({ type: "hover", on: false });
    retarget(SPRINGS.gentle);
  }

  export function reveal(): void {
    if(!hidden)return;
    clearTimeout(revealTimer);
    revealHold=true;
    setHidden(false, true);
  }

  function pointerEnter():void {
    // Back on the island mid-collapse: drop the flying cover/logo at once instead of letting it drift.
    finishMorph();
    clearTimeout(nativeLeaveTimer);
    pointerInside=true;
    clearTimeout(revealTimer);
    revealHold=false;
    if(!hidden&&!revealing&&preferences.hoverExpand)openExpanded();
  }
  function pointerLeave():void {
    pointerInside=false;
    if(settingsTab||revealing||lyricsOpen)return;
    if(!pinned&&machine.state==='expanded'){
      videoMenu=false;searchOpen=false;
    }
    collapseWithMorph(()=>dispatch({type:'hover',on:false}));
  }

  /**
   * Runs `collapse`; when it actually folds the big island to the small one, the page's centre
   * piece (Home's logo, the song cover if it shows one, or the Sudo orb) reshapes into the small
   * pill as the island shrinks. Otherwise (no cover, low motion) the island just shrinks.
   */
  function collapseWithMorph(collapse:()=>void):void {
    const open=machine.state==='expanded'&&desktopControls&&!settingsTab&&!lyricsOpen&&!customTab&&!searchOpen&&!lowMotion&&!frozen;
    const videoShown=canvasReady&&preferences.videoHideArtwork;
    let source:{el:HTMLElement;shape:MorphShape}|null=null;
    if(open){
      const root=contentEl;
      if(page==='home'){const el=root?.querySelector<HTMLElement>('[data-morph="logo"]');if(el)source={el,shape:'logo'};}
      else if(page==='spotify'&&track?.artwork&&!videoShown){const el=root?.querySelector<HTMLElement>('img.artwork');if(el)source={el,shape:'art'};}
      else if(page==='sudo'){const el=root?.querySelector<HTMLElement>('[data-morph="art"]');if(el)source={el,shape:'orb'};}
    }
    collapse();
    if(!source||machine.state==='expanded'||hidden)return;
    // Where each piece lands inside the small island. Read every frame from the live pill: it is
    // still springing there and may sit lower than PILL_TOP (edge avoidance, burn-in drift).
    const miniOrb=smallOrb||(jarvisOn&&!(spotifyCompact&&!!track?.artwork));
    const lands=source.shape==='art'&&spotifyCompact||source.shape==='orb'&&miniOrb;
    const shape=source.shape;
    const target=()=>{
      const left=pill.x+pill.w/2-MINI_SHAPE.w/2, top=pill.y;
      if(shape==='art'&&lands)return {left:left+7,top:top+(MINI_SHAPE.h-32)/2,width:32,height:32,radius:10};
      if(shape==='orb'&&lands)return {left:left+MINI_SHAPE.w-5-18,top:top+(MINI_SHAPE.h-18)/2,width:18,height:18,radius:9};
      if(shape==='art')return {left:pill.x+pill.w/2-16,top:top+(MINI_SHAPE.h-32)/2,width:32,height:32,radius:10};
      return {left,top,width:MINI_SHAPE.w,height:MINI_SHAPE.h,radius:MINI_SHAPE.r};
    };
    void toCompact(shape,source.el,track?.artwork??null,artSpot.r,target,!lands).catch(()=>{});
  }

  /** Native hit-testing can lose the WebView's DOM leave when click-through takes over. */
  export function setPointerInside(inside:boolean):void {
    clearTimeout(nativeLeaveTimer);
    if(inside){pointerEnter();return;}
    nativeLeaveTimer=setTimeout(()=>pointerLeave(),80);
  }

  // The small island hides after the chosen delay once the cursor has left it.
  // Any change (cursor back, expanding, opening Settings) cancels the countdown.
  $effect(()=>{
    if(!(desktopControls&&!preferences.compactAlwaysVisible&&resting&&!pointerInside&&!inRevealZone&&!revealHold&&!settingsTab&&!avoiding&&!revealing))return;
    const delay=preferences.compactHideDelay*1000;
    if(delay<=0){untrack(()=>setHidden(true));return;}
    const timer=setTimeout(()=>untrack(()=>setHidden(true)),delay);
    return()=>clearTimeout(timer);
  });

  // Burn-in protection: while small, drift a few pixels every two minutes and
  // dim after a minute without the cursor. Pointing at it restores full brightness.
  // Plain (non-reactive) offset: reading it inside retarget() must never make the
  // island's motion effects re-run, and only a real change of offset moves the island.
  let burnShift={x:0,y:0};
  let burnDim=$state(false);
  const burnActive=$derived(desktopControls&&preferences.burnInProtection&&resting&&!hidden);
  function applyBurnShift(x:number,y:number,move:boolean):void{
    if(burnShift.x===x&&burnShift.y===y)return;
    burnShift={x,y};
    if(renderer)renderer.offsetX=x;
    // While expanding/hiding, the surrounding transition already owns the motion.
    if(move)retarget(SPRINGS.gentle);
  }
  $effect(()=>{
    if(!burnActive){untrack(()=>applyBurnShift(0,0,false));return;}
    const move=()=>untrack(()=>applyBurnShift(Math.round(Math.random()*8-4),Math.round(Math.random()*3),true));
    move();
    const timer=setInterval(move,120_000);
    return()=>clearInterval(timer);
  });
  $effect(()=>{
    if(!burnActive||pointerInside){burnDim=false;return;}
    const timer=setTimeout(()=>burnDim=true,60_000);
    return()=>{clearTimeout(timer);burnDim=false;};
  });

  const MORPH_SHAPE: Record<IslandPage, MorphShape> = { home: "logo", spotify: "art", sudo: "orb" };
  /** A running cover ↔ orb morph (plain, read by the frame loop only). */
  let orbMorph: { t: number; into: boolean; box: { left: number; top: number; width: number; height: number } } | null = null;
  const smoothstep = (a: number, b: number, x: number) => { const k = Math.max(0, Math.min(1, (x - a) / (b - a))); return k * k * (3 - 2 * k); };

  /**
   * Home's logo, the album art and the Sudo orb share one box, so a page change between Spotify
   * and Home or Sudo reshapes that box in place (when the song has a cover image, not a Canvas
   * video). Every other change keeps the normal cross-fade.
   */
  function morphTo(from: IslandPage, to: IslandPage): void {
    finishMorph();
    const artwork = track?.artwork ?? null;
    const videoShown = canvasReady && preferences.videoHideArtwork;
    if (lowMotion || frozen || from === to || !artwork || videoShown || machine.state !== "expanded" || settingsTab || lyricsOpen) return;
    if (!(from === "spotify" || to === "spotify")) return;
    // Only when the shared spot is the measured cover box (not the default guess).
    if (!artSpots[spotKey]) return;
    const frame = contentEl?.querySelector<HTMLElement>(".frame");
    if (!frame) return;
    const f = frame.getBoundingClientRect();
    const box = { left: f.left + artSpot.x, top: f.top + artSpot.y, width: artSpot.w, height: artSpot.h, radius: artSpot.r };
    const hide = () => [...(contentEl?.querySelectorAll<HTMLElement>('img.artwork, [data-morph="logo"]') ?? [])];
    const a = MORPH_SHAPE[from], b = MORPH_SHAPE[to];
    // Cover ↔ orb: the live orb stays pinned to the cover's box and fades with the morph.
    const hooks = a === "orb" || b === "orb" ? {
      onprogress: (t: number) => { orbMorph = { t, into: b === "orb", box }; requestFrame(); },
      onend: () => { orbMorph = null; requestFrame(); },
    } : {};
    void morph(a, b, box, artwork, hide, 520, hooks).catch(() => {});
  }

  type Background = Preferences["spotifyBackground"];
  const BACKGROUND_ORDER: Background[] = ["image", "artwork", "canvas"];
  /** The Spotify icon is trying video sources in turn; `restore` is what to go back to. */
  let backgroundTrial = $state<{ next: Background[]; restore: Pick<Preferences, "spotifyBackground" | "video"> } | null>(null);
  let trialTimer: ReturnType<typeof setTimeout> | undefined;

  /**
   * On the Spotify tab, the Spotify icon switches what plays behind the player:
   * image → Apple Music video → Spotify Canvas → image. A video source this song doesn't have is
   * skipped; when it has neither, nothing changes. Each try keeps the cover until a video is ready.
   */
  function cycleBackground(): void {
    if (!track || backgroundTrial) return;
    const current = preferences.spotifyBackground;
    const after = [...BACKGROUND_ORDER.slice(BACKGROUND_ORDER.indexOf(current) + 1), ...BACKGROUND_ORDER.slice(0, BACKGROUND_ORDER.indexOf(current))];
    const videos = after.slice(0, after.indexOf("image") === -1 ? after.length : after.indexOf("image"));
    if (!videos.length) { onpreferences?.({ spotifyBackground: "image" }); return; }
    backgroundTrial = { next: videos.slice(1), restore: { spotifyBackground: current, video: preferences.video } };
    tryBackground(videos[0]);
  }
  /** The fullscreen view's "Cover / Apple Music video / Spotify video" button. */
  export function cycleBackgroundSource(): void { cycleBackground(); }
  function tryBackground(source: Background): void {
    clearTimeout(trialTimer);
    // A lookup that never answers counts as unavailable.
    trialTimer = setTimeout(() => trialOutcome(false), 12_000);
    onpreferences?.({ spotifyBackground: source, video: true });
  }
  function trialOutcome(found: boolean): void {
    const trial = backgroundTrial;
    if (!trial) return;
    clearTimeout(trialTimer);
    if (found) { backgroundTrial = null; return; }
    const [next, ...rest] = trial.next;
    if (next) { backgroundTrial = { ...trial, next: rest }; tryBackground(next); return; }
    backgroundTrial = null;
    // From the image: this song has no video at all, so nothing changes. From a video: back to the image.
    onpreferences?.(trial.restore.spotifyBackground === "image" ? trial.restore : { spotifyBackground: "image" });
  }

  /** +1 when the new page is below (Home → Spotify → Sudo), -1 above; 0 for any other change. */
  let pageDirection = $state(0);
  let pageDirectionTimer: ReturnType<typeof setTimeout> | undefined;
  const PAGE_ORDER: Record<IslandPage, number> = { home: 0, spotify: 1, sudo: 2 };

  function choosePage(next: IslandPage): void {
    if (next === "spotify" && page === "spotify" && spotifyExpanded && !lyricsOpen && !searchOpen) { cycleBackground(); return; }
    const previous = page;
    if (next !== previous) {
      pageDirection = Math.sign(PAGE_ORDER[next] - PAGE_ORDER[previous]);
      clearTimeout(pageDirectionTimer);
      pageDirectionTimer = setTimeout(() => (pageDirection = 0), 450);
    }
    morphTo(previous, next);
    customId=null;
    searchOpen=false;
    videoMenu = false;
    if (next !== "spotify") closeLyrics();
    selection = next;
    if (desktopControls && preferences.rememberPage) localStorage.setItem(PAGE_KEY, next);
  }

  function openSettings(tab: SettingsTab): void {
    searchOpen=false;
    videoMenu = false;
    lyricsOpen = false;
    resizeNativeWindow(480);
    settingsTab = tab;
  }

  /** Right Ctrl before Sudo AI is set up: open its one-click setup instead of a dead end. */
  /** Real audio features for the orb (~30 per second while Sudo AI listens or speaks). */
  export function pushVoiceLevel(f: OrbFeatures): void {
    // An onset must not be lost if two blocks arrive within one frame.
    const onset = f.onset || (!!heard && heard.f.onset);
    heard = { f: { ...f, onset }, at: performance.now() };
    requestFrame();
  }

  export function openJarvisSettings(): void {
    dispatch({ type: "idle_timeout" });
    dispatch({ type: "hover", on: true });
    openSettings("jarvis");
  }

  function closeSettings(): void {
    settingsTab = null;
    restoreWindowSize();
  }

  function restoreWindowSize(): void {
    if(settingsTab||avoiding||coverExpanded)return;
    resizeNativeWindow(280);
  }

  function resizeNativeWindow(height:number):void {
    // Always apply: the native window can be resized elsewhere (hide/reveal), so a cached
    // "already this height" let Settings open in a 280 px window with its bottom cut off.
    if(!live)return;
    nativeHeightTarget=height;
    clearTimeout(settingsResizeTimer);
    const apply=()=>{
      if(nativeHeightTarget!==height)return;
      nativeWindowHeight=height;
      void invoke('set_settings_open',{open:height>280,height}).catch(console.warn);
    };
    if(height>=nativeWindowHeight)apply();else settingsResizeTimer=setTimeout(apply,650);
  }

  function openExpanded(): void { dispatch({ type: "hover", on: true }); }

  function openLyrics(): void {
    searchOpen=false;
    videoMenu = false;
    lyricsOpen = true;
  }

  function closeLyrics(): void {
    if (!lyricsOpen) return;
    lyricsOpen = false;
    restoreWindowSize();
  }


  /** When a scroll last folded the island to its small pill (a further scroll up then hides it). */
  let foldedByScrollAt = -Infinity;

  function onWheel(event: WheelEvent): void {
    // The small island: scroll up hides it, scroll down opens it. A pause after the scroll that
    // folded it keeps one long flick from folding and hiding in a single gesture.
    if (desktopControls && resting && !hidden && !revealing && !settingsTab && !event.ctrlKey && Math.abs(event.deltaY) >= Math.abs(event.deltaX)) {
      const pixels = event.deltaY * (event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? 240 : 1);
      if (pixels <= -6 && performance.now() - foldedByScrollAt > 450) { event.preventDefault(); setHidden(true); }
      else if (pixels >= 6 && !revealHold) { event.preventDefault(); openExpanded(); }
      return;
    }
    if (!desktopControls || machine.state !== "expanded" || settingsTab || searchOpen || event.ctrlKey || Math.abs(event.deltaX) > Math.abs(event.deltaY)) return;
    // The scroll that just brought the island back must not also flip its tabs.
    if(revealHold||revealing){wheel.reset();return;}
    const pixels=event.deltaY*(event.deltaMode===1?40:event.deltaMode===2?240:1);
    if(lyricsOpen){
      if(pixels<=-6){event.preventDefault();clearScrollPins();closeLyrics();choosePage('home');wheel.reset();}
      return;
    }
    const action = wheel.handle(event.deltaY, event.deltaMode, event.timeStamp, page, !!track);
    // A deliberate vertical scroll releases the keep-open gesture, including the
    // first Home hide step. Returning to a tab must never resurrect its old pin.
    if(Math.abs(pixels)>=6)clearScrollPins();
    // Scrolling up past Home folds the island back to its small pill; another scroll up on the
    // small pill hides it.
    if (action === "hide") { foldedByScrollAt = performance.now(); collapseWithMorph(() => { pinnedPages[page] = false; dispatch({ type: "hover", on: false }); }); }
    else if (action) choosePage(action);
  }

  /** Feeds an event through the state machine and retargets the shape springs. */
  /** Typed chat on the Sudo tab: a reply streams into the conversation instead of switching views. */
  function chatEvent(e: IslandEvent): boolean {
    if (!sudoExpanded || (e.type !== "thinking" && e.type !== "reply")) return false;
    const last = chat.at(-1);
    if (e.type === "thinking") {
      chatBusy = true;
      if (!(last?.role === "sudo" && last.pending)) chat = [...chat, { role: "sudo", text: "", pending: true }];
      return true;
    }
    const message = { role: "sudo" as const, text: e.text, pending: !e.done };
    chat = last?.role === "sudo" && last.pending ? [...chat.slice(0, -1), message] : [...chat, message];
    if (e.done) chatBusy = false;
    requestFrame();
    return true;
  }

  async function sendChat(text: string): Promise<void> {
    chat = [...chat, { role: "user", text }];
    try { await invoke("jarvis_ask", { text }); }
    catch (error) { chat = [...chat, { role: "sudo", text: error instanceof Error ? error.message : String(error) }]; chatBusy = false; }
  }

  export function dispatch(e: IslandEvent): void {
    if (chatEvent(e)) return;
    if (e.type === "hover" && !e.on && (settingsTab || lyricsOpen || videoMenu || searchOpen || avoiding || pinned || askFocused)) return;
    if (e.type === "alarm_ring" || e.type === "hotkey_down" || e.type === "confirm_request") setHidden(false);
    clearTimeout(idleTimer);
    jump(reduce(machine, e));
    if (e.type === "reply" && e.done && machine.state === "responding" && !frozen && !sudoSpeaking) {
      idleTimer = setTimeout(() => dispatch({ type: "idle_timeout" }), IDLE_AFTER_REPLY_MS);
    }
  }

  /** jarvis.rs: Sudo started or stopped speaking an answer. */
  export function setSpeaking(on: boolean): void {
    sudoSpeaking = on;
    clearTimeout(idleTimer);
    if (!on && machine.state === "responding" && !frozen) {
      idleTimer = setTimeout(() => dispatch({ type: "idle_timeout" }), IDLE_AFTER_SPEECH_MS);
    }
  }

  /** Moves to `next` as a transition (springs, sounds, focus); used by dev shortcuts too. */
  export function jump(next: MachineState): void {
    if (next === machine) return;
    const from = machine.state;
    const fromConfirm = confirmId(machine);
    machine = next;
    if (from === next.state) {
      // Back-to-back confirms stay in `confirm`; the next question still gets its chime.
      if (from === "confirm" && confirmId(next) !== fromConfirm && !frozen && preferences.interfaceSounds) sfx.play("confirm");
      return requestFrame();
    }
    if (!frozen) {
      const fx = sfxForTransition(from, next.state);
      if (onalarmchange && (from === "alarm" || next.state === "alarm")) onalarmchange(next.state === "alarm");
      else {
        if (fx.stop) sfx.stop(fx.stop);
        if (fx.play && (fx.play === "alarm" || preferences.interfaceSounds)) sfx.play(fx.play);
      }
    }
    if (settingsTab && next.state !== "expanded") { settingsTab = null; restoreWindowSize(); }
    if (lyricsOpen && next.state !== "expanded") closeLyrics();
    if (next.state !== "expanded") videoMenu = false;
    if (next.state !== "expanded") searchOpen = false;
    retarget(SPRINGS[springFor(from, next.state)]);
  }

  function confirmId(m: MachineState): string | undefined {
    return (m.payload.confirm as { id?: string } | undefined)?.id;
  }

  export function getState(): MachineState {
    return machine;
  }

  export function getRenderer(): GlassRenderer | null {
    return renderer;
  }

  /** Measured content height; resizes content-sized states (`responding`) within their range. */
  export function setContentHeight(h: number): void {
    if (h === contentHeight) return;
    contentHeight = h;
    retarget(SPRINGS.snappy);
  }

  /** Schedules one frame; the loop keeps itself alive only while something moves. */
  export function requestFrame(): void {
    if (!raf && renderer) {
      // Start elapsed time at the request, rather than wasting the first display frame.
      const now=performance.now();
      if (!last) last = now;
      motionClock.wake(now);
      raf = requestAnimationFrame(frame);
    }
  }

  function retarget(cfg: SpringCfg): void {
    const now=performance.now();
    // Consume elapsed time under the old target before the input changes it.
    advance(now,motionClock.retarget(now));
    const s = machine.state;
    const shape = settingsTab ? SETTINGS_SHAPE : customTab && s==='expanded' ? SPOTIFY_SHAPE : spotifyExpanded || homeSameAsSpotify || sudoExpanded ? playerShape : resting && desktopControls ? MINI_SHAPE : { ...SIZES[s], h: heightFor(s, contentHeight) };
    // Clear the glass shader's drop shadow as well as the pill when hiding.
    const targets = [shape.w, shape.h, shape.r, hidden ? -Math.max(pill.h, shape.h) - 32 : PILL_TOP + (avoiding ? avoidOffset : 0) + (resting ? burnShift.y : 0)];
    const geometryCfg = desktopControls && (s === 'expanded' || resting) ? SPRINGS.panel : cfg;
    springs.forEach((sp, i) => sp.setTarget(targets[i], i < 3 ? geometryCfg : cfg));
    if (frozen) springs.forEach((sp) => sp.snap());
    else if (lowMotion) {
      springs[3].snap();
      tween = { from: springs.map((sp) => sp.x), t0: now };
    }
    requestFrame();
  }

  function advance(t: number,dt:number): void {
    if (tween) {
      const k = Math.min(1, (t - tween.t0) / REDUCED_MS);
      const ease = 1 - (1 - k) ** 3;
      springs.forEach((sp, i) => {
        sp.x = tween!.from[i] + (sp.target - tween!.from[i]) * ease;
        sp.v = 0;
      });
      if (k >= 1) {
        springs.forEach((sp) => sp.snap());
        tween = null;
      }
      return;
    }
    springs.forEach((sp) => sp.step(dt));
  }

  /** Adaptive tint toward the backdrop's luminance; true while still easing. */
  let glisseMix=0;
  function stepTint(r: GlassRenderer, dt: number): boolean {
    const lum = r.backdropLuminance;
    const color = (spotifyExpanded && songColorOn('spotify')) || compactTint || homeTint ? coverColor ?? NEUTRAL_COVER_TINT : undefined;
    const darkHome = desktopControls && (page === "home" || (resting&&!spotifyCompact&&!compactPackage)) && (resting || machine.state === "expanded") && !settingsTab;
    const amount = preferences.islandDarkness / 100;
    const target = color ? 0.75 + amount * 0.23 : darkHome ? 0.65 + amount * 0.33 : lum !== null ? tintAlphaFor(lum) : fallback ? TINT_ALPHA_BRIGHT : TINT_ALPHA_DARK;
    tintAlpha = frozen ? target : smoothTint(tintAlpha, target, dt);
    r.setTintAlpha(tintAlpha);
    const next = stepMaterial(material, color ?? TINT_RGB, color ? 1 : 0, dt, frozen || lowMotion);
    material = next;
    r.setTintColor(next.color, next.blend);
    // With the song colour on Home too, Home uses exactly Spotify's glass (same tint and darkness).
    const darknessTarget = darkHome && !color ? amount * 0.85 : amount * 0.55;
    darkness = frozen || lowMotion ? darknessTarget : smoothTint(darkness, darknessTarget, dt);
    const opacityTarget=1-preferences.islandTransparency/100;
    surfaceOpacity=frozen||lowMotion?opacityTarget:smoothTint(surfaceOpacity,opacityTarget,dt);
    // Glisse design: the island itself becomes Glisse's liquid glass, with its fixed
    // values; Sudo's border, transparency and blur settings don't apply (see Settings).
    const glisseTarget=preferences.islandDesign==='glisse'?1:0;
    glisseMix=frozen||lowMotion?glisseTarget:smoothTint(glisseMix,glisseTarget,dt);
    r.glisse=glisseMix;
    const glisse=glisseTarget===1;
    r.setAppearance(glisse?'glass':preferences.islandBorder, darkness, glisse?1:surfaceOpacity, glisse?0:preferences.backgroundBlur);
    return Math.abs(glisseMix-glisseTarget)>1e-3 || Math.abs(surfaceOpacity-opacityTarget)>1e-3 || Math.abs(tintAlpha - target) > 1e-3 || Math.abs(darkness - darknessTarget) > 1e-3 || !next.settled;
  }

  /** Orb mode, slot, fade and voice features; true while the orb needs more frames. */
  function stepOrb(dt: number): boolean {
    const speakingNow = !!heard && performance.now() - heard.at < HEARD_FRESH_MS && heard.f.source === "tts";
    const mode = sudoExpanded ? (chatBusy ? "thinking" : speakingNow ? "speaking" : "listening") : ORB_MODE[machine.state];
    if (!mode && orb.opacity === 0) return false;
    if (mode) orb.mode = smoother.mode = mode;
    const slots = contentEl?.querySelectorAll<HTMLElement>("[data-orb-slot]");
    const el = slots?.[slots.length - 1];
    if (el) {
      const r = el.getBoundingClientRect();
      const size = Math.min(r.width, r.height);
      orb.slot = { x: r.left + (r.width - size) / 2, y: r.top + (r.height - size) / 2, size };
    }
    const target = mode && el ? 1 : 0;
    const prev = orb.opacity;
    if (orbMorph) {
      // The orb disk is the cover box; its slot is 143/92 of that box, centred on it.
      const { box, t, into } = orbMorph;
      const size = box.width * 143 / 92;
      orb.slot = { x: box.left + box.width / 2 - size / 2, y: box.top + box.height / 2 - size / 2, size };
      orb.opacity = into ? smoothstep(0.2, 0.85, t) : 1 - smoothstep(0, 0.5, t);
    }
    else if (frozen) orb.opacity = target;
    else if (target > prev) orb.opacity = Math.min(1, prev + dt / ORB_IN_S);
    else orb.opacity = Math.max(0, prev - dt / ORB_OUT_S);
    if (frozen) orb.snap();

    const real = !!heard && performance.now() - heard.at < HEARD_FRESH_MS && !!mode && mode !== "thinking";
    if (sudoExpanded && !real && !chatBusy && orb.opacity === 1 && smoother.settled) return false; // resting orb: no frames
    const live = real || (mockVoice && !!mode && mode !== "thinking");
    let f = SILENT;
    if (real && heard) {
      f = heard.f;
      heard = { f: { ...heard.f, onset: false }, at: heard.at }; // one ripple per syllable
    } else if (live) {
      f = voice.next(dt);
      if (mode === "speaking") f.source = "tts";
    }
    orb.uniforms = smoother.push(f, dt);

    if (orbMorph) return true;
    if (frozen || orb.opacity === 0) return orb.opacity !== target;
    return orb.opacity !== target || live || mode === "thinking" || !smoother.settled || orb.transitioning;
  }

  function frame(t: number): void {
    raf = 0;
    if (!renderer) return;
    const now=performance.now();
    const dt = last ? Math.max(0,Math.min((now - last) / 1000, 0.1)) : 0;
    advance(now,motionClock.step(now));
    last = now;
    const [w, h, r, y] = springs;
    renderer.setShape(w.x, h.x, r.x, y.x);
    const nextPill = renderer.pillRect();
    if (nextPill.x !== pill.x || nextPill.y !== pill.y || nextPill.w !== pill.w || nextPill.h !== pill.h || nextPill.r !== pill.r) pill = nextPill;
    // Lay the content out at the new pill before measuring the orb slot inside it.
    if (ORB_MODE[machine.state] || orb.opacity > 0) flushSync();
    const tinting = stepTint(renderer, dt);
    const orbing = stepOrb(dt);
    const moving = !!tween || springs.some((sp) => !sp.atRest);
    // One final clear at the end of hiding; don't redraw an invisible canvas for desktop changes.
    if (!hidden || moving || wasMoving) renderer.render(frozen ? 0 : t);
    if(preferences.showFps){const sample=animationTiming.record(performance.now(),moving);if(sample)frameTiming=sample;}
    else animationTiming.record(performance.now(),false);
    const key = `${pill.x},${pill.y},${pill.w},${pill.h},${pill.r}`;
    if (key !== sentRect || (wasMoving && !moving)) {
      setPillRect(pill, !moving);
      sentRect = key;
    }
    if (revealing && y.atRest && y.x === PILL_TOP + (resting ? burnShift.y : 0)) {
      revealing = false;
      nativeHidden(false);
      clearTimeout(revealTimer);
      revealTimer=setTimeout(()=>{revealHold=false;},1000);
    }
    wasMoving = moving;
    if (live) setAnimating(moving, hidden);
    onframe?.(pill);
    if (!painted) {
      painted = true;
      requestAnimationFrame(() => onready?.());
    }
    if (moving || tinting || orbing) requestFrame();
    else {last = 0;motionClock.sleep();}
  }

  onMount(() => {
    if(live) {
      let disposed=false,off:(()=>void)|undefined;
      let revision=0;
      void listenVideoView(state=>{++revision;if(!disposed&&!(videoBusy&&!state))videoView=state;}).then(unsubscribe=>{
        if(disposed){unsubscribe();return;}
        off=unsubscribe;const initialRevision=revision;
        void getVideoView().then(state=>{if(!disposed&&!videoBusy&&revision===initialRevision)videoView=state;}).catch(console.warn);
      }).catch(console.warn);
      return ()=>{disposed=true;off?.();};
    }
  });

  onMount(() => {
    if(desktopControls)nativeHidden(hidden);
    if (desktopControls && preferences.rememberPage) {
      const stored = localStorage.getItem(PAGE_KEY);
      if (stored === "home" || stored === "spotify" || stored === "sudo") selection = stored;
    }
    try {
      renderer = new GlassRenderer(canvas, requestFrame);
      renderer.addPass(orb);
    } catch (err) {
      console.error("island: renderer unavailable", err);
      onready?.();
      return;
    }
    if (frozen) springs.forEach((sp) => sp.snap());
    const onResize = () => requestFrame();
    window.addEventListener("resize", onResize);
    requestFrame();
    let unsubscribe: (() => void) | null = null;
    let disposed = false;
    if (live) {
      const r = renderer;
      // WGC frames are tight BGRA8 rows; the renderer swizzles in the shader.
      const target = {
        setBackdrop: (f: { width: number; height: number; data: Uint8Array } | null) =>
          r.setBackdrop(f ? { ...f, bgra: true } : null),
        setBackdropRect: (rect: { x: number; y: number; w: number; h: number }) => r.setBackdropRect(rect),
      };
      subscribeBackdrop(target, {
        onFrame: requestFrame,
        onStatus: (s) => (fallback = s.mode === "acrylic"),
      })
        .then((off) => (disposed ? off() : (unsubscribe = off)))
        .catch((err) => console.warn("island: backdrop unavailable", err));
    }
    return () => {
      disposed = true;
      unsubscribe?.();
      clearTimeout(idleTimer);
      clearTimeout(settingsResizeTimer);
      clearTimeout(revealTimer);
      clearTimeout(nativeLeaveTimer);
      videoLookup?.stop();
      window.removeEventListener("resize", onResize);
      cancelAnimationFrame(raf);
      raf = 0;
      sfx.stop("alarm");
      renderer?.dispose();
      renderer = null;
    };
  });

  $effect(() => {
    if (!renderer || live) return;
    renderer.setBackdrop(backdrop);
    requestFrame();
  });

  $effect(() => {
    // Page changes use the same spring as the surrounding island without changing machine state.
    const mediaPanel = spotifyExpanded || spotifyCompact;
    void homeArt;
    void homeTint;
    void compactTint;
    void preferences.songColor;
    void preferences.islandBorder;
    void preferences.islandDarkness;
    void preferences.islandTransparency;
    void preferences.backgroundBlur;
    void preferences.islandDesign;
    const isSettings = !!settingsTab;
    void customTab;
    void preferences.spotifyLayout;
    void lyricsOpen;
    void avoiding;
    void avoidOffset;
    if (renderer) untrack(() => retarget(mediaPanel || isSettings ? SPRINGS.snappy : SPRINGS.gentle));
  });

  $effect(() => {
    const artwork = coverArt;
    if (!artwork) { coverColor = NEUTRAL_COVER_TINT; requestFrame(); return; }
    const image = new Image();
    let obsolete = false;
    image.src = artwork;
    image.decode().then(() => {
      if (obsolete) return;
      coverColor = tintFromCover(image);
      requestFrame();
    }).catch(() => { if (!obsolete) { coverColor = NEUTRAL_COVER_TINT; requestFrame(); } });
    return () => { obsolete = true; };
  });

  const backgroundActive=$derived(live&&preferences.video&&!!track&&spotifyExpanded&&!lyricsOpen&&!hidden);
  // Look the video up as soon as a song plays (not when the Spotify tab opens), so it is ready
  // the moment the player is shown; it only plays while visible.
  const videoLookupActive=$derived(live&&preferences.video&&!!track);
  $effect(()=>{onextrasactive?.(spotifyExpanded&&!lyricsOpen&&!searchOpen&&!hidden);});
  const backgroundUrl=$derived(backgroundResult?.url);
  const canvasTrackKey=$derived(track?JSON.stringify({key:track.key,title:track.title,artist:track.artist,album:track.album,duration:track.duration}):'');
  $effect(()=>{
    const imageOnly=preferences.spotifyBackground==='image';
    const metadata=live&&preferences.video&&!imageOnly&&canvasTrackKey?JSON.parse(canvasTrackKey):null;
    const active=videoLookupActive;
    const source:'canvas'|'artwork'=preferences.spotifyBackground==='artwork'?'artwork':'canvas';
    // While the Spotify icon is trying a source, test exactly that one.
    const fallback=preferences.videoCanvasFallback&&!backgroundTrial;
    const revision=canvasRevision;
    const bridgeRevision=track?.bridgeRevision;
    if(!live)return;
    if(metadata){canvasError='';artworkError='';}
    untrack(()=>{
      if(!videoLookup&&!metadata)return;
      videoLookup??=createVideoLookup((result,error)=>{
        backgroundResult=result;canvasReady=false;
        if(error){if(preferences.spotifyBackground==='canvas'||preferences.videoCanvasFallback)canvasError=error;else artworkError=error;}
        if(backgroundTrial&&(result||error))untrack(()=>trialOutcome(!!result));
      });
      videoLookup.request(metadata,{source,fallback,revision,bridgeRevision,active});
    });
  });

  $effect(()=>{
    const mode=videoMode,url=preferences.video?backgroundUrl:null,enabled=!!track;
    const quality=preferences.videoQuality,playing=!!track?.playing,pauseWithSong=preferences.videoPauseWithSong;
    const clock=songVideoClock,presentation=videoPresentation,expectedGeneration=videoGeneration;
    if(!live||!mode)return;
    untrack(()=>{
      if(!enabled)void closeVideoView().catch(console.warn);
      else void syncVideoView({url:url??null,presentation:presentation==='lyrics'?'lyrics':url?'video':'artwork',quality,playing,pauseWithSong,clock,expectedGeneration}).catch(error=>videoViewError=String(error));
    });
  });

  // One owner for keyboard focus, including every path that dismisses a popup or overlay.
  const wantsFocus = $derived(machine.state === "confirm" || machine.state === "alarm" || !!settingsTab || lyricsOpen || videoMenu || searchOpen || (!!customTab&&machine.state==='expanded') || askFocused);
  onMount(() => {
    if (!live) return;
    const apply = (s: { phase?: string } | null) => { jarvisOn = !!s && s.phase !== "off"; };
    const refresh = () => invoke<{ phase: string; enabled?: boolean; installed?: boolean; orbClick?: boolean }>("jarvis_status")
      .then((s) => {
        apply(s.enabled ? s : null); orbClickOn = s.orbClick !== false;
        sudoState = !s.installed ? "missing" : !s.enabled || s.phase === "off" ? "off" : s.phase === "ready" ? "ready" : s.phase === "error" ? "off" : "loading";
      }).catch(() => {});
    void refresh();
    let off: (() => void) | undefined, disposed = false;
    import("@tauri-apps/api/event").then(({ listen }) => listen<{ phase: string }>("jarvis_status", (e) => { apply(e.payload); void refresh(); }))
      .then((u) => { if (disposed) u(); else off = u; }).catch(() => {});
    return () => { disposed = true; off?.(); };
  });
  $effect(() => { setIslandFocus(wantsFocus); });

  const meterEnabled = $derived(live && (spotifyCompact || (spotifyExpanded&&preferences.spotifyLayout==='wide'&&!lyricsOpen&&!searchOpen)) && !hidden && !!track?.playing && preferences.waves && preferences.waveReactivity>0 && !lowMotion);
  $effect(() => {
    if (!meterEnabled) { audioLevel = null; return; }
    return watchSpotifyLevels(level => {
      audioLevel = level;
    });
  });

  $effect(() => {
    if (!track) { pinnedPages.spotify = false; closeLyrics(); }
  });

  const pinnedClearance=$derived(preferences.pinnedEdgeClearance&&(pinned||lyricsOpen)&&machine.state==='expanded'&&!settingsTab);
  const clearanceOffset=$derived(pinnedClearance?(customTab?218:page==='spotify'||page==='home'||page==='sudo'?playerShape.h+8:198):44);
  const edgeEnabled=$derived((preferences.edgeAvoidance||pinnedClearance) && !hidden && !settingsTab && (resting || machine.state === "expanded"));
  $effect(() => {
    if (!live) return;
    const enabled=edgeEnabled;
    void invoke("set_edge_avoidance", { enabled,offset:clearanceOffset,pinnedClearance }).catch(console.warn);
    if (!enabled) avoiding = false;
  });

  export function setAvoiding(active: boolean, offset=44, overIsland=false): void {
    if(active)avoidOffset=Number.isFinite(offset)?Math.max(44,Math.min(480,offset)):44;
    avoiding = active && (preferences.edgeAvoidance||pinnedClearance);
    if(avoiding&&machine.state==='expanded')resizeNativeWindow(Math.max(480,PILL_TOP+avoidOffset+(page==='spotify'||page==='home'||page==='sudo'?playerShape.h:customTab?210:190)+32));
    if (!avoiding) {
      restoreWindowSize();
      // Pointerleave was intentionally held while crossing the cleared corridor.
      // Native exit is the missing leave event once the pointer leaves that corridor.
      dispatch({ type: "hover", on: overIsland });
    }
  }

  $effect(()=>{
    if(live)void invoke('set_show_in_captures',{on:preferences.screenshotMode}).catch(console.warn);
  });

  $effect(() => {
    if (!live) return;
    // Home and Sudo take the Spotify panel's size, so a tall layout (Cover card, 420 px) needs a
    // taller window on all three tabs, not only on Spotify, or their lower part is cut off.
    const pageTall = machine.state === "expanded" && !customTab && (spotifyExpanded || homeSameAsSpotify || sudoExpanded) && PILL_TOP + playerShape.h + 24 > 280;
    const large = !!settingsTab || (avoiding && machine.state === "expanded") || coverExpanded || pageTall;
    const wanted=settingsTab?480:avoiding&&machine.state==='expanded'?Math.max(480,PILL_TOP+avoidOffset+(page==='spotify'||page==='home'||page==='sudo'?playerShape.h:customTab?210:190)+32):coverExpanded||pageTall?Math.max(480,PILL_TOP+playerShape.h+50):280;
    if(large)untrack(()=>resizeNativeWindow(wanted));else untrack(restoreWindowSize);
  });

  /** Natural layout size of a state's content (its target pill size). */
  const frameSize = (s: IslandState) => ({ w: SIZES[s].w, h: heightFor(s, contentHeight) });
</script>

{#snippet orbSlot()}{/snippet}

<canvas bind:this={canvas} class="glass" class:burn={preferences.burnInProtection} class:burn-dim={burnDim} aria-hidden="true"></canvas>

<svelte:window onkeydown={(event) => {
  if (event.key !== "Escape" || !desktopControls || machine.state !== "expanded") return;
  if (searchOpen) searchOpen=false;
  else if (videoMenu) videoMenu = false;
  else if (lyricsOpen) closeLyrics();
  else if (settingsTab) closeSettings();
  else if (page === "spotify") choosePage("home");
  else setHidden(true);
}} oncontextmenu={(event) => { if (desktopControls) event.preventDefault(); }} />

{#if desktopControls && preferences.showFps}<FpsMeter timing={frameTiming} />{/if}

<div
  bind:this={contentEl}
  class="content"
  class:stable-panel={desktopControls&&(machine.state==='expanded'||resting)}
  data-state={machine.state}
  data-page={page}
  data-hidden={hidden}
  class:burn={preferences.burnInProtection}
  class:burn-dim={burnDim}
  data-pinned={pinned}
  data-avoiding={avoiding}
  inert={hidden}
  style:left="{pill.x}px"
  style:top="{pill.y}px"
  style:width="{pill.w}px"
  style:height="{pill.h}px"
  style:border-radius="{pill.r}px"
  style:--surface-opacity={preferences.islandDesign==='glisse'?1:1-preferences.islandTransparency/100}
  role="presentation"
  onwheel={onWheel}
  oncontextmenu={(event) => {
    if (!desktopControls) return;
    event.preventDefault();
    if (lyricsOpen && track) lyricsMinimal = !lyricsMinimal;
    else if (spotifyExpanded && track?.title && !searchOpen) spotifyMinimal = !spotifyMinimal;
  }}
  onpointerenter={pointerEnter}
  onpointerleave={pointerLeave}
>
  {#if children}
    {@render children(machine, pill)}
  {:else}
    {#if homeArt && coverArt}
      <div class="home-art" aria-hidden="true"><img src={coverArt} alt="" /></div>
    {/if}
    {#if backgroundUrl}
      {#key `${backgroundUrl}:${preferences.videoQuality}:${backgroundResult?.generation}`}
        {@const sourceResult=backgroundResult!}
        <TrackVideo src={backgroundUrl} active={backgroundActive} quality={preferences.videoQuality} clock={songVideoClock} playing={!preferences.videoPauseWithSong||!!track?.playing} onready={ready=>{if(backgroundResult===sourceResult)canvasReady=ready;}} onerror={()=>videoLookup?.playbackFailed(sourceResult)}/>
      {/key}
    {/if}
    <Crossfade reduced={lowMotion} direction={pageDirection} stationary={desktopControls&&(resting||machine.state==='expanded')} key={machine.state === "confirm" ? `confirm:${confirmId(machine)}` : settingsTab ? "settings" : lyricsOpen ? "lyrics" : spotifyCompact ? "spotify-small" : machine.state === "expanded" ? page : machine.state}>
      {@const C = CONTENT[machine.state]}
      {#if settingsTab && settingsProps}
        <div class="frame" style:width="{SETTINGS_SHAPE.w}px" style:height="{SETTINGS_SHAPE.h}px">
          <Settings {...settingsProps} tab={settingsTab} {preferences} canvasError={canvasError} {artworkError} oncanvaschange={()=>{canvasError='';canvasRevision++;}} onclose={closeSettings} />
        </div>
      {:else if customTab && machine.state==='expanded'}
        <div class="frame" style:width="440px" style:height="210px"><CustomTab tab={customTab} remaining={focusRemaining}/></div>
      {:else if lyricsOpen && track}
        <div class="frame" style:width="{playerShape.w}px" style:height="{playerShape.h}px"><Lyrics {track} {preferences} minimal={lyricsMinimal} onupdate={onpreferences} onclose={closeLyrics} onfullscreen={()=>showVideo('fullscreen',true)} /></div>
      {:else if spotifyExpanded && !track?.title}
        <div class="frame" style:width="{playerShape.w}px" style:height="{playerShape.h}px"><SpotifyEmpty error={track?.sessionError??''} onretry={onspotifyrefresh}/></div>
      {:else if spotifyExpanded && track && onmediacontrol}
        <div class="frame" style:width="{playerShape.w}px" style:height="{playerShape.h}px">
          <Spotify {track} minimal={spotifyMinimal} marquee={preferences.spotifyMarquee} seekBar={preferences.spotifySeekBar} videoReady={!!backgroundUrl} fullscreenActive={videoMode==='fullscreen'} {videoBusy} videoError={videoViewError} onvideo={showVideo} oncontrol={onmediacontrol} layout={preferences.spotifyLayout} artworkVideo={canvasReady&&preferences.videoHideArtwork} songColor={coverColor??NEUTRAL_COVER_TINT} showRepeat={preferences.spotifyShowRepeat} showShuffle={preferences.spotifyShowShuffle} waves={preferences.waves} level={audioLevel} style={preferences.waveStyle} sensitivity={preferences.waveSensitivity} reactivity={preferences.waveReactivity} smoothness={preferences.waveSmoothness} reduceMotion={lowMotion} {pinned} onpin={togglePin} onlyrics={openLyrics} {searchOpen} onsearchchange={(open)=>searchOpen=open} />
        </div>
      {:else if sudoExpanded}
        <div class="frame" style:width="{playerShape.w}px" style:height="{playerShape.h}px">
          <SudoChat messages={chat} busy={chatBusy} mode={sudoState} spot={artSpot} width={playerShape.w}
            onsend={(text) => void sendChat(text)} onfocuschange={(focused) => (askFocused = focused)}
            onorb={() => void invoke("jarvis_listen").catch(() => {})} onsetup={() => openSettings("jarvis")}
            onturnon={() => void invoke<{ wake?: boolean }>("jarvis_status").then((s) => invoke("jarvis_set_enabled", { enabled: true, wake: !!s.wake })).catch(() => {})} />
        </div>
      {:else if spotifyCompact && track}
        <div class="frame" style:width="{MINI_SHAPE.w}px" style:height="{MINI_SHAPE.h}px">
          <CompactSpotify {track} orb={smallOrb} marquee={preferences.spotifyMarquee} waves={preferences.waves} reactive={true} sensitivity={preferences.waveSensitivity} style={preferences.waveStyle} reactivity={preferences.waveReactivity} smoothness={preferences.waveSmoothness} level={audioLevel} reduceMotion={lowMotion} onclick={openExpanded} />
        </div>
      {:else if desktopControls && resting}
        <div class="frame" style:width="246px" style:height="44px"><CompactHome {preferences} remaining={focusRemaining} item={upcoming?.[0]} tab={compactPackage} onclick={()=>{if(compactPackage)customId=compactPackage.id;openExpanded();}} /></div>
      {:else if C}
        {@const size = homeSameAsSpotify ? { w: playerShape.w, h: playerShape.h } : frameSize(machine.state)}
        <div class="frame" style:width="{size.w}px" style:height="{size.h}px">
          {#if desktopControls && machine.state === 'expanded' && page === 'home'}
            <HomeWidgets {preferences} extensionRail={activeTabs.length>0} items={upcoming} remaining={focusRemaining} running={focusRunning} {onfocus} spot={artSpot} width={playerShape.w} />
          {:else}<C
            {...contentProps(machine.state, machine.payload, handlers, orbSlot)}
            {...frozen && machine.state === "expanded" ? { now: FROZEN_NOW } : {}}
            {...machine.state === "expanded" ? { items: upcoming, chrome: desktopControls } : {}}
            {...machine.state === "compact" ? upcoming?.[0] : {}}
          />{/if}
          {#if desktopControls && machine.state === "expanded" && page === "home"}
            <button type="button" class="home-pin-surface" aria-label={pinned ? "Unpin Home" : "Keep Home open"} aria-pressed={pinned} onclick={togglePin}></button>
          {/if}
        </div>
      {/if}
    </Crossfade>
    {#if desktopControls && machine.state === 'expanded' && !settingsTab && pinned}
      <span class="pin-indicator" aria-hidden="true"></span>
    {/if}
    {#if videoMenu && spotifyExpanded && !lyricsOpen}
      <div class="video-menu island-content" role="menu" aria-label="Spotify options">
        <span class="video-unavailable" role="status">{canvasReady?(backgroundResult?.source==='artwork'?'Animated album artwork is playing':'Spotify Canvas is playing'):(preferences.spotifyBackground==='artwork'?artworkError||canvasError||'Enable animated album artwork in Settings → Spotify.':canvasError||'Enable Spotify Canvas and connect in Settings → Spotify. Full music videos are not supplied by Canvas.')}</span>
        <button type="button" role="menuitem" onclick={() => (videoMenu = false)}>Close menu</button>
      </div>
    {/if}
    {#if desktopControls && machine.state === "expanded" && !settingsTab && settingsProps && !(lyricsOpen && lyricsMinimal) && !(spotifyExpanded && !lyricsOpen && spotifyMinimal && !searchOpen)}
      <IslandNavigation top={pill.y} clipWidth={pill.w} clipHeight={pill.h} radius={pill.r} spotify={!!track} selected={page} customId={customTab?.id} tabs={activeTabs} oncustom={id=>{closeLyrics();searchOpen=false;selection='home';customId=id;}} onselect={choosePage} onsettings={() => openSettings(page === "spotify" ? "spotify" : page === "sudo" ? "jarvis" : "home")} />
    {:else if desktopControls && resting && !spotifyCompact && !preferences.hoverExpand}
      <button type="button" class="open-island" aria-label="Expand Sudo" onclick={openExpanded}></button>
    {/if}
    {#if smallOrb}
      <!-- Sudo AI's orb on the small island whenever no song is playing; clicking it starts listening. -->
      <div class="compact-orb"><MiniOrb size={18} /></div>
    {/if}
  {/if}
</div>

<style>
  .glass.burn, .content.burn { transition: opacity 2.4s ease; }
  .burn-dim { opacity: .5; }
  .glass {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
    pointer-events: none;
  }
  .content {
    position: fixed;
    overflow: hidden;
  }
  .video-menu { position: absolute; bottom: 18px; left: 24px; z-index: 4; padding: 5px; border: 1px solid #ffffff24; border-radius: 12px; background: #23262e; box-shadow: 0 8px 24px #0004; }
  .video-menu button { display: block; width: 100%; border: 0; border-radius: 8px; padding: 9px 12px; color: var(--text-primary); background: transparent; text-align: left; font: var(--type-label); cursor: pointer; }
  .video-menu button:hover { background: var(--surface-control-hover); }
  .video-unavailable { display: block; max-width: 240px; padding: 8px 12px; font: var(--type-caption); color: var(--text-primary); }
  .open-island { position: absolute; inset: 0; width: 100%; border: 0; border-radius: inherit; background: transparent; cursor: pointer; }
  .compact-orb { position: absolute; right: 5px; top: 50%; z-index: 5; transform: translateY(-50%); }
  .home-pin-surface { position: absolute; inset: 12px 64px 12px 20px; border: 0; border-radius: 18px; background: transparent; cursor: pointer; }
  .pin-indicator { position: absolute; bottom: 5px; left: 50%; width: 72px; height: 2px; border-radius: 2px; background: #ffffff70; transform: translateX(-50%); pointer-events: none; }
  .home-art { position: absolute; inset: 1px; border-radius: inherit; overflow: hidden; pointer-events: none; opacity: calc(0.32 * var(--surface-opacity, 1)); }
  .home-art img { width: 100%; height: 100%; object-fit: cover; filter: blur(20px) brightness(0.55); transform: scale(1.16); }
  /* Content is laid out at its state's target size and stays centred while the pill springs. */
  .frame {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
  }
  /* Keep desktop panels at their final position while the glass reveals them. */
  .stable-panel .frame { top: 0; transform: translateX(-50%); }
</style>
