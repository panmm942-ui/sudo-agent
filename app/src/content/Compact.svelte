<script lang="ts">
  /** Resting pill (220×36): one line of "up next". Calendar data arrives in a later phase. */
  let { label = "Nothing up next", time }: { label?: string; time?: string } = $props();
</script>

<div class="compact">
  <span class="dot" aria-hidden="true"></span>
  <span class="label">{label}</span>
  {#if time}<span class="time tabular">{time}</span>{/if}
</div>

<style>
  .compact {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 100%;
    padding: 0 18px;
    font: var(--type-label);
    white-space: nowrap;
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-secondary);
  }
  .time {
    flex: none;
    color: var(--text-primary);
  }
</style>
