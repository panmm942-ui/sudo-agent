<script lang="ts">
  import { untrack, type Snippet } from "svelte";
  import { prefersReducedMotion } from "./Crossfade.svelte";

  /** Responding (420×96–180): the reply, revealed word by word at 28 ms/word as it streams in; a small speaking orb leads it. */
  let {
    reply = "",
    onresize,
    orb,
  }: { reply?: string; onresize?: (height: number) => void; orb?: Snippet } = $props();

  const WORD_MS = 28;

  const tokens = $derived(reply.match(/\S+\s*/g) ?? []);
  let shown = $state(0);
  let shownText = "";
  let height = $state(0);
  let boxH = $state(0);

  // A streamed extension keeps the words already revealed; any other change starts over.
  $effect.pre(() => {
    const text = reply;
    if (!text.startsWith(shownText)) shown = 0;
    shownText = text;
  });

  $effect(() => {
    const n = tokens.length;
    if (prefersReducedMotion()) {
      shown = n;
      return;
    }
    if (untrack(() => shown) >= n) return;
    const timer = setInterval(() => {
      shown = Math.min(shown + 1, n);
      if (shown >= n) clearInterval(timer);
    }, WORD_MS);
    return () => clearInterval(timer);
  });

  $effect(() => {
    if (height > 0) onresize?.(height);
  });
</script>

<div class="responding" class:clipped={height > boxH + 1} bind:clientHeight={boxH}>
  <div class="body" class:with-orb={!!orb} bind:clientHeight={height}>
    {#if orb}<div class="orb" data-orb-slot>{@render orb()}</div>{/if}
    <p class="sr-only">{reply}</p>
    <p class="reply" aria-hidden="true">
      {#each tokens.slice(0, shown) as word, i (i)}<span class="w">{word}</span>{/each}
    </p>
  </div>
</div>

<style>
  .responding {
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    height: 100%;
    overflow: hidden;
  }
  /* Past the 180 px cap the newest lines stay in view and older ones fade off the top. */
  .responding.clipped {
    mask-image: linear-gradient(to bottom, transparent 0, #000 28px);
  }
  /* Padding lives on the measured element so onresize reports the full pill height wanted. */
  .body {
    flex: none;
    padding: 18px 28px;
  }
  /* The orb sits in the top-left corner, concentric with the 36 px end radius. */
  .body.with-orb {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding-left: var(--space-4);
  }
  .orb {
    flex: none;
    width: 40px;
    height: 40px;
    margin-top: -9px;
  }
  .reply {
    flex: 1;
    min-width: 0;
    margin: 0;
    font: var(--type-text);
    letter-spacing: var(--tracking-tight);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .w {
    animation: word-in var(--word-fade-ms) var(--ease-out) both;
  }
  @keyframes word-in {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .w {
      animation: none;
    }
  }
</style>
