import assert from 'node:assert/strict';
import vm from 'node:vm';
import fs from 'node:fs/promises';
const source=(await fs.readFile(new URL('../extensions/sudo-spotify-bridge.js',import.meta.url),'utf8')).replace('__SUDO_BRIDGE_KEY__','a'.repeat(64));
const uri='spotify:track:3OHfY25tqY28d16oZczHc8';
async function setup({canvas=true,entity=uri,canvasFailure=false,syllables=false}={}){
 const timers=[],calls=[],events={},stored=new Map();
 const item={uri,metadata:{title:'Original fixture song',artist_name:'Fixture artist',album_title:'Fixture album',image_xlarge_url:'spotify:image:client-original-large','canvas.url':canvas?'https://canvaz.scdn.co/fixture.mp4':undefined,'canvas.entityUri':entity},images:[{url:'https://i.scdn.co/image/small',width:64,height:64}]};
 let testNow=Date.now();
 const context={TextEncoder,TextDecoder,URL,Uint8Array,ArrayBuffer,Date:{now:()=>testNow},AbortController,AbortSignal,Promise,console,
   document:{querySelectorAll:()=>[]},localStorage:{getItem:key=>stored.get(key)??null,setItem:(key,value)=>stored.set(key,value)},
   setInterval:(fn,ms)=>{timers.push({fn,ms});return timers.length;},clearInterval:()=>{},setTimeout:()=>1,clearTimeout:()=>{},
   Spicetify:{Player:{data:{item},getDuration:()=>180000,getProgress:()=>25000,isPlaying:()=>true,addEventListener:(name,fn)=>events[name]=fn},CosmosAsync:{get:async()=>({})},Platform:{AuthorizationAPI:{getState:()=>({token:{accessToken:'fixture-spotify-token',accessTokenExpirationTimestampMs:Date.now()+60000}})}}},
   fetch:async(url,options)=>{calls.push({url,options});if(url.startsWith('http://127.0.0.1'))return{ok:true};if(canvasFailure&&url.includes('canvaz-cache'))throw new TypeError('Temporary network failure');return{ok:true,status:200,json:async()=>({lyrics:syllables?{syncType:'SYLLABLE_SYNCED',lines:[{startTimeMs:'0',endTimeMs:'0',words:'One moment',syllables:[{startTimeMs:'0',numChars:'4'},{startTimeMs:'500',numChars:'6'}]},{startTimeMs:'2000',words:'Next line'}]}:{syncType:'LINE_SYNCED',lines:[{startTimeMs:'0',words:'Original first fixture line'},{startTimeMs:'20000',words:'Original second fixture line'}]}}),arrayBuffer:async()=>new ArrayBuffer(0)};},
 };
 vm.runInNewContext(source,context);timers[0].fn();for(let i=0;i<20;i++)await Promise.resolve();
 return{context,timers,calls,events,item,advance:ms=>testNow+=ms};
}
{
 const test=await setup({canvas:false,canvasFailure:true});
 const tick=test.timers.find(timer=>timer.ms===1000).fn;
 assert.equal(test.calls.filter(c=>c.url.includes('canvaz-cache')).length,1);
 test.advance(21000);await tick();for(let i=0;i<20;i++)await Promise.resolve();
 assert.equal(test.calls.filter(c=>c.url.includes('canvaz-cache')).length,2,'Transient Canvas failures must retry after backoff rather than remain cached for the song');
 for(let attempt=0;attempt<5;attempt++){test.advance(31000);await tick();for(let i=0;i<20;i++)await Promise.resolve();}
 assert.equal(test.calls.filter(c=>c.url.includes('canvaz-cache')).length,3,'Retries must stay bounded per current song');
 test.item.metadata['canvas.url']='https://canvaz.scdn.co/recovered.mp4';test.item.metadata['canvas.entityUri']=uri;
 await tick();for(let i=0;i<20;i++)await Promise.resolve();
 const last=test.calls.filter(c=>c.url.startsWith('http:')).map(c=>JSON.parse(c.options.body)).at(-1);
 assert.equal(last.canvasUrl,'https://canvaz.scdn.co/recovered.mp4');assert.equal(last.canvasError,null,'Fresh metadata must clear the obsolete Canvas failure');
}
{
 const test=await setup();
 assert.equal(test.calls.filter(call=>call.url.includes('canvaz-cache')).length,0,'Current-track Canvas metadata should skip network lookup');
 const packets=test.calls.filter(call=>call.url.startsWith('http://127.0.0.1')).map(call=>JSON.parse(call.options.body));
 assert.ok(packets.some(packet=>packet.canvasUrl==='https://canvaz.scdn.co/fixture.mp4'));
 assert.ok(packets.some(packet=>packet.lyrics?.length===2));
 assert.equal(packets[0].images[0].url,'https://i.scdn.co/image/client-original-large');
 assert.equal(packets[0].position,25);
 assert.ok(packets.every(packet=>!JSON.stringify(packet).includes('fixture-spotify-token')),'Spotify token must never enter the local bridge packet');
 assert.ok(test.calls.filter(call=>call.url.startsWith('http:')).every(call=>call.options.headers.Authorization==='Bearer '+'a'.repeat(64)));
 test.events.songchange();for(let i=0;i<10;i++)await Promise.resolve();assert.equal(test.calls.filter(call=>call.url.includes('canvaz-cache')).length,0);
}
{
 const test=await setup({entity:'spotify:track:4OHfY25tqY28d16oZczHc8'});
 assert.equal(test.calls.filter(call=>call.url.includes('canvaz-cache')).length,1,'Stale Canvas entity must not be reused');
 const packets=test.calls.filter(call=>call.url.startsWith('http:')).map(call=>JSON.parse(call.options.body));assert.ok(packets.every(packet=>packet.canvasUrl===null));
 assert.ok(test.calls.every(call=>!call.url.includes('api.spotify.com/v1')),'Bridge must not call rate-limited Web API search');
}
{
 const test=await setup({syllables:true});
 const packet=test.calls.filter(c=>c.url.startsWith('http:')).map(c=>JSON.parse(c.options.body)).find(p=>p.lyrics?.[0]?.words);
 assert.deepEqual(packet.lyrics[0].words,[{time:0,end:.5,text:'One '},{time:.5,end:2,text:'moment'}]);
}
console.log('Spotify bridge: metadata Canvas, stale-entity rejection, bounded retry/backoff, recovered metadata, covers, clock, lyrics, local pairing, no token export and no Web API search passed.');
