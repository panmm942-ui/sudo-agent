import { invoke } from '@tauri-apps/api/core';
import type { SpotifyTrack } from './spotify';

export type CanvasTrack = Pick<SpotifyTrack, 'key' | 'title' | 'artist' | 'album' | 'duration'>;
export type CanvasResult = { url: string; trackId: string };
export type CanvasConnection = { stored: boolean; kind: 'cookie' | 'token' | null };

/** Independently check the native result before it can become a media source. */
export function validCanvasUrl(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 2048) return false;
  try {
    const url = new URL(value);
    return url.protocol === 'https:' && url.hostname === 'canvaz.scdn.co' && !url.port &&
      !url.username && !url.password && !url.hash && url.pathname.endsWith('.mp4');
  } catch { return false; }
}

/** Caller supplies a track only while the visible Spotify panel needs video.
 * Cancelling clears the published URL immediately and invalidates late replies.
 * Native cancellation also prevents a pending chain starting its next HTTP call.
 * The optional callback phase distinguishes loading clears from final misses.
 */
export function createCanvasLookup(update: (result: CanvasResult | null, error?: string, phase?: 'loading' | 'complete') => void) {
  let revision = 0;
  let stopped = false;
  let identity: string | null = null;
  return {
    request(track: CanvasTrack | null) {
      if (stopped) return;
      const metadata = track ? { key: track.key, title: track.title, artist: track.artist,
        album: track.album ?? null, duration: track.duration ?? null } : null;
      const next = metadata ? JSON.stringify(metadata) : null;
      if (next === identity) return;
      identity = next;
      const ticket = ++revision;
      update(null, undefined, 'loading');
      void (async () => {
        try {
          await invoke('canvas_cancel');
          if (!metadata || stopped || ticket !== revision) return;
          const result = await invoke<CanvasResult | null>('canvas_lookup', { track: metadata });
          if (stopped || ticket !== revision) return;
          if (result && (!validCanvasUrl(result.url) || !/^[a-zA-Z0-9]{22}$/.test(result.trackId))) {
            update(null, 'Spotify returned an unsupported Canvas video.', 'complete');
          } else update(result, undefined, 'complete');
        } catch (error) {
          if (!stopped && ticket === revision) update(null, typeof error === 'string' ? error : 'Canvas is unavailable. Reconnect in Settings → Spotify.', 'complete');
        }
      })();
    },
    stop() {
      stopped = true;
      identity = null;
      ++revision;
      update(null, undefined, 'loading');
      void invoke('canvas_cancel').catch(() => {});
    },
  };
}
