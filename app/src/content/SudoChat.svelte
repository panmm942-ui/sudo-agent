<script lang="ts">
  import { onDestroy } from "svelte";

  export type ChatMessage = { role: "user" | "sudo"; text: string; pending?: boolean };
  export type SudoState = "missing" | "off" | "loading" | "ready";
  /** The album art box on the Spotify tab (position, size, corner radius) inside the panel. */
  export type ArtSpot = { x: number; y: number; w: number; h: number; r: number };

  /**
   * The Sudo tab: you type, Sudo answers aloud, and the tab shows only what Sudo is saying right
   * now. The live orb sits exactly where the album art sits on the Spotify tab (same size), so
   * scrolling between them morphs one into the other.
   */
  let {
    messages = [],
    busy = false,
    mode = "missing",
    spot,
    width = 440,
    onsend,
    onorb,
    onfocuschange,
    onsetup,
    onturnon,
  }: {
    messages?: ChatMessage[];
    busy?: boolean;
    mode?: SudoState;
    spot: ArtSpot;
    width?: number;
    onsend: (text: string) => void;
    onorb?: () => void;
    onfocuschange?: (focused: boolean) => void;
    onsetup?: () => void;
    onturnon?: () => void;
  } = $props();

  let value = $state("");
  const ready = $derived(mode === "ready");
  const stacked = $derived(Math.abs(spot.x + spot.w / 2 - width / 2) < 30);
  const saying = $derived([...messages].reverse().find((m) => m.role === "sudo")?.text ?? "");

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const text = value.trim();
    if (!text || busy || !ready) return;
    value = "";
    onsend(text);
  }
  onDestroy(() => onfocuschange?.(false));
</script>

<section class="sudo" class:stacked aria-label="Sudo AI" style:--x="{spot.x}px" style:--y="{spot.y}px" style:--s="{spot.w}px">
  <div class="art" data-morph="art">
    <button type="button" class="orb-button" aria-label="Talk to Sudo AI" onclick={() => onorb?.()}>
      <!-- The WebGL orb is drawn into this slot (1.554 × the box), so the orb disk itself is exactly the art box. -->
      <div class="orb-slot" data-orb-slot></div>
    </button>
  </div>
  <div class="body">
    <div class="line" aria-live="polite">
      {#if mode === "missing"}
        <p class="muted">Sudo AI isn't set up yet. <button type="button" class="link" onclick={() => onsetup?.()}>Set it up</button></p>
      {:else if mode === "off"}
        <p class="muted">Sudo AI is off. <button type="button" class="link" onclick={() => onturnon?.()}>Turn it on</button></p>
      {:else if mode === "loading"}
        <p class="muted">Starting Sudo AI…</p>
      {:else if busy && !saying}
        <p class="muted">Thinking…</p>
      {:else if saying}
        <p class="said">{saying}</p>
      {:else}
        <p class="muted">Type a message — Sudo answers aloud.</p>
      {/if}
    </div>
    <form class="compose" onsubmit={submit}>
      <input aria-label="Message Sudo" placeholder="Message Sudo…" maxlength="4000" autocomplete="off" bind:value disabled={!ready}
        onfocus={() => onfocuschange?.(true)} onblur={() => onfocuschange?.(false)}
        onkeydown={(e) => { if (e.key === "Escape") { value = ""; e.currentTarget.blur(); } }} />
      <button type="submit" aria-label="Send" disabled={!ready || busy || !value.trim()}>
        <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 19V5M6 11l6-6 6 6" /></svg>
      </button>
    </form>
  </div>
</section>

<style>
  .sudo { position: relative; height: 100%; }
  /* Exactly the album art's box on the Spotify tab. */
  .art { position: absolute; left: var(--x); top: var(--y); width: var(--s); height: var(--s); }
  .orb-button { position: absolute; inset: 0; padding: 0; border: 0; border-radius: 50%; background: transparent; cursor: pointer; }
  .orb-button:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .orb-slot { position: absolute; left: 50%; top: 50%; width: calc(var(--s) * 143 / 92); height: calc(var(--s) * 143 / 92); transform: translate(-50%, -50%); pointer-events: none; }
  /* Beside the orb (side layouts) … */
  .body { position: absolute; left: calc(var(--x) + var(--s) + 18px); right: 64px; top: var(--y); height: max(var(--s), 72px); display: flex; flex-direction: column; justify-content: space-between; gap: 6px; min-width: 0; }
  /* … or below it (centered layout). */
  .stacked .body { left: 64px; right: 64px; top: calc(var(--y) + var(--s) + 10px); bottom: 14px; height: auto; align-items: stretch; }
  .line { min-height: 0; overflow: hidden; display: flex; flex-direction: column; justify-content: flex-end; }
  .stacked .line { text-align: center; justify-content: flex-start; }
  p { margin: 0; font: var(--type-caption); line-height: 17px; }
  /* Only the latest words: two lines, the newest visible. */
  .said { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; color: var(--text-primary); }
  .muted { color: var(--text-secondary); }
  .link { padding: 0; border: 0; background: none; color: var(--text-primary); font: inherit; text-decoration: underline; text-underline-offset: 3px; cursor: pointer; }
  .compose { display: flex; gap: 6px; align-items: center; }
  .compose input { flex: 1; min-width: 0; height: 30px; padding: 0 12px; border: 1px solid #ffffff22; border-radius: 15px; background: #ffffff10; color: inherit; font: var(--type-caption); outline: none; transition: background-color 120ms ease, border-color 120ms ease; }
  .compose input::placeholder { color: var(--text-secondary); }
  .compose input:focus { background: #ffffff1a; border-color: #ffffff55; }
  .compose input:disabled { opacity: 0.5; }
  .compose button { flex: none; width: 30px; height: 30px; display: grid; place-items: center; padding: 0; border: 0; border-radius: 50%; background: var(--text-primary); color: #111; cursor: pointer; transition: opacity 120ms ease, transform 120ms ease; }
  .compose button svg { width: 16px; height: 16px; }
  .compose button:disabled { opacity: 0.3; cursor: default; }
  .compose button:not(:disabled):hover { transform: scale(1.06); }
  @media (prefers-reduced-motion: reduce) { .compose input, .compose button { transition: none; } }
</style>
