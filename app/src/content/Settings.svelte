<script lang="ts">
  import { onDestroy } from "svelte";
  import { SONG_COLOR_SCOPES, type AlarmTone, type Preferences, type SettingsTab } from "../island/preferences";
  import {HOME_WIDGETS} from '../island/homeWidgets';
  import {currentVersion} from '../island/updates';
  import Marketplace from './Marketplace.svelte';
  import ProgramEditor from './ProgramEditor.svelte';
  import VocalSettings from './VocalSettings.svelte';
  import JarvisSettings from './JarvisSettings.svelte';
  import LyricsSettings from './LyricsSettings.svelte';
  import {BORDER_STYLES} from '../gl/borders';
  import CanvasSettings from './CanvasSettings.svelte';
  import SpotifyBridgeSettings from './SpotifyBridgeSettings.svelte';
  import IPhoneAlarmSettings from './IPhoneAlarmSettings.svelte';
  import OnlineCompanionSettings from './OnlineCompanionSettings.svelte';
  import ToolsSettings from './ToolsSettings.svelte';
  import type {CloudState} from '../island/onlineCompanion';
  import { invoke } from '@tauri-apps/api/core';

  type SwitchKey = { [K in keyof Preferences]: Preferences[K] extends boolean ? K : never }[keyof Preferences];
  let { tab = "alarm", preferences, soundName, videoName, trackTitle, onupdate, onclose, onpreview,
    onstoppreview, ontest, onfile, startupEnabled = null, startupBusy = false, startupError = "", onstartup,
    updateBusy=false,updateMessage='',oncheckupdates,onreleases,updateAvailable=false,updateDownloading=false,updateInstalling=false,updateReady=false,ondownloadupdate,oninstallupdate,canvasError='',artworkError='',oncanvaschange,companionState=null,companionMessage='',oncompanionsync,calendarStatus={},onrefreshcalendars }: {
    calendarStatus?:Record<string,string>;onrefreshcalendars?:()=>void;
    companionState?:CloudState|null;companionMessage?:string;oncompanionsync?:()=>void;
    canvasError?:string;artworkError?:string;oncanvaschange?:()=>void;
    tab?: SettingsTab;
    preferences: Preferences;
    soundName?: string;
    videoName?: string;
    trackTitle?: string;
    onupdate: (patch: Partial<Preferences>) => void;
    onclose: () => void;
    onpreview: (ended: () => void) => void;
    onstoppreview: () => void;
    ontest?: () => void;
    onfile: (kind: "sound" | "video", file?: File) => Promise<void>;
    startupEnabled?: boolean | null;
    startupBusy?: boolean;
    startupError?: string;
    onstartup?: (enabled: boolean) => void;
    updateAvailable?:boolean;updateDownloading?:boolean;updateInstalling?:boolean;updateReady?:boolean;ondownloadupdate?:()=>void;oninstallupdate?:()=>void;
    updateBusy?:boolean;updateMessage?:string;oncheckupdates?:()=>void;onreleases?:()=>void;
  } = $props();

  const sections: { id: SettingsTab; title: string }[] = [
    { id: "general", title: "General" }, { id: "look", title: "Look" }, { id: "island", title: "Island" }, { id: "home", title: "Home" },
    { id: "spotify", title: "Spotify" }, { id: "jarvis", title: "Sudo AI" }, { id: "tools", title: "Tools" }, { id: "alarm", title: "Alarms" },
    { id: "program", title: "Program" }, { id: "marketplace", title: "Extensions" },
  ];
  // svelte-ignore state_referenced_locally
  let section = $state(tab);
  let body: HTMLDivElement;
  let previewing = $state(false);
  let loading = $state(false);
  let status = $state("");
  let uninstallBusy = $state(false);
  let uninstallStatus = $state('');
  const appearancePresets = [
    {name:'Clear glass', islandBorder:'glass', islandDarkness:50, islandTransparency:0, backgroundBlur:0},
    {name:'Quiet glass', islandBorder:'glass', islandDarkness:78, islandTransparency:0, backgroundBlur:55},
    {name:'Fine outline', islandBorder:'white-line', islandDarkness:50, islandTransparency:0, backgroundBlur:0},
  ] satisfies ({name:string}&Pick<Preferences,'islandBorder'|'islandDarkness'|'islandTransparency'|'backgroundBlur'>)[];
  function moveWidget(index:number, direction:number) {
    const widgets=[...preferences.homeWidgets], target=index+direction;
    if(target<0||target>=widgets.length)return;
    [widgets[index],widgets[target]]=[widgets[target],widgets[index]];
    onupdate({homeWidgets:widgets});
  }
  async function uninstall() {
    uninstallBusy=true;uninstallStatus='';
    try { uninstallStatus=await invoke<string>('open_uninstall_settings'); }
    catch(error) { uninstallStatus=String(error); }
    finally { uninstallBusy=false; }
  }

  onDestroy(() => { if (previewing) onstoppreview(); });

  function preview() {
    if (previewing) { onstoppreview(); previewing = false; }
    else { previewing = true; onpreview(() => (previewing = false)); }
  }

  async function saveFile(kind: "sound" | "video", file?: File) {
    loading = true;
    status = "";
    try { await onfile(kind, file); }
    catch (error) { status = error instanceof Error ? error.message : "Could not save that file."; }
    finally { loading = false; }
  }
</script>

{#snippet toggle(key: SwitchKey, label: string, help?: string)}
  <label class="row switch">
    <span><strong>{label}</strong>{#if help}<small>{help}</small>{/if}</span>
    <input type="checkbox" role="switch" checked={preferences[key]} onchange={(e) => onupdate({ [key]: e.currentTarget.checked })} />
  </label>
{/snippet}

<section class="settings" aria-label="Sudo settings">
  <header>
    <h2>Settings</h2>
    <button type="button" class="pill-btn close" onclick={onclose}>Done</button>
  </header>
  <nav aria-label="Settings sections">
    {#each sections as item (item.id)}
      <button type="button" class="tab" aria-pressed={section === item.id} onclick={() => { section = item.id; status = ""; if (body) body.scrollTop = 0; }}>{item.title}</button>
    {/each}
  </nav>
  <div class="body" bind:this={body} role="region" aria-label={`${section} settings`}>
    {#if section === "alarm"}
      <h3 class="group-label">Schedule</h3>
      {@render toggle("alarms", "Program alarms")}
      <h3 class="group-label">iPhone Clock</h3>
      <OnlineCompanionSettings cloud={companionState} message={companionMessage} onrefresh={oncompanionsync}/>
      <IPhoneAlarmSettings {preferences} onchange={iphoneAlarms=>onupdate({iphoneAlarms})}/>
      <h3 class="group-label">Sound & volume</h3>
      <fieldset class="sound-presets"><legend>Alarm sound</legend><div class="preset-options">
        {#each [{id:"chime",label:"Digital watch"},{id:"bell",label:"Twin bell"},{id:"custom",label:"My audio"}] as tone (tone.id)}
          <button type="button" class="pill-btn" aria-pressed={preferences.tone === tone.id || (tone.id==='chime'&&preferences.tone==='pulse')} onclick={() => onupdate({tone:tone.id as AlarmTone})}>{tone.label}</button>
        {/each}
      </div></fieldset>
      <p class="note">Recorded alarms · Joseph SARDIN / BigSoundBank · CC0. Available offline.</p>
        <label class="row file"><span><strong>Audio file</strong><small>{soundName || "Choose MP3, WAV, OGG or FLAC"}</small></span>
          <input type="file" accept="audio/*,.mp3,.wav,.ogg,.flac,.m4a" disabled={loading} onchange={(e) => { const file = e.currentTarget.files?.[0]; if (file) void saveFile("sound", file); }} /></label>
      {@render toggle("alarmFade", "Gentle alarm start", "Fade from silence to your chosen volume over 10 seconds.")}
      <label class="row"><strong>Alarm volume <small class="inline">{preferences.volume}%</small></strong><input type="range" min="0" max="100" value={preferences.volume} oninput={(e) => onupdate({ volume: e.currentTarget.valueAsNumber })} /></label>
      <label class="row"><strong>Snooze duration</strong><select value={preferences.snooze} onchange={(e) => onupdate({ snooze: Number(e.currentTarget.value) })}>
        {#each [1, 3, 5, 10, 15, 20] as minute (minute)}<option value={minute}>{minute} min</option>{/each}
      </select></label>
      {@render toggle("pauseSpotify", "Pause Spotify during alarms")}
      {#if preferences.pauseSpotify}{@render toggle("resumeSpotify", "Resume Spotify after the alarm", "If Sudo paused it and the same track is still selected.")}{/if}
      <div class="actions"><button type="button" class="pill-btn" onclick={preview}>{previewing ? "Stop preview" : "Preview sound"}</button>
        {#if ontest}<button type="button" class="pill-btn" onclick={ontest}>Test full alarm</button>{/if}</div>
    {:else if section === 'marketplace'}
      <Marketplace repository={preferences.catalogueRepository} onrepository={catalogueRepository=>onupdate({catalogueRepository})} tabs={preferences.tabs} disabledTabs={preferences.disabledTabs} ondisabled={disabledTabs=>onupdate({disabledTabs})} onchange={tabs=>onupdate({tabs})}/>
    {:else if section === "program"}
      <ProgramEditor {preferences} {onupdate} />
    {:else if section === "general"}
      <p class="intro">Startup, sounds, motion and updates.</p>
      <h3 class="group-label">Startup</h3>
      <label class="row switch"><span><strong>Run on startup</strong><small>Open Sudo when you sign in to Windows.</small></span>
        <input type="checkbox" role="switch" checked={startupEnabled === true} disabled={startupBusy || startupEnabled === null || !onstartup}
          onchange={(e) => { const wanted = e.currentTarget.checked; e.currentTarget.checked = startupEnabled === true; onstartup?.(wanted); }} /></label>
      {#if startupError}<p class="note" role="status">{startupError}</p>{/if}
      {@render toggle("rememberPage", "Remember the selected page")}
      <h3 class="group-label">Comfort</h3>
      {@render toggle("reduceMotion", "Reduce island motion")}
      {@render toggle("interfaceSounds", "Interface sounds", "Applies to Sudo’s interaction sounds.")}
      {@render toggle("screenshotMode", "Visible in screenshots", "For any capture tool. Freezes the glass backdrop while on; turn off for live refraction. Print Screen and Win+Shift+S also enable a temporary capture window automatically.")}
      <h3 class="group-label">App & updates</h3>
      <div class="row"><span><strong>Updates</strong><small>Installed version {currentVersion}. Download new versions here, then open Windows setup. Installation is not automatic.</small></span></div>
      {@render toggle('checkUpdates','Check GitHub on startup','Checks once when Sudo opens. No background polling.')}
      <div class="actions"><button type="button" class="pill-btn" disabled={updateBusy||updateDownloading||updateInstalling} onclick={oncheckupdates}>{updateBusy?'Checking…':'Check for updates'}</button><button type="button" class="pill-btn" onclick={onreleases}>GitHub releases</button></div>
      {#if updateAvailable||updateReady}<div class="actions">{#if updateReady}<button type="button" class="pill-btn" disabled={updateInstalling} onclick={oninstallupdate}>{updateInstalling?'Opening setup…':'Install update'}</button>{:else}<button type="button" class="pill-btn" disabled={updateDownloading} onclick={ondownloadupdate}>{updateDownloading?'Downloading…':'Download update'}</button>{/if}</div>{/if}
      {#if updateMessage}<p class="note" role="status">{updateMessage}</p>{/if}
      <div class="row"><span><strong>Uninstall Sudo</strong><small>Open Windows Installed apps, find sudo-island and choose Uninstall. For a portable copy, close Sudo and delete its executable.</small></span><button type="button" class="pill-btn" disabled={uninstallBusy} onclick={uninstall}>{uninstallBusy?'Opening…':'Uninstall Sudo…'}</button></div>
      {#if uninstallStatus}<p class="note" role="status">{uninstallStatus}</p>{/if}
      <h3 class="group-label">Advanced</h3>
      {@render toggle('showFps','Show FPS counter','Glass draw submissions during animation, including average and worst frame gaps. Idle shows the last animation; this is not an on-screen presentation measurement.')}
    {:else if section === "look"}
      <p class="intro">How the island looks. Changes apply instantly.</p>
      <h3 class="group-label">Style</h3>
      <fieldset class="sound-presets"><legend>Design</legend><p class="note">Glisse gives buttons, switches and sliders Apple's Liquid Glass look (after <a href="https://github.com/0kzh/glisse" target="_blank" rel="noreferrer">Glisse</a> by Kelvin Zhang, MIT).</p><div class="preset-options">{#each [['standard','Sudo'],['glisse','Glisse']] as [id,label] (id)}<button type="button" class="pill-btn" aria-pressed={preferences.islandDesign===id} onclick={()=>onupdate({islandDesign:id as 'standard'|'glisse'})}>{label}</button>{/each}</div></fieldset>
      <fieldset class="design-locked" disabled={preferences.islandDesign==='glisse'} aria-describedby={preferences.islandDesign==='glisse'?'glisse-lock-note':undefined}>{#if preferences.islandDesign==='glisse'}<p class="note lock-note" id="glisse-lock-note">Glisse sets the island's glass, border and colours. Switch Design back to Sudo to change these.</p>{/if}
      <fieldset class="sound-presets"><legend>Quick appearance</legend><div class="preset-options">{#each appearancePresets as preset (preset.name)}<button type="button" class="pill-btn" aria-pressed={preferences.islandBorder===preset.islandBorder&&preferences.islandDarkness===preset.islandDarkness&&preferences.islandTransparency===preset.islandTransparency&&preferences.backgroundBlur===preset.backgroundBlur} onclick={()=>{const {name,...appearance}=preset;onupdate(appearance);}}>{preset.name}</button>{/each}</div><p class="note">Start with a preset, then adjust the controls below. Your choices are saved automatically.</p></fieldset>
      <fieldset class="border-picker"><legend>Island border</legend><p class="note">Every edge keeps the liquid-glass background.</p><div>{#each BORDER_STYLES as border}<button type="button" class={'border-option '+border.id} aria-pressed={preferences.islandBorder===border.id} onclick={()=>onupdate({islandBorder:border.id})}><i aria-hidden="true"></i><span>{border.label}<small>{border.description}</small></span></button>{/each}</div></fieldset>
      <label class="row"><span><strong>Island darkness <small class="inline">{preferences.islandDarkness}%</small></strong><small>Choose how dark the background looks.</small></span><input type="range" min="0" max="100" value={preferences.islandDarkness} oninput={(event)=>onupdate({islandDarkness:event.currentTarget.valueAsNumber})} /></label>
      <label class="row"><span><strong>Island transparency <small class="inline">{preferences.islandTransparency}%</small></strong><small>Make the surface see-through while keeping text clear.</small></span><input type="range" min="0" max="100" value={preferences.islandTransparency} oninput={(event)=>onupdate({islandTransparency:event.currentTarget.valueAsNumber})} /></label>
      <label class="row"><span><strong>Background blur <small class="inline">{preferences.backgroundBlur}%</small></strong><small>Soften the desktop visible through liquid glass.</small></span><input type="range" min="0" max="100" value={preferences.backgroundBlur} oninput={(event)=>onupdate({backgroundBlur:event.currentTarget.valueAsNumber})} /></label>
      </fieldset>
      <h3 class="group-label">Song color</h3>
      <label class="row"><span><strong>Song color on the island</strong><small>Tint the island with the current cover’s color. Settings always keeps the default look.</small></span><select aria-label="Song color on the island" value={preferences.songColor} onchange={e=>onupdate({songColor:e.currentTarget.value as Preferences['songColor']})}>{#each SONG_COLOR_SCOPES as scope (scope.id)}<option value={scope.id}>{scope.label}</option>{/each}</select></label>
      <h3 class="group-label">Music</h3>
      {@render toggle("reactiveEdges", "Music-reactive edges", "The island's edge glows with the beat, in the song's color, while Spotify plays.")}
      {#if preferences.reactiveEdges}<label class="row"><span><strong>Edge glow <small class="inline">{preferences.edgeIntensity}%</small></strong><small>How strongly the edge reacts.</small></span><input type="range" min="10" max="100" value={preferences.edgeIntensity} oninput={(event)=>onupdate({edgeIntensity:event.currentTarget.valueAsNumber})} /></label>{/if}
    {:else if section === "tools"}
      <ToolsSettings {preferences} {onupdate} {calendarStatus} {onrefreshcalendars} />
    {:else if section === "island"}
      <p class="intro">How the island opens, hides and behaves at the top of your screen.</p>
      <h3 class="group-label">Opening and hiding</h3>
      {@render toggle("hoverExpand", "Expand on hover", "When off, click the small island to expand it.")}
      {@render toggle('compactAlwaysVisible','Always on screen','Keep the small island visible. When off, leaving an unpinned island hides it; scroll down where it sits at the top of the screen to bring it back. Pinned panels and Settings stay open.')}
      {#if !preferences.compactAlwaysVisible}<label class="row"><span><strong>Hide the small island after</strong><small>How long to wait after your cursor leaves the small island. The expanded island never hides on its own.</small></span><select aria-label="Hide the small island after" value={preferences.compactHideDelay} onchange={(e)=>onupdate({compactHideDelay:Number(e.currentTarget.value)})}>
        {#each [[0,'Immediately'],[2,'2 seconds'],[5,'5 seconds'],[10,'10 seconds'],[20,'20 seconds'],[30,'30 seconds'],[60,'1 minute'],[120,'2 minutes'],[300,'5 minutes']] as [seconds,label] (seconds)}<option value={seconds}>{label}</option>{/each}
      </select></label>{/if}
      {@render toggle('burnInProtection','Burn-in protection','For OLED and other displays that can burn in. The small island moves by a few pixels every couple of minutes and dims after a minute without your cursor. It brightens again as soon as you point at it.')}
      {@render toggle("edgeAvoidance", "Make room at the top edge", "Move down smoothly to clear the area behind the island.")}
      {@render toggle('pinnedEdgeClearance','Clear space above a pinned panel','Only while a large panel is kept open (clicked to pin, or showing lyrics): move your cursor above it to slide it down by its full height, clearing the original area. Enter the lowered panel or leave the area to return it. Other edge gestures keep their small movement.')}
      <h3 class="group-label">Small island</h3>
      <label class="row"><span><strong>Always show compact tab</strong><small>Automatic follows your expanded tab.</small></span><select value={preferences.compactTab} onchange={e=>onupdate({compactTab:e.currentTarget.value})}><option value="auto">Automatic</option><option value="home">Home</option><option value="spotify">Spotify</option>{#each preferences.tabs as tab (tab.id)}<option value={'tab:'+tab.id}>{tab.name}</option>{/each}</select></label>
      <label class="row"><strong>Compact Home widget</strong><select value={preferences.compactWidget} onchange={e=>onupdate({compactWidget:e.currentTarget.value as Preferences['compactWidget']})}><option value="clock">Clock & date</option><option value="reminders">Upcoming reminder</option><option value="focus">Focus timer</option><option value="note">Personal note</option><option value="system">CPU, memory & GPU</option></select></label>
    {:else if section === "home"}
      <p class="intro">What Home shows. Pick up to three cards below the clock; the first three in your order are shown.</p>
      <h3 class="group-label">Widgets</h3>
      <fieldset class="widget-picker"><legend>Home widgets</legend>
        {#each HOME_WIDGETS as widget (widget.id)}<label class="row switch"><strong>{widget.label}</strong><input type="checkbox" role="switch" checked={preferences.homeWidgets.includes(widget.id)} onchange={(event)=>onupdate({homeWidgets:event.currentTarget.checked?[...preferences.homeWidgets,widget.id]:preferences.homeWidgets.filter(id=>id!==widget.id)})} /></label>{/each}
        {#if preferences.homeWidgets.length>1}<p class="note">Home display order</p><ol class="widget-order">{#each preferences.homeWidgets as id,index (id)}<li><span>{HOME_WIDGETS.find(widget=>widget.id===id)?.label}</span><button class="pill-btn" type="button" aria-label={`Move ${id} up`} disabled={index===0} onclick={()=>moveWidget(index,-1)}>↑</button><button class="pill-btn" type="button" aria-label={`Move ${id} down`} disabled={index===preferences.homeWidgets.length-1} onclick={()=>moveWidget(index,1)}>↓</button></li>{/each}</ol>{/if}
        {#if preferences.homeWidgets.includes('note')}<label class="note-editor"><strong>Personal note</strong><textarea aria-label="Personal note" maxlength="500" rows="3" value={preferences.homeNote} placeholder="Something to keep in view…" oninput={(event)=>onupdate({homeNote:event.currentTarget.value})}></textarea><small>{preferences.homeNote.length}/500 · saved automatically</small></label>{/if}
      </fieldset>
      {#if preferences.homeWidgets.includes('weather')}<label class="row"><span><strong>Weather city</strong><small>For the weather card. Only this name is sent, to Open-Meteo (free, no account).</small></span><input aria-label="Weather city" class="text-input" type="text" maxlength="80" placeholder="e.g. Athens" value={preferences.homeCity} onchange={(event)=>onupdate({homeCity:event.currentTarget.value.trim()})} /></label>{/if}
      <h3 class="group-label">Details</h3>
      <label class="row"><span><strong>Focus duration</strong><small>Minutes. Changes apply to a stopped timer or the next reset.</small></span><input aria-label="Focus duration" type="number" min="1" max="180" value={preferences.focusMinutes} onchange={e=>onupdate({focusMinutes:e.currentTarget.valueAsNumber})}/></label>
      {@render toggle('homeArtwork','Artwork on Home','Show the current cover image separately from its color.')}
    {:else if section === "jarvis"}
      <JarvisSettings hoverExpand={preferences.hoverExpand} onhoverexpand={hoverExpand=>onupdate({hoverExpand})}/>
    {:else}
      <h3 class="group-label">Player</h3>
      <label class="row"><span><strong>Player layout</strong><small>Wide player adds a timeline. Cover card puts artwork above the controls. Both use the song’s artwork colors.</small></span><select aria-label="Player layout" value={preferences.spotifyLayout}
        onchange={(e) => onupdate({ spotifyLayout: e.currentTarget.value as Preferences["spotifyLayout"] })}>
        <option value="centered">Centered</option><option value="artwork-left">Artwork left</option><option value="cover-card">Cover card</option><option value="wide">Wide player</option></select></label>
      <h3 class="group-label">Visible playback buttons</h3>
      {@render toggle('spotifySeekBar','Show song position bar','Drag or click the bar to jump to any moment in the song.')}
      {@render toggle('spotifyShowRepeat','Show Repeat button','Cycle through Off, Repeat all and Repeat one. Spotify confirms the active mode.')}
      {@render toggle('spotifyShowShuffle','Show Shuffle button','Turn Spotify shuffle on or off. You can enable any combination of these buttons.')}
      {@render toggle('spotifyMarquee','Scroll long titles','When a song title or artist is too long to fit, it glides from right to left. Off: it ends with “…”.')}

      {@render toggle("waves", "Show the compact waveform", "Seven real frequency bands from Spotify’s audio. No generated motion.")}
      {#if preferences.waves}
      <label class="row"><strong>Waveform style</strong><select aria-label="Waveform style" value={preferences.waveStyle} onchange={e=>onupdate({waveStyle:e.currentTarget.value as Preferences['waveStyle']})}><option value="classic">Classic contour</option><option value="spectrum">Equal-height spectrum</option><option value="fine">Fine bars</option></select></label>
      <label class="row"><span><strong>Wave reactivity <small class="inline">{preferences.waveReactivity}%</small></strong><small>How far the bars move. Zero keeps them still.</small></span><input aria-label="Wave reactivity" type="range" min="0" max="100" step="5" value={preferences.waveReactivity} oninput={e=>onupdate({waveReactivity:e.currentTarget.valueAsNumber})}/></label>
      <details class="reader-options"><summary>Wave response tuning</summary>
      <label class="row"><span><strong>Movement smoothness <small class="inline">{preferences.waveSmoothness}%</small></strong><small>Low is quick; high softens peaks and releases.</small></span><input aria-label="Wave smoothness" type="range" min="0" max="100" step="5" value={preferences.waveSmoothness} oninput={e=>onupdate({waveSmoothness:e.currentTarget.valueAsNumber})}/></label>
      <label class="row"><span><strong>Wave sensitivity <small class="inline">{preferences.waveSensitivity.toFixed(1)}×</small></strong><small>Adjust the input level of the real music frequencies.</small></span><input aria-label="Wave sensitivity" type="range" min="0.5" max="3" step="0.1" value={preferences.waveSensitivity} oninput={e=>onupdate({waveSensitivity:e.currentTarget.valueAsNumber})}/></label>
      <button type="button" class="pill-btn" onclick={()=>onupdate({waveStyle:'classic',waveReactivity:40,waveSmoothness:65,waveSensitivity:1.6})}>Reset waveform</button></details>
      {/if}
      <h3 class="group-label">Lyrics</h3>
      <SpotifyBridgeSettings enabled={preferences.spotifyBridge} onchange={spotifyBridge=>{onupdate({spotifyBridge});oncanvaschange?.();}}/>
      <details class="reader-options"><summary>Appearance & reading options <small>Layout, type, spacing, motion and timing</small></summary><LyricsSettings {preferences} {onupdate}/></details>
      <label class="row"><strong>Translation language</strong><select value={preferences.lyricLanguage} onchange={(e) => onupdate({lyricLanguage:e.currentTarget.value})}>
        <option value="auto">System language</option><option value="el">Greek</option><option value="en">English</option><option value="ro">Romanian</option><option value="es">Spanish</option><option value="fr">French</option><option value="de">German</option><option value="it">Italian</option><option value="pt">Portuguese</option><option value="ru">Russian</option><option value="ja">Japanese</option><option value="ko">Korean</option><option value="zh">Chinese</option><option value="ar">Arabic</option>
      </select></label>
      <h3 class="group-label">Video background</h3>
      <label class="row"><span><strong>Show behind the player</strong><small>The song image only, Apple Music's video, or Spotify's Canvas video. You can also switch on the island: on the Spotify tab, click the Spotify icon (sources this song doesn't have are skipped).</small></span><select aria-label="Show behind the player" value={preferences.spotifyBackground} onchange={e=>{const value=e.currentTarget.value as Preferences['spotifyBackground'];onupdate(value==='image'?{spotifyBackground:value}:{spotifyBackground:value,video:true});}}><option value="image">Image only</option><option value="artwork">Video from Apple Music</option><option value="canvas">Video from Spotify (Canvas)</option></select></label>
      {#if preferences.spotifyBackground!=='image'}
      <label class="row switch">
        <span><strong>Show song cover over the video</strong><small>When a Spotify Canvas or Apple Music video plays behind the player, keep the album cover visible on top. Turn off to show only the video. Songs without a video always show the cover.</small></span>
        <input type="checkbox" role="switch" checked={!preferences.videoHideArtwork} onchange={(e) => onupdate({ videoHideArtwork: !e.currentTarget.checked })} />
      </label>
      {@render toggle('videoPauseWithSong','Pause video with the song','When off, the silent background video keeps looping while the song is paused.')}
      <label class="row"><span><strong>Video quality</strong><small>Use the best matching stream up to this resolution. Canvas uses its original file when no quality variants exist.</small></span><select aria-label="Video quality" value={preferences.videoQuality} onchange={e=>onupdate({videoQuality:e.currentTarget.value as Preferences['videoQuality']})}><option value="best">Best available</option><option value="2160">2160p / 4K</option><option value="1080">1080p</option><option value="720">720p</option><option value="480">480p</option></select></label>
      {@render toggle('videoCanvasFallback','Try the other video source if unavailable',preferences.spotifyBackground==='canvas'?'Try Spotify Canvas first, then animated album artwork. If neither can play, show the song cover and color.':'Try animated album artwork first, then Spotify Canvas. If neither can play, show the song cover and color.')}
      {#if preferences.spotifyBackground==='artwork'}
        {#if preferences.videoCanvasFallback}<CanvasSettings enabled={preferences.video} onchange={video=>onupdate({video})} showToggle={false} connectionTitle="Canvas fallback connection" onconnectionchange={oncanvaschange} lookupError={canvasError}/>{/if}
      {/if}
      {#if preferences.spotifyBackground === 'canvas'}
        <CanvasSettings enabled={preferences.video} onchange={video=>onupdate({video})} connectionTitle="Canvas connection" onconnectionchange={oncanvaschange} lookupError={canvasError}/>
      {:else}
        {@render toggle('video', 'Animated artwork background', 'Look up the current artist and album in the public iTunes catalogue and play Apple Music’s motion album cover when one exists. Album artwork loops are separate from Spotify music videos. No Spotify connection is needed.')}
        <p class="hint">Each new song is looked up as it starts, so the video is ready when you open the player; it only plays while the player is visible. Apple receives the artist, album and song title. If no matching animation is available, the song artwork and color stay visible.</p>
        {#if artworkError}<p role="status">{artworkError}</p>{/if}
      {/if}
      {/if}
      <VocalSettings volume={preferences.vocalVolume} onchange={vocalVolume=>onupdate({vocalVolume})}/>
    {/if}
    {#if status}<p class="note" role="status">{status}</p>{/if}
  </div>
</section>

<style>
  .widget-order {list-style:none;padding:0;margin:8px 0;display:grid;gap:4px;}
  .widget-order li {display:flex;align-items:center;gap:4px;font:var(--type-body);}
  .widget-order li span {flex:1;}
  .widget-order button {min-width:28px;min-height:28px;padding:0 6px;}
 .group-label{font:600 11px/16px var(--font-sans);letter-spacing:.065em;text-transform:uppercase;opacity:.62;margin:20px 0 3px}.group-label:first-child{margin-top:6px}.reader-options,.connection-details{margin:10px 0;border-bottom:1px solid #ffffff15;padding:8px 0 12px}summary{cursor:pointer;font:560 13px/18px var(--font-sans)}summary small{display:block;margin-left:14px}.border-picker{padding:8px 0 12px;margin:0;border:0}.border-picker>div{display:grid;grid-template-columns:1fr 1fr;gap:6px}.border-option{display:flex;align-items:center;gap:9px;text-align:left;min-height:54px;padding:8px;border:1px solid #ffffff15;border-radius:12px;background:#ffffff04;color:inherit;font:500 12px/16px var(--font-sans);cursor:pointer}.border-option:hover,.border-option[aria-pressed=true]{background:#ffffff12;border-color:#ffffff48}.border-option small{font-size:10px;line-height:13px;opacity:.6;margin:2px 0 0}.border-option i{display:block;width:32px;height:22px;border-radius:12px;border:1px solid #ffffff60;flex:none;background:linear-gradient(145deg,#ffffff30,#ffffff04);box-shadow:inset 1px 2px 4px #ffffff38}.border-option.white-line i{border-color:#ffffffdd;box-shadow:none}.border-option.pearl i{border-color:#eae6df80;box-shadow:inset 0 2px 3px #ffffff30}.border-option.graphite i{border-color:#626773;box-shadow:inset 0 1px 1px #ffffff22}.border-option.none i{border-color:transparent;box-shadow:none}
 input[type="number"]{width:68px;flex:none;min-height:32px;padding:4px 8px;border:1px solid #ffffff24;border-radius:9px;background:var(--surface-control);color:inherit;color-scheme:dark;font:var(--type-body);}
  .settings { display: grid; grid-template-rows: auto auto minmax(0, 1fr); height: 100%; padding: 18px 28px 16px; gap: 10px; background: oklch(0.17 0.012 255 / calc(0.52 * var(--surface-opacity, 1))); }
  .widget-picker{border:0;padding:10px 0;margin:0;}legend{font:var(--type-label);font-weight:600;}.note-editor{display:block;margin:10px 0;}.note-editor textarea{display:block;box-sizing:border-box;width:100%;resize:vertical;max-height:110px;margin-top:8px;padding:10px;border:1px solid #ffffff26;border-radius:10px;background:#ffffff0b;color:inherit;font:var(--type-body);}
  header, nav, .row { display: flex; align-items: center; }
  header, .row { justify-content: space-between; gap: var(--space-2); }
  h2 { margin: 0; font: 600 19px/24px var(--font-sans); letter-spacing: var(--tracking-tight); }
  nav { display: flex; gap: 4px; padding: 2px 0 10px; border-bottom: 1px solid oklch(0.98 0 0 / 0.12); overflow-x: auto; scrollbar-width: none; overscroll-behavior: contain; mask-image: linear-gradient(to right, #000 calc(100% - 16px), transparent); }
  nav::-webkit-scrollbar { display: none; }
  .tab, .close { min-height: 28px; height: 28px; }
  .tab { flex: none; padding: 0 12px; border: 0; border-radius: 999px; color: inherit; background: transparent; font: 500 11.5px/16px var(--font-sans); cursor: pointer; white-space: nowrap; opacity: .72; transition: background-color 120ms ease, opacity 120ms ease; }
  .tab[aria-pressed="true"] { opacity: 1; }
  .intro { margin: 10px 0 2px; font: var(--type-caption); color: var(--text-secondary); line-height: 18px; }
  .text-input { width: 140px; flex: none; min-height: 32px; padding: 4px 10px; border: 1px solid #ffffff24; border-radius: 9px; background: var(--surface-control); color: inherit; font: var(--type-body); }
  .tab:hover, .tab[aria-pressed="true"] { background: var(--surface-control-hover); }
  .body { min-height: 0; overflow-y: auto; padding: 0 12px 4px 2px; scrollbar-width: thin; scrollbar-color: oklch(0.98 0 0 / 0.22) transparent; scrollbar-gutter: stable; color-scheme: dark; overscroll-behavior: contain; }
  .body::-webkit-scrollbar { width: 5px; }
  .body::-webkit-scrollbar-track { background: transparent; }
  .body::-webkit-scrollbar-thumb { border-radius: 8px; background: oklch(0.98 0 0 / 0.22); }
  .body::-webkit-scrollbar-thumb:hover { background: oklch(0.98 0 0 / 0.36); }
  .row { min-height: 48px; border-bottom: 1px solid oklch(0.98 0 0 / 0.07); padding: 9px 0; font: var(--type-body); }
  .row > span { min-width: 0; }
  strong { font-weight: 560; }
  small, .note { display: block; margin: 3px 0 0; color: var(--text-primary); opacity: 0.8; font: var(--type-caption); }
  .note { margin: 10px 0; }
  .inline { display: inline; }
  input, select { color-scheme: dark; accent-color: var(--accent); }
  input[type="range"] { width: 112px; }
  input[type="checkbox"] { width: 17px; height: 17px; flex: none; }
  .switch input { appearance: none; width: 34px; height: 20px; margin: 0; border: 0; border-radius: 20px; background: oklch(0.55 0.01 255); position: relative; cursor: pointer; transition: background-color 160ms ease; }
  .switch input::before { content: ""; position: absolute; width: 14px; height: 14px; top: 3px; left: 3px; border-radius: 50%; background: var(--text-primary); transition: transform 160ms ease; }
  .switch input:checked { background: var(--accent); }
  .switch input:checked::before { transform: translateX(14px); background: var(--on-accent); }
  .switch input:disabled { opacity: 0.4; cursor: wait; }
  .design-locked { border: 0; margin: 0; padding: 0; min-width: 0; }
  .design-locked:disabled > :not(.lock-note) { opacity: 0.38; pointer-events: none; }
  .lock-note { margin: 4px 0 8px; }
  .switch { position: relative; }
  .switch::after { content: ""; position: absolute; width: 34px; height: 28px; right: 0; pointer-events: none; }
  select { background: var(--surface-control); color: inherit; border: 1px solid var(--surface-control-hover); border-radius: 9px; min-height: 32px; padding: 3px 7px; max-width: 150px; flex-shrink: 0; }
  .file input { width: 115px; font: var(--type-caption); color: transparent; }
  .file input::file-selector-button { height: 30px; padding: 0 10px; border: 1px solid var(--surface-control-hover); border-radius: 9px; background: var(--surface-control); color: var(--text-primary); font: var(--type-caption); cursor: pointer; }
  .file span { overflow-wrap: anywhere; }
  .actions { display: flex; gap: var(--space-2); padding-top: var(--space-2); }
  .sound-presets { border: 0; padding: 10px 0 12px; margin: 0; border-bottom: 1px solid oklch(0.98 0 0 / 0.07); }
  legend { font: var(--type-label); padding: 0; }
  .preset-options { display: flex; gap: 6px; margin-top: 8px; }
  .preset-options button { flex: 1; padding: 0 8px; }
  .preset-options button[aria-pressed="true"] { background: var(--accent); color: var(--on-accent); }
  option { background: #24262d; color: #f5f5f5; }
  @media (prefers-reduced-motion: reduce) { .switch input, .switch input::before, .tab { transition: none; } }
</style>
