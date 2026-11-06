import {invoke} from '@tauri-apps/api/core';
import type {Preferences} from '../island/preferences';
import {STORAGE_KEY} from './scheduler';
import {iphoneAlarmSnapshot} from './iphoneAlarms';

type Transport=(command:string,args?:Record<string,unknown>)=>Promise<unknown>;
type Schedule=Pick<Preferences,'alarms'|'gradualSleep'|'reminders'|'program'>;

/** Serialize native state changes; a superseded request cannot publish or re-enable later. */
export function createIPhoneConnection(transport:Transport,readRoutine:()=>string|null) {
  let generation=0,enabled=false,preferences:Schedule|undefined,error='';
  let pending:Promise<boolean>=Promise.resolve(true);
  const schedule=()=>{
    const request=++generation;
    pending=pending.then(async()=>{
      if(request!==generation)return true;
      try{
        await transport('iphone_alarms_configure',{enabled});
        if(request!==generation)return true;
        if(enabled&&preferences){
          let saved:{start?:unknown}|null;
          try{const raw=readRoutine();saved=raw?JSON.parse(raw):null;}
          catch{throw new Error('The saved routine schedule could not be read. Open Program before syncing.');}
          if(saved!==null&&(typeof saved!=='object'||typeof saved.start!=='string'))throw new Error('The saved routine start is invalid. Open Program before syncing.');
          const snapshot=iphoneAlarmSnapshot(preferences,saved?.start as string|null??null);
          await transport('iphone_alarms_publish',{snapshot});
        }
        error='';return true;
      }catch(cause){error=cause instanceof Error?cause.message:String(cause);return false;}
    });
    return pending;
  };
  return {
    update(next:boolean,config:Schedule){enabled=next;preferences=config;return schedule();},
    refresh(){return enabled?schedule():Promise.resolve(true);},
    error:()=>error,
  };
}

export const iphoneConnection=createIPhoneConnection((command,args)=>invoke(command,args),()=>localStorage.getItem(STORAGE_KEY));
