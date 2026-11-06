import { describe, expect, it, vi } from "vitest";
import { fetchLyrics, stripLrc } from "./lyrics";
describe("lyrics lookup", () => {
  it('does not borrow timing from a release five seconds longer',async()=>{
    const get=vi.fn(async(input:RequestInfo|URL)=>{
      const url=new URL(String(input));
      return {ok:true,status:200,json:async()=>url.pathname.endsWith('/search')?[]:{trackName:'Duration guarded',artistName:'Artist',albumName:url.searchParams.has('album_name')?'Album':'Single',duration:url.searchParams.has('album_name')?220:225,plainLyrics:'Original verse',syncedLyrics:url.searchParams.has('album_name')?null:'[00:03]Original verse'}} as Response;
    });
    expect(await fetchLyrics({title:'Duration guarded',artist:'Artist',album:'Album',duration:220},new AbortController().signal,get)).toEqual({kind:'lyrics',text:'Original verse'});
  });
  it('keeps readable originals when an exact-title fallback has an incompatible duration',async()=>{
    const get=vi.fn(async(input:RequestInfo|URL)=>{
      const url=new URL(String(input));
      if(url.searchParams.has('album_name'))return {ok:false,status:404} as Response;
      return {ok:true,status:200,json:async()=>url.pathname.endsWith('/search')?[]:{trackName:'Plain fallback guarded',artistName:'Artist',duration:225,syncedLyrics:'[00:03]Original fallback verse'}} as Response;
    });
    expect(await fetchLyrics({title:'Plain fallback guarded',artist:'Artist',album:'Album',duration:220},new AbortController().signal,get)).toEqual({kind:'lyrics',text:'Original fallback verse'});
  });
  it('rejects speed-up timing and prefers the requested album over search ordering',async()=>{
    const get=vi.fn(async(input:RequestInfo|URL)=>({ok:new URL(String(input)).pathname.endsWith('/search'),status:new URL(String(input)).pathname.endsWith('/search')?200:404,json:async()=>[
      {trackName:'Search guarded',artistName:'Artist',albumName:'Search guarded (Speed Up)',duration:223,plainLyrics:'Speed version',syncedLyrics:'[00:01]Speed version'},
      {trackName:'Search guarded',artistName:'Artist',albumName:'Compilation',duration:223,plainLyrics:'Compilation verse',syncedLyrics:'[00:02]Compilation verse'},
      {trackName:'Search guarded',artistName:'Artist',albumName:'Requested album',duration:223,plainLyrics:'Album verse',syncedLyrics:'[00:03]Album verse'},
    ]}) as Response);
    expect(await fetchLyrics({title:'Search guarded',artist:'Artist',album:'Requested album',duration:223},new AbortController().signal,get)).toMatchObject({kind:'lyrics',lines:[{time:3,text:'Album verse'}]});
  });
  it('retains original lyrics when provider timestamps exceed the recording duration',async()=>{
    const get=vi.fn(async(input:RequestInfo|URL)=>({ok:true,status:200,json:async()=>new URL(String(input)).pathname.endsWith('/search')?[]:{trackName:'Broken timing',artistName:'Artist',duration:215,plainLyrics:'Original verse',syncedLyrics:'[00:30]Original verse\n[03:42]Last verse'}}) as Response);
    expect(await fetchLyrics({title:'Broken timing',artist:'Artist',duration:215},new AbortController().signal,get)).toEqual({kind:'lyrics',text:'Original verse'});
  });
  it('does not reject usable timing because an empty end marker is beyond the recording',async()=>{
    const get=vi.fn(async()=>({ok:true,status:200,json:async()=>({trackName:'Empty end marker',artistName:'Artist',duration:215,plainLyrics:'Original verse',syncedLyrics:'[00:30]Original verse\n[03:42]'})}) as Response);
    expect(await fetchLyrics({title:'Empty end marker',artist:'Artist',duration:215},new AbortController().signal,get)).toMatchObject({kind:'lyrics',lines:[{time:30,text:'Original verse'},{time:222,text:''}]});
  });
  it('does not serve a cached result after its reader was cancelled',async()=>{
    const get=vi.fn(async()=>({ok:true,status:200,json:async()=>({plainLyrics:'Cached original'})}) as Response);
    const track={title:'Cancellation guarded',artist:'Artist'};
    await fetchLyrics(track,new AbortController().signal,get);
    const controller=new AbortController();controller.abort();
    await expect(fetchLyrics(track,controller.signal,get)).rejects.toMatchObject({name:'AbortError'});
  });
  it('recovers Greek-script titles indexed in Latin and preserves available plain lyrics',async()=>{
    const get=vi.fn(async(input:RequestInfo|URL)=>{
      const url=new URL(String(input));
      if(url.pathname.endsWith('/get'))return {ok:false,status:404} as Response;
      const records=url.searchParams.get('track_name')==='To Dentro'?[{trackName:'To Dentro',artistName:'Fivos Delivorias',duration:247,plainLyrics:'Δοκιμαστικό κείμενο'}]:[];
      return {ok:true,status:200,json:async()=>records} as Response;
    });
    expect(await fetchLyrics({title:'Το Δέντρο',artist:'Fivos Delivorias',duration:247},new AbortController().signal,get)).toMatchObject({kind:'lyrics',text:'Δοκιμαστικό κείμενο'});
  });
  it('finds a Greek title through search after metadata misses and rejects unrelated timed results',async()=>{
    const get=vi.fn().mockResolvedValueOnce({ok:false,status:404}).mockResolvedValueOnce({ok:false,status:404})
      .mockResolvedValueOnce({ok:false,status:404}).mockResolvedValue({ok:true,status:200,json:async()=>[
        {trackName:'Αλλο τραγούδι',artistName:'Other',duration:200,syncedLyrics:'[00:01]Wrong'},
        {trackName:'Το Δέντρο',artistName:'Γιώργος Νταλάρας',duration:201,plainLyrics:'Test verse',syncedLyrics:'[00:01]Test verse'},
      ]});
    expect(await fetchLyrics({title:'Το Δέντρο (feat. Guest)',artist:'Γιώργος Νταλάρας, Guest',duration:201},new AbortController().signal,get)).toMatchObject({kind:'lyrics',lines:[{time:1,text:'Test verse'}]});
  });
  it("recovers from an exact-query service error with an available timed title match", async () => {
    const get=vi.fn().mockResolvedValueOnce({ok:false,status:500})
      .mockResolvedValueOnce({ok:true,status:200,json:async()=>({plainLyrics:"Recovered sample",syncedLyrics:"[00:01]Recovered sample",duration:235})});
    expect(await fetchLyrics({title:"Recovery test",artist:"Artist",album:"Specific release",duration:235},new AbortController().signal,get)).toMatchObject({kind:"lyrics",lines:[{time:1,text:"Recovered sample"}]});
  });
  it("finds timed High Class lyrics when the album-specific version is plain only", async () => {
    const get=vi.fn().mockResolvedValueOnce({ok:true,status:200,json:async()=>({plainLyrics:"Sample plain version",syncedLyrics:null})})
      .mockResolvedValueOnce({ok:true,status:200,json:async()=>({trackName:'High Class',artistName:'Wang, NIGHT GRIND',duration:235,plainLyrics:"Sample timed version",syncedLyrics:"[00:01]Sample timed version"})});
    const result=await fetchLyrics({title:"High Class",artist:"Wang, NIGHT GRIND",album:"Version",duration:235},new AbortController().signal,get);
    expect(result).toMatchObject({kind:"lyrics",lines:[{time:1,text:"Sample timed version"}]});
    expect(String(get.mock.calls[1][0])).not.toContain('album_name');
  });
  it("looks up metadata on demand and returns plain text, never markup", async () => {
    const get=vi.fn().mockResolvedValue({ok:true,status:200,json:async()=>({plainLyrics:"Test verse\nSecond line",instrumental:false})});
    expect(await fetchLyrics({title:"Example",artist:"Artist"},new AbortController().signal,get)).toEqual({kind:"lyrics",text:"Test verse\nSecond line"});
    expect(String(get.mock.calls[0][0])).toContain('track_name=Example');
  });
  it("handles missing and instrumental tracks, and strips timestamp metadata", async () => {
    const get=vi.fn().mockResolvedValue({ok:false,status:404});
    expect(await fetchLyrics({title:"None",artist:"Artist"},new AbortController().signal,get)).toEqual({kind:"missing"});
    get.mockResolvedValue({ok:true,status:200,json:async()=>({instrumental:true})});
    expect(await fetchLyrics({title:"Instrumental",artist:"Artist"},new AbortController().signal,get)).toEqual({kind:"instrumental"});
    expect(stripLrc('[ar:Artist]\n[00:01.23]First\n[01:02.00][01:04.00]Second')).toBe('First\nSecond');
  });
});
