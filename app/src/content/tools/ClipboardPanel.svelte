<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";

  type Item = { id: number; kind: "text" | "image" | "files"; text?: string; files?: string[]; thumb?: string; width: number; height: number; source?: string; at: number; pinned: boolean };

  /** Clipboard history: click to copy again, Paste to put it straight into the app you were in. */
  let { enabled, onenable, onfocuschange }: { enabled: boolean; onenable: () => void; onfocuschange?: (focused: boolean) => void } = $props();
  let items = $state<Item[]>([]);
  let query = $state("");
  let status = $state("");
  let statusTimer: ReturnType<typeof setTimeout> | undefined;

  const refresh = () => invoke<Item[]>("clipboard_history").then((list) => (items = list)).catch(() => {});
  onMount(() => {
    void refresh();
    let off: (() => void) | undefined, disposed = false;
    listen("clipboard_changed", () => void refresh()).then((u) => (disposed ? u() : (off = u))).catch(() => {});
    return () => { disposed = true; off?.(); clearTimeout(statusTimer); };
  });

  function say(text: string): void { status = text; clearTimeout(statusTimer); statusTimer = setTimeout(() => (status = ""), 1800); }
  const name = (path: string) => path.split(/[\\/]/).pop() || path;
  function preview(item: Item): string {
    if (item.kind === "text") return (item.text ?? "").replace(/\s+/g, " ").trim();
    if (item.kind === "files") return item.files!.length === 1 ? name(item.files![0]) : `${item.files!.length} files: ${item.files!.map(name).join(", ")}`;
    return `Image ${item.width} × ${item.height}`;
  }
  function ago(at: number): string {
    const s = Math.max(0, (Date.now() - at) / 1000);
    return s < 60 ? "now" : s < 3600 ? `${Math.floor(s / 60)} min` : s < 86400 ? `${Math.floor(s / 3600)} h` : new Date(at).toLocaleDateString([], { day: "numeric", month: "short" });
  }
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = q ? items.filter((i) => preview(i).toLowerCase().includes(q) || i.text?.toLowerCase().includes(q)) : items;
    return [...list.filter((i) => i.pinned), ...list.filter((i) => !i.pinned)];
  });

  async function use(item: Item, paste: boolean): Promise<void> {
    try { await invoke("clipboard_use", { id: item.id, paste }); say(paste ? "Pasted" : "Copied"); await refresh(); }
    catch (e) { say(String(e)); }
  }
  async function pin(item: Item): Promise<void> { await invoke("clipboard_pin", { id: item.id, pinned: !item.pinned }); await refresh(); }
  async function remove(item: Item): Promise<void> { await invoke("clipboard_delete", { id: item.id }); await refresh(); }
  async function clear(): Promise<void> { await invoke("clipboard_clear"); await refresh(); say("Cleared (pinned kept)"); }
  onDestroy(() => onfocuschange?.(false));
</script>

<div class="clip">
  {#if !enabled}
    <div class="empty">Clipboard history is off.<button type="button" class="small-btn accent" onclick={onenable}>Turn on</button></div>
  {:else}
    <div class="bar">
      <input class="field" type="search" placeholder="Search clipboard" aria-label="Search clipboard history" bind:value={query}
        onpointerdown={() => onfocuschange?.(true)} onfocus={() => onfocuschange?.(true)} onblur={() => onfocuschange?.(false)} />
      {#if status}<span class="caption status" role="status">{status}</span>{:else if items.length}<button type="button" class="small-btn" onclick={clear}>Clear</button>{/if}
    </div>
    <div class="list">
      {#each shown as item (item.id)}
        <div class="row item">
          <button type="button" class="main" title="Copy" onclick={() => use(item, false)}>
            {#if item.thumb}<img src={item.thumb} alt="" />{:else if item.kind === "files"}<span class="glyph" aria-hidden="true">🗎</span>{/if}
            <span class="grow"><span class="title">{preview(item)}</span><span class="caption">{item.pinned ? "Pinned · " : ""}{ago(item.at)}{item.source ? ` · ${item.source}` : ""}</span></span>
          </button>
          <button type="button" class="small-btn" onclick={() => use(item, true)}>Paste</button>
          <button type="button" class="icon-btn" aria-label={item.pinned ? "Unpin" : "Pin"} aria-pressed={item.pinned} onclick={() => pin(item)}>
            <svg viewBox="0 0 24 24" fill={item.pinned ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" aria-hidden="true"><path d="M9 3h6l-1 6 4 4H6l4-4zM12 13v8" /></svg>
          </button>
          <button type="button" class="icon-btn" aria-label="Delete" onclick={() => remove(item)}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17" /></svg>
          </button>
        </div>
      {:else}
        <div class="empty">{query ? "Nothing matches." : "Copy something and it appears here."}</div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .clip { height: 100%; display: grid; grid-template-rows: auto minmax(0, 1fr); gap: 6px; }
  .bar { display: flex; gap: 6px; align-items: center; }
  .status { flex: none; }
  .item { padding: 2px 4px 2px 2px; }
  .main { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; padding: 3px 6px; border: 0; border-radius: 8px; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .main .grow { display: grid; }
  .main img { width: 34px; height: 26px; object-fit: cover; border-radius: 5px; flex: none; background: #0004; }
  .glyph { width: 22px; text-align: center; opacity: .75; }
  .item .small-btn, .item .icon-btn { opacity: 0; }
  .item:hover .small-btn, .item:hover .icon-btn, .item .icon-btn[aria-pressed="true"], .item :focus-visible { opacity: 1; }
</style>
