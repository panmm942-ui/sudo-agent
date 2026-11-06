import {invoke,isTauri} from '@tauri-apps/api/core';
import {isNetworkOrigin,type ExecutableTabPackage,type TabPackage,type TabConnection} from './extensions';
export const EXTENSION_CHANNEL='sudo.extension.v2';
const STATE_PREFIX='sudo.extension.state.';
const MAX_STATE=32_000;
export class ExtensionError extends Error{constructor(public code:string,message:string){super(message);}}
function native<T>(command:string,args:Record<string,unknown>):Promise<T>{if(!isTauri())return Promise.reject(new ExtensionError('NETWORK_UNAVAILABLE','Connections and network requests require the installed Windows app.'));return invoke<T>(command,args);}
function connection(tab:TabPackage,id:string):TabConnection{const c=tab.version===2?tab.connections.find(c=>c.id===id):undefined;if(!c)throw new ExtensionError('UNKNOWN_CONNECTION','This connection is not declared by the extension.');return c;}
export function connectionStatus(tab:TabPackage,id:string):Promise<boolean>{return native('extension_connection_status',{extensionId:tab.id,connection:connection(tab,id)});}
export function saveConnection(tab:TabPackage,id:string,value:string):Promise<void>{if(!value.trim()||value.length>2500||/[\r\n\0]/.test(value))return Promise.reject(new Error('Enter a credential of 1–2500 characters without newlines.'));return native('extension_save_connection',{extensionId:tab.id,connection:connection(tab,id),value});}
export function removeConnection(tab:TabPackage,id:string):Promise<void>{return native('extension_remove_connection',{extensionId:tab.id,connection:connection(tab,id)});}
export async function cleanupExtension(tab:TabPackage):Promise<void>{
 if(isTauri())await native('extension_cleanup',{extensionId:tab.id});
 localStorage.removeItem(STATE_PREFIX+tab.id);
}
export type ExtensionRequest={url:string;method:string;headers:Record<string,string>;body?:string;connection?:string};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const key=(v:unknown):string=>{if(typeof v!=='string'||!/^[-\w.]{1,80}$/.test(v))throw new ExtensionError('INVALID_KEY','State keys need 1–80 letters, digits, dots, underscores or hyphens.');return v;};
export function validateRequest(tab:ExecutableTabPackage,value:unknown):ExtensionRequest{
 if(!object(value)||Object.keys(value).some(k=>!['url','method','headers','body','connection'].includes(k))||typeof value.url!=='string'||value.url.length>4000)throw new ExtensionError('INVALID_REQUEST','Invalid network request.');
 let url:URL;try{url=new URL(value.url);}catch{throw new ExtensionError('INVALID_URL','Enter a valid HTTPS request URL.');}
 if(!isNetworkOrigin(url.origin)||url.username||url.password||url.hash||!tab.permissions.network.includes(url.origin))throw new ExtensionError('ORIGIN_DENIED','This exact HTTPS origin was not approved for this extension.');
 const method=value.method??'GET';if(typeof method!=='string'||!['GET','POST','PUT','PATCH','DELETE'].includes(method))throw new ExtensionError('INVALID_METHOD','Unsupported request method.');
 if(value.body!==undefined&&(typeof value.body!=='string'||new TextEncoder().encode(value.body).length>32000||method==='GET'))throw new ExtensionError('INVALID_BODY','Request body must be at most 32000 bytes and cannot be used with GET.');
 const headers=value.headers??{};if(!object(headers)||Object.keys(headers).length>16)throw new ExtensionError('INVALID_HEADERS','Use at most 16 request headers.');
 for(const[name,content]of Object.entries(headers))if(!/^[-A-Za-z0-9]{1,64}$/.test(name)||typeof content!=='string'||content.length>2000||/[\r\n\0]/.test(content)||/^(authorization|cookie|set-cookie|host|proxy-|content-length|origin|referer|connection|transfer-encoding|upgrade|sec-)/i.test(name)||tab.connections.some(c=>c.header.toLowerCase()===name.toLowerCase()))throw new ExtensionError('INVALID_HEADERS','Credentials and restricted headers must be managed by the host.');
 if(value.connection!==undefined){if(typeof value.connection!=='string')throw new ExtensionError('UNKNOWN_CONNECTION','Invalid connection.');const c=connection(tab,value.connection);if(c.origin!==url.origin)throw new ExtensionError('CONNECTION_ORIGIN','Connection cannot be sent to this origin.');}
 return{url:url.href,method,headers:headers as Record<string,string>,...(value.body!==undefined?{body:value.body as string}:{}),...(value.connection?{connection:value.connection as string}:{})};
}
type StorageLike=Pick<Storage,'getItem'|'setItem'|'removeItem'>;
export function createExtensionHandler(tab:ExecutableTabPackage,storage:StorageLike,request=(payload:ExtensionRequest)=>native<unknown>('extension_request',{extensionId:tab.id,request:payload,allowedOrigins:tab.permissions.network,connection:payload.connection?connection(tab,payload.connection):null}),status=(id:string)=>connectionStatus(tab,id)){
 let active=0;
 return async(method:unknown,params:unknown):Promise<unknown>=>{
  if(!object(params))throw new ExtensionError('INVALID_MESSAGE','Expected request parameters.');
  if(method==='request'){if(active>=4)throw new ExtensionError('BUSY','At most four concurrent requests are allowed.');const payload=validateRequest(tab,params);active++;try{return await request(payload);}finally{active--;}}
  if(method==='connection.status')return status(key(params.id));
  if(!['state.get','state.set','state.remove'].includes(method as string))throw new ExtensionError('UNKNOWN_METHOD','Unsupported extension method.');
  const stateKey=key(params.key),storageKey=STATE_PREFIX+tab.id;
  let state:Record<string,unknown>={};try{const saved=JSON.parse(storage.getItem(storageKey)??'{}');if(object(saved))state=saved;}catch{}
  if(method==='state.get')return Object.hasOwn(state,stateKey)?state[stateKey]:null;
  if(method==='state.remove')delete state[stateKey];else{
   if(!Object.hasOwn(params,'value'))throw new ExtensionError('INVALID_VALUE','State value must be JSON.');
   let json:string;try{json=JSON.stringify(params.value);if(json===undefined)throw 0;Object.defineProperty(state,stateKey,{value:JSON.parse(json),enumerable:true,writable:true,configurable:true});}catch{throw new ExtensionError('INVALID_VALUE','State value must be JSON.');}
  }
  const encoded=JSON.stringify(state);if(new TextEncoder().encode(encoded).length>MAX_STATE)throw new ExtensionError('STATE_LIMIT','Extension state exceeds 32000 bytes.');
  try{storage.setItem(storageKey,encoded);}catch{throw new ExtensionError('STORAGE_UNAVAILABLE','Could not save extension state on this computer.');}return true;
 };
}
export function attachExtension(frame:HTMLIFrameElement,tab:ExecutableTabPackage,session:string):()=>void{
 const handle=createExtensionHandler(tab,localStorage);let disposed=false;
 const listener=async(event:MessageEvent)=>{
  if(disposed||event.source!==frame.contentWindow||!object(event.data))return;const msg=event.data;
  if(msg.channel!==EXTENSION_CHANNEL||msg.session!==session||typeof msg.id!=='string'||msg.id.length>80)return;
  let reply:Record<string,unknown>;try{reply={ok:true,result:await handle(msg.method,msg.params)};}catch(error){reply={ok:false,error:{code:error instanceof ExtensionError?error.code:'HOST_ERROR',message:error instanceof Error?error.message:typeof error==='string'?error:'Extension operation failed.'}};}
  if(!disposed)frame.contentWindow?.postMessage({channel:EXTENSION_CHANNEL,session,id:msg.id,...reply},'*');
 };window.addEventListener('message',listener);return()=>{disposed=true;window.removeEventListener('message',listener);};
}
const literal=(value:unknown)=>JSON.stringify(value).replace(/</g,'\\u003c').replace(/\u2028/g,'\\u2028').replace(/\u2029/g,'\\u2029');
export function extensionDocument(tab:ExecutableTabPackage,session:string):string{
 const nonce=crypto.randomUUID().replace(/-/g,'');
 const bootstrap=`(()=>{const session=${literal(session)},channel=${literal(EXTENSION_CHANNEL)},pending=new Map();let seq=0;function call(method,params){return new Promise((resolve,reject)=>{const id=String(++seq),timer=setTimeout(()=>{pending.delete(id);reject(Object.assign(new Error('The host did not respond in time.'),{code:'TIMEOUT'}))},25000);pending.set(id,{resolve,reject,timer});parent.postMessage({channel,session,id,method,params},'*')})}addEventListener('message',e=>{if(e.source!==parent)return;const m=e.data;if(!m||m.channel!==channel||m.session!==session)return;const p=pending.get(m.id);if(!p)return;pending.delete(m.id);clearTimeout(p.timer);if(m.ok)p.resolve(m.result);else p.reject(Object.assign(new Error(m.error?.message||'Host request failed.'),{code:m.error?.code||'HOST_ERROR'}))});Object.defineProperty(window,'sudo',{value:Object.freeze({state:Object.freeze({get:key=>call('state.get',{key}),set:(key,value)=>call('state.set',{key,value}),remove:key=>call('state.remove',{key})}),connection:Object.freeze({status:id=>call('connection.status',{id})}),request:params=>call('request',params)}),writable:false,configurable:false});document.body.innerHTML=${literal(tab.html)};const style=document.createElement('style');style.textContent=${literal(tab.css)};document.head.append(style);const report=message=>{let p=document.querySelector('#sudo-runtime-error');if(!p){p=document.createElement('p');p.id='sudo-runtime-error';p.setAttribute('role','alert');p.style.cssText='padding:12px;color:#ffb4b4;font:12px system-ui';document.body.append(p)}p.textContent='Extension error: '+message};addEventListener('error',e=>report(e.message));addEventListener('unhandledrejection',e=>report(e.reason?.message||'Unhandled extension failure'));const script=document.createElement('script');script.nonce=${literal(nonce)};script.textContent=${literal(tab.js)};document.body.append(script)})();`;
 return `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'nonce-${nonce}'; style-src 'unsafe-inline'; connect-src 'none'; img-src 'none'; font-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"><style>:root{color-scheme:normal;--text:#f5f5f5;--muted:#a8a8ad;--accent:#b8a4ff;--border:rgba(255,255,255,.14);--surface:rgba(255,255,255,.06);--font-sans:system-ui,sans-serif}*{box-sizing:border-box}html,body{margin:0;width:100%;height:100%;background:transparent;color:var(--text);font:12px/1.4 var(--font-sans);overflow-wrap:anywhere}body{overflow:auto;scrollbar-width:thin}button,input,select,textarea{color-scheme:dark;font:inherit;color:inherit;border-radius:9px;padding:7px 10px;border:1px solid var(--border);background:var(--surface)}button{cursor:pointer;background:var(--accent);color:#18151f}button:disabled{opacity:.5;cursor:default}:focus-visible{outline:2px solid var(--accent);outline-offset:2px}@media(prefers-reduced-motion:reduce){*,*::before,*::after{animation:none!important;transition:none!important}}</style></head><body><script nonce="${nonce}">${bootstrap}</script></body></html>`;
}
