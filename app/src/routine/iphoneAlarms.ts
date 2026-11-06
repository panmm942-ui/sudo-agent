import type {Preferences} from '../island/preferences';
import {localDay,occurrencesFor} from './program';

export type IPhoneAlarm={id:string;label:string;at:number;time:string;utcOffset:string};
export type IPhoneSnapshot={generatedAt:number;validUntil:number;timeZone:string;utcOffset:string;alarms:IPhoneAlarm[]};
export type IPhoneStatus={enabled:boolean;setupUrls:string[];feedUrl:string|null;pairingKey:string|null;ownerPrefix:string|null;lastFetchedAt:number|null;lastImportedAt:number|null;importedCount:number|null};
type AlarmPreferences=Pick<Preferences,'alarms'|'program'|'reminders'|'gradualSleep'>;

function offset(date:Date):string {
  const minutes=-date.getTimezoneOffset(),absolute=Math.abs(minutes);
  return `${minutes<0?'-':'+'}${String(Math.floor(absolute/60)).padStart(2,'0')}:${String(absolute%60).padStart(2,'0')}`;
}

/** Reuse the actual scheduler, without advancing or acknowledging local alarms. */
export function iphoneAlarmSnapshot(preferences:AlarmPreferences,startDay:string|null,now=new Date()):IPhoneSnapshot {
  const day=startDay??localDay(now);
  const start=new Date(`${day}T00:00:00`);
  if(!/^\d{4}-\d{2}-\d{2}$/.test(day)||!Number.isFinite(+start)||localDay(start)!==day)throw new Error('The routine start date is invalid. Open Program before connecting iPhone alarms.');
  const at=+now;
  const alarms=preferences.alarms?[-1,0,1,2,3].flatMap(delta=>occurrencesFor(new Date(now.getFullYear(),now.getMonth(),now.getDate()+delta),start,preferences))
    .filter(alarm=>alarm.at>at&&alarm.at<=at+48*3600_000)
    .sort((a,b)=>a.at-b.at)
    .map(({id,label,at,time})=>({id,label:label.replace(/\s+/gu,' ').trim().slice(0,120),at,time,utcOffset:offset(new Date(at))})):[];
  return {generatedAt:at,validUntil:at+90_000,timeZone:Intl.DateTimeFormat().resolvedOptions().timeZone,utcOffset:offset(now),alarms};
}
