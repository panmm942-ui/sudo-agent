import { invoke } from '@tauri-apps/api/core';
import type { SpotifyTrack } from './spotify';
export type ArtworkTrack = Pick<SpotifyTrack, 'key' | 'title' | 'artist' | 'album' | 'duration'>;
/** `url` is null when Apple has the album but no motion artwork; `image` is a
 * high-resolution still on Apple's image CDN when available. */
export type ArtworkResult = { url: string | null; artist: string; album: string; image?: string | null };
export function validArtworkUrl(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 2048) return false;
  try {
    const url = new URL(value);
    return url.protocol === 'https:' && url.hostname === 'mvod.itunes.apple.com' &&
      !url.port && !url.username && !url.password && !url.hash &&
      url.pathname.startsWith('/itunes-assets/') && url.pathname.endsWith('.m3u8');
  } catch { return false; }
}
/** A large cover from Apple Music's image CDN, or Deezer's (for songs Apple doesn't have). */
export function validArtworkImageUrl(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 2048) return false;
  try {
    const url = new URL(value);
    if (url.protocol !== 'https:' || url.port || url.username || url.password || url.hash || url.search) return false;
    const apple = /^is[1-5]-ssl\.mzstatic\.com$/.test(url.hostname) && url.pathname.startsWith('/image/thumb/');
    const deezer = /^(?:cdn-images|e-cdns-images)\.dzcdn\.net$/.test(url.hostname) && url.pathname.startsWith('/images/cover/');
    return (apple || deezer) && /\.(?:jpe?g|png|webp)$/.test(url.pathname);
  } catch { return false; }
}

/** No network work until a visible player requests an opted-in track.
 * The optional callback phase distinguishes a loading clear from a completed
 * miss, allowing callers to defer fallback until the lookup actually finishes.
 */
export function createArtworkLookup(update: (result: ArtworkResult | null, error?: string, phase?: 'loading' | 'complete') => void) {
  let revision = 0;
  let stopped = false;
  let identity: string | null = null;
  return {
    request(track: ArtworkTrack | null) {
      if (stopped) return;
      const metadata = track?.album?.trim() && track.artist.trim() && track.title.trim() ? {
        key: track.key, title: track.title, artist: track.artist,
        album: track.album, duration: track.duration ?? null,
      } : null;
      const next = metadata ? JSON.stringify(metadata) : null;
      if (next === identity) return;
      identity = next;
      const ticket = ++revision;
      update(null, undefined, 'loading');
      void (async () => {
        try {
          await invoke('artwork_cancel');
          if (!metadata || stopped || ticket !== revision) return;
          const result = await invoke<ArtworkResult | null>('artwork_lookup', { track: metadata });
          if (stopped || ticket !== revision) return;
          if (result && ((result.url !== null && !validArtworkUrl(result.url)) || (result.image != null && !validArtworkImageUrl(result.image)) || typeof result.artist !== 'string' || typeof result.album !== 'string')) {
            update(null, 'Artwork service returned an unsupported video.', 'complete');
          } else update(result, undefined, 'complete');
        } catch (error) {
          if (!stopped && ticket === revision) update(null, typeof error === 'string' ? error : 'Animated artwork is unavailable.', 'complete');
        }
      })();
    },
    stop() {
      stopped = true;
      identity = null;
      ++revision;
      update(null, undefined, 'loading');
      void invoke('artwork_cancel').catch(() => {});
    },
  };
}
