export type TimedWord = { time:number; end:number; text:string };
export type TimedLine = { time: number; text: string; words?:TimedWord[] };
/** Optional word timing must describe the exact original text, in order. */
export function timedWords(raw:unknown,text:string,start:number,limit:number):TimedWord[]|undefined {
  if(!Array.isArray(raw)||!raw.length||raw.length>256)return;
  const words:TimedWord[]=[];let previous=start;
  for(const word of raw){
    if(!word||typeof word.text!=='string'||!word.text.length||typeof word.time!=='number'||typeof word.end!=='number'||!Number.isFinite(word.time)||!Number.isFinite(word.end)||word.time<previous||word.end<=word.time||word.end>limit)return;
    words.push({time:word.time,end:word.end,text:word.text});previous=word.end;
  }
  return words.map(word=>word.text).join('')===text?words:undefined;
}
export function parseLrc(lrc: string): TimedLine[] {
  const offset = Number(/\[offset:([+-]?\d+)\]/i.exec(lrc)?.[1] ?? 0) / 1000;
  const result: TimedLine[] = [];
  for (const row of lrc.split(/\r?\n/)) {
    const stamps = [...row.matchAll(/\[(\d+):(\d+(?:\.\d+)?)\]/g)];
    const content = row.replace(/\[[^\]]*\]/g, "").trim();
    const text=content.replace(/<\d+:\d+(?:\.\d+)?>/g,'');
    // LRC's positive metadata offset advances the lyrics (equivalent to
    // adding it to playback), so subtract it from each line's timestamp.
    for (const stamp of stamps) {
      const time=Math.max(0,Number(stamp[1])*60+Number(stamp[2])-offset);
      const chunks=[...content.matchAll(/<(\d+):(\d+(?:\.\d+)?)>([^<]*)/g)];
      const parts=chunks.map((chunk,index)=>({time:Math.max(0,Number(chunk[1])*60+Number(chunk[2])-offset),end:index+1<chunks.length?Math.max(0,Number(chunks[index+1][1])*60+Number(chunks[index+1][2])-offset):NaN,text:chunk[3]}));
      // An explicit trailing timestamp supplies the last sung word's end.
      const words=timedWords(parts.filter(word=>word.text),text,time,86400);
      result.push({time,text,...(words?{words}:{})});
    }
  }
  return result.sort((a, b) => a.time - b.time);
}
export function lyricWindow(lines: TimedLine[], position: number): (TimedLine | undefined)[] {
  let low = 0, high = lines.length;
  while (low < high) { const mid = (low + high) >>> 1; if (lines[mid].time <= position) low = mid + 1; else high = mid; }
  const at = low - 1;
  return [lines[at - 1], lines[at], lines[at + 1]];
}
export function playbackSeconds(track: { position?: number | null; positionUpdatedAt?: number; playing: boolean; duration?: number | null }, now: number): number {
  const position = Math.max(0, (track.position ?? 0) + (track.playing && track.positionUpdatedAt ? Math.max(0, now - track.positionUpdatedAt) / 1000 : 0));
  return track.duration ? Math.min(position, track.duration) : position;
}
/** Split display text into sung tokens; trailing spaces/punctuation stay with
 * the preceding word so the tokens join back to the exact text. */
export function wordTokens(text:string):string[] {
  const tokens:string[]=[];
  const Segmenter=(Intl as unknown as {Segmenter?:new(locale?:string,options?:{granularity:'word'})=>{segment(text:string):Iterable<{segment:string;isWordLike?:boolean}>}}).Segmenter;
  const segments=Segmenter?[...new Segmenter(undefined,{granularity:'word'}).segment(text)]:(text.match(/\s+|\S+/g)??[]).map(segment=>({segment,isWordLike:/\S/.test(segment)}));
  for(const {segment,isWordLike} of segments){
    if(isWordLike||!tokens.length)tokens.push(segment);else tokens[tokens.length-1]+=segment;
  }
  // A leading non-word token (e.g. an opening quote) joins the first real word.
  if(tokens.length>1&&!/[\p{L}\p{N}]/u.test(tokens[0])){tokens[1]=tokens[0]+tokens[1];tokens.shift();}
  return tokens;
}
/** Approximate word timing for line-synced lyrics: spread the sung part of the
 * line across its words, weighted by length, ending before the next line. */
export function estimateWords(text:string,start:number,next?:number):TimedWord[]|undefined {
  const tokens=wordTokens(text);
  if(!tokens.length||!text.trim())return;
  const weights=tokens.map(token=>token.replace(/[^\p{L}\p{N}]/gu,'').length+1.5);
  const total=weights.reduce((a,b)=>a+b,0);
  // ~13 weighted characters per second is a typical sung pace; never run past the next line.
  const natural=Math.max(.8,total/13);
  const gap=next!==undefined&&next>start?next-start:natural+1;
  const span=Math.max(.3,Math.min(natural,gap-.12));
  const words:TimedWord[]=[];let time=start;
  tokens.forEach((token,index)=>{const end=index===tokens.length-1?start+span:time+span*weights[index]/total;words.push({time,end,text:token});time=end;});
  return words;
}
