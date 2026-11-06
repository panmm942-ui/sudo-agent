<script lang="ts">
  import type { Snippet } from "svelte";

  /** Listening (360×84): voice orb on the left, live transcript on the right, newest word fading in. */
  let { transcript = "", orb }: { transcript?: string; orb?: Snippet } = $props();

  const words = $derived(transcript.trim().split(/\s+/).filter(Boolean));
  const head = $derived(words.slice(0, -1).join(" "));
  const last = $derived(words.at(-1) ?? "");

  const MAX_H = 44;
  let textH = $state(0);
</script>

<div class="listening">
  <div class="orb" data-orb-slot>
    {#if orb}{@render orb()}{/if}
  </div>
  <div class="window" class:clipped={textH > MAX_H}>
    <p class="transcript" bind:clientHeight={textH}>
      {#if words.length === 0}
        <span class="hint">Listening</span>
      {:else}
        {#if head}{head}{" "}{/if}{#key words.length}<span class="fresh">{last}</span>{/key}
      {/if}
    </p>
  </div>
</div>

<style>
  .listening {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 100%;
    padding: var(--bezel) 28px var(--bezel) var(--bezel);
  }
  /* 56×56 keeps the orb concentric with the pill's 42 px end radius. */
  .orb {
    flex: none;
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
  }
  /* At most two lines; when longer, the newest text stays visible and older lines fade off the top. */
  .window {
    flex: 1;
    min-width: 0;
    max-height: 44px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    overflow: hidden;
  }
  .window.clipped {
    mask-image: linear-gradient(to bottom, transparent 0, #000 14px);
  }
  .transcript {
    flex: none;
    margin: 0;
    font: var(--type-text);
    letter-spacing: var(--tracking-tight);
    overflow-wrap: anywhere;
  }
  .hint {
    color: var(--text-secondary);
  }
  .fresh {
    animation: word-in var(--word-fade-ms) var(--ease-out) both;
  }
  @keyframes word-in {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fresh {
      animation: none;
    }
  }
</style>
