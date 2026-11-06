<script module lang="ts">
  import type { TransitionConfig } from "svelte/transition";

  /** True when the OS asks for reduced motion (false outside a browser). */
  export function prefersReducedMotion(): boolean {
    return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  }

  const OUT_MS = 140;
  const IN_MS = 220;
  const IN_DELAY_MS = 60;
  const BLUR_PX = 8;
  const SCALE_FROM = 0.96;
  const REDUCED_MS = 120;

  const easeOut = (t: number) => 1 - (1 - t) ** 3;
  const easeIn = (t: number) => t ** 3;
  type Options = boolean | { reduced?: boolean; stationary?: boolean; direction?: number };
  const options = (value: Options) => typeof value === 'boolean' ? { reduced: value, stationary: false, direction: 0 } : value;
  /** Switching island pages: the old page leaves and the new one arrives along the scroll. */
  const PAGE_SHIFT_PX = 18;
  const PAGE_OUT_MS = 200;
  const PAGE_IN_MS = 340;
  const PAGE_IN_DELAY_MS = 50;
  const easeInOut = (t: number) => (t < 0.5 ? 4 * t ** 3 : 1 - (-2 * t + 2) ** 3 / 2);
  function slide(u: number, offset: number): string {
    return `opacity: ${u}; filter: blur(${(1 - u) * 4}px); transform: translateY(${offset.toFixed(2)}px);`;
  }

  /** `u` is 0 when hidden and 1 when fully shown. */
  function look(u: number, stationary = false): string {
    const blur = (1 - u) * BLUR_PX;
    const scale = SCALE_FROM + (1 - SCALE_FROM) * u;
    return `opacity: ${u}; filter: blur(${blur}px); transform: ${stationary ? 'none' : `scale(${scale})`};`;
  }

  function fadeOnly(): TransitionConfig {
    return { duration: REDUCED_MS, easing: easeOut, css: (u) => `opacity: ${u};` };
  }

  /** Incoming content: blur 8 -> 0 px, scale 0.96 -> 1, opacity 0 -> 1 over 220 ms after a 60 ms wait. */
  export function contentIn(_node: Element, value: Options = false): TransitionConfig {
    const { reduced, stationary, direction = 0 } = options(value);
    if (reduced || prefersReducedMotion()) return fadeOnly();
    if (direction) return { delay: PAGE_IN_DELAY_MS, duration: PAGE_IN_MS, easing: easeOut, css: u => slide(u, (1 - u) * PAGE_SHIFT_PX * direction) };
    return { delay: IN_DELAY_MS, duration: IN_MS, easing: easeOut, css: u => look(u, stationary) };
  }

  /** Outgoing content: blur 0 -> 8 px, scale 1 -> 0.96, opacity 1 -> 0 over 140 ms. */
  export function contentOut(_node: Element, value: Options = false): TransitionConfig {
    const { reduced, stationary, direction = 0 } = options(value);
    if (reduced || prefersReducedMotion()) return fadeOnly();
    if (direction) return { duration: PAGE_OUT_MS, easing: easeInOut, css: u => slide(u, -(1 - u) * PAGE_SHIFT_PX * direction) };
    return { duration: OUT_MS, easing: easeIn, css: u => look(u, stationary) };
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  let { key, children, reduced = false, stationary = false, direction = 0 }: { key: unknown; children: Snippet; reduced?: boolean; stationary?: boolean; direction?: number } = $props();
</script>

<!--
  Re-renders `children` whenever `key` changes. The old layer stays (frozen) while it fades out,
  stacked in the same grid cell as the new one.
-->
<div class="stack">
  {#key key}
    <div class="layer island-content" in:contentIn={{reduced,stationary,direction}} out:contentOut={{reduced,stationary,direction}}>
      {@render children()}
    </div>
  {/key}
</div>

<style>
  .stack {
    display: grid;
    width: 100%;
    height: 100%;
  }
  .layer {
    grid-area: 1 / 1;
    min-width: 0;
    min-height: 0;
    transform-origin: 50% 50%;
  }
</style>
