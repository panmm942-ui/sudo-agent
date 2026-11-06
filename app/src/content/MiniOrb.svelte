<script lang="ts">
  import {invoke} from '@tauri-apps/api/core';

  /** Resting Sudo AI orb: a small breathing bead. Clicking it starts listening (the island then
   * shows the live orb). Used on the expanded Home view and on the small island. */
  let {size=22,onerror}:{size?:number;onerror?:(message:string)=>void}=$props();
  async function listen(event:MouseEvent){
    event.stopPropagation(); // never also open/close the island underneath
    try{await invoke('jarvis_listen');}catch(e){onerror?.(e instanceof Error?e.message:String(e));}
  }
</script>

<button type="button" class="orb" style:--size="{size}px" aria-label="Talk to Sudo AI" onclick={listen}><span></span></button>

<style>
  .orb{flex:none;width:calc(var(--size) + 8px);height:calc(var(--size) + 8px);padding:0;border:0;border-radius:50%;background:transparent;cursor:pointer;display:grid;place-items:center;pointer-events:auto}
  .orb span{width:var(--size);height:var(--size);border-radius:50%;background:radial-gradient(circle at 38% 34%,#fffdf6 0 18%,oklch(.84 .1 85) 45%,oklch(.66 .14 150) 100%);box-shadow:0 0 10px oklch(.8 .11 110 / .55),inset 0 0 4px #fff8;animation:breathe 3.2s ease-in-out infinite;transition:transform 140ms ease}
  .orb:hover span{transform:scale(1.12)}
  .orb:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
  @keyframes breathe{50%{box-shadow:0 0 16px oklch(.8 .11 110 / .75),inset 0 0 4px #fff8}}
  @media(prefers-reduced-motion:reduce){.orb span{animation:none;transition:none}}
</style>
