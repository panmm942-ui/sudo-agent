import { beforeEach, describe, expect, it, vi } from 'vitest';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import { createCanvasLookup, validCanvasUrl, type CanvasTrack } from './canvas';
const track: CanvasTrack = { key: 'one', title: 'One', artist: 'Artist', duration: 180 };
const result = { url: 'https://canvaz.scdn.co/upload/song.cnvs.mp4', trackId: '3OHfY25tqY28d16oZczHc8' };
async function flush() { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); }
describe('optional native Spotify Canvas lookup', () => {
  beforeEach(() => { invoke.mockReset(); invoke.mockResolvedValue(undefined); });
  it('accepts only HTTPS Spotify Canvas MP4 sources', () => {
    expect(validCanvasUrl(result.url)).toBe(true);
    for (const value of ['http://canvaz.scdn.co/a.mp4', 'https://canvaz.scdn.co.evil.test/a.mp4', 'https://user@canvaz.scdn.co/a.mp4', 'https://canvaz.scdn.co:444/a.mp4', 'file:///a.mp4', 'https://canvaz.scdn.co/a.jpg', null]) expect(validCanvasUrl(value)).toBe(false);
  });
  it('does no lookup until a caller requests an active track and deduplicates metadata', async () => {
    invoke.mockImplementation((command: string) => Promise.resolve(command === 'canvas_lookup' ? result : undefined));
    const update = vi.fn(); const lookup = createCanvasLookup(update);
    lookup.request(null); await flush(); expect(invoke).not.toHaveBeenCalled();
    lookup.request(track); await flush(); expect(update).toHaveBeenLastCalledWith(result, undefined, 'complete');
    lookup.request({ ...track }); await flush();
    expect(invoke.mock.calls.filter(([name]) => name === 'canvas_lookup')).toHaveLength(1);
    const payload = invoke.mock.calls.find(([name]) => name === 'canvas_lookup')?.[1];
    expect(payload).toEqual({ track: { ...track, album: null } });
  });
  it('clears the source immediately when inactive and ignores late results', async () => {
    let complete!: (value: typeof result) => void;
    invoke.mockImplementation((command: string) => command === 'canvas_lookup' ? new Promise(resolve => { complete = resolve; }) : Promise.resolve());
    const update = vi.fn(); const lookup = createCanvasLookup(update);
    lookup.request(track); await flush(); lookup.request(null); await flush(); complete(result); await flush();
    expect(update).toHaveBeenLastCalledWith(null, undefined, 'loading');
    expect(update).not.toHaveBeenCalledWith(result, undefined, 'complete');
    expect(invoke.mock.calls.filter(([name]) => name === 'canvas_cancel')).toHaveLength(2);
  });
  it('publishes only the latest track and rejects malformed native replies', async () => {
    const requests: Array<(value: typeof result) => void> = [];
    invoke.mockImplementation((command: string) => command === 'canvas_lookup' ? new Promise(resolve => requests.push(resolve)) : Promise.resolve());
    const update = vi.fn(); const lookup = createCanvasLookup(update);
    lookup.request(track); await flush(); lookup.request({ ...track, key: 'two', title: 'Two' }); await flush();
    requests[0](result); await flush(); expect(update).not.toHaveBeenCalledWith(result, undefined, 'complete');
    requests[1]({ ...result, url: 'https://evil.test/a.mp4' }); await flush();
    expect(update).toHaveBeenLastCalledWith(null, 'Spotify returned an unsupported Canvas video.', 'complete');
  });
  it('stops permanently and handles failures as an empty background', async () => {
    invoke.mockImplementation((command: string) => command === 'canvas_lookup' ? Promise.reject('Spotify rejected this connection.') : Promise.resolve());
    const update = vi.fn(); const lookup = createCanvasLookup(update);
    lookup.request(track); await flush(); expect(update).toHaveBeenLastCalledWith(null, 'Spotify rejected this connection.', 'complete');
    lookup.stop(); const count = invoke.mock.calls.length; lookup.request(track); await flush();
    expect(invoke.mock.calls).toHaveLength(count); expect(update).toHaveBeenLastCalledWith(null, undefined, 'loading');
  });
});
