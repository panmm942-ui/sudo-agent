/* Optional Sudo bridge for the built-in connector or an existing Spicetify client.
 * Spotify tokens stay inside this Spotify renderer and are sent only to Spotify.
 * The native exporter fills the local-only pairing key in a prepared copy.
 */
(function sudoSpotifyBridge() {
  'use strict';
  const endpoint='http://127.0.0.1:43893/v1/player', pairingKey='__SUDO_BRIDGE_KEY__';
  if(pairingKey.startsWith('__'))return;
  if(globalThis.__sudoSpotifyBridgeActive)return;
  globalThis.__sudoSpotifyBridgeActive=true;
  const client=()=>globalThis.__sudoSpotifyClient??globalThis.Spicetify;
  let current='',generation=0,packet=null,publishing=false,canvasAbort=null,lyricAbort=null,canvasAttempts=0,nextCanvasAttempt=0,canvasPending=false;
  const cache=new Map(),encoder=new TextEncoder();
  const httpsImage=value=>{
    if(typeof value!=='string')return null;
    if(value.startsWith('spotify:image:'))value='https://i.scdn.co/image/'+value.slice(14);
    try{const url=new URL(value);return url.protocol==='https:'&&url.hostname==='i.scdn.co'&&!url.port&&!url.username&&!url.password&&!url.hash&&url.pathname.startsWith('/image/')?value:null;}catch{return null;}
  };
  const canvasUrl=value=>{try{const u=new URL(value);return u.protocol==='https:'&&u.hostname==='canvaz.scdn.co'&&!u.port&&!u.username&&!u.password&&!u.hash&&u.pathname.endsWith('.mp4')?value:null;}catch{return null;}};
  function images(item){
    const found=[],seen=new Set();
    const add=(value,width=null,height=null)=>{const url=httpsImage(value);if(url&&!seen.has(url)){seen.add(url);found.push({url,width,height});}};
    const metadata=item.metadata??{};
    for(const name of ['image_xlarge_url','image_large_url','image_url','image_small_url'])add(metadata[name]);
    for(const image of [...(item.images??[]),...(item.album?.images??[])].sort((a,b)=>(b.width??0)-(a.width??0)))add(image.url,image.width??null,image.height??null);
    return found.slice(0,12);
  }
  function snapshot(){
    const player=client()?.Player,item=player?.data?.item??player?.data?.track,meta=item?.metadata??{};
    if(!item?.uri||!/^spotify:track:[A-Za-z0-9]{22}$/.test(item.uri))return null;
    const artists=item.artists?.map(a=>a.name).filter(Boolean).join(', ')||Object.keys(meta).filter(k=>/^artist_name(?::\d+)?$/.test(k)).sort().map(k=>meta[k]).filter(Boolean).join(', ');
    return {uri:item.uri,title:meta.title??item.name??'',artist:artists,album:meta.album_title??item.album?.name??'',
      duration:player.getDuration()/1000,position:player.getProgress()/1000,positionUpdatedAt:Date.now(),playing:player.isPlaying(),images:images(item),canvasUrl:meta['canvas.entityUri']===item.uri?canvasUrl(meta['canvas.url']):null,canvasError:null,lyrics:null};
  }
  async function publish(){
    if(publishing||!packet)return;
    const fresh=snapshot();if(!fresh||fresh.uri!==packet.uri)return;
    packet={...packet,position:fresh.position,positionUpdatedAt:fresh.positionUpdatedAt,playing:fresh.playing,images:fresh.images.length?fresh.images:packet.images};
    let body=JSON.stringify(packet);if(encoder.encode(body).length>262144){packet.lyrics=packet.lyrics?.map(({time,text})=>({time,text}))??null;body=JSON.stringify(packet);}
    if(encoder.encode(body).length>262144){packet.lyrics=null;body=JSON.stringify(packet);}
    publishing=true;
    try{await fetch(endpoint,{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+pairingKey},body,credentials:'omit',signal:AbortSignal.timeout(1500)});}catch{/* Sudo may be closed. No provider request or log is needed. */}
    finally{publishing=false;}
  }
  function fields(bytes){
    let offset=0;const rows=[];
    const integer=()=>{let value=0,multiple=1;for(let count=0;count<10;count++){if(offset>=bytes.length)throw Error();const b=bytes[offset++];value+=(b&127)*multiple;if(!(b&128))return value;multiple*=128;}throw Error();};
    while(offset<bytes.length&&rows.length<256){const tag=integer(),wire=tag&7;if(wire===2){const length=integer(),end=offset+length;if(end>bytes.length)throw Error();rows.push([tag>>3,bytes.subarray(offset,end)]);offset=end;}else if(wire===0)integer();else if(wire===1)offset+=8;else if(wire===5)offset+=4;else throw Error();if(offset>bytes.length)throw Error();}return rows;
  }
  const string=bytes=>new TextDecoder().decode(bytes);
  async function token(){
    const auth=client()?.Platform?.AuthorizationAPI;
    const cached=typeof auth?.getState==='function'?auth.getState()?.token:null;
    if(typeof cached?.accessToken==='string'&&cached.accessTokenExpirationTimestampMs>Date.now()+1000)return cached.accessToken;
    if(typeof auth?._tokenProvider?.getToken==='function'){
      const value=await auth._tokenProvider.getToken();if(typeof value?.accessToken==='string')return value.accessToken;
    }
    if(typeof auth?._tokenProvider==='function')return(await auth._tokenProvider({preferCached:true}))?.accessToken;
    if(typeof auth?.getAccessToken==='function'){const value=await auth.getAccessToken();return typeof value==='string'?value:value?.accessToken;}
    return null;
  }
  async function loadCanvas(uri,ticket){
    // Spotify's declared current-track metadata is stronger evidence than a
    // DOM video, which may still belong to the previous song during a fade.
    if(packet?.uri===uri&&packet.canvasUrl){void publish();return;}
    if(canvasPending||canvasAttempts>=3||Date.now()<nextCanvasAttempt)return;
    const last=Number(localStorage.getItem('sudo.bridge.canvasRetryAt')||0);if(Date.now()<last){packet.canvasError='Spotify Canvas requested a cooldown.';nextCanvasAttempt=last;return;}
    canvasPending=true;++canvasAttempts;nextCanvasAttempt=Infinity;
    const controller=canvasAbort=new AbortController();const timeout=setTimeout(()=>controller.abort(),6000);
    try{
      const accessToken=await token();if(!accessToken)throw Error('Spotify client did not expose Canvas authorization.');
      if(ticket!==generation||controller.signal.aborted)return;
      const text=encoder.encode(uri),nested=new Uint8Array([10,text.length,...text]),body=new Uint8Array([10,nested.length,...nested]);
      const response=await fetch('https://spclient.wg.spotify.com/canvaz-cache/v0/canvases',{method:'POST',headers:{Authorization:'Bearer '+accessToken,'Content-Type':'application/x-protobuf',Accept:'application/protobuf'},body,signal:controller.signal,credentials:'omit'});
      if(response.status===429){const wait=Math.max(1,Math.min(31536000,Number(response.headers.get('Retry-After'))||60));localStorage.setItem('sudo.bridge.canvasRetryAt',String(Date.now()+wait*1000));throw Error('Spotify Canvas requested a cooldown.');}
      if(!response.ok)throw Error('Spotify client Canvas request was declined.');
      const buffer=await response.arrayBuffer();if(buffer.byteLength>256000)throw Error('Canvas response exceeded its limit.');
      let selected=null;
      for(const [number,bytes]of fields(new Uint8Array(buffer))){if(number!==1)continue;const row=fields(bytes);if(string(row.find(([key])=>key===5)?.[1]??[])!==uri)continue;selected=canvasUrl(string(row.find(([key])=>key===2)?.[1]??[]));if(selected)break;}
      if(ticket===generation){packet.canvasUrl=selected;packet.canvasError=selected?null:'Spotify did not provide a Canvas loop for this song.';cache.set(uri,{canvasUrl:packet.canvasUrl,canvasError:packet.canvasError,lyrics:packet.lyrics});void publish();}
    }catch(error){if(ticket===generation){packet.canvasError=error?.name==='AbortError'?'Spotify Canvas request timed out.':String(error?.message??'Spotify Canvas is unavailable.');nextCanvasAttempt=Math.max(Date.now()+20000,Number(localStorage.getItem('sudo.bridge.canvasRetryAt')||0));cache.set(uri,{canvasUrl:null,canvasError:packet.canvasError,lyrics:packet.lyrics});void publish();}}
    finally{clearTimeout(timeout);if(ticket===generation)canvasPending=false;}
  }
  async function loadLyrics(uri,ticket){
    if(Date.now()<Number(localStorage.getItem('sudo.bridge.lyricRetryAt')||0))return;
    lyricAbort=new AbortController();const signal=lyricAbort.signal;
    try{
      const accessToken=await token();
      const response=await (async()=>{
        if(!accessToken)return client()?.CosmosAsync?.get('https://spclient.wg.spotify.com/color-lyrics/v2/track/'+uri.slice(14)+'?format=json&vocalRemoval=false');
        const response=await fetch('https://spclient.wg.spotify.com/color-lyrics/v2/track/'+uri.slice(14)+'?format=json&vocalRemoval=false',{headers:{Authorization:'Bearer '+accessToken,'App-Platform':'WebPlayer'},credentials:'omit',signal:AbortSignal.any([signal,AbortSignal.timeout(6000)])});
        if(response.status===429){const wait=Math.max(1,Math.min(31536000,Number(response.headers.get('Retry-After'))||60));localStorage.setItem('sudo.bridge.lyricRetryAt',String(Date.now()+wait*1000));return null;}
        if(!response.ok)return null;return response.json();
      })();
      if(ticket!==generation||signal.aborted||!['LINE_SYNCED','SYLLABLE_SYNCED'].includes(response?.lyrics?.syncType))return;
      const rows=response.lyrics.lines;if(!Array.isArray(rows))return;
      const lyrics=rows.slice(0,600).flatMap((line,index)=>{
        const time=Number(line.startTimeMs)/1000,text=typeof line.words==='string'?line.words:'';
        if(!Number.isFinite(time)||time<0||!text.trim()||text.length>1000)return [];
        let offset=0,previous=time;const words=[];
        if(Array.isArray(line.syllables)&&line.syllables.length<=256){
          for(let i=0;i<line.syllables.length;i++){
            const syllable=line.syllables[i],count=Number(syllable.numChars),start=Number(syllable.startTimeMs)/1000;
            const explicitEnd=Number(line.endTimeMs)/1000;
            const end=i+1<line.syllables.length?Number(line.syllables[i+1].startTimeMs)/1000:explicitEnd>start?explicitEnd:Number(rows[index+1]?.startTimeMs)/1000;
            if(!Number.isInteger(count)||count<1||offset+count>text.length||!Number.isFinite(start)||!Number.isFinite(end)||start<previous||end<=start||end>packet.duration+2){words.length=0;break;}
            words.push({time:start,end,text:text.slice(offset,offset+count)});offset+=count;previous=end;
          }
        }
        return [{time,text,...(words.length&&offset===text.length?{words}:{})}];
      });
      if(lyrics.length){packet.lyrics=lyrics;cache.set(uri,{canvasUrl:packet.canvasUrl,canvasError:packet.canvasError,lyrics});void publish();}
    }catch(error){if(error?.status===429||error?.code===429)localStorage.setItem('sudo.bridge.lyricRetryAt',String(Date.now()+60000));/* No lyric text or token is logged. */}
  }
  async function changed(){
    const next=snapshot();if(!next)return;
    if(next.uri!==current){current=next.uri;++generation;canvasAbort?.abort();lyricAbort?.abort();canvasAttempts=0;nextCanvasAttempt=0;canvasPending=false;packet={...next,...(cache.get(current)??{})};if(next.canvasUrl){packet.canvasUrl=next.canvasUrl;packet.canvasError=null;}if(cache.size>32)cache.delete(cache.keys().next().value);void publish();if(!cache.has(current)||(!packet.canvasUrl&&packet.canvasError!=='Spotify did not provide a Canvas loop for this song.'))void loadCanvas(current,generation);if(!packet.lyrics)void loadLyrics(current,generation);}
    else{packet={...next,canvasUrl:next.canvasUrl??packet?.canvasUrl??null,canvasError:next.canvasUrl?null:packet?.canvasError??null,lyrics:packet?.lyrics??null};void publish();if(!packet.canvasUrl&&Date.now()>=nextCanvasAttempt)void loadCanvas(current,generation);}
  }
  const ready=setInterval(()=>{const api=client();if(!api?.Player?.data)return;clearInterval(ready);api.Player.addEventListener('songchange',changed);api.Player.addEventListener('onplaypause',changed);setInterval(changed,1000);void changed();},300);
})();
