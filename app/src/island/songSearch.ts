import { invoke } from '@tauri-apps/api/core';

export type SongSource = 'spotify'|'deezer';
export type SongResult = { id:string; uri:string; title:string; artist:string; album:string; artwork:string|null; duration:number; source:SongSource };
/** `played`: Spotify confirmed playback. `opened`: the song was opened in the Spotify app instead.
 * `searched`: a Deezer result couldn't be matched, so the Spotify app's search for it was opened. */
export type PlayOutcome = 'played'|'opened'|'searched';
type NativeCall = (command:string,args?:Record<string,unknown>)=>Promise<unknown>;
const validSpotifyId=(value:unknown):value is string=>typeof value==='string'&&/^[a-zA-Z0-9]{22}$/.test(value);
const validDeezerId=(value:unknown):value is string=>typeof value==='string'&&/^[1-9][0-9]{0,19}$/.test(value);
const deezerCoverHosts=new Set(['cdn-images.dzcdn.net','e-cdns-images.dzcdn.net']);
function identity(row:Record<string,unknown>):SongSource|null {
  if(row.source==='spotify'&&validSpotifyId(row.id)&&row.uri===`spotify:track:${row.id}`)return 'spotify';
  if(row.source==='deezer'&&validDeezerId(row.id)&&row.uri===`deezer:track:${row.id}`)return 'deezer';
  return null;
}
function artwork(value:unknown,source:SongSource):string|null {
  if(typeof value!=='string'||value.length>2048)return null;
  try {
    const url=new URL(value);
    if(url.protocol!=='https:'||url.port||url.username||url.password||url.hash)return null;
    if(source==='spotify')return url.hostname==='i.scdn.co'&&url.pathname.startsWith('/image/')?value:null;
    return deezerCoverHosts.has(url.hostname)&&!url.search&&url.pathname.startsWith('/images/cover/')?value:null;
  }
  catch{return null;}
}
const failure=(error:unknown)=>error instanceof Error?error:new Error(typeof error==='string'?error:'Song search is unavailable. Try again later.');

/** Only deliberate submits call native search. Spotify's catalogue is asked
 * first; only when Spotify's own search can't answer do the search words go to
 * Deezer (and a chosen Deezer song to song.link). Closing/replacing the view
 * aborts promptly and discards any already-running native HTTP reply.
 * Credentials, response bodies and account state remain in the native layer.
 */
export async function searchSongs(query:string,signal:AbortSignal,call:NativeCall=invoke):Promise<SongResult[]> {
  signal.throwIfAborted();
  const term=query.trim();
  if(term.length<2)return [];
  if(term.length>200)throw new Error('Search with up to 200 characters.');
  const data=await new Promise<unknown>((resolve,reject)=>{
    const abort=()=>reject(signal.reason??new DOMException('Search cancelled.','AbortError'));
    signal.addEventListener('abort',abort,{once:true});
    void call('spotify_search_tracks',{query:term}).then(resolve,error=>reject(failure(error)))
      .finally(()=>signal.removeEventListener('abort',abort));
    if(signal.aborted)abort();
  });
  signal.throwIfAborted();
  if(!Array.isArray(data))throw new Error('Song search returned an unreadable response.');
  const seen=new Set<string>();
  return data.slice(0,10).flatMap((row:Record<string,unknown>)=>{
    const source=row?identity(row):null;
    if(!source||seen.has(String(row.uri))||
      typeof row.title!=='string'||!row.title.trim()||typeof row.artist!=='string'||!row.artist.trim()||
      typeof row.album!=='string'||typeof row.duration!=='number'||!Number.isFinite(row.duration)||row.duration<=0)return [];
    seen.add(String(row.uri));
    return [{id:String(row.id),uri:String(row.uri),title:row.title.slice(0,500),artist:row.artist.slice(0,500),
      album:row.album.slice(0,500),artwork:artwork(row.artwork,source),duration:row.duration,source}];
  });
}

export async function playSearchResult(result:SongResult,trackKey:string,call:NativeCall=invoke):Promise<PlayOutcome> {
  if(identity(result)!==result.source||!trackKey||trackKey.length>2000)throw new Error('This song selection is unavailable. Search again.');
  // A Deezer result also carries "artist title" so native code can open the
  // Spotify app's own search if the song can't be matched to a Spotify id.
  const args=result.source==='deezer'?{uri:result.uri,trackKey,search:`${result.artist} ${result.title}`.slice(0,200)}:{uri:result.uri,trackKey};
  let outcome:unknown;
  try{outcome=await call('spotify_play_track',args);}
  catch(error){throw failure(error);}
  if(outcome!=='played'&&outcome!=='opened'&&outcome!=='searched')throw new Error('Spotify did not confirm this selection. Try again.');
  return outcome;
}
