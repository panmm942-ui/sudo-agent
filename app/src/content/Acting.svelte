<script lang="ts">
  import type { ActionPayload } from "./index";

  /** Acting (380×72): what sudo is doing, plus step dots when the action has a known length. */
  let { action }: { action: ActionPayload } = $props();

  const total = $derived(action.total && action.total > 0 ? Math.min(Math.floor(action.total), 12) : 0);
  const step = $derived(total ? Math.min(Math.max(Math.floor(action.step ?? 1), 1), total) : 0);
</script>

<div class="acting">
  <p class="label">{action.label}</p>
  {#if total}
    <div class="steps" role="img" aria-label={`Step ${step} of ${total}`}>
      {#each { length: total } as _, i (i)}
        <span class="dot" class:done={i + 1 < step} class:current={i + 1 === step}></span>
      {/each}
    </div>
  {:else}
    <span class="busy" role="img" aria-label="Working"></span>
  {/if}
</div>

<style>
  .acting {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: 100%;
    padding: 0 28px;
  }
  .label {
    flex: 1;
    min-width: 0;
    margin: 0;
    font: var(--type-text);
    letter-spacing: var(--tracking-tight);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .steps {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  /* Done = small solid, current = wider accent capsule, pending = dim: shape carries state, not hue alone. */
  .dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
    background: var(--indicator-idle);
    transition:
      width 240ms var(--ease-out),
      background-color 240ms var(--ease-out);
  }
  .dot.done {
    background: var(--text-primary);
  }
  .dot.current {
    width: 16px;
    background: var(--accent);
  }
  .busy {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    animation: breathe 1.4s ease-in-out infinite;
  }
  @keyframes breathe {
    50% {
      opacity: 0.35;
      transform: scale(0.8);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .dot {
      transition: background-color 120ms ease-out;
    }
    .busy {
      animation: none;
    }
  }
</style>
