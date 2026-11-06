import {expect,it} from 'vitest';
// @ts-ignore Node-only test helper.
import {readFileSync} from 'node:fs';
// @ts-ignore Node-only test helper.
import {runInNewContext} from 'node:vm';
const source=readFileSync(new URL('../../src-tauri/src/main_frame_ipc.js',import.meta.url),'utf8');
const rendered=source.replace('__INVOKE_KEY__',JSON.stringify('test-native-invoke-key'));
it('never evaluates the native key or installs a transport inside an iframe',()=>{
 const frame={top:{},__TAURI_INTERNALS__:{}};
 expect(()=>runInNewContext(source.replace('__INVOKE_KEY__','MUST_NOT_EVALUATE'),{window:frame})).not.toThrow();
 expect(Object.keys(frame.__TAURI_INTERNALS__)).toEqual([]);
});
it('preserves the binary backdrop fast path and channel serialization in the main frame',async()=>{
 const requests:any[]=[],callbacks:any[]=[];
 const host:any={__TAURI_INTERNALS__:{convertFileSrc:(cmd:string)=>'http://ipc.localhost/'+cmd,runCallback:(...args:unknown[])=>callbacks.push(args)},ipc:{postMessage:()=>{throw Error('Binary IPC unexpectedly fell back to JSON');}}};host.top=host;
 const buffer=new Uint8Array([1,2,3]).buffer;
 runInNewContext(rendered,{window:host,Map,Uint8Array,ArrayBuffer,Object,JSON,Headers,console,fetch:async(url:string,options:unknown)=>{requests.push({url,options});return{headers:new Headers({'Tauri-Response':'ok','content-type':'application/octet-stream'}),arrayBuffer:async()=>buffer};}});
 host.__TAURI_INTERNALS__.postMessage({cmd:'plugin:__TAURI_CHANNEL__|fetch',callback:7,error:8,options:{headers:{'X-Test':'yes'}},payload:{channel:{__TAURI_TO_IPC_KEY__:()=> '__CHANNEL__:91'}}});
 await new Promise(resolve=>setTimeout(resolve,0));
 expect(requests[0].options.headers.get('Tauri-Invoke-Key')).toBe('test-native-invoke-key');
 expect(JSON.parse(requests[0].options.body)).toEqual({channel:'__CHANNEL__:91'});
 expect(callbacks).toEqual([[7,buffer]]);
 expect(host.__TAURI_INTERNALS__.postMessage.toString()).not.toContain('test-native-invoke-key');
});
it('retains Tauri postMessage fallback for a failed protocol request',async()=>{
 const messages:string[]=[];const host:any={__TAURI_INTERNALS__:{convertFileSrc:()=> 'http://ipc.localhost/test'},ipc:{postMessage:(data:string)=>messages.push(data)}};host.top=host;
 runInNewContext(rendered,{window:host,Map,Uint8Array,ArrayBuffer,Object,JSON,Headers,console:{warn:()=>{}},fetch:async()=>{throw Error('protocol blocked');}});
 host.__TAURI_INTERNALS__.postMessage({cmd:'test',callback:7,error:8,payload:{map:new Map([['a',1]]),bytes:new Uint8Array([2,3]),buffer:new Uint8Array([4,5]).buffer}});
 await new Promise(resolve=>setTimeout(resolve,0));
 expect(JSON.parse(messages[0])).toEqual({cmd:'test',callback:7,error:8,payload:{map:{a:1},bytes:[2,3],buffer:[4,5]},options:{customProtocolIpcBlocked:true},__TAURI_INVOKE_KEY__:'test-native-invoke-key'});
});
