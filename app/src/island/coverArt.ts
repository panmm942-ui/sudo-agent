import {invoke} from '@tauri-apps/api/core';
import type {SpotifyTrack} from './spotify';
export type CoverArtResult={url:string;width:number|null;height:number|null;spotifyUrl:string;trackId:string;sourceUrl:string};
type Metadata=Pick<SpotifyTrack,'key'|'title'|'artist'|'album'|'duration'|'bridgeRevision'>;
type Cached={result:CoverArtResult|null;expires:number};
const cache=new Map<string,Cached>();
const pending=new Map<string,Promise<CoverArtResult|null>>();
type Pixels={width:number;height:number};
async function measureImage(source:string):Promise<Pixels|null>{
 if(typeof Image==='undefined')return null;
 return new Promise(resolve=>{
  const image=new Image();const timeout=setTimeout(()=>{image.onload=null;image.onerror=null;resolve(null);},2500);
  const finish=(value:Pixels|null)=>{clearTimeout(timeout);image.onload=null;image.onerror=null;resolve(value);};
  image.onload=()=>finish(image.naturalWidth&&image.naturalHeight?{width:image.naturalWidth,height:image.naturalHeight}:null);image.onerror=()=>finish(null);image.src=source;
 });
}
export async function isCoverUpgrade(original:string|null,result:CoverArtResult,measure:(source:string)=>Promise<Pixels|null>=measureImage):Promise<boolean>{
 // Node/native tests have no image decoder; browser consumers measure the
 // actual pixels of both sources once before replacing a readable thumbnail.
 if(typeof Image==='undefined'&&measure===measureImage)return true;
 const [replacement,current]=await Promise.all([measure(result.url),original?measure(original):null]);
 return !!replacement&&(!current||replacement.width>=current.width&&replacement.height>=current.height);
}
export function validCoverArtUrl(value:unknown):value is string{
 if(typeof value!=='string'||value.length>2048)return false;
 try{const url=new URL(value);return url.protocol==='https:'&&url.hostname==='i.scdn.co'&&!url.port&&!url.username&&!url.password&&!url.hash&&!url.search&&url.pathname.startsWith('/image/')&&url.pathname.length>7;}catch{return false;}
}
function validResult(value:CoverArtResult):boolean{
 const image=typeof value.url==='string'&&value.url.length<=5_600_000&&/^data:image\/(?:jpeg|png|webp);base64,[A-Za-z0-9+/]+=*$/.test(value.url);
 return image&&validCoverArtUrl(value.sourceUrl)&&typeof value.trackId==='string'&&/^[A-Za-z0-9]{22}$/.test(value.trackId)&&value.spotifyUrl===`https://open.spotify.com/track/${value.trackId}`&&[value.width,value.height].every(size=>size===null||Number.isInteger(size)&&size>0&&size<=16384);
}
async function resolve(key:string,track:Metadata,current:()=>boolean):Promise<CoverArtResult|null>{
 const stored=cache.get(key);if(stored&&stored.expires>Date.now())return stored.result;
 const active=pending.get(key);if(active)return active;
 while(pending.size>=2){
  // Saturation is temporary, not a completed miss. Recheck the reader after
  // capacity frees so only its latest song can start the deferred request.
  await Promise.race([...pending.values()].map(value=>value.catch(()=>null)));
  if(!current())return null;
  const cached=cache.get(key);if(cached&&cached.expires>Date.now())return cached.result;
  const shared=pending.get(key);if(shared)return shared;
 }
 if(!current())return null;
 const request=(async()=>{
  const value=await invoke<CoverArtResult|null>('spotify_cover_art',{trackKey:track.key,title:track.title,artist:track.artist,album:track.album??null,duration:track.duration??null});
  const result=value&&validResult(value)?value:null;
  if(cache.size>=8)cache.delete(cache.keys().next().value!);
  cache.set(key,{result,expires:Date.now()+(result?86_400_000:300_000)});return result;
 })();pending.set(key,request);
 try{return await request;}finally{if(pending.get(key)===request)pending.delete(key);}
}
/** Request once per metadata identity, never per playback observation. Errors
 * preserve the native thumbnail. stop/song changes discard pending results. */
export function createCoverArtLookup(update:(result:CoverArtResult|null,error?:string)=>void){
 let revision=0,identity:string|null=null,stopped=false;
 return {
  request(track:Metadata|null){
   if(stopped)return;
   const metadata=track?.key&&typeof track.title==='string'&&track.title.trim()&&typeof track.artist==='string'&&track.artist.trim()&&track.album?.trim()?{key:track.key,title:track.title,artist:track.artist,album:track.album,duration:track.duration??null}:null;
   const key=metadata?JSON.stringify([metadata,track?.bridgeRevision??null]):null;if(key===identity)return;identity=key;const ticket=++revision;update(null);
   if(!metadata||!key)return;
   void resolve(key,metadata,()=>!stopped&&ticket===revision).then(value=>{if(!stopped&&ticket===revision)update(value);}).catch(error=>{if(!stopped&&ticket===revision)update(null,typeof error==='string'?error:'Spotify cover art is unavailable.');});
  },
  stop(){if(stopped)return;stopped=true;++revision;identity=null;}
 };
}
