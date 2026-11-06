import { beforeEach, describe, expect, it, vi } from 'vitest';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import { createArtworkLookup, validArtworkImageUrl, validArtworkUrl, type ArtworkTrack } from './artwork';

const track: ArtworkTrack = { key: 'one', title: 'On Melancholy Hill', artist: 'Gorillaz', album: 'Plastic Beach' };
const result = { url: 'https://mvod.itunes.apple.com/itunes-assets/HLSMusic211/v4/example/default.m3u8', artist: 'Gorillaz', album: 'Plastic Beach' };
async function flush() { for (let i = 0; i < 6; i++) await Promise.resolve(); }

describe('optional animated album artwork', () => {
  beforeEach(() => { invoke.mockReset(); invoke.mockResolvedValue(undefined); });
  it('allows only public Apple HLS artwork sources', () => {
    expect(validArtworkUrl(result.url)).toBe(true);
    for (const value of ['http://mvod.itunes.apple.com/itunes-assets/a.m3u8', 'https://mvod.itunes.apple.com.evil.test/itunes-assets/a.m3u8', 'https://user@mvod.itunes.apple.com/itunes-assets/a.m3u8', 'https://mvod.itunes.apple.com:444/itunes-assets/a.m3u8', 'https://mvod.itunes.apple.com/itunes-assets/a.mp4', 'https://mvod.itunes.apple.com/elsewhere/a.m3u8', 'file:///a.m3u8', null]) expect(validArtworkUrl(value)).toBe(false);
  });
  it('stays idle until requested with album metadata and deduplicates a track', async () => {
    invoke.mockImplementation((command: string) => Promise.resolve(command === 'artwork_lookup' ? result : undefined));
    const update = vi.fn(); const lookup = createArtworkLookup(update);
    lookup.request(null); lookup.request({ ...track, album: '' }); await flush(); expect(invoke).not.toHaveBeenCalled();
    lookup.request(track); await flush(); expect(update).toHaveBeenLastCalledWith(result, undefined, 'complete');
    lookup.request({ ...track }); await flush();
    expect(invoke.mock.calls.filter(([command]) => command === 'artwork_lookup')).toHaveLength(1);
    expect(invoke.mock.calls.find(([command]) => command === 'artwork_lookup')?.[1]).toEqual({ track: { ...track, duration: null } });
  });
  it('invalidates a late response when hidden or the track changes', async () => {
    const requests: Array<(value: typeof result) => void> = [];
    invoke.mockImplementation((command: string) => command === 'artwork_lookup' ? new Promise(resolve => requests.push(resolve)) : Promise.resolve());
    const update = vi.fn(); const lookup = createArtworkLookup(update);
    lookup.request(track); await flush(); lookup.request({ ...track, key: 'two' }); await flush();
    requests[0](result); await flush(); expect(update).not.toHaveBeenCalledWith(result, undefined, 'complete');
    lookup.request(null); requests[1](result); await flush(); expect(update).toHaveBeenLastCalledWith(null, undefined, 'loading');
  });
  it('rejects malformed native results without showing an untrusted video', async () => {
    invoke.mockImplementation((command: string) => Promise.resolve(command === 'artwork_lookup' ? { ...result, url: 'https://evil.test/a.m3u8' } : undefined));
    const update = vi.fn(); const lookup = createArtworkLookup(update);
    lookup.request(track); await flush(); expect(update.mock.lastCall?.[0]).toBe(null); expect(update.mock.lastCall?.[1]).toMatch(/unsupported/i);
  });
  it('accepts Apple still-only results and rejects untrusted still images', async () => {
    const image = 'https://is1-ssl.mzstatic.com/image/thumb/Music122/v4/67/b5/01/x/cover.rgb.jpg/1200x1200bb.jpg';
    expect(validArtworkImageUrl(image)).toBe(true);
    // Deezer covers, for songs Apple Music doesn't have.
    expect(validArtworkImageUrl('https://cdn-images.dzcdn.net/images/cover/6ca14bc28f28b8a8ef8bca77069e4bbb/1000x1000-000000-80-0-0.jpg')).toBe(true);
    for (const value of ['https://cdn-images.dzcdn.net.evil.test/images/cover/a.jpg', 'https://cdn-images.dzcdn.net/images/artist/a.jpg', 'http://cdn-images.dzcdn.net/images/cover/a.jpg']) expect(validArtworkImageUrl(value)).toBe(false);
    for (const value of ['https://is6-ssl.mzstatic.com/image/thumb/a.jpg/1200x1200bb.jpg', 'http://is1-ssl.mzstatic.com/image/thumb/a.jpg/1200x1200bb.jpg', 'https://evil.test/image/thumb/a.jpg', `${image}?x=1`, null]) expect(validArtworkImageUrl(value)).toBe(false);
    const still = { ...result, url: null, image };
    invoke.mockImplementation((command: string) => Promise.resolve(command === 'artwork_lookup' ? still : undefined));
    const update = vi.fn(); const lookup = createArtworkLookup(update);
    lookup.request(track); await flush(); expect(update).toHaveBeenLastCalledWith(still, undefined, 'complete');
    invoke.mockImplementation((command: string) => Promise.resolve(command === 'artwork_lookup' ? { ...result, image: 'https://evil.test/a.jpg' } : undefined));
    lookup.request({ ...track, key: 'two' }); await flush(); expect(update.mock.lastCall?.[0]).toBe(null); expect(update.mock.lastCall?.[1]).toMatch(/unsupported/i);
  });
  it('reports availability failures and permanently releases after stop', async () => {
    invoke.mockImplementation((command: string) => command === 'artwork_lookup' ? Promise.reject('Artwork service is unavailable.') : Promise.resolve());
    const update = vi.fn(); const lookup = createArtworkLookup(update);
    lookup.request(track); await flush(); expect(update).toHaveBeenLastCalledWith(null, 'Artwork service is unavailable.', 'complete');
    lookup.stop(); const count = invoke.mock.calls.length; lookup.request(track); await flush(); expect(invoke.mock.calls.length).toBe(count); expect(update).toHaveBeenLastCalledWith(null, undefined, 'loading');
  });
});
