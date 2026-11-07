<script lang="ts">
  import { onMount } from "svelte";
  import { percent, watchSystem, type SystemStats } from "../../island/system";

  /** CPU, memory and GPU load with the last minute as a line. */
  const SAMPLES = 40;
  let stats = $state<SystemStats | null>(null);
  let history = $state<{ cpu: number[]; memory: number[]; gpu: number[] }>({ cpu: [], memory: [], gpu: [] });

  onMount(() => watchSystem((next) => {
    stats = next;
    const push = (list: number[], value: number | null) => [...list, value ?? 0].slice(-SAMPLES);
    history = { cpu: push(history.cpu, next.cpu), memory: push(history.memory, next.memory), gpu: push(history.gpu, next.gpu) };
  }));

  const line = (values: number[]) => values.map((v, i) => `${(i / (SAMPLES - 1)) * 100},${30 - (Math.max(0, Math.min(100, v)) / 100) * 28}`).join(" ");
  const cards = $derived([
    { id: "cpu", label: "CPU", value: stats?.cpu ?? null, detail: "Processor", values: history.cpu },
    { id: "memory", label: "Memory", value: stats?.memory ?? null, detail: stats ? `${stats.memoryUsedGb.toFixed(1)} of ${stats.memoryTotalGb.toFixed(0)} GB` : "", values: history.memory },
    { id: "gpu", label: "GPU", value: stats?.gpu ?? null, detail: stats && stats.gpu === null ? "Not reported" : "Graphics (3D)", values: history.gpu },
  ]);
</script>

<div class="system">
  {#each cards as card (card.id)}
    <section class="card" aria-label={card.label}>
      <p class="caption">{card.label}</p>
      <p class="big tabular">{percent(card.value)}</p>
      <p class="caption">{card.detail}</p>
      <svg viewBox="0 0 100 32" preserveAspectRatio="none" aria-hidden="true">
        {#if card.values.length > 1}<polyline points={line(card.values)} fill="none" stroke="currentColor" stroke-width="1.5" vector-effect="non-scaling-stroke" />{/if}
      </svg>
      <span class="fill" style:width="{Math.max(0, Math.min(100, card.value ?? 0))}%" aria-hidden="true"></span>
    </section>
  {/each}
</div>

<style>
  .system { height: 100%; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; }
  .card { position: relative; overflow: hidden; display: grid; grid-template-rows: auto auto auto minmax(0, 1fr); padding: 10px 12px 0; border-radius: 14px; background: #ffffff0c; box-shadow: inset 0 0 0 1px #ffffff12; }
  .card p { margin: 0; }
  .big { font: 600 26px/30px var(--font-sans); letter-spacing: -.02em; }
  svg { align-self: end; width: calc(100% + 24px); margin: 0 -12px; height: 100%; max-height: 54px; color: #ffffffb0; }
  .fill { position: absolute; left: 0; bottom: 0; height: 2px; background: #fff; transition: width .4s ease; }
</style>
