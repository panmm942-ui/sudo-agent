import type {Preferences} from './preferences';
import type {FocusTimer} from './homeWidgets';
import {iphoneAlarmSnapshot} from '../routine/iphoneAlarms';
import {localDay} from '../routine/program';
export type CloudState={note:{text:string;updatedAt:number};focus:FocusTimer&{minutes:number;updatedAt:number};inbox:{id:string;text:string;createdAt:number}[];battery:{percent:number;charging:boolean;updatedAt:number}|null;lastDesktopAt:number|null;coverageUntil:number|null;sourceUpdatedAt:number|null;phonePaired:boolean;lastImportedAt:number|null;importedCount:number|null};
export function cloudSchedule(preferences:Pick<Preferences,'alarms'|'gradualSleep'|'program'|'reminders'>,start:string|null,now=new Date()){
  start??=localDay(now);
  const snapshot=iphoneAlarmSnapshot(preferences,start,now);
  const second=iphoneAlarmSnapshot(preferences,start,new Date(+now+2*86400000));
  const third=iphoneAlarmSnapshot(preferences,start,new Date(+now+4*86400000));
  const fourth=iphoneAlarmSnapshot(preferences,start,new Date(+now+6*86400000));
  const coverageUntil=+now+7*86400000;
  const entries=[...snapshot.alarms,...second.alarms,...third.alarms,...fourth.alarms].filter(a=>a.at<=coverageUntil);
  const alarms=[...new Map(entries.map(a=>[a.id,a])).values()].sort((a,b)=>a.at-b.at);
  return {generatedAt:+now,coverageUntil,timeZone:snapshot.timeZone,alarms};
}
type Local={note:string;focus:FocusTimer;minutes:number;schedule:unknown};
type Request=(path:string,method:string,body:Record<string,unknown>)=>Promise<any>;
/** Publish only actual edits, and do not overwrite edits made while a request was running. */
export function createCompanionSync(request:Request,read:()=>Local,apply:(state:CloudState)=>void){
  let baseline:{note:string;focus:string}|null=null,running=false,stopped=false;
  let pending:Promise<CloudState|null>=Promise.resolve(null);
  const focusKey=(value:Local)=>JSON.stringify([value.focus.remaining,value.focus.deadline,value.minutes]);
  return {
    refresh(){
      if(running||stopped)return pending;
      running=true;
      pending=(async()=>{
        try{
          const before=read(),beforeFocus=focusKey(before),at=Date.now();
          const body:Record<string,unknown>={schedule:before.schedule};
          if(baseline&&before.note!==baseline.note)body.note={text:before.note,updatedAt:at};
          if(baseline&&beforeFocus!==baseline.focus)body.focus={...before.focus,minutes:before.minutes,updatedAt:at};
          let remote=await request('/v1/desktop/sync','POST',body) as CloudState;
          if(!baseline&&(remote.note.updatedAt===0||remote.focus.updatedAt===0)){
            const seed:Record<string,unknown>={schedule:before.schedule};
            if(remote.note.updatedAt===0)seed.note={text:before.note,updatedAt:at};
            if(remote.focus.updatedAt===0)seed.focus={...before.focus,minutes:before.minutes,updatedAt:at};
            remote=await request('/v1/desktop/sync','POST',seed) as CloudState;
          }
          if(stopped)return null;
          const after=read(),afterFocus=focusKey(after);
          // Keep in-flight local edits pending for the next sync.
          const state={...remote,note:after.note===before.note?remote.note:{text:after.note,updatedAt:at},focus:afterFocus===beforeFocus?remote.focus:{...after.focus,minutes:after.minutes,updatedAt:at}};
          apply(state);
          baseline={note:after.note===before.note?state.note.text:before.note,focus:afterFocus===beforeFocus?JSON.stringify([state.focus.remaining,state.focus.deadline,state.focus.minutes]):beforeFocus};
          return remote;
        }finally{running=false;}
      })();
      return pending;
    },
    stop(){stopped=true;},
  };
}
