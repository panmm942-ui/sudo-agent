<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";

  type Item = { id: number; path: string; name: string; size: number; folder: boolean; icon?: string; missing: boolean };

  /**
   * File shelf: drop files on the island to park them here, then drag them out to any app
   * (File Explorer, a chat, an email), copy them, or open them.
   */
  let items = $state<Item[]>([]);
  let selected = $state<Set<number>>(new Set());
  let status = $state("");
  let statusTimer: ReturnType<typeof setTimeout> | undefined;

  const refresh = () => invoke<Item[]>("shelf_list").then((list) => (items = list)).catch(() => {});
  onMount(() => {
    void refresh();
    let off: (() => void) | undefined, disposed = false;
    listen("shelf_changed", () => void refresh()).then((u) => (disposed ? u() : (off = u))).catch(() => {});
    return () => { disposed = true; off?.(); clearTimeout(statusTimer); };
  });

  function say(text: string): void { status = text; clearTimeout(statusTimer); statusTimer = setTimeout(() => (status = ""), 2200); }
  const ids = (item: Item) => (selected.has(item.id) ? [...selected] : [item.id]);
  function size(bytes: number): string {
    if (!bytes) return "";
    const units = ["B", "KB", "MB", "GB"];
    let i = 0, n = bytes;
    while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
    return `${n < 10 && i ? n.toFixed(1) : Math.round(n)} ${units[i]}`;
  }

  // Dragging a tile (or the selection) out of the island hands the files to Windows' drag and drop.
  let press: { item: Item; x: number; y: number } | null = null;
  function down(event: PointerEvent, item: Item): void { if (event.button === 0 && !item.missing) press = { item, x: event.clientX, y: event.clientY }; }
  function move(event: PointerEvent): void {
    if (!press || Math.hypot(event.clientX - press.x, event.clientY - press.y) < 5) return;
    const item = press.item;
    press = null;
    void invoke("shelf_drag", { ids: ids(item) }).catch((e) => say(String(e)));
  }
  function select(event: MouseEvent, item: Item): void {
    const next = new Set(event.ctrlKey || event.shiftKey ? selected : []);
    if (next.has(item.id)) next.delete(item.id); else next.add(item.id);
    selected = next;
  }
  async function run(command: string, item: Item, done?: string): Promise<void> {
    try {
      if (command === "shelf_copy") await invoke(command, { ids: ids(item) });
      else await invoke(command, { id: item.id });
      if (done) say(done);
    } catch (e) { say(String(e)); }
  }
  async function remove(item: Item): Promise<void> { await invoke("shelf_remove", { ids: ids(item) }); selected = new Set(); await refresh(); }
  async function clear(): Promise<void> { await invoke("shelf_clear"); selected = new Set(); await refresh(); }
</script>

<svelte:window onpointermove={move} onpointerup={() => (press = null)} />

<div class="shelf">
  {#if !items.length}
    <div class="empty">
      <svg viewBox="0 0 24 24" width="26" height="26" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 4v11m-4-4 4 4 4-4M4 17v2a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-2" /></svg>
      Drop files on the island to keep them here.<br />Drag them out to any app when you need them.
    </div>
  {:else}
    <div class="bar">
      <span class="caption grow">{status || (selected.size > 1 ? `${selected.size} selected · drag, copy or remove them together` : "Drag out · double-click to open · Ctrl+click to select more")}</span>
      <button type="button" class="small-btn" onclick={clear}>Clear</button>
    </div>
    <div class="grid" role="list">
      {#each items as item (item.id)}
        <div class="tile" role="listitem" class:selected={selected.has(item.id)} class:missing={item.missing} title={item.missing ? `${item.path} (moved or deleted)` : item.path}>
          <button type="button" class="face" aria-label={item.name} onpointerdown={(e) => down(e, item)} onclick={(e) => select(e, item)} ondblclick={() => run("shelf_open", item)}>
            {#if item.icon}<img src={item.icon} alt="" width="32" height="32" draggable="false" />{:else}<span class="glyph" aria-hidden="true">{item.folder ? "📁" : "🗎"}</span>{/if}
            <span class="name">{item.name}</span>
            <span class="caption">{item.missing ? "Missing" : item.folder ? "Folder" : size(item.size)}</span>
          </button>
          <div class="actions">
            <button type="button" class="icon-btn" aria-label="Copy" title="Copy (paste in File Explorer, a chat or an email)" onclick={() => run("shelf_copy", item, "Copied, ready to paste")}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" aria-hidden="true"><rect x="8" y="8" width="12" height="12" rx="2" /><path d="M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3" /></svg>
            </button>
            <button type="button" class="icon-btn" aria-label="Show in folder" title="Show in File Explorer" onclick={() => run("shelf_reveal", item)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" aria-hidden="true"><path d="M3 7a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" /></svg>
            </button>
            <button type="button" class="icon-btn" aria-label="Remove from shelf" title="Remove from the shelf (the file stays where it is)" onclick={() => remove(item)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17" /></svg>
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .shelf { height: 100%; display: grid; grid-template-rows: auto minmax(0, 1fr); gap: 6px; }
  .bar { display: flex; align-items: center; gap: 6px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(84px, 1fr)); gap: 4px; overflow-y: auto; scrollbar-width: thin; align-content: start; }
  .tile { position: relative; border-radius: 12px; }
  .tile:hover, .tile.selected { background: #ffffff10; }
  .tile.selected { box-shadow: inset 0 0 0 1px #ffffff40; }
  .tile.missing { opacity: .45; }
  .face { width: 100%; display: grid; justify-items: center; gap: 2px; padding: 8px 4px 6px; border: 0; background: transparent; color: inherit; cursor: grab; text-align: center; }
  .face img { pointer-events: none; }
  .glyph { font-size: 26px; line-height: 32px; }
  .name { max-width: 100%; font: var(--type-caption); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .actions { position: absolute; top: 2px; right: 2px; display: none; gap: 0; background: #1d2026e6; border-radius: 13px; }
  .tile:hover .actions, .tile:focus-within .actions { display: flex; }
  .actions .icon-btn { width: 22px; height: 22px; }
  .actions .icon-btn svg { width: 13px; height: 13px; }
</style>
