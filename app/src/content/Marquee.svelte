<script lang="ts">
  import type { Snippet } from "svelte";

  /**
   * One line of text that, when it does not fit, pauses, then glides right to left and loops
   * seamlessly (two copies with a gap). Text that fits stays still. Off when `enabled` is false
   * or the system asks for reduced motion.
   */
  let { text, enabled = true, speed = 32, children }: { text: string; enabled?: boolean; speed?: number; children?: Snippet } = $props();

  const GAP = 36; // px between the end of the text and its next copy
  let box = $state<HTMLSpanElement>();
  let inner = $state<HTMLSpanElement>();
  let overflow = $state(false);
  let width = $state(0);

  $effect(() => {
    void text;
    const outer = box, content = inner;
    if (!outer || !content) return;
    const measure = () => {
      width = content.scrollWidth;
      overflow = width > outer.clientWidth + 1;
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(outer);
    observer.observe(content);
    return () => observer.disconnect();
  });

  const moving = $derived(enabled && overflow && width > 0);
  // Travel = one text width plus the gap. Each cycle rests 15 % of its time (keyframes below),
  // so the moving 85 % keeps the requested speed.
  const travel = $derived(width + GAP);
  const duration = $derived(Math.max(4, travel / speed / 0.85));
</script>

<span class="marquee" class:moving bind:this={box}>
  <span class="track" style:--travel="{-travel}px" style:--duration="{duration}s">
    <span class="copy" bind:this={inner}>{#if children}{@render children()}{:else}{text}{/if}</span>
    {#if moving}<span class="copy" aria-hidden="true" style:padding-left="{GAP}px">{#if children}{@render children()}{:else}{text}{/if}</span>{/if}
  </span>
</span>

<style>
  .marquee { display: block; min-width: 0; overflow: hidden; white-space: nowrap; }
  .track { display: inline-flex; }
  .copy { flex: none; white-space: nowrap; }
  .marquee:not(.moving) .copy { overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  .marquee:not(.moving) .track { display: block; }
  .moving { mask-image: linear-gradient(to right, transparent 0, #000 10px, #000 calc(100% - 18px), transparent 100%); }
  .moving .track { animation: glide var(--duration) linear infinite; will-change: transform; }
  @keyframes glide {
    0%, 15% { transform: translateX(0); }
    100% { transform: translateX(var(--travel)); }
  }
  @media (prefers-reduced-motion: reduce) { .moving .track { animation: none; } }
</style>
