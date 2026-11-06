import type Hls from 'hls.js';
import { validArtworkUrl } from './artwork';
import { selectVideoLevel, type VideoQuality } from './videoQuality';

/** The caller owns play/pause and loadeddata. Disposing always stops all loading.
 * This loads an ordinary HLS player on demand, with demuxing off the UI thread.
 */
export function attachArtworkVideo(video: HTMLVideoElement, url: string, onerror: () => void = () => {}, quality: VideoQuality = 'best'): () => void {
  let disposed = false;
  let failed = false;
  let player: Hls | undefined;
  function clear() {
    player?.destroy(); player = undefined;
    video.pause(); video.removeAttribute('src'); video.load();
  }
  function fail() {
    if (disposed || failed) return;
    failed = true; clear(); onerror();
  }
  if (!validArtworkUrl(url)) { queueMicrotask(fail); }
  else void import('hls.js').then(({ default: Hls }) => {
    if (disposed || failed) return;
    // Chromium can advertise native HLS yet stall on Apple's master playlist.
    // Prefer the MSE implementation; native HLS remains a Safari fallback.
    if (!Hls.isSupported()) {
      if (video.canPlayType('application/vnd.apple.mpegurl')) { video.src = url; video.load(); }
      else fail();
      return;
    }
    const bounded = { default: { maxTimeToFirstByteMs: 6000, maxLoadTimeMs: 8000, timeoutRetry: null, errorRetry: null } };
    player = new Hls({
      enableWorker: true,
      autoStartLoad: false,
      capLevelToPlayerSize: false,
      maxBufferLength: 12,
      maxMaxBufferLength: 12,
      maxBufferSize: 8_000_000,
      backBufferLength: 0,
      manifestLoadPolicy: bounded,
      playlistLoadPolicy: bounded,
      fragLoadPolicy: bounded,
      keyLoadPolicy: bounded,
      xhrSetup(xhr, requestUrl) {
        const source = new URL(requestUrl);
        if (source.protocol !== 'https:' || source.hostname !== 'mvod.itunes.apple.com' || source.port || source.username || source.password || !source.pathname.startsWith('/itunes-assets/')) throw new Error('Untrusted artwork media source.');
        xhr.withCredentials = false;
      },
    });
    player.on(Hls.Events.ERROR, (_event, data) => { if (data.fatal) fail(); });
    player.on(Hls.Events.MANIFEST_PARSED, () => {
      if (!player) return;
      // HLS has already filtered unsupported codecs. Start with the best playable
      // source, rather than stretching a low-resolution rendition across the card.
      const best = selectVideoLevel(player.levels, quality);
      if(best < 0){fail();return;}
      player.startLevel = best;
      player.loadLevel = best;
      player.startLoad();
    });
    player.attachMedia(video);
    player.loadSource(url);
  }).catch(fail);
  return () => { if (!disposed) { disposed = true; clear(); } };
}
