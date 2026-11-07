<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  type App = { key: string; name: string; volume: number; muted: boolean; peak: number; icon?: string };
  type Output = { id: string; name: string; default: boolean };
  type State = { volume: number; muted: boolean; apps: App[]; outputs: Output[] };

  /** Per-app volume and mute, the master volume, and the output device. */
  let mix = $state<State | null>(null);
  let error = $state("");
  let choosing = $state(false);
  /** While a slider is being dragged, polling must not move it back. */
  const held = new Map<string, number>();

  async function refresh(): Promise<void> {
    try {
      const next = await invoke<State>("mixer_state");
      const now = performance.now();
      for (const app of next.apps) if ((held.get(app.key) ?? 0) > now && mix) app.volume = mix.apps.find((a) => a.key === app.key)?.volume ?? app.volume;
      if ((held.get("master") ?? 0) > now && mix) next.volume = mix.volume;
      mix = next; error = "";
    } catch (e) { error = String(e); }
  }

  onMount(() => {
    let alive = true, timer: ReturnType<typeof setTimeout>;
    const loop = async () => { await refresh(); if (alive) timer = setTimeout(loop, 700); };
    void loop();
    return () => { alive = false; clearTimeout(timer); };
  });

  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  /** Volume changes are sent at most every 60 ms while dragging. */
  function setVolume(key: string, volume: number): void {
    held.set(key, performance.now() + 900);
    if (mix) {
      if (key === "master") mix.volume = volume;
      else { const app = mix.apps.find((a) => a.key === key); if (app) app.volume = volume; }
    }
    if (timers.has(key)) return;
    timers.set(key, setTimeout(() => {
      timers.delete(key);
      const current = key === "master" ? mix?.volume : mix?.apps.find((a) => a.key === key)?.volume;
      if (current === undefined) return;
      void (key === "master" ? invoke("mixer_set_master", { volume: current }) : invoke("mixer_set_app", { key, volume: current })).catch((e) => (error = String(e)));
    }, 60));
  }

  async function toggleMute(key: string, muted: boolean): Promise<void> {
    try {
      if (key === "master") await invoke("mixer_set_master", { muted });
      else await invoke("mixer_set_app", { key, muted });
      await refresh();
    } catch (e) { error = String(e); }
  }

  async function chooseOutput(id: string): Promise<void> {
    choosing = false;
    try { await invoke("mixer_set_output", { id }); await refresh(); } catch (e) { error = String(e); }
  }

  const current = $derived(mix?.outputs.find((o) => o.default));
</script>

{#snippet speaker(muted: boolean)}
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M4 9h4l5-4v14l-5-4H4z" fill="currentColor" stroke="none" />
    {#if muted}<path d="m16 9 5 6m0-6-5 6" />{:else}<path d="M16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12" />{/if}
  </svg>
{/snippet}

<div class="mixer">
  {#if !mix}
    <div class="empty">{error || "Reading Windows audio…"}</div>
  {:else}
    <div class="master">
      <button type="button" class="output" aria-haspopup="listbox" aria-expanded={choosing} onclick={() => (choosing = !choosing)} title="Choose where sound plays">
        <span class="caption">Output</span><span class="title">{current?.name ?? "Default output"}</span><span aria-hidden="true" class="chev">▾</span>
      </button>
      <button type="button" class="icon-btn" aria-label={mix.muted ? "Unmute all sound" : "Mute all sound"} aria-pressed={mix.muted} onclick={() => toggleMute("master", !mix!.muted)}>{@render speaker(mix.muted)}</button>
      <input type="range" min="0" max="1" step="0.01" value={mix.volume} aria-label="Master volume" oninput={(e) => setVolume("master", +e.currentTarget.value)} />
      <span class="value tabular">{Math.round(mix.volume * 100)}</span>
    </div>
    {#if choosing}
      <div class="outputs list" role="listbox" aria-label="Sound outputs">
        {#each mix.outputs as output (output.id)}
          <button type="button" role="option" class="row" aria-selected={output.default} onclick={() => chooseOutput(output.id)}>
            <span class="dot" class:on={output.default} aria-hidden="true"></span><span class="title grow">{output.name}</span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="list apps">
        {#each mix.apps as app (app.key)}
          <div class="row app">
            <span class="meter" style:width="{Math.min(100, app.peak * 140 * app.volume)}%" aria-hidden="true"></span>
            {#if app.icon}<img src={app.icon} alt="" width="18" height="18" />{:else}<span class="glyph" aria-hidden="true">♪</span>{/if}
            <span class="title name">{app.name}</span>
            <input type="range" min="0" max="1" step="0.01" value={app.volume} aria-label={`${app.name} volume`} oninput={(e) => setVolume(app.key, +e.currentTarget.value)} />
            <button type="button" class="icon-btn" aria-label={app.muted ? `Unmute ${app.name}` : `Mute ${app.name}`} aria-pressed={app.muted} onclick={() => toggleMute(app.key, !app.muted)}>{@render speaker(app.muted)}</button>
          </div>
        {:else}
          <div class="empty">No app is playing sound right now.</div>
        {/each}
      </div>
    {/if}
    {#if error}<p class="caption error" role="status">{error}</p>{/if}
  {/if}
</div>

<style>
  .mixer { height: 100%; display: grid; grid-template-rows: auto minmax(0, 1fr) auto; gap: 6px; }
  .master { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .output { display: flex; align-items: baseline; gap: 6px; min-width: 0; flex: 1; height: 28px; padding: 0 10px; border: 0; border-radius: var(--radius-pill); background: var(--surface-control); color: inherit; cursor: pointer; text-align: left; }
  .output:hover { background: var(--surface-control-hover); }
  .output .title { flex: 1; }
  .chev { font-size: 10px; opacity: .7; }
  .master input { width: 88px; flex: none; }
  .value { width: 24px; text-align: right; font: var(--type-caption); color: var(--text-secondary); }
  .app { position: relative; overflow: hidden; }
  .meter { position: absolute; left: 0; top: 0; bottom: 0; background: linear-gradient(90deg, #ffffff10, #ffffff1c); border-radius: 10px; pointer-events: none; transition: width 160ms linear; }
  .app > :not(.meter) { position: relative; }
  .app img { flex: none; border-radius: 4px; }
  .glyph { width: 18px; text-align: center; opacity: .7; }
  .name { flex: 1; }
  .app input { width: 110px; flex: none; }
  .outputs .row { width: 100%; border: 0; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .dot { width: 8px; height: 8px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--text-secondary); flex: none; }
  .dot.on { background: #fff; box-shadow: none; }
  .error { color: #ffb4b4; }
</style>
