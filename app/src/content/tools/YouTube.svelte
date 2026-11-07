<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { emit, listen } from "@tauri-apps/api/event";
  import { youtubeId, type YoutubeState } from "../../island/youtube";

  type Video = { id: string; title: string; channel: string; duration: string; thumbnail: string };

  /** Search YouTube (or paste a link) and play it in the floating mini-player. */
  let { onfocuschange }: { onfocuschange?: (focused: boolean) => void } = $props();
  let query = $state("");
  let results = $state<Video[]>([]);
  let busy = $state(false);
  let error = $state("");
  let playing = $state<YoutubeState>(null);

  onMount(() => {
    let off: (() => void) | undefined, disposed = false;
    listen<YoutubeState>("youtube:state", (e) => (playing = e.payload)).then((u) => { if (disposed) u(); else { off = u; void emit("youtube:hello"); } }).catch(() => {});
    return () => { disposed = true; off?.(); };
  });

  async function search(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const text = query.trim();
    if (!text || busy) return;
    const id = youtubeId(text);
    if (id) { await open(id); return; }
    busy = true; error = "";
    try { results = await invoke<Video[]>("youtube_search", { query: text }); }
    catch (e) { error = String(e); results = []; }
    finally { busy = false; }
  }
  async function open(id: string): Promise<void> {
    try { await invoke("youtube_open", { id }); error = ""; } catch (e) { error = String(e); }
  }
  const control = (action: string) => void invoke("youtube_control", { action }).catch((e) => (error = String(e)));
  onDestroy(() => onfocuschange?.(false));
</script>

<div class="youtube">
  <form class="bar" onsubmit={search}>
    <input class="field" type="search" placeholder="Search YouTube or paste a link" aria-label="Search YouTube" bind:value={query}
      onpointerdown={() => onfocuschange?.(true)} onfocus={() => onfocuschange?.(true)} onblur={() => onfocuschange?.(false)} />
    <button type="submit" class="small-btn" disabled={busy}>{busy ? "…" : "Go"}</button>
  </form>
  {#if playing}
    <div class="row now">
      <span class="grow"><span class="title">{playing.title || "YouTube"}</span><span class="caption">{playing.author || "Playing in the mini-player"}</span></span>
      <button type="button" class="icon-btn" aria-label={playing.playing ? "Pause" : "Play"} onclick={() => control("toggle")}>
        {#if playing.playing}<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="1" /><rect x="14" y="5" width="4" height="14" rx="1" /></svg>
        {:else}<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7z" /></svg>{/if}
      </button>
      <button type="button" class="icon-btn" aria-label="Close the mini-player" onclick={() => control("close")}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17" /></svg>
      </button>
    </div>
  {/if}
  <div class="list">
    {#each results as video (video.id)}
      <button type="button" class="row video" onclick={() => open(video.id)}>
        <span class="thumb"><img src={video.thumbnail} alt="" loading="lazy" />{#if video.duration}<span class="duration">{video.duration}</span>{/if}</span>
        <span class="grow"><span class="title">{video.title}</span><span class="caption">{video.channel}</span></span>
      </button>
    {:else}
      <div class="empty">{error || "Videos play in a small window that stays on top. Drag it anywhere; resize it from its edges."}</div>
    {/each}
  </div>
  {#if error && results.length}<p class="caption error">{error}</p>{/if}
</div>

<style>
  .youtube { height: 100%; display: grid; grid-template-rows: auto auto minmax(0, 1fr) auto; gap: 6px; }
  .bar { display: flex; gap: 6px; }
  .now { background: #ffffff0d; }
  .now .grow, .video .grow { display: grid; }
  .video { width: 100%; border: 0; background: transparent; color: inherit; text-align: left; cursor: pointer; padding: 3px 6px; }
  .thumb { position: relative; flex: none; width: 64px; height: 36px; border-radius: 6px; overflow: hidden; background: #0005; }
  .thumb img { width: 100%; height: 100%; object-fit: cover; }
  .duration { position: absolute; right: 2px; bottom: 2px; padding: 0 3px; border-radius: 3px; background: #000c; font: 600 9px/13px var(--font-sans); }
  .error { color: #ffb4b4; }
</style>
