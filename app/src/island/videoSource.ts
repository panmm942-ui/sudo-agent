import { createArtworkLookup } from './artwork';
import { createCanvasLookup, type CanvasTrack } from './canvas';

export type VideoSource = 'artwork' | 'canvas';
export type VideoResult = { url: string; source: VideoSource; generation: number };
export type VideoLookupOptions = { source: VideoSource; fallback: boolean; revision?: number;bridgeRevision?:string|null;active?:boolean };

/** Selected source -> optional other source -> still artwork.
 * Lookups and late playback failures belong to one metadata generation. The
 * caller supplies null when disabled or no song exists; inactive retains a
 * ready current-song result. Still artwork stays visible until video loads.
 */
export function createVideoLookup(update: (result: VideoResult | null, error?: string) => void) {
  let stopped = false;
  let identity: string | null = null;
  let metadataIdentity: string | null = null;
  let bridgeRevision: string | null = null;
  let generation = 0;
  let track: CanvasTrack | null = null;
  let options: VideoLookupOptions = { source: 'artwork', fallback: false };
  let active: VideoSource | null = null;
  let ready = false;
  let published: VideoResult | null = null;
  let requested=false;
  let failures: string[] = [];
  const attempted = new Set<VideoSource>();
  let artwork: ReturnType<typeof createArtworkLookup> | undefined;
  let canvas: ReturnType<typeof createCanvasLookup> | undefined;

  function cancel() {
    // Invalidate callbacks before stop() publishes its loading clear.
    ++generation;
    active = null;
    ready = false;
    published = null;
    requested=false;
    failures = [];
    attempted.clear();
    artwork?.stop(); artwork = undefined;
    canvas?.stop(); canvas = undefined;
  }

  function completeEmpty(source: VideoSource, ticket: number, error?: string) {
    if (stopped || ticket !== generation || active !== source) return;
    ready = false;
    published = null;
    const reason = error ?? (source === 'artwork'
      ? 'Animated album artwork is unavailable for this track.'
      : 'Spotify Canvas is unavailable for this track. Full Spotify music videos are separate from Canvas loops.');
    failures.push(reason);
    const other:VideoSource=source==='artwork'?'canvas':'artwork';
    if (options.fallback && !attempted.has(other) && options.active!==false) {
      begin(other, ticket);
    } else {
      active = null;
      requested = true;
      update(null, `${failures.join(' ')}${options.fallback?'':' Other-source fallback is off in Settings → Spotify.'}`);
    }
  }

  function begin(source: VideoSource, ticket: number) {
    if (!track || stopped || ticket !== generation) return;
    requested=true;
    attempted.add(source);
    active = source;
    ready = false;
    if (source === 'artwork') {
      // The artwork service needs an album; its low-level lookup intentionally
      // stays idle on incomplete metadata, so finish this stage explicitly.
      if (!track.album?.trim() || !track.artist.trim() || !track.title.trim()) {
        completeEmpty(source, ticket, 'Album metadata is required for animated artwork.');
        return;
      }
      artwork = createArtworkLookup((result, error, phase) => {
        if (stopped || ticket !== generation || active !== source || phase !== 'complete') return;
        // A still-only Apple result (no motion artwork) is a video miss.
        if (result?.url) { ready = true; published = { url: result.url, source, generation: ticket }; update(published); }
        else completeEmpty(source, ticket, error);
      });
      artwork.request(track);
    } else {
      canvas = createCanvasLookup((result, error, phase) => {
        if (stopped || ticket !== generation || active !== source || phase !== 'complete') return;
        if (result) { ready = true; published = { url: result.url, source, generation: ticket }; update(published); }
        else completeEmpty(source, ticket, error);
      });
      canvas.request(track);
    }
  }

  return {
    request(nextTrack: CanvasTrack | null, nextOptions: VideoLookupOptions) {
      if (stopped) return;
      const metadata = nextTrack ? {
        key: nextTrack.key, title: nextTrack.title, artist: nextTrack.artist,
        album: nextTrack.album ?? null, duration: nextTrack.duration ?? null,
      } : null;
      const metadataKey = metadata ? JSON.stringify(metadata) : null;
      if (metadataKey !== metadataIdentity) {
        metadataIdentity = metadataKey;
        bridgeRevision = null;
      }
      // Bridge freshness describes the live clock, not the lifetime of a
      // validated media URL. An expired snapshot must not unload the same
      // song's video. A new snapshot can still refresh media, and metadata
      // changes/disable discard the remembered revision before any next lookup.
      if (metadata && nextOptions.bridgeRevision != null) bridgeRevision = nextOptions.bridgeRevision;
      const next = metadata ? JSON.stringify([metadata, nextOptions.source, nextOptions.fallback, nextOptions.revision ?? 0,bridgeRevision]) : null;
      if(next===identity){
        options={...nextOptions};
        if(nextOptions.active===false){if(requested&&!ready){cancel();update(null);}return;}
        if(ready||requested||!next)return;
      }
      identity = next;
      cancel();
      track = nextTrack ? { key: nextTrack.key, title: nextTrack.title, artist: nextTrack.artist, album: nextTrack.album, duration: nextTrack.duration } : null;
      options = { ...nextOptions };
      const ticket = generation;
      update(null);
      if (track && ticket === generation&&options.active!==false) begin(options.source, ticket);
    },
    playbackFailed(expected?: VideoResult) {
      if (stopped || !active || !ready) return;
      // Callers capture the source record when binding a media element. A late
      // decoder error must not clear a ready fallback or a new track that uses
      // the very same album-artwork URL. Property comparison also works with
      // Svelte's state proxy around the published record.
      if (expected && (!published || expected.generation !== published.generation ||
        expected.url !== published.url || expected.source !== published.source)) return;
      const source = active;
      const ticket = generation;
      // Clear the failed media before a Canvas lookup begins. A second failure
      // from that discarded element cannot advance the chain again.
      active = null;
      ready = false;
      requested=false;
      update(null);
      if (stopped || ticket !== generation) return;
      active = source;
      completeEmpty(source, ticket, source === 'artwork' ? 'Animated artwork playback failed.' : 'Spotify Canvas playback failed.');
    },
    stop() {
      if (stopped) return;
      stopped = true;
      identity = null;
      metadataIdentity = null;
      bridgeRevision = null;
      track = null;
      cancel();
      update(null);
    },
  };
}
