import {invoke} from '@tauri-apps/api/core';
import {fetchLyrics,type LyricsResult} from './lyrics';
import type {SpotifyTrack} from './spotify';
import {parseLrc,timedWords,type TimedLine} from './lyricTiming';
import {importedLyrics} from './importedLyrics';
type Track=Pick<SpotifyTrack,'key'|'title'|'artist'|'album'|'duration'>;
type Options={provider?:(track:Track,signal:AbortSignal,get?:typeof fetch,force?:boolean)=>Promise<LyricsResult>;local?:(track:Track)=>Promise<unknown>;extra?:(track:Track)=>Promise<string|null>;imported?:(track:Track)=>string|null;now?:()=>number};
/** An imported file: timed LRC, or plain text shown as unsynced lyrics. */
export function importedLyricsResult(text:string|null|undefined):LyricsResult|null{
 if(typeof text!=='string'||!text.trim())return null;
 const timed=extraLyricsResult(text);
 if(timed)return timed;
 const plain=text.split(/\r?\n/).map(line=>line.replace(/^\[[^\]]*\]/,'').trim()).join('\n').trim();
 return plain?{kind:'lyrics',text:plain}:null;
}
/** Timed lyrics from the optional second source, when it matched this exact song. */
export function extraLyricsResult(lrc:string|null|undefined):LyricsResult|null{
 if(typeof lrc!=='string'||!lrc.trim())return null;
 const lines=parseLrc(lrc.slice(0,150_000));
 if(lines.filter(line=>line.text.trim()).length<3)return null;
 return {kind:'lyrics',text:lines.map(line=>line.text).join('\n'),lines};
}
export function localLyricsResult(snapshot:unknown,track:Track,now=Date.now()):LyricsResult|null{
 if(!snapshot||typeof snapshot!=='object'||Array.isArray(snapshot)||typeof track.duration!=='number'||!Number.isFinite(track.duration)||track.duration<=0)return null;
 const data=snapshot as Record<string,unknown>;
 const uri=typeof data.uri==='string'?/^spotify:track:([A-Za-z0-9]{22})$/.exec(data.uri):null;
 const normalize=(text:string)=>text.toLowerCase().replace(/[^\p{L}\p{N}]/gu,'');
 const artistMatches=typeof data.artist==='string'&&track.artist.split(/[,;]/).every(part=>!!normalize(part)&&(data.artist as string).split(/[,;]/).some(credit=>normalize(credit)===normalize(part)));
 if(!uri||data.trackId!==uri[1]||data.title!==track.title||!artistMatches||data.album!==track.album||typeof data.duration!=='number'||!Number.isFinite(data.duration)||Math.abs(data.duration-track.duration)>2||typeof data.positionUpdatedAt!=='number'||!Number.isFinite(data.positionUpdatedAt)||Math.abs(now-data.positionUpdatedAt)>5000)return null;
 if(!Array.isArray(data.lyrics)||!data.lyrics.length||data.lyrics.length>1000)return null;
 const lines:TimedLine[]=[];
 let previous=-1;
 for(const line of data.lyrics){
  if(!line||typeof line!=='object'||typeof line.time!=='number'||!Number.isFinite(line.time)||line.time<0||line.time<previous||line.time>track.duration+2||typeof line.text!=='string'||line.text.length>1000)return null;
  const words=timedWords(line.words,line.text,line.time,track.duration+2);
  previous=line.time;lines.push({time:line.time,text:line.text,...(words?{words}:{})});
 }
 if(!lines.some(line=>line.text.trim()))return null;
 return {kind:'lyrics',text:lines.map(line=>line.text).join('\n'),lines};
}

/** Read local Spotify timing while the reader is open. LRCLIB is requested
 * once per song; a late matching local result upgrades it without clearing
 * the reader. Independent request sessions prevent stale source publication. */
export function createLyricsSource(update:(result:LyricsResult|null,error?:string)=>void,options:Options={}){
 const provider=options.provider??fetchLyrics;
 const local=options.local??((track:Track)=>invoke<unknown>('spotify_bridge_snapshot',{trackKey:track.key,title:track.title,artist:track.artist,album:track.album??null,duration:track.duration??null}));
 const now=options.now??Date.now;
 // Second sources, in order: NetEase, then Kugou and QQ Music (more_lyrics).
 const extra=options.extra??(async(track:Track)=>{
  if(typeof track.duration!=='number')return null;
  const args={title:track.title,artist:track.artist,duration:track.duration};
  return (await invoke<string|null>('netease_lyrics',args).catch(()=>null))??(await invoke<string|null>('more_lyrics',args).catch(()=>null));
 });
 const imported=options.imported??((track:Track)=>{try{return importedLyrics(track.key);}catch{return null;}});
 let identity:string|null=null,stopped=false;
 let cancel:(()=>void)|undefined;
 return {
  request(track:Track,force=false){
   if(stopped)return;
   const metadata={key:track.key,title:track.title,artist:track.artist,album:track.album,duration:track.duration};
   const key=JSON.stringify(metadata),changed=key!==identity;
   if(!changed&&!force)return;
   cancel?.();identity=key;
   let disposed=false,localWon=false;
   let pollTimer:ReturnType<typeof setTimeout>|undefined;
   const controller=new AbortController();
   const timeout=setTimeout(()=>controller.abort(),12_000);
   cancel=()=>{disposed=true;controller.abort();clearTimeout(timeout);clearTimeout(pollTimer);};
   if(changed)update(null);
   // Lyrics the user imported for this song win over every source.
   const own=importedLyricsResult(imported(metadata));
   if(own){clearTimeout(timeout);update(own);return;}
   // When the main source has no timing, a second source may still have it.
   const fallback=async()=>{try{return extraLyricsResult(await extra(metadata));}catch{return null;}};
   void provider(metadata,controller.signal,fetch,force).then(async value=>{
    if(disposed||localWon)return;
    const timed=value.kind==='lyrics'&&!!value.lines?.length;
    if(!timed&&value.kind!=='instrumental'){
     if(value.kind==='lyrics')update(value);
     const upgrade=await fallback();
     if(disposed||localWon)return;
     if(upgrade){update(upgrade);return;}
    }
    update(value);
   }).catch(async()=>{
    if(disposed||localWon)return;
    const upgrade=await fallback();
    if(!disposed&&!localWon)update(upgrade,upgrade?undefined:'Lyrics couldn’t be loaded. Try again.');
   }).finally(()=>clearTimeout(timeout));
   const poll=async()=>{
    try{
     const snapshot=await local(metadata);
     if(disposed||localWon)return;
     const result=localLyricsResult(snapshot,metadata,now());
     if(result){localWon=true;clearTimeout(timeout);controller.abort();update(result);return;}
    }catch{/* An optional bridge never replaces readable provider lyrics with an error. */}
    if(!disposed&&!localWon)pollTimer=setTimeout(()=>void poll(),1000);
   };
   void poll();
  },
  stop(){if(stopped)return;stopped=true;cancel?.();cancel=undefined;identity=null;},
 };
}
