import {DEFAULT_REPOSITORY,repositoryName} from './catalogue';
import {validateTab,type TabPackage} from './extensions';
import { HOME_WIDGETS, type HomeWidget } from './homeWidgets';
import {BORDER_STYLES,type IslandBorder} from '../gl/borders';
export type AlarmTone = "chime" | "bell" | "pulse" | "custom";
export type SongColorScope = 'off'|'spotify'|'spotify-home'|'spotify-compact'|'all';
export const SONG_COLOR_SCOPES: { id: SongColorScope; label: string }[] = [
  { id: 'off', label: 'Off' },
  { id: 'spotify', label: 'Spotify tab' },
  { id: 'spotify-home', label: 'Spotify and Home' },
  { id: 'spotify-compact', label: 'Spotify and small island' },
  { id: 'all', label: 'Spotify, Home and small island' },
];

export type SettingsTab = "general" | "look" | "island" | "home" | "alarm" | "program" | "spotify" | "marketplace" | "jarvis";
export type ReminderOverride = { enabled?: boolean; time?: string };
export type ProgramEntry = {id:string;label:string;detail:string;time:string;days:number[];enabled:boolean};

export type Preferences = {
  alarms: boolean;
  iphoneAlarms: boolean;
  tone: AlarmTone;
  volume: number;
  snooze: number;
  gradualSleep: boolean;
  pauseSpotify: boolean;
  resumeSpotify: boolean;
  reminders: Record<string, ReminderOverride>;
  hoverExpand: boolean;
  interfaceSounds: boolean;
  reduceMotion: boolean;
  rememberPage: boolean;
  waves: boolean;
  video: boolean;
  spotifyBridge:boolean;
  /** Behind the Spotify player: the song image only, Apple Music's video, or Spotify's Canvas video. */
  spotifyBackground: 'image' | 'canvas' | 'artwork';
  videoCanvasFallback: boolean;
  videoHideArtwork: boolean;
  videoPauseWithSong: boolean;
  videoQuality: 'best' | '2160' | '1080' | '720' | '480';
  spotifyLayout: "centered" | "artwork-left" | "cover-card" | "wide";
  spotifyShowRepeat: boolean;
  /** Draggable song position bar in the player. */
  spotifySeekBar: boolean;
  spotifyShowShuffle: boolean;
  /** Song titles and artists too long to fit glide right to left. */
  spotifyMarquee: boolean;
  homeArtwork: boolean;
  homeSongColor:boolean;
  /** Where the island takes the current song's color. Settings always keeps the default look. */
  songColor:SongColorScope;
  alarmFade: boolean;
  reactiveWaves: boolean;
  waveSensitivity:number;
  waveStyle:'classic'|'spectrum'|'fine';
  waveReactivity:number;
  waveSmoothness:number;
  compactAlwaysVisible:boolean;
  /** Seconds the small island waits after the cursor leaves before hiding (when not always on screen). */
  compactHideDelay:number;
  /** Gently shifts and dims the resting island to protect OLED/plasma displays. */
  burnInProtection:boolean;
  pinnedEdgeClearance:boolean;
  lyricFlow:boolean;
  lyricWordSync:boolean;
  lyricLayout:'spotlight'|'balanced'|'focus';
  lyricFont:'sans'|'rounded'|'serif';
  lyricAlign:'left'|'center'|'right';
  lyricScale:number;
  lyricWeight:number;
  lyricSpacing:'compact'|'comfortable'|'airy';
  lyricContext:'dim'|'soft'|'clear';
  lyricGuide:boolean;
  lyricFlowDuration:number;
  lyricOffset:number;
  vocalVolume:number;
  edgeAvoidance: boolean;
  lyricTranslation: boolean;
  lyricRomanization: boolean;
  lyricLanguage: string;
  islandBorder: IslandBorder;
  /** Control style: Sudo's own, or Glisse (Apple Liquid Glass) buttons, switches and sliders. */
  islandDesign: 'standard'|'glisse';
  islandDarkness: number;
  islandTransparency: number;
  homeWidgets: HomeWidget[];
  homeNote: string;
  /** City for the Home weather card (sent only to Open-Meteo). */
  homeCity: string;
  checkUpdates: boolean;
  focusMinutes: number;
  compactWidget: "clock"|"reminders"|"focus"|"note";
  compactTab: string;
  tabs: TabPackage[];
  disabledTabs:string[];
  catalogueRepository:string;
  backgroundBlur: number;
  showFps: boolean;
  screenshotMode: boolean;
  program: ProgramEntry[] | null;
};

export const DEFAULTS: Preferences = {
  alarms: true, iphoneAlarms: false, tone: "chime", volume: 70, snooze: 5, gradualSleep: true,
  pauseSpotify: false, resumeSpotify: true, reminders: {},
  hoverExpand: true, interfaceSounds: true, reduceMotion: false,
  rememberPage: true, waves: true, video: false, spotifyBackground: 'canvas', spotifyLayout: "centered", homeArtwork: false,homeSongColor:false,songColor:'spotify-compact',
  spotifyShowRepeat:false,spotifyShowShuffle:false,spotifyMarquee:true,spotifySeekBar:false,spotifyBridge:false,
  videoCanvasFallback:false,videoHideArtwork:false,videoPauseWithSong:true,videoQuality:'best',
  alarmFade: false, reactiveWaves: false, waveSensitivity:1.6,lyricFlow:false,vocalVolume:30, edgeAvoidance: false,
  waveStyle:'classic',waveReactivity:40,waveSmoothness:65,compactAlwaysVisible:true,compactHideDelay:0,burnInProtection:false,pinnedEdgeClearance:false,
  lyricTranslation: false, lyricRomanization: false, lyricLanguage: "auto",
  lyricLayout:'spotlight',lyricFont:'sans',lyricAlign:'left',lyricScale:1,lyricWeight:650,
  lyricSpacing:'comfortable',lyricContext:'dim',lyricGuide:true,lyricFlowDuration:420,lyricOffset:0,lyricWordSync:false,
  islandBorder: "glass",islandDesign:"standard",
  islandDarkness: 50,
  islandTransparency: 0, homeWidgets: ['clock','reminders'], homeNote: '', homeCity: '', checkUpdates: false,
  focusMinutes:25,compactWidget:"reminders",compactTab:"auto",tabs:[],disabledTabs:[],catalogueRepository:DEFAULT_REPOSITORY,
  backgroundBlur: 0, showFps: false, screenshotMode: false, program: null,
};

export const SETTINGS_KEY = "sudo.preferences.v1";
export const PAGE_KEY = "sudo.island.page";

export function loadPreferences(storage: Pick<Storage, "getItem">): Preferences {
  try {
    const saved = JSON.parse(storage.getItem(SETTINGS_KEY) ?? "null") as Partial<Preferences> | null;
    if (!saved || typeof saved !== "object") return { ...DEFAULTS };
    const result = { ...DEFAULTS };
    const switches = ["showFps", "alarms", "iphoneAlarms", "gradualSleep", "pauseSpotify", "resumeSpotify", "hoverExpand",
      "interfaceSounds", "reduceMotion", "rememberPage", "waves", "video", "spotifyBridge", "videoCanvasFallback", "videoHideArtwork", "videoPauseWithSong", "spotifyShowRepeat", "spotifyShowShuffle", "spotifyMarquee", "spotifySeekBar", "homeArtwork", "alarmFade", "reactiveWaves", "edgeAvoidance", "lyricTranslation", "lyricRomanization"] as const;
    for (const key of switches) if (typeof saved[key] === "boolean") result[key] = saved[key];
    if(typeof saved.catalogueRepository==='string'){try{result.catalogueRepository=repositoryName(saved.catalogueRepository);}catch{}}
    if(typeof saved.focusMinutes==='number'&&Number.isFinite(saved.focusMinutes))result.focusMinutes=Math.max(1,Math.min(180,Math.round(saved.focusMinutes)));
    if(['clock','reminders','focus','note'].includes(saved.compactWidget??''))result.compactWidget=saved.compactWidget!;
    if(Array.isArray(saved.tabs))result.tabs=saved.tabs.slice(0,12).flatMap(value=>{try{return [validateTab(value)];}catch{return [];}}).filter((tab,i,all)=>all.findIndex(t=>t.id===tab.id)===i);
    if(typeof saved.compactTab==='string'&&(['auto','home','spotify'].includes(saved.compactTab)||result.tabs.some(t=>'tab:'+t.id===saved.compactTab)))result.compactTab=saved.compactTab;
    if(typeof saved.lyricFlow==='boolean')result.lyricFlow=saved.lyricFlow;
    if(typeof saved.lyricWordSync==='boolean')result.lyricWordSync=saved.lyricWordSync;
    if(typeof saved.homeSongColor==='boolean')result.homeSongColor=saved.homeSongColor;
    // Before this option the colour showed on Spotify and the small island, plus Home when that switch was on.
    result.songColor=SONG_COLOR_SCOPES.some(scope=>scope.id===saved.songColor)?saved.songColor as SongColorScope:result.homeSongColor?'all':'spotify-compact';
    if(typeof saved.lyricGuide==='boolean')result.lyricGuide=saved.lyricGuide;
    if(['spotlight','balanced','focus'].includes(saved.lyricLayout??''))result.lyricLayout=saved.lyricLayout!;
    if(['sans','rounded','serif'].includes(saved.lyricFont??''))result.lyricFont=saved.lyricFont!;
    if(['left','center','right'].includes(saved.lyricAlign??''))result.lyricAlign=saved.lyricAlign!;
    if(['compact','comfortable','airy'].includes(saved.lyricSpacing??''))result.lyricSpacing=saved.lyricSpacing!;
    if(['dim','soft','clear'].includes(saved.lyricContext??''))result.lyricContext=saved.lyricContext!;
    for(const [key,min,max] of [['lyricScale',.7,1.4],['lyricOffset',-5,5],['lyricWeight',400,800],['lyricFlowDuration',200,800]] as const){const value=saved[key];if(typeof value==='number'&&Number.isFinite(value))result[key]=Math.max(min,Math.min(max,value));}
    if(Array.isArray(saved.disabledTabs))result.disabledTabs=saved.disabledTabs.filter((id):id is string=>typeof id==='string'&&result.tabs.some(t=>t.id===id));
    if(typeof saved.vocalVolume==='number'&&Number.isFinite(saved.vocalVolume))result.vocalVolume=Math.max(0,Math.min(100,Math.round(saved.vocalVolume)));
    if(typeof saved.waveSensitivity==='number'&&Number.isFinite(saved.waveSensitivity))result.waveSensitivity=Math.max(.5,Math.min(3,saved.waveSensitivity));
    if(['classic','spectrum','fine'].includes(saved.waveStyle??''))result.waveStyle=saved.waveStyle!;
    for(const key of ['waveReactivity','waveSmoothness'] as const){const value=saved[key];if(typeof value==='number'&&Number.isFinite(value))result[key]=Math.max(0,Math.min(100,value));}
    if(typeof saved.compactAlwaysVisible==='boolean')result.compactAlwaysVisible=saved.compactAlwaysVisible;
    if(typeof saved.compactHideDelay==='number'&&Number.isFinite(saved.compactHideDelay))result.compactHideDelay=Math.max(0,Math.min(300,Math.round(saved.compactHideDelay)));
    if(typeof saved.burnInProtection==='boolean')result.burnInProtection=saved.burnInProtection;
    if(typeof saved.pinnedEdgeClearance==='boolean')result.pinnedEdgeClearance=saved.pinnedEdgeClearance;
    if (result.lyricRomanization) result.lyricTranslation = false;
    if(typeof saved.screenshotMode==='boolean')result.screenshotMode=saved.screenshotMode;
    if(typeof saved.backgroundBlur==='number'&&Number.isFinite(saved.backgroundBlur))result.backgroundBlur=Math.max(0,Math.min(100,saved.backgroundBlur));
    if(Array.isArray(saved.program)) {
      const ids=new Set<string>();
      result.program=saved.program.slice(0,250).flatMap((entry)=>{
        if(!entry||typeof entry!=='object'||typeof entry.id!=='string'||!entry.id||entry.id.length>100||ids.has(entry.id)||typeof entry.label!=='string'||!entry.label.trim()||typeof entry.time!=='string'||!isTime(entry.time)||!Array.isArray(entry.days))return [];
        const days=[...new Set(entry.days.filter((day):day is number=>Number.isInteger(day)&&day>=0&&day<=6))];
        if(!days.length)return [];
        ids.add(entry.id);
        return [{id:entry.id,label:entry.label.trim().slice(0,100),detail:typeof entry.detail==='string'?entry.detail.slice(0,1000):'',time:entry.time,days,enabled:entry.enabled!==false}];
      });
    }
    if (typeof saved.checkUpdates === 'boolean') result.checkUpdates=saved.checkUpdates;
    if (typeof saved.islandTransparency === 'number' && Number.isFinite(saved.islandTransparency)) result.islandTransparency=Math.max(0,Math.min(100,saved.islandTransparency));
    if (Array.isArray(saved.homeWidgets)) result.homeWidgets=[...new Set(saved.homeWidgets.filter((id):id is HomeWidget=>HOME_WIDGETS.some(w=>w.id===id)))];
    if (typeof saved.homeNote === 'string') result.homeNote=saved.homeNote.slice(0,500);
    if (typeof saved.homeCity === 'string') result.homeCity=saved.homeCity.trim().slice(0,80);
    if (typeof saved.islandDarkness === "number" && Number.isFinite(saved.islandDarkness)) result.islandDarkness = Math.min(100, Math.max(0,saved.islandDarkness));
    if (typeof saved.lyricLanguage === "string" && /^(auto|[a-z]{2,3}(-[A-Za-z]{2,4})?)$/.test(saved.lyricLanguage)) result.lyricLanguage = saved.lyricLanguage;
    if (saved.spotifyLayout === "centered" || saved.spotifyLayout === "artwork-left" || saved.spotifyLayout === "cover-card" || saved.spotifyLayout === "wide") result.spotifyLayout = saved.spotifyLayout;
    if (saved.spotifyBackground === 'image' || saved.spotifyBackground === 'canvas' || saved.spotifyBackground === 'artwork') result.spotifyBackground = saved.spotifyBackground;
    if (['best','2160','1080','720','480'].includes(saved.videoQuality??'')) result.videoQuality=saved.videoQuality!;
    if (BORDER_STYLES.some(b=>b.id===saved.islandBorder)) result.islandBorder = saved.islandBorder!;
    if (saved.islandDesign==='glisse'||saved.islandDesign==='standard') result.islandDesign = saved.islandDesign;
    if (["chime", "bell", "pulse", "custom"].includes(saved.tone ?? "")) result.tone = saved.tone!;
    if (typeof saved.volume === "number") result.volume = Math.min(100, Math.max(0, saved.volume));
    if ([1, 3, 5, 10, 15, 20].includes(saved.snooze ?? 0)) result.snooze = saved.snooze!;
    if (saved.reminders && typeof saved.reminders === "object") result.reminders = saved.reminders;
    return result;
  } catch {
    return { ...DEFAULTS };
  }
}

export function isTime(value: string): boolean {
  return /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
}
