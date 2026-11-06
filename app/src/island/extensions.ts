export const MAX_TAB_BYTES=120_000;
export const TAB_ICONS=['sparkles','mail','calendar','check','chart','globe','code','note','timer','cloud','music','bolt'] as const;
export type TabIcon=typeof TAB_ICONS[number];
type Identity={format:'sudo-tab';id:string;name:string;compact:string;icon?:TabIcon};
export type LegacyTabPackage=Identity&{version:1;blocks:({type:'text';text:string}|{type:'clock'}|{type:'focus'})[]};
export type TabConnection={id:string;name:string;origin:string;header:string;prefix:string};
export type ExecutableTabPackage=Identity&{version:2;icon:TabIcon;html:string;css:string;js:string;permissions:{network:string[]};connections:TabConnection[]};
export type TabPackage=LegacyTabPackage|ExecutableTabPackage;
const record=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const keys=(v:Record<string,unknown>,allowed:string[])=>Object.keys(v).every(k=>allowed.includes(k));
export function isNetworkOrigin(value:unknown):value is string {
 if(typeof value!=='string'||value.length>250)return false;
 try{const u=new URL(value);return u.protocol==='https:'&&!u.username&&!u.password&&u.origin===value&&u.hostname.includes('.')&&u.hostname.split('.').every(label=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(label))&&!/^\d+(\.\d+){3}$/.test(u.hostname)&&!/(^|\.)(localhost|local|internal)$/.test(u.hostname);}catch{return false;}
}
export function validateTab(value:unknown):TabPackage{
 const fail=(message='Invalid Sudo tab package. Check the authoring prompt and JSON schema.'):never=>{throw new Error(message);};
 let serialized:string;try{serialized=JSON.stringify(value);}catch{return fail();}if(new TextEncoder().encode(serialized!).length>MAX_TAB_BYTES)return fail('Tab package exceeds 120 KB.');
 if(!record(value))return fail();const v=value;
 if(v.format!=='sudo-tab'||typeof v.id!=='string'||!/^[-a-z0-9]{1,40}$/.test(v.id)||typeof v.name!=='string'||!v.name.trim()||v.name.length>40||typeof v.compact!=='string'||v.compact.length>80||('icon'in v&&!TAB_ICONS.includes(v.icon as TabIcon)))return fail();
 const identity={format:'sudo-tab' as const,id:v.id,name:v.name.trim(),compact:v.compact,...(v.icon?{icon:v.icon as TabIcon}:{})};
 if(v.version===1){
  if(!keys(v,['format','version','id','name','compact','icon','blocks'])||!Array.isArray(v.blocks)||v.blocks.length<1||v.blocks.length>8)return fail();
  const blocks:LegacyTabPackage['blocks']=v.blocks.map(b=>{if(!record(b))return fail();if(b.type==='text'&&keys(b,['type','text'])&&typeof b.text==='string'&&b.text.length<=1500)return{type:'text',text:b.text};if((b.type==='clock'||b.type==='focus')&&Object.keys(b).length===1)return{type:b.type};return fail();});
  return{...identity,version:1,blocks};
 }
 if(v.version!==2||!keys(v,['format','version','id','name','compact','icon','html','css','js','permissions','connections'])||!TAB_ICONS.includes(v.icon as TabIcon))return fail();
 for(const[field,limit]of[['html',30000],['css',20000],['js',60000]]as const)if(typeof v[field]!=='string'||v[field].length>limit)return fail(`${field} must be a string of at most ${limit} characters.`);
 if(!(v.html as string).trim()||!(v.js as string).trim())return fail('Executable tabs need nonempty html and js.');
 // Compile for syntax only. This function is never invoked in the host; execution stays in the sandbox.
 try{new Function(v.js as string);}catch{return fail('JavaScript syntax is invalid. Ask the authoring AI to repair it before importing.');}
 if(/<\s*\/?\s*(script|iframe|frame|object|embed|base|meta|link)\b|\bon\w+\s*=|\b(?:src|href|action|formaction)\s*=/i.test(v.html as string))return fail('HTML cannot include scripts, embedded documents, event attributes or resource/navigation URLs. Use js event listeners and the Sudo request bridge.');
 if(!record(v.permissions)||!keys(v.permissions,['network'])||!Array.isArray(v.permissions.network)||v.permissions.network.length>8||!v.permissions.network.every(isNetworkOrigin)||new Set(v.permissions.network).size!==v.permissions.network.length)return fail('Declare up to 8 unique, exact public HTTPS origins in permissions.network.');
 const network=v.permissions.network as string[];
 if(v.connections!==undefined&&(!Array.isArray(v.connections)||v.connections.length>8))return fail();
 const connections:TabConnection[]=((v.connections as unknown[]|undefined)??[]).map(c=>{
  if(!record(c)||!keys(c,['id','name','origin','header','prefix'])||typeof c.id!=='string'||!/^[-a-z0-9]{1,40}$/.test(c.id)||typeof c.name!=='string'||!c.name.trim()||c.name.length>60||!network.includes(c.origin as string)||typeof c.header!=='string'||!/^[-A-Za-z0-9]{1,64}$/.test(c.header)||/^(host|cookie|set-cookie|proxy-|content-length|origin|referer)/i.test(c.header)||typeof c.prefix!=='string'||c.prefix.length>40||/[\r\n]/.test(c.prefix))return fail('Invalid connection declaration. Use a declared origin and valid credential header.');
  return{id:c.id,name:c.name.trim(),origin:c.origin as string,header:c.header,prefix:c.prefix};
 });
 if(new Set(connections.map(c=>c.id)).size!==connections.length)return fail('Connection IDs must be unique.');
 return{...identity,version:2,icon:v.icon as TabIcon,html:v.html as string,css:v.css as string,js:v.js as string,permissions:{network},connections};
}
export const EXAMPLE_TAB:ExecutableTabPackage={format:'sudo-tab',version:2,id:'tiny-counter',name:'Tiny counter',compact:'A little progress',icon:'check',permissions:{network:[]},connections:[],html:'<main><p class="eyebrow">A LITTLE PROGRESS</p><h1 id="count">0</h1><div><button id="add">Add one</button><button id="reset" class="secondary">Reset</button></div><p id="status" role="status">Loading saved progress…</p></main>',css:'main{padding:16px 22px}.eyebrow{color:var(--muted);font-size:10px;letter-spacing:.12em}h1{font-size:42px;line-height:1;margin:8px 0 14px}button{margin-right:8px}.secondary{background:transparent;color:var(--text);border:1px solid var(--border)}#status{color:var(--muted);font-size:11px;margin-top:10px}',js:'(async()=>{const status=document.querySelector("#status"),count=document.querySelector("#count");try{let value=Number(await sudo.state.get("count"))||0;const render=()=>{count.textContent=String(value)};render();status.textContent="Saved on this computer";async function save(next){value=next;render();try{await sudo.state.set("count",value);status.textContent="Progress saved"}catch(e){status.textContent=e.message}}document.querySelector("#add").addEventListener("click",()=>save(value+1));document.querySelector("#reset").addEventListener("click",()=>save(0))}catch(e){status.textContent=e.message}})();'};
export const TAB_PROMPT=`Build a REAL executable extension for Sudo, a personal Windows Dynamic Island (Svelte 5 / Tauri 2). Ask what the extension should DO, which Home icon I want, and which real service/account is needed. Return ONE downloadable UTF-8 .json file, valid JSON, no markdown fences. Implement real working logic and interactions, persistence, loading/empty/disconnected/error/success states. Never present sample email/account data as live. Structural validation cannot guarantee AI-generated behavior is correct. Never embed secrets in a package or publication.

EXACT VERSION 2 SCHEMA:
{"format":"sudo-tab","version":2,"id":"my-extension","name":"My extension","compact":"Small island text","icon":"sparkles","html":"<main>...</main>","css":"main{padding:16px 22px}","js":"...","permissions":{"network":[]},"connections":[]}
No extra keys. id: lowercase a-z, digits, hyphens, 1–40 chars; name 1–40; compact <=80 (static compact caption). Required icon: ${TAB_ICONS.join(', ')}. JSON <=120000 UTF-8 bytes; html <=30000 characters, css <=20000, js <=60000; html/js nonempty. Escape JSON strings correctly. Plain browser JavaScript only, no external dependencies/imports/remote fonts/build step/framework.

ENVIRONMENT / DESIGN:
Expanded outer area 440x210 CSS px; left extension navigation reserves 60px and right core navigation reserves 62px, giving iframe 318x210 (design responsive down to 318x150). Compact is 246x44 and host renders static compact text/icon. Use internal scrolling if needed. Transparent dark glass; body 11–13px, heading 16–18px; useful hierarchy, compact spacing, one clear workflow. Injected tokens: --text:#f5f5f5, --muted:#a8a8ad, --accent:#b8a4ff, --border:rgba(255,255,255,.14), --surface:rgba(255,255,255,.06), --font-sans:system-ui,sans-serif. Basic dark inputs/buttons exist. Add accessible labels, visible focus, status text and reduced-motion support. HTML cannot contain script/iframe/frame/object/embed/base/meta/link, inline event attributes or src/href/action/formaction. Bind events in js. Render untrusted API text with textContent. No navigation/popups/native OS/Tauri access.

RUNTIME / SDK:
Opaque-origin iframe sandbox="allow-scripts" ONLY. Strict CSP blocks direct fetch/XHR/WebSocket/images/external scripts/frames/forms. Parent CSP also blocks all URL frame navigation (the initial srcdoc document is allowed). No host DOM/storage. window.sudo is injected BEFORE js executes. Use it for per-extension persistent state and declared network. All methods return Promises; errors are Error objects with readable message and code. Catch failures. No background work after the tab unmounts; avoid infinite loops and intervals below 1000ms. The host cannot certify arbitrary JS behavior/CPU use.
await sudo.state.get(key) -> JSON value or null. await sudo.state.set(key,jsonValue) -> true. await sudo.state.remove(key) -> true. key: 1–80 chars of a-z A-Z 0-9 _ . -. Total state <=32000 UTF-8 bytes. State survives reopen/restart. Do not store secrets. Removing extension clears state and connections.
await sudo.connection.status(id) -> boolean (credential exists; never reveals value). User saves/removes secrets in Settings → Marketplace. Secrets live in Windows Credential Manager, separately from package/export.
await sudo.request({url:"https://api.example.com/v1/items",method:"GET",headers:{Accept:"application/json"},connection:"service-token"}) -> {status:number,ok:boolean,headers:Record<string,string>,text:string}. Parse explicitly: if(!response.ok) throw new Error('Service returned '+response.status); const data=JSON.parse(response.text). Optional method GET(default)/POST/PUT/PATCH/DELETE; optional UTF-8 body <=32000 bytes (not GET). <=16 string headers <=2000 chars/value; Authorization/Cookie/Host/etc prohibited directly, use connection. Max 4 concurrent requests, 20 second native timeout, <=256000 response bytes. Redirects rejected. Browser-only previews reject NETWORK_UNAVAILABLE; installed Windows host handles CORS. Do not automatically send/delete user data; show explicit actions and actual service results.

PERMISSIONS / REAL CONNECTIONS:
permissions.network: <=8 unique exact public HTTPS origins, e.g. "https://api.open-meteo.com". No path/query/wildcards/trailing slash/userinfo/IP/private host. Host only requests those origins; import review asks approval for executable code/destinations.
connections: <=8 objects exactly {"id":"gmail-token","name":"Gmail access token","origin":"https://gmail.googleapis.com","header":"Authorization","prefix":"Bearer "}. Origin must be declared. id follows package id rules; name <=60 chars; header ASCII name <=64; prefix <=40, no newline. NO secret/value property. The user provides a valid credential AFTER import. Host injects prefix+secret only for that connection's exact origin and never returns secret to iframe. A Gmail extension may call Gmail REST with user-supplied valid access token/scopes; automatic OAuth/refresh, email account access and AI classification are NOT built in. Implement disconnected until connected, handle 401/403/expiration and explain reconnection. Rule-based triage is real local logic; model-based triage needs a separate real API/origin/credential. Label sample data clearly if explicitly requested.

MESSAGING (prefer SDK): parent.postMessage({channel:"sudo.extension.v2",session:<injected token>,id:<unique string>,method:"state.get"|"state.set"|"state.remove"|"connection.status"|"request",params:{...}},"*"). Host checks exact iframe window + session. Replies {channel:"sudo.extension.v2",session,id,ok:true,result} or {channel:"sudo.extension.v2",session,id,ok:false,error:{code,message}}. SDK owns token/IDs and has 25 second timeout. Do not replace it.

WORKING EXAMPLE: real executable counter with persisted state:
${JSON.stringify(EXAMPLE_TAB)}

Before delivering: parse JSON, validate every size/schema/icon/origin/connection rule; exercise buttons/calculations, persistence after reopening, connected/disconnected/loading/empty/error/success, API text escaping, keyboard and 318x150–210 layout. Explain real service/token setup separately. Never promise arbitrary AI code is guaranteed to work.`;
