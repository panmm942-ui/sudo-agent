import {it,expect} from 'vitest';
import {DEFAULTS} from './preferences';
import {cloudSchedule,createCompanionSync} from './onlineCompanion';
it('exports seven days using actual schedule rules including midnight and disabled entries',()=>{
  const now=new Date(2026,10,4,12);
  const result=cloudSchedule({...DEFAULTS,program:[{id:'n',label:'Νύχτα',detail:'private',time:'00:15',days:[0,1,2,3,4,5,6],enabled:true}]},'2026-11-04',now);
  expect(result.alarms).toHaveLength(7);expect(result.coverageUntil).toBe(+now+7*86400000);
  expect(result.alarms.every(a=>a.at>+now&&a.at<=result.coverageUntil&&!('detail' in a))).toBe(true);
  expect(cloudSchedule({...DEFAULTS,alarms:false},null,now).alarms).toEqual([]);
});
it('first sync reads cloud changes before publishing and does not echo them back',async()=>{
  let local={note:'local',focus:{remaining:1500000,deadline:null as number|null},minutes:25};
  const remote={note:{text:'phone note',updatedAt:1000},focus:{remaining:300000,deadline:null,minutes:5,updatedAt:1000}};
  const bodies:any[]=[];
  const sync=createCompanionSync(async(path,_method,body)=>{bodies.push(body);return remote;},()=>({...local,schedule:{}}),state=>{local={note:state.note.text,focus:state.focus,minutes:state.focus.minutes};});
  await sync.refresh();await sync.refresh();
  expect(local.note).toBe('phone note');expect(bodies[0].note).toBeUndefined();expect(bodies[1].note).toBeUndefined();
  local.note='edited on desktop';await sync.refresh();expect(bodies[2].note.text).toBe('edited on desktop');
});
it('preserves a local edit made while the first cloud request is in flight',async()=>{
  let local={note:'before',focus:{remaining:60000,deadline:null as number|null},minutes:1};let resolve!:(v:any)=>void;
  const sync=createCompanionSync(()=>new Promise(done=>resolve=done),()=>({...local,schedule:{}}),state=>{local.note=state.note.text;});
  const pending=sync.refresh();local.note='draft changed';resolve({note:{text:'remote',updatedAt:10},focus:{remaining:60000,deadline:null,minutes:1,updatedAt:10}});await pending;
  expect(local.note).toBe('draft changed');
});
it('seeds a new service from existing Home values instead of clearing the personal note',async()=>{
  const bodies:any[]=[];let local={note:'Keep this note',focus:{remaining:1500000,deadline:null as number|null},minutes:25};
  const remote={note:{text:'',updatedAt:0},focus:{remaining:0,deadline:null,minutes:25,updatedAt:0}};
  const sync=createCompanionSync(async(_path,_method,body)=>{bodies.push(body);return bodies.length===1?remote:{note:body.note,focus:body.focus};},()=>({...local,schedule:{}}),state=>{local.note=state.note.text;});
  await sync.refresh();expect(local.note).toBe('Keep this note');expect(bodies[1].note.text).toBe('Keep this note');expect(bodies[1].focus.remaining).toBe(1500000);
});
