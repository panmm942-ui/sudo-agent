<script lang="ts">
  import type { Snippet } from "svelte";

  /** Thinking (360×84): orb slot, a "Thinking" caption with a working pulse, and what was heard. */
  let { transcript = "", orb }: { transcript?: string; orb?: Snippet } = $props();
</script>

<div class="thinking">
  <div class="orb" data-orb-slot>
    {#if orb}{@render orb()}{/if}
  </div>
  <div class="text">
    <p class="caption" role="status">
      Thinking<span class="pulse" aria-hidden="true"><i></i><i></i><i></i></span>
    </p>
    {#if transcript}<p class="heard">{transcript}</p>{/if}
  </div>
</div>

<style>
  .thinking {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 100%;
    padding: var(--bezel) 28px var(--bezel) var(--bezel);
  }
  .orb {
    flex: none;
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  p {
    margin: 0;
  }
  .caption {
    display: flex;
    align-items: baseline;
    font: var(--type-caption);
    color: var(--text-secondary);
  }
  .heard {
    margin-top: 2px;
    font: var(--type-text);
    letter-spacing: var(--tracking-tight);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pulse {
    display: inline-flex;
    gap: 3px;
    margin-left: 5px;
  }
  .pulse i {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: currentColor;
    animation: pulse 1.2s ease-in-out infinite;
  }
  .pulse i:nth-child(2) {
    animation-delay: 0.15s;
  }
  .pulse i:nth-child(3) {
    animation-delay: 0.3s;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 0.25;
    }
    40% {
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse i {
      animation: none;
      opacity: 0.6;
    }
  }
</style>
