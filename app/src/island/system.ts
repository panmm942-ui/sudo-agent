import { invoke } from "@tauri-apps/api/core";

/** CPU / memory / GPU load from system_stats (sysmon.rs). */
export type SystemStats = { cpu: number | null; memory: number; memoryUsedGb: number; memoryTotalGb: number; gpu: number | null };

const listeners = new Set<(stats: SystemStats) => void>();
let timer: ReturnType<typeof setTimeout> | undefined;
let latest: SystemStats | null = null;

async function poll(): Promise<void> {
  try {
    latest = await invoke<SystemStats>("system_stats");
    for (const listener of listeners) listener(latest);
  } catch { /* outside Tauri or Windows: no stats */ }
  if (listeners.size) timer = setTimeout(poll, 1500);
}

/** Shared 1.5 s poll while anything shows the stats; returns the unsubscribe. */
export function watchSystem(listener: (stats: SystemStats) => void): () => void {
  listeners.add(listener);
  if (latest) listener(latest);
  if (listeners.size === 1) { clearTimeout(timer); void poll(); }
  return () => { listeners.delete(listener); if (!listeners.size) clearTimeout(timer); };
}

export const percent = (value: number | null | undefined) => (value === null || value === undefined ? "–" : `${Math.round(value)}%`);

/** "CPU 12% · RAM 48% · GPU 5%" for the small island. */
export function systemLine(stats: SystemStats | null): string {
  if (!stats) return "CPU – · RAM –";
  return `CPU ${percent(stats.cpu)} · RAM ${percent(stats.memory)}${stats.gpu === null ? "" : ` · GPU ${percent(stats.gpu)}`}`;
}
