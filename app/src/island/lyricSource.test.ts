import {afterEach,describe,expect,it,vi} from 'vitest';
import {createLyricsSource,localLyricsResult} from './lyricSource';
import type {LyricsResult} from './lyrics';
const track={key:'native-current',title:'KHFISOS',artist:'FLY LO',album:'KHFISOS',duration:197};
const snapshot=(overrides:Record<string,unknown>={})=>({uri:'spotify:track:3OHfY25tqY28d16oZczHc8',trackId:'3OHfY25tqY28d16oZczHc8',title:'KHFISOS',artist:'FLY LO',album:'KHFISOS',duration:197,positionUpdatedAt:Date.now(),lyrics:[{time:8.25,text:'Δοκιμή'},{time:12.5,text:'Original sample'}],...overrides});
const plain:LyricsResult={kind:'lyrics',text:'Original fallback sample'};
const settle=()=>vi.advanceTimersByTimeAsync(0);
afterEach(()=>vi.useRealTimers());
describe('current Spotify lyrics with LRCLIB fallback',()=>{
 it('accepts native primary artist in full client credits and preserves validated word timing',()=>{
  const wordLine={time:8,text:'One moment',words:[{time:8,end:9,text:'One '},{time:9,end:10,text:'moment'}]};
  const data=snapshot({artist:'FLY LO, Guest',lyrics:[wordLine]});
  expect(localLyricsResult(data,track)).toMatchObject({lines:[{text:'One moment',words:[{time:8,end:9,text:'One '},{time:9,end:10,text:'moment'}]}]});
  wordLine.words[0].text='Wrong';
  expect(localLyricsResult(data,track)).toMatchObject({lines:[{time:8,text:'One moment'}]});
  expect((localLyricsResult(data,track) as {lines:any[]}).lines[0].words).toBeUndefined();
 });
 it('upgrades a plain fallback when local timed lyrics arrive later and does not refetch the provider',async()=>{
  vi.useFakeTimers();let reads=0;const provider=vi.fn(async()=>plain);const local=vi.fn(async()=>++reads<3?null:snapshot());const updates:{result:LyricsResult|null;error?:string}[]=[];
  const source=createLyricsSource((result,error)=>updates.push({result,error}),{provider,local});source.request(track);await settle();expect(updates.at(-1)?.result).toEqual(plain);
  await vi.advanceTimersByTimeAsync(2000);expect(updates.at(-1)?.result).toMatchObject({kind:'lyrics',lines:[{time:8.25,text:'Δοκιμή'},{time:12.5,text:'Original sample'}]});
  await vi.advanceTimersByTimeAsync(5000);expect(provider).toHaveBeenCalledTimes(1);expect(local).toHaveBeenCalledTimes(3);expect(updates.filter(value=>value.result===null)).toHaveLength(1);source.stop();
 });
 it('never lets a slower provider response replace local Spotify timing',async()=>{
  vi.useFakeTimers();let finish:(value:LyricsResult)=>void=()=>{};const provider=vi.fn(()=>new Promise<LyricsResult>(resolve=>finish=resolve));const updates:(LyricsResult|null)[]=[];
  const source=createLyricsSource(value=>updates.push(value),{provider,local:async()=>snapshot()});source.request(track);await settle();finish(plain);await settle();expect(updates.at(-1)).toMatchObject({kind:'lyrics',lines:[{time:8.25,text:'Δοκιμή'},{time:12.5,text:'Original sample'}]});source.stop();
 });
 it('ignores old local results after a song change and stops pending polling on disposal',async()=>{
  vi.useFakeTimers();let finish:(value:unknown)=>void=()=>{};const local=vi.fn().mockImplementationOnce(()=>new Promise(resolve=>finish=resolve)).mockResolvedValue(null);const updates:(LyricsResult|null)[]=[];
  const source=createLyricsSource(value=>updates.push(value),{provider:async()=>plain,local});source.request(track);source.request({...track,key:'next-native',title:'Next song'});await settle();finish(snapshot());await settle();expect(updates.at(-1)).toEqual(plain);
  source.stop();const calls=local.mock.calls.length,count=updates.length;await vi.advanceTimersByTimeAsync(5000);expect(local).toHaveBeenCalledTimes(calls);expect(updates).toHaveLength(count);
 });
 it('does not overlap local snapshot reads when one is slow',async()=>{
  vi.useFakeTimers();let finish:(value:unknown)=>void=()=>{};const local=vi.fn(()=>new Promise(resolve=>finish=resolve));
  const source=createLyricsSource(()=>{},{provider:async()=>plain,local});source.request(track);await vi.advanceTimersByTimeAsync(5000);expect(local).toHaveBeenCalledTimes(1);finish(null);await settle();await vi.advanceTimersByTimeAsync(999);expect(local).toHaveBeenCalledTimes(1);await vi.advanceTimersByTimeAsync(1);expect(local).toHaveBeenCalledTimes(2);source.stop();finish(null);await settle();
 });
 it('keeps provider lyrics while local snapshots are missing, malformed, stale or from another recording',()=>{
  expect(localLyricsResult(snapshot(),track,Date.now())).toMatchObject({kind:'lyrics',lines:[{time:8.25,text:'Δοκιμή'},{time:12.5,text:'Original sample'}]});
  for(const invalid of [null,snapshot({title:'Other'}),snapshot({artist:'Other'}),snapshot({album:'Compilation'}),snapshot({duration:210}),snapshot({positionUpdatedAt:Date.now()-6000}),snapshot({lyrics:[]}),snapshot({lyrics:[{time:12,text:'A'},{time:8,text:'B'}]}),snapshot({lyrics:[{time:200,text:'A'}]}),snapshot({lyrics:[{time:NaN,text:'A'}]}),snapshot({lyrics:Array.from({length:1001},()=>({time:1,text:'A'}))})])expect(localLyricsResult(invalid,track,Date.now())).toBeNull();
 });
 it('silently falls back after a local bridge failure',async()=>{
  vi.useFakeTimers();const values:{result:LyricsResult|null;error?:string}[]=[];const source=createLyricsSource((result,error)=>values.push({result,error}),{provider:async()=>plain,local:async()=>{throw Error('Unavailable local bridge');}});
  source.request(track);await settle();expect(values.at(-1)).toEqual({result:plain,error:undefined});source.stop();
 });
});

import {extraLyricsResult,createLyricsSource as createSource} from './lyricSource';
describe('second lyrics source',()=>{
  it('accepts only real timed lyrics',()=>{
    expect(extraLyricsResult(null)).toBeNull();
    expect(extraLyricsResult('[00:01.00]one')).toBeNull();
    const result=extraLyricsResult('[00:10.00]Πάνω μου βράχοι\n[00:14.00]Με το scooter\n[00:18.00]Χρωστάς πολλά');
    expect(result).toMatchObject({kind:'lyrics'});
    expect(result&&result.kind==='lyrics'&&result.lines?.length).toBe(3);
  });
  it('upgrades plain or missing lyrics to timed ones from the second source',async()=>{
    const seen:unknown[]=[];
    const track={key:'k',title:'KHFISOS',artist:'Fly Lo',album:'KHFISOS',duration:197};
    const source=createSource((value)=>seen.push(value),{
      provider:async()=>({kind:'lyrics',text:'plain words'}),
      local:async()=>null,
      extra:async()=>'[00:10.00]one\n[00:12.00]two\n[00:14.00]three',
    });
    source.request(track);
    await new Promise(resolve=>setTimeout(resolve,20));
    source.stop();
    const last=seen.at(-1) as {kind:string;lines?:unknown[]};
    expect(last.kind).toBe('lyrics');expect(last.lines).toHaveLength(3);
  });
  it('keeps timed main-source lyrics without asking the second source',async()=>{
    let asked=false;const seen:unknown[]=[];
    const source=createSource((value)=>seen.push(value),{provider:async()=>({kind:'lyrics',text:'a',lines:[{time:1,text:'a'}]}),local:async()=>null,extra:async()=>{asked=true;return null;}});
    source.request({key:'k',title:'T',artist:'A',album:'B',duration:100});
    await new Promise(resolve=>setTimeout(resolve,20));source.stop();
    expect(asked).toBe(false);
  });
});

describe('imported lyrics',()=>{
  it('a file the user imported wins over every source, timed or plain',async()=>{
    const {importedLyricsResult,createLyricsSource:create}=await import('./lyricSource');
    expect(importedLyricsResult('[00:01.00]a\n[00:02.00]b\n[00:03.00]c')?.kind).toBe('lyrics');
    expect(importedLyricsResult('[ti:Song]\nfirst line\nsecond line')).toEqual({kind:'lyrics',text:'Song\nfirst line\nsecond line'.replace('Song\n','')});
    expect(importedLyricsResult('   ')).toBeNull();
    const seen:unknown[]=[];
    let asked=false;
    const source=create(value=>seen.push(value),{provider:async()=>{asked=true;return {kind:'missing'};},local:async()=>null,extra:async()=>null,imported:()=> '[00:01.00]mine\n[00:02.00]all\n[00:03.00]mine'});
    source.request({key:'k',title:'T',artist:'A',album:'',duration:100});
    await new Promise(r=>setTimeout(r,10));
    source.stop();
    expect(asked).toBe(false);
    expect((seen.at(-1) as {text:string}).text).toBe('mine\nall\nmine');
  });
});
