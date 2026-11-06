import { beforeEach, describe, expect, it, vi } from 'vitest';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import { createVideoLookup, type VideoLookupOptions, type VideoResult } from './videoSource';
import type { CanvasTrack } from './canvas';

const track: CanvasTrack = { key: 'one', title: 'One', artist: 'Artist', album: 'Album', duration: 180 };
const artwork = { url: 'https://mvod.itunes.apple.com/itunes-assets/HLSMusic211/v4/example/default.m3u8', artist: 'Artist', album: 'Album' };
const canvas = { url: 'https://canvaz.scdn.co/upload/song.cnvs.mp4', trackId: '3OHfY25tqY28d16oZczHc8' };
const options: VideoLookupOptions = { source: 'artwork', fallback: true };
async function flush() { for (let index = 0; index < 16; index++) await Promise.resolve(); }
function calls(command: string) { return invoke.mock.calls.filter(([name]) => name === command); }

describe('optional sequential track video sources', () => {
  it('tries artwork after unavailable Canvas and stops after both fail without cycling',async()=>{
    invoke.mockResolvedValue(null);const update=vi.fn(),lookup=createVideoLookup(update);
    lookup.request(track,{source:'canvas',fallback:true});await flush();
    expect(calls('canvas_lookup')).toHaveLength(1);expect(calls('artwork_lookup')).toHaveLength(1);
    lookup.request(track,{source:'canvas',fallback:true});lookup.playbackFailed();await flush();
    expect(calls('canvas_lookup')).toHaveLength(1);expect(calls('artwork_lookup')).toHaveLength(1);
    expect(update.mock.lastCall?.[1]).toContain('Animated album artwork');
  });
  it('pauses visibility without clearing or re-querying an already resolved current-song video',async()=>{
    const update=vi.fn(),lookup=createVideoLookup(update);
    lookup.request(track,options);await flush();const current=update.mock.lastCall?.[0];update.mockClear();
    for(let i=0;i<3;i++){lookup.request(track,{...options,active:false});await flush();lookup.request(track,{...options,active:true});await flush();}
    expect(update).not.toHaveBeenCalled();expect(calls('artwork_lookup')).toHaveLength(1);
    lookup.request({...track,key:'next'},{...options,active:false});await flush();expect(update).toHaveBeenLastCalledWith(null);expect(calls('artwork_lookup')).toHaveLength(1);
    lookup.request({...track,key:'next'},options);await flush();expect(calls('artwork_lookup')).toHaveLength(2);expect(update.mock.lastCall?.[0]?.generation).not.toBe(current.generation);
  });
  it('does not start provider requests while inactive and cancels unfinished work on close',async()=>{
    let resolve!:(value:typeof artwork)=>void;
    invoke.mockImplementation(command=>command==='artwork_lookup'?new Promise(done=>resolve=done):Promise.resolve());
    const update=vi.fn(),lookup=createVideoLookup(update);lookup.request(track,{...options,active:false});await flush();expect(calls('artwork_lookup')).toHaveLength(0);
    lookup.request(track,options);await flush();lookup.request(track,{...options,active:false});resolve(artwork);await flush();expect(update).not.toHaveBeenCalledWith(expect.objectContaining({url:artwork.url}));
    lookup.request(track,options);await flush();expect(calls('artwork_lookup')).toHaveLength(2);
  });
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation((command: string) => Promise.resolve(command === 'artwork_lookup' ? artwork : command === 'canvas_lookup' ? canvas : undefined));
  });
  it('stays idle when inactive and never requests Canvas when artwork succeeds', async () => {
    const update = vi.fn(); const lookup = createVideoLookup(update);
    lookup.request(null, options); await flush(); expect(invoke).not.toHaveBeenCalled();
    lookup.request(track, options); await flush();
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ url: artwork.url, source: 'artwork' }));
    expect(calls('artwork_lookup')).toHaveLength(1); expect(calls('canvas_lookup')).toHaveLength(0);
  });
  it('waits for completed artwork before using opted-in Canvas fallback', async () => {
    let resolve!: (value: null) => void;
    invoke.mockImplementation((command: string) => command === 'artwork_lookup' ? new Promise(done => { resolve = done; }) : Promise.resolve(command === 'canvas_lookup' ? canvas : undefined));
    const update = vi.fn(); const lookup = createVideoLookup(update); lookup.request(track, options); await flush();
    expect(calls('canvas_lookup')).toHaveLength(0); expect(update).toHaveBeenLastCalledWith(null);
    resolve(null); await flush(); expect(calls('canvas_lookup')).toHaveLength(1);
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ url: canvas.url, source: 'canvas' }));
  });
  it('uses still artwork when fallback is off or Canvas is unavailable, with no retry loop', async () => {
    invoke.mockImplementation((command: string) => Promise.resolve(command.endsWith('_lookup') ? null : undefined));
    const update = vi.fn(); const lookup = createVideoLookup(update); lookup.request(track, { ...options, fallback: false }); await flush();
    expect(calls('canvas_lookup')).toHaveLength(0); expect(update.mock.lastCall?.[0]).toBe(null);
    lookup.request(track, options); await flush();
    expect(calls('canvas_lookup')).toHaveLength(1); expect(update).toHaveBeenLastCalledWith(null, expect.stringContaining('Canvas'));
    lookup.playbackFailed(); lookup.request(track, options); await flush(); expect(calls('canvas_lookup')).toHaveLength(1);
  });
  it('treats an Apple still-only result (no motion artwork) as a video miss', async () => {
    const still = { ...artwork, url: null, image: 'https://is1-ssl.mzstatic.com/image/thumb/Music/a.jpg/1200x1200bb.jpg' };
    invoke.mockImplementation((command: string) => Promise.resolve(command === 'artwork_lookup' ? still : command === 'canvas_lookup' ? canvas : undefined));
    const update = vi.fn(); const lookup = createVideoLookup(update); lookup.request(track, options); await flush();
    expect(update).not.toHaveBeenCalledWith(expect.objectContaining({ source: 'artwork' }));
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ url: canvas.url, source: 'canvas' }));
  });
  it('falls back after an artwork service error or missing album metadata', async () => {
    invoke.mockImplementation((command: string) => command === 'artwork_lookup' ? Promise.reject('Service unavailable') : Promise.resolve(command === 'canvas_lookup' ? canvas : undefined));
    const update = vi.fn(); const lookup = createVideoLookup(update); lookup.request(track, options); await flush();
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ url: canvas.url, source: 'canvas' }));
    lookup.request({ ...track, key: 'two', album: undefined }, options); await flush();
    expect(calls('artwork_lookup')).toHaveLength(1); expect(calls('canvas_lookup')).toHaveLength(2);
  });
  it('advances on actual playback failure once and ignores duplicate errors during fallback loading', async () => {
    let resolve!: (value: typeof canvas) => void;
    invoke.mockImplementation((command: string) => command === 'canvas_lookup' ? new Promise(done => { resolve = done; }) : Promise.resolve(command === 'artwork_lookup' ? artwork : undefined));
    const update = vi.fn(); const lookup = createVideoLookup(update); lookup.request(track, options); await flush();
    lookup.playbackFailed(); lookup.playbackFailed(); await flush(); expect(calls('canvas_lookup')).toHaveLength(1);
    resolve(canvas); await flush(); expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ url: canvas.url, source: 'canvas' }));
    lookup.playbackFailed(); await flush(); expect(update).toHaveBeenLastCalledWith(null, expect.stringContaining('Spotify Canvas playback failed.'));
    expect(calls('artwork_lookup')).toHaveLength(1); expect(calls('canvas_lookup')).toHaveLength(1);
  });
  it('deduplicates position/playback changes but refreshes on metadata or explicit revision changes', async () => {
    const lookup = createVideoLookup(vi.fn()); lookup.request(track, options); await flush();
    lookup.request({ ...track, position: 20, playing: false } as CanvasTrack, options); await flush();
    expect(calls('artwork_lookup')).toHaveLength(1);
    lookup.request({ ...track, album: 'Corrected album' }, options); await flush(); expect(calls('artwork_lookup')).toHaveLength(2);
    lookup.request({ ...track, album: 'Corrected album' }, { ...options, revision: 1 }); await flush(); expect(calls('artwork_lookup')).toHaveLength(3);
  });
  it('keeps ready video mounted when the same song temporarily loses its bridge snapshot', async () => {
    const update = vi.fn(); const lookup = createVideoLookup(update);
    const connected = { ...options, bridgeRevision: 'same-song-media' };
    lookup.request(track, connected); await flush();
    const ready = update.mock.lastCall?.[0];
    update.mockClear();
    for (let poll = 0; poll < 3; poll++) {
      lookup.request(track, { ...options, bridgeRevision: null }); await flush();
      lookup.request(track, connected); await flush();
    }
    expect(update).not.toHaveBeenCalled();
    expect(calls('artwork_lookup')).toHaveLength(1);
    // New bridge media still refreshes the current song.
    lookup.request(track, { ...options, bridgeRevision: 'updated-song-media' }); await flush();
    expect(calls('artwork_lookup')).toHaveLength(2);
    expect(update.mock.lastCall?.[0]?.generation).not.toBe(ready.generation);
  });
  it('drops retained bridge media on song changes and hide, then accepts a fresh snapshot', async () => {
    const update = vi.fn(); const lookup = createVideoLookup(update);
    lookup.request(track, { ...options, bridgeRevision: 'first-song-media' }); await flush();
    update.mockClear();
    const next = { ...track, key: 'next-song' };
    lookup.request(next, { ...options, bridgeRevision: null });
    expect(update).toHaveBeenLastCalledWith(null);
    await flush();
    lookup.request(next, { ...options, bridgeRevision: 'next-song-media' }); await flush();
    expect(calls('artwork_lookup')).toHaveLength(3);
    lookup.request(null, options);
    expect(update).toHaveBeenLastCalledWith(null);
    lookup.request(next, { ...options, bridgeRevision: null }); await flush();
    lookup.request(next, { ...options, bridgeRevision: 'next-song-media' }); await flush();
    expect(calls('artwork_lookup')).toHaveLength(5);
  });
  it('rejects old-track results, cancels both sources on hide, and cannot publish after stop', async () => {
    const requests: Array<(value: typeof artwork) => void> = [];
    invoke.mockImplementation((command: string) => command === 'artwork_lookup' ? new Promise(done => requests.push(done)) : Promise.resolve());
    const update = vi.fn(); const lookup = createVideoLookup(update); lookup.request(track, options); await flush();
    lookup.request({ ...track, key: 'two' }, options); await flush(); requests[0](artwork); await flush();
    expect(update).not.toHaveBeenCalledWith(expect.objectContaining({ url: artwork.url, source: 'artwork' }));
    lookup.request(null, options); requests[1](artwork); await flush(); expect(update).toHaveBeenLastCalledWith(null);
    lookup.request(track, options); await flush(); lookup.stop(); requests[2](artwork); await flush();
    expect(update).toHaveBeenLastCalledWith(null); const count = invoke.mock.calls.length; lookup.request(track, options); lookup.playbackFailed(); await flush(); expect(invoke.mock.calls).toHaveLength(count);
  });
  it('supports a directly selected Canvas source without starting artwork', async () => {
    const update = vi.fn(); const lookup = createVideoLookup(update); lookup.request(track, { source: 'canvas', fallback: true }); await flush();
    expect(calls('artwork_lookup')).toHaveLength(0); expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ url: canvas.url, source: 'canvas' }));
    lookup.playbackFailed(); await flush(); expect(calls('artwork_lookup')).toHaveLength(1); expect(calls('canvas_lookup')).toHaveLength(1);
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({source:'artwork'}));
    lookup.playbackFailed(); await flush(); expect(calls('canvas_lookup')).toHaveLength(1);
    lookup.request(track,{source:'canvas',fallback:true});await flush();
    expect(calls('canvas_lookup')).toHaveLength(1);expect(calls('artwork_lookup')).toHaveLength(1);
  });
  it('cancels both stages after fallback and ignores a late Canvas response when hidden', async () => {
    let resolve!: (value: typeof canvas) => void;
    invoke.mockImplementation((command: string) => command === 'canvas_lookup' ? new Promise(done => { resolve = done; }) : Promise.resolve(command === 'artwork_lookup' ? null : undefined));
    const update = vi.fn(); const lookup = createVideoLookup(update); lookup.request(track, options); await flush();
    const artworkCancels = calls('artwork_cancel').length; const canvasCancels = calls('canvas_cancel').length;
    lookup.request(null, options); await flush();
    expect(calls('artwork_cancel')).toHaveLength(artworkCancels + 1); expect(calls('canvas_cancel')).toHaveLength(canvasCancels + 1);
    resolve(canvas); await flush(); expect(update).toHaveBeenLastCalledWith(null);
    expect(update).not.toHaveBeenCalledWith(expect.objectContaining({ url: canvas.url, source: 'canvas' }));
  });
  it('keeps both provider failure reasons when opted-in fallback also fails', async () => {
    invoke.mockImplementation((command: string) => command === 'artwork_lookup' ? Promise.reject('Artwork service is rate limited.') : command === 'canvas_lookup' ? Promise.reject('Spotify rejected this connection.') : Promise.resolve());
    const update=vi.fn(); const lookup=createVideoLookup(update); lookup.request(track,options); await flush();
    expect(update.mock.lastCall?.[0]).toBeNull();
    expect(update.mock.lastCall?.[1]).toContain('Artwork service is rate limited.');
    expect(update.mock.lastCall?.[1]).toContain('Spotify rejected this connection.');
  });
  it('explains that fallback is off without attempting the saved Canvas connection', async () => {
    invoke.mockResolvedValue(null); const update=vi.fn(); const lookup=createVideoLookup(update);
    lookup.request(track,{...options,fallback:false}); await flush();
    expect(calls('canvas_lookup')).toHaveLength(0);
    expect(update.mock.lastCall?.[1]).toMatch(/fallback is off/i);
  });
  it('does not interpret a full Spotify music video as proof that a Canvas loop exists', async () => {
    invoke.mockResolvedValue(null); const update=vi.fn(); const lookup=createVideoLookup(update);
    lookup.request(track,{source:'canvas',fallback:false}); await flush();
    expect(update.mock.lastCall?.[0]).toBeNull();
    expect(update.mock.lastCall?.[1]).toMatch(/full.*music video.*separate/i);
  });
  it('ignores a late artwork decoder error after Canvas has already become ready', async () => {
    const update=vi.fn(); const lookup=createVideoLookup(update); lookup.request(track,options); await flush();
    const old=update.mock.lastCall?.[0] as VideoResult;
    lookup.playbackFailed(old); await flush();
    const current=update.mock.lastCall?.[0]; expect(current?.source).toBe('canvas');
    lookup.playbackFailed(old); await flush();
    expect(update.mock.lastCall?.[0]).toBe(current);
    expect(calls('canvas_lookup')).toHaveLength(1);
  });
  it('ignores old-track decoder failures even when two tracks resolve to the same artwork URL', async () => {
    const update=vi.fn(); const lookup=createVideoLookup(update); lookup.request(track,options); await flush();
    const old=update.mock.lastCall?.[0] as VideoResult;
    lookup.request({...track,key:'next'},options); await flush(); const current=update.mock.lastCall?.[0];
    lookup.playbackFailed(old); await flush();
    expect(update.mock.lastCall?.[0]).toBe(current); expect(calls('canvas_lookup')).toHaveLength(0);
  });
});
