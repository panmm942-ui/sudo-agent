import { invoke } from "@tauri-apps/api/core";
import {createCoverArtLookup,isCoverUpgrade} from './coverArt';

export type MediaAction = "previous" | "toggle" | "next" | "like" | "repeat" | "shuffle";
/** Transport requests share a lane; independent options must not lock the entire player. */
export function isSpotifyActionPending(action: MediaAction, pending: readonly MediaAction[]): boolean {
  const transport = (value: MediaAction) => value === 'previous' || value === 'toggle' || value === 'next';
  return pending.some(value => value === action || (transport(action) && transport(value)));
}
export type SpotifyTrack = {
  bridgeRevision?:string|null;
  key: string;
  title: string;
  artist: string;
  artwork: string | null;
  playing: boolean;
  canToggle: boolean;
  canPrevious: boolean;
  canNext: boolean;
  liked?: boolean | null;
  likeError?: string | null;
  canLike?: boolean;
  shuffle?: boolean | null;
  repeatMode?: "off" | "context" | "track" | null;
  shuffleError?: string | null;
  repeatError?: string | null;
  canShuffle?: boolean;
  canRepeat?: boolean;
  album?: string;
  duration?: number | null;
  position?: number | null;
  positionUpdatedAt?: number;
  sessionError?: string | null;
};

export type SpotifyFeatures = { like?: boolean; shuffle?: boolean; repeat?: boolean };
export function openSpotify() {
  return invoke<boolean>("spotify_open");
}

export function watchSpotify(update: (track: SpotifyTrack | null) => void, features: () => SpotifyFeatures = () => ({})) {
  let stopped = false;
  let running = false;
  let fastUntil = 0;
  let timer: ReturnType<typeof setTimeout>;
  let last: SpotifyTrack | null = null;
  let highResolution:{identity:string;url:string}|null=null;
  const coverIdentity=(track:SpotifyTrack)=>JSON.stringify([track.key,track.title,track.artist,track.album,track.duration,track.bridgeRevision]);
  const covers=createCoverArtLookup(result=>{
    if(stopped||!last||!result)return;
    const identity=coverIdentity(last),original=last.artwork;
    void isCoverUpgrade(original,result).then(upgrade=>{
      if(!upgrade||stopped||!last||coverIdentity(last)!==identity)return;
      highResolution={identity,url:result.url};
      if(last.artwork!==result.url){last={...last,artwork:result.url};update(last);}
    });
  });
  // Windows only hands over a 300 px cover, which looks soft on 150-200% scaled screens. When
  // Spotify's own large cover isn't available, Apple Music's (600 px) replaces it; a real
  // Spotify 640 px cover is never replaced (the sharper image always wins).
  let appleIdentity:string|null=null;
  const appleCover=(track:SpotifyTrack|null)=>{
    if(!track?.album?.trim()||!track.title?.trim()||!track.artist?.trim())return;
    const identity=coverIdentity(track);
    if(identity===appleIdentity)return;
    appleIdentity=identity;
    void invoke<string|null>('artwork_still_image',{track:{key:track.key,title:track.title,artist:track.artist,album:track.album,duration:track.duration??null}})
      .then(async url=>{
        if(!url||!/^data:image\/(?:jpeg|png);base64,/.test(url)||stopped||!last||coverIdentity(last)!==identity)return;
        const upgrade=await isCoverUpgrade(last.artwork,{url,width:600,height:600,spotifyUrl:'',trackId:'',sourceUrl:''});
        if(!upgrade||stopped||!last||coverIdentity(last)!==identity)return;
        highResolution={identity,url};
        if(last.artwork!==url){last={...last,artwork:url};update(last);}
      }).catch(()=>{});
  };
  let failedReads = 0;
  const read = async () => {
    if (stopped || running) return;
    running = true;
    try {
      const track = await invoke<SpotifyTrack | null>("spotify_state", { knownTrack: last?.artwork ? last.key : null, features: features() });
      if (stopped) return;
      const kind = track === null ? "null" : track instanceof ArrayBuffer ? "ArrayBuffer" : typeof track;
      const hasKey = typeof track?.key === "string";
      if (track !== null && !hasKey) throw new Error(`Spotify returned an unreadable ${kind} response.`);
      failedReads = 0;
      if (track && highResolution?.identity===coverIdentity(track))track.artwork=highResolution.url;
      else if (track && track.key === last?.key) track.artwork ??= last.artwork;
      else highResolution=null;
      if (track?.key !== last?.key || track?.artwork !== last?.artwork || track?.playing !== last?.playing || track?.bridgeRevision!==last?.bridgeRevision ||
          track?.liked !== last?.liked || track?.likeError !== last?.likeError || track?.canLike !== last?.canLike ||
          track?.shuffleError !== last?.shuffleError || track?.repeatError !== last?.repeatError || track?.shuffle !== last?.shuffle || track?.repeatMode !== last?.repeatMode || track?.canShuffle !== last?.canShuffle || track?.canRepeat !== last?.canRepeat ||
          track?.canToggle !== last?.canToggle || track?.canPrevious !== last?.canPrevious || track?.canNext !== last?.canNext || track?.duration !== last?.duration || track?.position !== last?.position || track?.positionUpdatedAt !== last?.positionUpdatedAt || track?.sessionError !== last?.sessionError) {
        last = track;
        update(track);
      }
      covers.request(track);
      appleCover(track);
    } catch (error) {
      if (stopped) return;
      failedReads += 1;
      if (failedReads === 1) console.warn("Spotify state could not be read; retrying.", error);
      // A transient IPC/session failure is not evidence that Spotify closed. Keep the
      // last successful read briefly, but stop presenting stale controls after retries.
      if (failedReads >= 3 && last) {
        last = null;
        highResolution=null;covers.request(null);
        update(null);
      }
    } finally {
      running = false;
      if (!stopped) timer = setTimeout(read, performance.now() < fastUntil ? 160 : 1250);
    }
  };
  void read();
  return {
    refresh() {
      fastUntil = performance.now() + 2200;
      clearTimeout(timer);
      void read();
    },
    stop() { stopped = true; clearTimeout(timer);covers.stop(); },
  };
}

export type Tint = [number, number, number];
/** A quiet graphite material when there is no readable artwork. Never hash titles into random hues. */
export const NEUTRAL_COVER_TINT: Tint = [0.24, 0.25, 0.27];

/** Find the cover's strongest hue, giving saturated accents a voice over dark borders. */
export function coverTint(pixels: Uint8ClampedArray): Tint {
  const buckets = Array.from({ length: 24 }, () => ({ weight: 0, rgb: [0, 0, 0] }));
  let visible = 0;
  let neutralLight = 0;
  for (let i = 0; i < pixels.length; i += 4) {
    if (pixels[i + 3] < 128) continue;
    visible++;
    const [r, g, b] = [pixels[i], pixels[i + 1], pixels[i + 2]].map((v) => v / 255);
    neutralLight += 0.2126 * r + 0.7152 * g + 0.0722 * b;
    const max = Math.max(r, g, b);
    const chroma = max - Math.min(r, g, b);
    if (max < 0.13 || chroma < 0.09) continue;
    const sector = max === r ? (g - b) / chroma : max === g ? 2 + (b - r) / chroma : 4 + (r - g) / chroma;
    const hue = (sector + 6) % 6;
    const bucket = buckets[Math.round(hue * 4) % buckets.length];
    const weight = chroma ** 2;
    bucket.weight += weight;
    [r, g, b].forEach((channel, index) => { bucket.rgb[index] += channel * weight; });
  }
  const best = buckets.reduce((a, b) => a.weight >= b.weight ? a : b);
  if (!best.weight || best.weight < visible * 0.002) {
    if (!visible) return [...NEUTRAL_COVER_TINT];
    const gray = 0.19 + (neutralLight / visible) * 0.12;
    return [gray, gray + 0.008, gray + 0.018];
  }
  const rgb = best.rgb.map((v) => v / best.weight);
  // Preserve hue while bringing bright covers into the dark range of the glass material.
  const brightness = Math.max(...rgb);
  const scale = Math.min(0.56 / brightness, Math.max(0.37 / brightness, 1));
  const linear = rgb.map((channel) => {
    const value = channel * scale;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  const luminance = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
  const veil = Math.min(1, 0.13 / luminance);
  return linear.map((channel) => {
    const value = channel * veil;
    return value <= 0.0031308 ? value * 12.92 : 1.055 * value ** (1 / 2.4) - 0.055;
  }) as Tint;
}

export function tintFromCover(image: HTMLImageElement): Tint {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 48;
  const context = canvas.getContext("2d");
  if (!context) return [...NEUTRAL_COVER_TINT];
  try {
    context.drawImage(image, 0, 0, 48, 48);
    return coverTint(context.getImageData(0, 0, 48, 48).data);
  } catch { return [...NEUTRAL_COVER_TINT]; }
}
