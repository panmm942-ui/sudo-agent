<script lang="ts">
 import type {FrameTiming} from '../island/frameTiming';
 let {timing}:{timing:FrameTiming}=$props();
</script>
<output aria-label="Animation frame rate">{timing.active?'Drawing':'Idle · last animation'} {timing.fps?`${Math.round(timing.fps)} FPS`:'…'}{#if timing.frames} · {timing.meanMs.toFixed(1)} ms · worst {timing.worstMs.toFixed(1)} ms{/if}</output>
<style>
  output {position:fixed;bottom:6px;left:8px;pointer-events:none;padding:4px 7px;border-radius:6px;background:#101318db;color:#fff;font:11px/16px ui-monospace,monospace;z-index:20;font-variant-numeric:tabular-nums;}
</style>
