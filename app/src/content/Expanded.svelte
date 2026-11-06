<script lang="ts">
  import { onMount } from "svelte";

  /** Expanded (420×190, on hover): the clock and an "up next" list. Calendar data arrives in a later phase. */
  let {
    items = [],
    now,
    chrome = false,
  }: {
    items?: { label: string; time: string }[];
    /** Fixed time (goldens); omitted = live clock, updated on the minute. */
    now?: Date;
    chrome?: boolean;
  } = $props();

  const hhmm = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });
  const day = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" });

  let live = $state(new Date());
  const at = $derived(now ?? live);

  onMount(() => {
    if (now) return;
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      live = new Date();
      timer = setTimeout(tick, 60_000 - (live.getTime() % 60_000) + 50);
    };
    tick();
    return () => clearTimeout(timer);
  });
</script>

<div class="expanded" class:chrome>
  <header class="head">
    <p class="clock tabular">{hhmm.format(at)}</p>
    <p class="date">{day.format(at)}</p>
  </header>
  <section class="next" aria-label="Up next">
    <div class="next-head">
      <p class="caption">Up next</p>
    </div>
    {#if items.length}
      <ul>
        {#each items.slice(0, 3) as item, i (i)}
          <li><span class="time tabular">{item.time}</span><span class="what">{item.label}</span></li>
        {/each}
      </ul>
    {:else}
      <p class="empty">Nothing scheduled. Hold Right Ctrl to talk.</p>
    {/if}
  </section>
</div>

<style>
  .expanded {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    height: 100%;
    padding: 22px 30px;
  }
  .expanded.chrome { padding-right: 64px; }
  p,
  ul {
    margin: 0;
    padding: 0;
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-3);
  }
  .clock {
    font: 500 24px/28px var(--font-sans);
    letter-spacing: var(--tracking-tight);
  }
  .date {
    font: var(--type-label);
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .caption {
    font: var(--type-caption);
    color: var(--text-secondary);
  }
  .next-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  ul {
    list-style: none;
    margin-top: var(--space-2);
    display: grid;
    gap: var(--space-1);
  }
  li {
    display: flex;
    gap: var(--space-3);
    font: var(--type-body);
  }
  .time {
    flex: none;
    min-width: 44px;
    color: var(--text-secondary);
  }
  .what {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .empty {
    margin-top: var(--space-2);
    font: var(--type-body);
    color: var(--text-primary);
  }
</style>
