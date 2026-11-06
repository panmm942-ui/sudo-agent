import {it,expect} from 'vitest';
import {createIPhoneConnection} from './iphoneConnection';
import {DEFAULTS} from '../island/preferences';

it('serializes enable/disable so a slow enable cannot leave the LAN server running',async()=>{
  let release!:()=>void;
  const first=new Promise<void>(resolve=>release=resolve);
  const calls:string[]=[];
  const connection=createIPhoneConnection(async(command,args)=>{
    calls.push(command+':'+String(args?.enabled??''));
    if(calls.length===1)await first;
  },()=>null);
  const enabled=connection.update(true,DEFAULTS);
  await Promise.resolve();
  const disabled=connection.update(false,DEFAULTS);
  release();await Promise.all([enabled,disabled]);
  expect(calls).toEqual(['iphone_alarms_configure:true','iphone_alarms_configure:false']);
});

it('publishes only after successful enable and reports failure without sending an empty replacement',async()=>{
  const calls:string[]=[];
  const connection=createIPhoneConnection(async(command)=>{calls.push(command);},()=>'{corrupt');
  await connection.update(true,DEFAULTS);
  expect(calls).toEqual(['iphone_alarms_configure']);
  expect(connection.error()).toMatch(/routine|schedule/i);
});

it('refreshes only while enabled and uses the saved scheduler start',async()=>{
  const calls:{command:string;args:Record<string,unknown>|undefined}[]=[];
  const connection=createIPhoneConnection(async(command,args)=>{calls.push({command,args});},()=>JSON.stringify({start:'2026-11-04'}));
  await connection.update(true,{...DEFAULTS,program:[]});
  await connection.refresh();
  expect(calls.filter(c=>c.command==='iphone_alarms_publish')).toHaveLength(2);
  expect(calls[1].args?.snapshot).toMatchObject({alarms:[]});
  await connection.update(false,DEFAULTS);
  const count=calls.length;await connection.refresh();expect(calls).toHaveLength(count);
});
