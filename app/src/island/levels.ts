import { Channel, invoke } from "@tauri-apps/api/core";
import type { SpotifySpectrum } from './waveform';

/** On-demand process loopback: PCM stays in bounded native memory; only seven band levels reach the UI. */
export function watchSpotifyLevels(update: (level: SpotifySpectrum | null) => void): () => void {
  let stopped = false;
  let id: number | undefined;
  const channel = new Channel<SpotifySpectrum | null>(level => { if (!stopped) update(level); });
  void invoke<number>("start_spotify_meter", { channel }).then(value => {
    id = value;
    if (stopped) void invoke("stop_spotify_meter", { id }).catch(() => {});
  }).catch(() => { if (!stopped) update(null); });
  return () => { stopped = true; if (id !== undefined) void invoke("stop_spotify_meter", { id }).catch(() => {}); };
}
