import {describe,it,expect} from 'vitest';
import {DEFAULTS} from '../island/preferences';
import {iphoneAlarmSnapshot} from './iphoneAlarms';

const now=new Date(2026,10,4,10,0,0);
const start='2026-11-04';
const entry={id:'wake',label:'  Ξύπνημα  ',detail:'private detail',time:'11:15',days:[3,4,5],enabled:true};
describe('iPhone alarm schedule',()=>{
  it('exports actual upcoming weekday occurrences with stable identifiers and no private details',()=>{
    const result=iphoneAlarmSnapshot({...DEFAULTS,program:[entry]},start,now);
    expect(result.generatedAt).toBe(+now);
    expect(result.validUntil).toBe(+now+90_000);
    expect(result.alarms.map(a=>[a.label,a.time])).toEqual([['Ξύπνημα','11:15'],['Ξύπνημα','11:15']]);
    expect(result.alarms[0].id).toBe('routine:2026-11-04:custom-wake');
    expect(result.alarms[0]).not.toHaveProperty('detail');
    expect(result.utcOffset).toMatch(/^[+-]\d\d:\d\d$/);
  });
  it('never exports elapsed alarms, wrong days, or disabled entries',()=>{
    const result=iphoneAlarmSnapshot({...DEFAULTS,program:[entry,{...entry,id:'past',time:'09:30',days:[3]}, {...entry,id:'off',enabled:false},{...entry,id:'wrong-day',days:[0]}]},start,now);
    expect(result.alarms).toHaveLength(2);
    expect(iphoneAlarmSnapshot({...DEFAULTS,alarms:false,program:[entry]},start,now).alarms).toEqual([]);
  });
  it('places a just-after-midnight reminder on the next calendar day',()=>{
    const late={...entry,id:'late',label:'Lights out',time:'00:30',days:[4]};
    const result=iphoneAlarmSnapshot({...DEFAULTS,program:[late]},start,new Date(2026,10,4,23,0));
    const alarm=result.alarms.find(a=>a.id==='routine:2026-11-05:custom-late');
    expect(alarm?.time).toBe('00:30');
    expect(new Date(alarm!.at).getDate()).toBe(5);
  });
  it('fails closed on a corrupt routine start instead of clearing the phone schedule',()=>{
    expect(()=>iphoneAlarmSnapshot(DEFAULTS,'not-a-date',now)).toThrow(/start/i);
    expect(()=>iphoneAlarmSnapshot(DEFAULTS,'2026-02-31',now)).toThrow(/start/i);
  });
  it('uses today for a first launch and has a bounded sorted 48-hour horizon',()=>{
    const result=iphoneAlarmSnapshot({...DEFAULTS,program:[entry]},null,now);
    expect(result.alarms.every(a=>a.at>+now&&a.at<=+now+48*3600_000)).toBe(true);
    expect(result.alarms.map(a=>a.at)).toEqual([...result.alarms.map(a=>a.at)].sort());
  });
});
