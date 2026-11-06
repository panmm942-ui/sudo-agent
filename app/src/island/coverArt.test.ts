import {describe,it,expect,vi,beforeEach} from 'vitest';
import {createCoverArtLookup,validCoverArtUrl,isCoverUpgrade} from './coverArt';
const invoke=vi.hoisted(()=>vi.fn());
vi.mock('@tauri-apps/api/core',()=>({invoke}));
const track={key:'native-key',title:'Song',artist:'Artist',album:'Album',duration:180};
const cover={url:'data:image/png;base64,AQID',sourceUrl:'https://i.scdn.co/image/official-large',width:640,height:640,spotifyUrl:'https://open.spotify.com/track/3OHfY25tqY28d16oZczHc8',trackId:'3OHfY25tqY28d16oZczHc8'};
const flush=()=>new Promise(resolve=>setTimeout(resolve,0));
describe('high resolution Spotify covers',()=>{
 beforeEach(()=>{invoke.mockReset();});
 it('requests once for a song while native polling timestamps change',async()=>{
  invoke.mockResolvedValue(cover);const updates:unknown[]=[];const lookup=createCoverArtLookup(value=>updates.push(value));
  lookup.request(track);await flush();lookup.request({...track});await flush();
  expect(updates.at(-1)).toEqual(cover);expect(invoke).toHaveBeenCalledTimes(1);expect(invoke).toHaveBeenCalledWith('spotify_cover_art',{trackKey:track.key,title:'Song',artist:'Artist',album:'Album',duration:180});lookup.stop();
 });
 it('discards an old song result after a new song request and after stop',async()=>{
  let finish:(value:unknown)=>void=()=>{};invoke.mockImplementationOnce(()=>new Promise(resolve=>finish=resolve)).mockResolvedValueOnce(null);
  const updates:unknown[]=[];const lookup=createCoverArtLookup(value=>updates.push(value));lookup.request({...track,key:'old'});lookup.request({...track,key:'new'});await flush();finish(cover);await flush();expect(updates.at(-1)).toBeNull();lookup.stop();
 });
 it('keeps fallback available on cooldown and does not retry on every observation',async()=>{
  invoke.mockRejectedValue('Spotify rate limited API.');const updates:unknown[]=[];const lookup=createCoverArtLookup(value=>updates.push(value));const key={...track,key:'cooldown'};
  lookup.request(key);await flush();lookup.request(key);await flush();expect(updates.at(-1)).toBeNull();expect(invoke).toHaveBeenCalledTimes(1);lookup.stop();
 });
 it('rejects forged source URLs and image dimensions',async()=>{
  expect(validCoverArtUrl(cover.sourceUrl)).toBe(true);expect(validCoverArtUrl('https://evil.test/image/x')).toBe(false);
  invoke.mockResolvedValue({...cover,url:'https://i.scdn.co.evil.test/image/x'});const updates:unknown[]=[];const lookup=createCoverArtLookup(value=>updates.push(value));lookup.request({...track,key:'forged'});await flush();expect(updates.at(-1)).toBeNull();lookup.stop();
 });
 it('does not replace a sharper Windows image with a smaller provider image',async()=>{
  const measure=async(source:string)=>source==='native'?{width:1000,height:1000}:{width:640,height:640};
  expect(await isCoverUpgrade('native',cover,measure)).toBe(false);
  expect(await isCoverUpgrade('small',cover,async(source)=>source==='small'?{width:300,height:300}:{width:640,height:640})).toBe(true);
  expect(await isCoverUpgrade('native',cover,async(source)=>source==='native'?{width:300,height:300}:null)).toBe(false);
 });
 it('waits for capacity and resolves the latest song without exceeding two requests',async()=>{
  const releases=new Map<string,(value:unknown)=>void>();let active=0,peak=0;
  invoke.mockImplementation((_command,args)=>new Promise(resolve=>{active++;peak=Math.max(peak,active);releases.set(args.trackKey,value=>{active--;resolve(value);});}));
  const updates:unknown[]=[];const lookup=createCoverArtLookup(value=>updates.push(value));
  lookup.request({...track,key:'capacity-a'});lookup.request({...track,key:'capacity-b'});lookup.request({...track,key:'capacity-c'});
  expect(invoke).toHaveBeenCalledTimes(2);releases.get('capacity-a')!(null);await flush();
  expect(releases.has('capacity-c')).toBe(true);expect(invoke).toHaveBeenCalledTimes(3);expect(peak).toBe(2);
  releases.get('capacity-b')!(null);releases.get('capacity-c')!(cover);await flush();expect(updates.at(-1)).toEqual(cover);lookup.stop();
 });
 it('discards superseded and stopped songs while they are waiting for capacity',async()=>{
  const releases=new Map<string,(value:unknown)=>void>();invoke.mockImplementation((_command,args)=>new Promise(resolve=>releases.set(args.trackKey,resolve)));
  const updates:unknown[]=[];const lookup=createCoverArtLookup(value=>updates.push(value));
  lookup.request({...track,key:'queue-a'});lookup.request({...track,key:'queue-b'});lookup.request({...track,key:'queue-c'});lookup.request({...track,key:'queue-d'});
  releases.get('queue-a')!(null);await flush();expect(releases.has('queue-c')).toBe(false);expect(releases.has('queue-d')).toBe(true);
  lookup.request({...track,key:'queue-stopped'});lookup.stop();const count=updates.length;
  releases.get('queue-b')!(null);releases.get('queue-d')!(cover);await flush();expect(releases.has('queue-stopped')).toBe(false);expect(updates).toHaveLength(count);
 });
});
