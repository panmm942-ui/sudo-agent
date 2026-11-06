import { parseLrc, type TimedLine } from "./lyricTiming";
export type LyricsResult = { kind: "lyrics"; text: string; lines?: TimedLine[] } | { kind: "missing" | "instrumental" };
const cache = new Map<string, { result: LyricsResult; expires: number }>();
type RecordMatch={trackName?:string;artistName?:string;albumName?:string;duration?:number;plainLyrics?:string;syncedLyrics?:string;instrumental?:boolean};
export const cleanTitle=(text:string)=>text.replace(/\s*[([](?:feat\.?|ft\.?|featuring|with)\s+[^)\]]*[)\]]/gi,'').replace(/\s+(?:feat\.?|ft\.?|featuring)\s+.+$/i,'').trim();
const firstArtist=(text:string)=>text.split(/,|;|\s+&\s+|\s+(?:feat\.?|ft\.?|featuring|x|×)\s+/i)[0].trim();
const fold=(text:string)=>text.normalize('NFKD').replace(/\p{M}/gu,'').toLowerCase().replace(/[^\p{L}\p{N}]+/gu,' ').trim();
const overlap=(a:string,b:string)=>{
  if(a===b)return 1;
  const aa=new Set(a.split(' ').filter(Boolean)),bb=new Set(b.split(' ').filter(Boolean));
  return [...aa].filter(x=>bb.has(x)).length/Math.max(aa.size,bb.size,1);
};
const version=(title:string,album='')=>[...new Set(`${title} ${album}`.toLowerCase().match(/speed\s*up|sped\s*up|slowed(?:\s*down)?|nightcore|\blive\b|\bremix\b|\bacoustic\b|\binstrumental\b|\bkaraoke\b/g)??[])].map(value=>/speed|sped/.test(value)?'speed up':value).sort().join('|');
const durationMatches=(expected?:number|null,actual?:number)=>!expected||typeof actual==='number'&&Number.isFinite(actual)&&Math.abs(expected-actual)<=2;
const validTiming=(record:RecordMatch,duration?:number|null)=>{
  if(typeof record.syncedLyrics!=='string'||!durationMatches(duration,record.duration))return false;
  const lines=parseLrc(record.syncedLyrics.slice(0,150_000)).filter(line=>line.text);
  const end=duration??record.duration;
  return lines.length>0&&(!end||lines.every(line=>line.time<=end+2));
};

export function stripLrc(value: string): string {
  return value.split(/\r?\n/).filter(line => !/^\[(ar|ti|al|by|offset|length):/i.test(line))
    .map(line => line.replace(/^(\[\d+:\d+(?:\.\d+)?\])+\s*/, "").replace(/<\d+:\d+(?:\.\d+)?>/g,'')).join("\n").trim();
}

export async function fetchLyrics(track: { title: string; artist: string; album?: string; duration?: number | null }, signal: AbortSignal, get: typeof fetch = fetch, force = false): Promise<LyricsResult> {
  signal.throwIfAborted();
  if (!track.title.trim() || !track.artist.trim()) return { kind: "missing" };
  const key = JSON.stringify([track.title, track.artist, track.album, track.duration]);
  const cached = cache.get(key);
  if (!force && cached && cached.expires > Date.now()) return cached.result;
  const query = new URLSearchParams({ track_name: track.title, artist_name: track.artist });
  if (track.album) query.set("album_name", track.album);
  if (track.duration && track.duration >= 1 && track.duration <= 3600) query.set("duration", String(Math.round(track.duration)));
  const compatible=(raw:RecordMatch)=>{
    if(!raw||typeof raw!=='object')return null;
    // Keep readable original text even when a record's version/duration cannot
    // support its timestamps. Never stretch or offset a different recording.
    const sameVersion=version(track.title,track.album)===version(raw.trackName??track.title,raw.albumName);
    return typeof raw.syncedLyrics==='string'&&(!sameVersion||!validTiming(raw,track.duration))?{...raw,plainLyrics:raw.plainLyrics||stripLrc(raw.syncedLyrics),syncedLyrics:undefined}:raw;
  };
  const request = async (params: URLSearchParams):Promise<RecordMatch|null> => {
    const response = await get(`https://lrclib.net/api/get?${params}`, { signal, headers: { "Lrclib-Client": "SudoIsland/0.1.0 (local Windows desktop app)" } });
    if (response.status === 404) return null;
    if (!response.ok) throw new Error("Lyrics service unavailable. Try again shortly.");
    return compatible(await response.json());
  };
  let data: Awaited<ReturnType<typeof request>> = null;
  let failure: unknown;
  try { data = await request(query); }
  catch (error) { if (signal.aborted) throw error; failure = error; }
  // Album releases can have a plain-only record while the same song has a timed single.
  if (!data?.syncedLyrics && !data?.instrumental && (track.album || track.duration)) {
    try {
      const fallback = await request(new URLSearchParams({track_name:track.title,artist_name:track.artist}));
      if (fallback && (fallback.syncedLyrics || !data)) data = fallback;
    } catch (error) { if (signal.aborted) throw error; failure = error; }
  }
  if(!data?.syncedLyrics&&!data?.instrumental){
    const title=cleanTitle(track.title),artist=firstArtist(track.artist);
    if(title!==track.title||artist!==track.artist){
      try{
        const normalized=await request(new URLSearchParams({track_name:title,artist_name:artist}));
        if(normalized&&(normalized.syncedLyrics||!data))data=normalized;
      }catch(error){if(signal.aborted)throw error;failure=error;}
    }
    if(!data?.syncedLyrics){
      const {transliterate}=await import('transliteration');
      const normalize=(text:string)=>fold(transliterate(cleanTitle(text)));
      const titleKey=normalize(title),artistKey=normalize(artist);
      const latinTitle=transliterate(title);
      const queries=[new URLSearchParams({track_name:title,artist_name:artist}),new URLSearchParams({track_name:title}),...(fold(latinTitle)!==fold(title)?[new URLSearchParams({track_name:latinTitle})]:[])];
      for(const params of queries){
        try{
          const response=await get(`https://lrclib.net/api/search?${params}`,{signal,headers:{'Lrclib-Client':'SudoIsland/0.1.0 (local Windows desktop app)'}});
          if(!response.ok){if(response.status!==404)failure=new Error('Lyrics service unavailable. Try again shortly.');continue;}
          const candidates:unknown=await response.json();
          if(!Array.isArray(candidates))continue;
          const matches=candidates.slice(0,100).flatMap((raw:RecordMatch)=>{
            if(!raw||typeof raw.trackName!=='string'||typeof raw.artistName!=='string')return [];
            const t=overlap(titleKey,normalize(raw.trackName));
            const a=normalize(raw.artistName);
            const artistMatch=a===artistKey||a.includes(artistKey)||artistKey.includes(a)||overlap(artistKey,a)>=.5;
            const durationMatch=durationMatches(track.duration,raw.duration);
            if(t<.65||!artistMatch||!durationMatch)return [];
            const record=compatible(raw)!;
            const albumMatch=!!track.album&&!!raw.albumName&&normalize(track.album)===normalize(raw.albumName);
            return [{record,score:t*10+(record.syncedLyrics?5:0)+(albumMatch?3:0)+(typeof raw.duration==='number'&&track.duration?1-Math.min(1,Math.abs(raw.duration-track.duration)/2):0)}];
          }).sort((a,b)=>b.score-a.score);
          if(matches[0]&&(matches[0].record.syncedLyrics||!data))data=matches[0].record;
          if(data?.syncedLyrics)break;
        }catch(error){if(signal.aborted)throw error;failure=error;}
      }
    }
  }
  if (!data && failure) throw failure;
  let result: LyricsResult = { kind: "missing" };
  if (data) {
    const text = (typeof data.plainLyrics === "string" ? data.plainLyrics.trim() : "") || (typeof data.syncedLyrics === "string" ? stripLrc(data.syncedLyrics) : "");
    const lines = typeof data.syncedLyrics === "string" ? parseLrc(data.syncedLyrics.slice(0, 150_000)) : [];
    result = data.instrumental === true ? { kind: "instrumental" } : text ? { kind: "lyrics", text: text.slice(0, 100_000), ...(lines.length ? { lines } : {}) } : { kind: "missing" };
  }
  signal.throwIfAborted();
  if (cache.size >= 30) cache.delete(cache.keys().next().value!);
  cache.set(key, { result, expires: Date.now() + (result.kind === "lyrics" && result.lines?.length ? 900_000 : 30_000) });
  return result;
}
