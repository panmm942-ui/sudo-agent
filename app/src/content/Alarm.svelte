<script lang="ts">
  import type { AlarmPayload } from "./index";

  let {
    alarm,
    onsnooze,
    onstop,
  }: {
    alarm?: AlarmPayload;
    onsnooze?: (id: string) => void;
    onstop?: (id: string) => void;
  } = $props();

  const hhmm = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });
  let track = $state<HTMLDivElement>();
  let offset = $state(0);
  let pointer = $state<number | null>(null);
  let origin = 0;
  let distance = 0;
  let travel = 1;
  let tapArmed = $state(false);

  function start(event: PointerEvent): void {
    if (pointer !== null || event.button !== 0) return;
    const thumb = event.currentTarget as HTMLButtonElement;
    travel = Math.max(1, track!.getBoundingClientRect().width - thumb.getBoundingClientRect().width - 8);
    origin = event.clientX;
    distance = 0;
    pointer = event.pointerId;
    thumb.setPointerCapture(event.pointerId);
  }

  function move(event: PointerEvent): void {
    if (event.pointerId !== pointer) return;
    offset = Math.min(travel, Math.max(0, event.clientX - origin));
    distance = Math.max(distance, Math.abs(event.clientX - origin));
  }

  function reset(event: PointerEvent): void {
    if (event.pointerId !== pointer) return;
    pointer = null;
    offset = 0;
  }

  function finish(event: PointerEvent): void {
    if (event.pointerId !== pointer) return;
    move(event);
    const completed = offset >= travel * 0.9;
    reset(event);
    if (completed || (distance < 5 && tapArmed)) stop();
    else tapArmed = distance < 5;
  }

  const notice = $derived(!!alarm?.id.startsWith("notice:"));

  function stop(): void {
    if (alarm) onstop?.(alarm.id);
  }

  const time = $derived.by(() => {
    const t = alarm?.time;
    if (t === undefined) return "";
    if (typeof t === "string") return t;
    const d = new Date(t);
    return Number.isNaN(d.getTime()) ? "" : hhmm.format(d);
  });
</script>

<div class="alarm" role="alertdialog" aria-labelledby="alarm-label" aria-describedby="alarm-time">
  <div class="head">
    <p id="alarm-time" class="time tabular">{time}</p>
    <p id="alarm-label" class="label">{alarm?.label || "Alarm"}</p>
  </div>
  {#if alarm?.detail}<p class="detail">{alarm.detail}</p>{/if}
  {#if notice}
    <!-- A notice (calendar event, a coding agent finishing): no ringing, just dismiss. -->
    <div class="actions">
      {#if alarm?.id.startsWith("notice:calendar:")}<button type="button" class="pill-btn" onclick={() => alarm && onsnooze?.(alarm.id)}>In 5 min</button>{/if}
      <button type="button" class="pill-btn accent dismiss" onclick={stop}>Dismiss</button>
    </div>
  {:else}
  <div class="actions">
    {#if alarm?.id !== "routine-preview"}
      <button type="button" class="pill-btn snooze" onclick={() => alarm && onsnooze?.(alarm.id)}>Snooze</button>
    {/if}
    <div class="track" bind:this={track}>
      <span class="prompt" aria-hidden="true">{tapArmed ? "Tap again to stop" : "Slide to stop"}</span>
      <button type="button" class="thumb" class:dragging={pointer !== null}
        style:--offset="{offset}px" aria-label="Stop alarm"
        onpointerdown={start} onpointermove={move} onpointerup={finish}
        onpointercancel={reset} onlostpointercapture={reset}
        onclick={(event) => { if (event.detail === 0) stop(); }}>
        <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="22" height="22">
          <path d="M4 12h15m-6-6 6 6-6 6" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
    </div>
  </div>
  {/if}
</div>

<style>
  .alarm {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    height: 100%;
    padding: var(--space-5) 32px;
  }
  p {
    margin: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .label { font: var(--type-title); }
  .detail {
    font: var(--type-body);
    overflow-wrap: anywhere;
  }
  .time {
    font: var(--type-time);
    letter-spacing: var(--tracking-tight);
  }
  .actions {
    margin-top: auto;
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .snooze { height: 44px; }
  .dismiss { margin-left: auto; min-width: 110px; }
  .track {
    position: relative;
    flex: 1;
    height: 52px;
    padding: 4px;
    border-radius: var(--radius-pill);
    background: var(--surface-control);
  }
  .prompt {
    position: absolute;
    inset: 0 12px 0 54px;
    display: grid;
    place-items: center;
    font: var(--type-label);
    pointer-events: none;
  }
  .thumb {
    position: relative;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border: 0;
    border-radius: var(--radius-pill);
    color: var(--on-accent);
    background: var(--accent);
    cursor: grab;
    touch-action: none;
    transform: translateX(var(--offset));
    transition: transform 150ms var(--ease-out);
  }
  .thumb.dragging { cursor: grabbing; transition: none; }
  @media (prefers-reduced-motion: reduce) {
    .thumb { transition: none; }
  }
</style>
