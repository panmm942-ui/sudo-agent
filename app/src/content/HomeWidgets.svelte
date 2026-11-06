<script lang="ts">
  import {onMount} from 'svelte';
  import type {Preferences} from '../island/preferences';
  import {describe, loadWeather, type Weather} from '../island/weather';
  import SudoLogo from './SudoLogo.svelte';

  /**
   * Home: the Sudo capsule sits where the album art sits on Spotify (it morphs into the cover when
   * scrolling there), the time is the hero, and chosen widgets sit below as calm glass cards.
   */
  let {preferences,items=[],remaining=1500000,running=false,onfocus,extensionRail=false,spot={x:174,y:14,w:92,h:92,r:12},width=440}: {
    extensionRail?:boolean;
    /** Where the album art sits on the Spotify tab (same size: 92 px); the logo sits exactly there. */
    spot?:{x:number;y:number;w:number;h:number;r:number};width?:number;
    preferences:Preferences;items?:{label:string;time:string}[];remaining?:number;running?:boolean;onfocus?:(action:'toggle'|'reset')=>void;
  }=$props();

  let now=$state(new Date());
  let weather=$state<Weather|null>(null);
  let battery=$state<{level:number;charging:boolean}|null>(null);

  onMount(()=>{
    let timer:ReturnType<typeof setTimeout>;
    const tick=()=>{now=new Date();timer=setTimeout(tick,60_000-now.getTime()%60_000+50);};
    tick();
    return()=>clearTimeout(timer);
  });
  // Weather: only when the widget is on and a city is set; cached 30 min inside loadWeather.
  $effect(()=>{
    const city=preferences.homeCity;
    if(!preferences.homeWidgets.includes('weather')||!city){weather=null;return;}
    let alive=true;
    const refresh=()=>void loadWeather(city).then(value=>{if(alive)weather=value;}).catch(()=>{});
    refresh();
    const timer=setInterval(refresh,30*60_000);
    return()=>{alive=false;clearInterval(timer);};
  });
  $effect(()=>{
    if(!preferences.homeWidgets.includes('battery'))return;
    const nav=navigator as Navigator&{getBattery?:()=>Promise<{level:number;charging:boolean;addEventListener:(t:string,f:()=>void)=>void}>};
    let alive=true;
    void nav.getBattery?.().then(b=>{
      const read=()=>{if(alive)battery={level:b.level,charging:b.charging};};
      read();b.addEventListener('levelchange',read);b.addEventListener('chargingchange',read);
    }).catch(()=>{});
    return()=>{alive=false;};
  });

  const time=new Intl.DateTimeFormat(undefined,{hour:'2-digit',minute:'2-digit'});
  const date=new Intl.DateTimeFormat(undefined,{weekday:'long',month:'long',day:'numeric'});
  const greeting=$derived(now.getHours()<5?'Good night':now.getHours()<12?'Good morning':now.getHours()<18?'Good afternoon':'Good evening');
  const seconds=$derived(Math.ceil(remaining/1000));
  const cards=$derived(preferences.homeWidgets.filter(id=>id!=='clock'&&id!=='greeting').slice(0,3));
  const showClock=$derived(preferences.homeWidgets.includes('clock'));
  const showGreeting=$derived(preferences.homeWidgets.includes('greeting'));
  // Centered Spotify layout: art at the top centre, so Home stacks below the logo.
  const stacked=$derived(Math.abs(spot.x+spot.w/2-width/2)<30);
</script>

<div class="home" class:stacked class:extension-rail={extensionRail} aria-label="Home" style:--x="{spot.x}px" style:--y="{spot.y}px" style:--s="{spot.w}px">
  <div class="logo" data-morph="logo"><SudoLogo size={spot.w} /></div>
  <div class="main">
    <header class="hero">
      {#if showGreeting}<p class="greeting">{greeting}</p>{/if}
      {#if showClock}<p class="clock tabular">{time.format(now)}</p><p class="date">{date.format(now)}</p>
      {:else if !showGreeting}<p class="clock">Sudo</p>{/if}
    </header>
    {#if cards.length}
      <div class="cards" style:--count={cards.length}>
        {#each cards as widget (widget)}
          <section class="card" aria-label={widget}>
            {#if widget==='reminders'}
              <p class="label">Up next</p>
              {#if items[0]}<p class="value"><span class="tabular">{items[0].time}</span> {items[0].label}</p>{:else}<p class="value muted">Nothing scheduled</p>{/if}
            {:else if widget==='focus'}
              <p class="label">Focus</p>
              <div class="focus"><p class="value tabular">{Math.floor(seconds/60).toString().padStart(2,'0')}:{(seconds%60).toString().padStart(2,'0')}</p>
                <button type="button" aria-label={running?'Pause focus timer':'Start focus timer'} onclick={()=>onfocus?.('toggle')}>{running?'❚❚':'▶'}</button>
                <button type="button" class="ghost" aria-label="Reset focus timer" onclick={()=>onfocus?.('reset')}>↺</button></div>
            {:else if widget==='note'}
              <p class="label">Note</p><p class="value note">{preferences.homeNote||'Add a note in Settings → Home.'}</p>
            {:else if widget==='weather'}
              <p class="label">{preferences.homeCity||'Weather'}</p>
              {#if weather}{@const w=describe(weather.code,weather.isDay)}<p class="value"><span aria-hidden="true">{w.glyph}</span> {weather.temperature}° <span class="muted">{w.label}</span></p>
              {:else}<p class="value muted">{preferences.homeCity?'…':'Set a city in Settings → Home'}</p>{/if}
            {:else if widget==='battery'}
              <p class="label">Battery</p>
              {#if battery}<p class="value">{Math.round(battery.level*100)}%{#if battery.charging} <span class="muted">· charging</span>{/if}</p>{:else}<p class="value muted">Unavailable</p>{/if}
            {/if}
          </section>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .home{position:relative;z-index:2;pointer-events:none;height:100%;}
  /* Exactly the album art's box on the Spotify tab, without any tile around the capsule. */
  .logo{position:absolute;left:var(--x);top:var(--y);width:var(--s);height:var(--s);display:grid;place-items:center;}
  .main{position:absolute;left:calc(var(--x) + var(--s) + 20px);right:66px;top:var(--y);min-height:var(--s);display:flex;flex-direction:column;justify-content:center;gap:10px;min-width:0;}
  .stacked .main{left:66px;right:66px;top:calc(var(--y) + var(--s) + 6px);bottom:12px;min-height:0;justify-content:space-between;gap:6px;}
  .stacked .hero{text-align:center;justify-items:center;}
  .stacked .clock{font-size:24px;line-height:28px;}
  .home.extension-rail:not(.stacked) .main{right:66px;}
  p{margin:0;}
  .hero{display:grid;gap:1px;min-width:0;}
  .greeting{font:var(--type-caption);color:var(--text-secondary);}
  .clock{font:600 34px/38px var(--font-sans);letter-spacing:-.03em;}
  .date{font:var(--type-caption);color:var(--text-secondary);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}
  .cards{display:grid;grid-template-columns:repeat(var(--count),minmax(0,1fr));gap:8px;}
  .card{min-width:0;padding:7px 10px;border-radius:12px;background:#ffffff0c;box-shadow:inset 0 0 0 1px #ffffff12;display:grid;gap:1px;align-content:start;}
  .label{font:600 10px/14px var(--font-sans);letter-spacing:.06em;text-transform:uppercase;color:var(--text-secondary);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}
  .value{font:var(--type-label);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}
  .muted{color:var(--text-secondary);}
  .note{white-space:nowrap;}
  .focus{display:flex;align-items:center;gap:4px;min-width:0;}
  .focus .value{flex:1;}
  .focus button{pointer-events:auto;position:relative;flex:none;width:22px;height:22px;display:grid;place-items:center;padding:0;border:0;border-radius:50%;background:var(--surface-control-hover);color:inherit;font:10px/1 var(--font-sans);cursor:pointer;}
  .focus .ghost{background:transparent;font-size:14px;}
</style>
