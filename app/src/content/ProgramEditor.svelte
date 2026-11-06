<script lang="ts">
  import {tick} from 'svelte';
  import {editableProgram} from '../routine/program';
  import {isTime,loadPreferences,type Preferences,type ProgramEntry} from '../island/preferences';
  let {preferences,onupdate}:{preferences:Preferences;onupdate:(patch:Partial<Preferences>)=>void}=$props();
  const weekdays=[{id:1,name:'Mon'},{id:2,name:'Tue'},{id:3,name:'Wed'},{id:4,name:'Thu'},{id:5,name:'Fri'},{id:6,name:'Sat'},{id:0,name:'Sun'}];
  let day=$state(new Date().getDay());
  let draft=$state<ProgramEntry|null>(null);
  let previous=$state<ProgramEntry[]|null>(null);
  let status=$state('');
  let root:HTMLDivElement;
  $effect(()=>{
    void draft?.id;
    void tick().then(()=>root?.closest('.body')?.scrollTo({top:0,behavior:'instant'}));
  });
  const entries=$derived(editableProgram(preferences));
  const shown=$derived(entries.filter(e=>e.days.includes(day)).toSorted((a,b)=>a.time.localeCompare(b.time)));
  function commit(program:ProgramEntry[],message='Saved'){
    previous=entries;onupdate({program});status=message;
  }
  function save(){
    if(!draft||!draft.label.trim()||!isTime(draft.time)||!draft.days.length)return;
    const value={...draft,label:draft.label.trim(),detail:draft.detail.trim(),days:[...draft.days]};
    commit(entries.some(e=>e.id===value.id)?entries.map(e=>e.id===value.id?value:e):[...entries,value]);draft=null;
  }
  function edit(entry:ProgramEntry){draft={...entry,days:[...entry.days]};status='';}
  function exportProgram(){
    const url=URL.createObjectURL(new Blob([JSON.stringify({format:'sudo-program',version:1,program:entries},null,2)],{type:'application/json'}));
    const link=document.createElement('a');link.href=url;link.download='sudo-program.json';link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
    status='Program exported';previous=null;
  }
  async function importProgram(event:Event&{currentTarget:HTMLInputElement}){
    const file=event.currentTarget.files?.[0];event.currentTarget.value='';if(!file)return;
    try{
      if(file.size>512_000)throw new Error('too large');
      const parsed=JSON.parse(await file.text());const list=Array.isArray(parsed)?parsed:parsed?.program;
      if(!Array.isArray(list))throw new Error('no program');
      // Same validation as saved settings: bad rows are dropped, never trusted.
      const program=loadPreferences({getItem:()=>JSON.stringify({program:list})}).program??[];
      if(!program.length&&list.length)throw new Error('no valid reminders');
      commit(program,`Imported ${program.length} reminder${program.length===1?'':'s'}`);
    }catch{status='That file is not a Sudo program.';previous=null;}
  }
  function add(){draft={id:crypto.randomUUID(),label:'',detail:'',time:'09:00',days:[day],enabled:true};status='';}
</script>
<div class="program-editor" bind:this={root}>
  {#if draft}
    <form onsubmit={(event)=>{event.preventDefault();save();}} aria-label="Edit program reminder">
      <div class="heading"><strong>{entries.some(e=>e.id===draft?.id)?'Edit reminder':'New reminder'}</strong><button class="pill-btn" type="button" onclick={()=>draft=null}>Cancel</button></div>
      <label>Name<input aria-label="Reminder name" required maxlength="100" bind:value={draft.label} placeholder="What do you want to remember?" /></label>
      <label>Time<input aria-label="Reminder time" type="time" required bind:value={draft.time} /></label>
      <fieldset><legend>Repeat on</legend><div class="days">{#each weekdays as weekday (weekday.id)}<button type="button" aria-pressed={draft.days.includes(weekday.id)} onclick={()=>{if(draft)draft.days=draft.days.includes(weekday.id)?draft.days.filter(d=>d!==weekday.id):[...draft.days,weekday.id];}}>{weekday.name}</button>{/each}</div></fieldset>
      <label>Details<textarea aria-label="Reminder details" rows="3" maxlength="1000" bind:value={draft.detail} placeholder="Optional notes"></textarea></label>
      <label class="enabled"><input type="checkbox" bind:checked={draft.enabled} />Enabled</label>
      {#if !draft.days.length}<p class="muted" role="status">Choose at least one repeat day.</p>{/if}
      <button type="submit" class="pill-btn save" disabled={!draft.label.trim()||!draft.days.length||!isTime(draft.time)}>Save reminder</button>
    </form>
  {:else}
    <div class="heading"><strong>My program</strong><button type="button" class="pill-btn" onclick={add} disabled={entries.length>=250}>+ New reminder</button></div>
    <p class="muted">Your personal weekly schedule. Add reminders, or import a program you exported before.</p>
    <nav class="days" aria-label="Program day">{#each weekdays as weekday (weekday.id)}<button type="button" aria-pressed={day===weekday.id} onclick={()=>day=weekday.id}>{weekday.name}</button>{/each}</nav>
    <div class="entries">
      {#each shown as entry (entry.id)}<div class="entry">
        <label class="enable"><input type="checkbox" aria-label={`Enable ${entry.label}`} checked={entry.enabled} onchange={(event)=>commit(entries.map(e=>e.id===entry.id?{...e,enabled:event.currentTarget.checked}:e))} /></label>
        <button type="button" class="edit" aria-label={`Edit ${entry.label}`} onclick={()=>edit(entry)}><span class="time tabular">{entry.time}</span><span class="label">{entry.label}<small>{entry.detail}</small></span></button>
        <button type="button" class="delete" aria-label={`Delete ${entry.label}`} onclick={()=>commit(entries.filter(e=>e.id!==entry.id),'Reminder deleted')}>×</button>
      </div>{:else}<p class="empty">No reminders for this day. Add one to start.</p>{/each}
    </div>
    {#if status}<div class="status" role="status">{status}{#if previous}<button type="button" onclick={()=>{if(previous){onupdate({program:previous});previous=null;status='Change undone';}}}>Undo</button>{/if}</div>{/if}
    <div class="file-actions">
      <label class="text-button">Import program<input type="file" accept="application/json,.json" onchange={importProgram} hidden /></label>
      {#if entries.length}<button type="button" class="text-button" onclick={exportProgram}>Export program</button>
      <button type="button" class="text-button" onclick={()=>commit([],'Program cleared')}>Clear program</button>{/if}
    </div>
  {/if}
</div>
<style>
  .file-actions{display:flex;flex-wrap:wrap;gap:4px 16px;}.file-actions label{cursor:pointer;}
  .program-editor{padding:6px 0;}button,input,textarea{font:inherit;color:inherit;}.heading{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:9px;}.heading strong{font:600 16px/22px var(--font-sans);}.muted{color:var(--text-secondary);font:var(--type-caption);margin:7px 0 12px;}.days{display:flex;gap:4px;margin:12px 0;}.days button{flex:1;border:0;border-radius:8px;padding:7px 2px;background:transparent;font:var(--type-caption);cursor:pointer;}.days button[aria-pressed=true],.days button:hover{background:var(--surface-control-hover);}.entry{display:flex;align-items:center;gap:8px;min-height:56px;border-bottom:1px solid #ffffff12;}.edit{flex:1;min-width:0;border:0;background:transparent;display:flex;align-items:center;gap:12px;text-align:left;padding:10px 0;cursor:pointer;}.time{font:500 14px/20px var(--font-sans);flex:none;}.label{font:var(--type-label);min-width:0;overflow-wrap:anywhere;}.label small{display:block;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:235px;color:var(--text-secondary);font:var(--type-caption);margin-top:3px;}.delete{border:0;background:transparent;font-size:23px;opacity:.7;padding:7px;cursor:pointer;}.delete:hover{opacity:1;color:#ffaaa5;}.enable input,.enabled input{accent-color:#e1e5ef;width:16px;height:16px;}.status{display:flex;justify-content:space-between;align-items:center;font:var(--type-caption);padding:10px 0;}.status button,.text-button{border:0;background:transparent;color:inherit;text-decoration:underline;text-underline-offset:3px;cursor:pointer;}.text-button{font:var(--type-caption);opacity:.75;margin:16px 0 4px;padding:0;}.empty{font:var(--type-body);padding:18px 0;}form>label{display:block;font:var(--type-label);margin:12px 0;}form input:not([type=checkbox]),textarea{display:block;box-sizing:border-box;width:100%;margin-top:6px;padding:9px 10px;border:1px solid #ffffff2a;border-radius:9px;background:#ffffff0b;color:inherit;color-scheme:dark;outline-offset:3px;}textarea{resize:vertical;max-height:150px;}fieldset{padding:0;border:0;margin:14px 0;}legend{font:var(--type-label);}.enabled{display:flex;align-items:center;gap:8px;}.save{margin:5px 0 8px;}
</style>
