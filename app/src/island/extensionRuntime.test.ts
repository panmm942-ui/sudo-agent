import {expect,it,vi} from 'vitest';
import {EXAMPLE_TAB} from './extensions';
import {createExtensionHandler,extensionDocument,validateRequest} from './extensionRuntime';
const storage=()=>{const values=new Map<string,string>();return{getItem:(key:string)=>values.get(key)??null,setItem:(key:string,value:string)=>{values.set(key,value);},removeItem:(key:string)=>{values.delete(key);}};};
const networkTab={...EXAMPLE_TAB,permissions:{network:['https://api.example.com']},connections:[{id:'token',name:'Token',origin:'https://api.example.com',header:'X-Api-Key',prefix:''}]};
it('actually executes host persistence operations across remount handlers and isolates extension IDs',async()=>{
 const saved=storage(),run=createExtensionHandler(EXAMPLE_TAB,saved);
 expect(await run('state.get',{key:'counter'})).toBe(null);
 await run('state.set',{key:'counter',value:{count:5}});
 expect(await createExtensionHandler(EXAMPLE_TAB,saved)('state.get',{key:'counter'})).toEqual({count:5});
 expect(await createExtensionHandler({...EXAMPLE_TAB,id:'another'},saved)('state.get',{key:'counter'})).toBe(null);
 await run('state.set',{key:'__proto__',value:{safely:'data'}});
 expect(await run('state.get',{key:'__proto__'})).toEqual({safely:'data'});
 await run('state.remove',{key:'counter'});expect(await run('state.get',{key:'counter'})).toBe(null);
 await expect(run('state.set',{key:'huge',value:'x'.repeat(32000)})).rejects.toThrow('32000');
 await expect(run('state.get',{key:'../other'})).rejects.toThrow();
});
it('enforces destination, credential and body boundaries before native requests',()=>{
 expect(validateRequest(networkTab,{url:'https://api.example.com/v1',connection:'token'})).toEqual({url:'https://api.example.com/v1',method:'GET',headers:{},connection:'token'});
 for(const request of [{url:'https://other.example.com'},{url:'http://api.example.com'},{url:'https://user:secret@api.example.com'},{url:'https://api.example.com',headers:{Authorization:'secret'}},{url:'https://api.example.com',headers:{'X-Api-Key':'secret'}},{url:'https://api.example.com',body:'GET body'},{url:'https://api.example.com',method:'TRACE'},{url:'https://api.example.com',connection:'missing'},{url:'https://api.example.com',method:'POST',body:'x'.repeat(32001)}])expect(()=>validateRequest(networkTab,request)).toThrow();
});
it('returns real request results/errors and bounds concurrent network calls',async()=>{
 let resolve!:(v:unknown)=>void;const waiting=new Promise(r=>resolve=r);const fetch=vi.fn(()=>waiting);const run=createExtensionHandler(networkTab,storage(),fetch,async()=>true);
 expect(await run('connection.status',{id:'token'})).toBe(true);
 const calls=Array.from({length:4},()=>run('request',{url:'https://api.example.com'}));
 await expect(run('request',{url:'https://api.example.com'})).rejects.toThrow('four');resolve({status:200,text:'real response'});expect(await Promise.all(calls)).toHaveLength(4);expect(fetch).toHaveBeenCalledTimes(4);
 await expect(createExtensionHandler(networkTab,storage(),async()=>{throw new Error('Service offline');})('request',{url:'https://api.example.com'})).rejects.toThrow('Service offline');
});
it('embeds code as escaped data and applies restrictive CSP',()=>{
 const doc=extensionDocument({...EXAMPLE_TAB,js:'document.body.textContent="</script><script>bad()</script>"'},'test-session');
 expect(doc).toContain("connect-src 'none'");expect(doc).toContain("frame-src 'none'");expect(doc).toContain("form-action 'none'");expect(doc).not.toContain('<script>bad()');expect(doc.match(/<script\b/g)).toHaveLength(1);expect(doc).toContain('Object.defineProperty(window');
});
