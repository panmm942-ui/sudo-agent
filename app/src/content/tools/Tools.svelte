<script lang="ts" module>
  export type ToolsSection = "mixer" | "clipboard" | "shelf" | "agents" | "system" | "youtube";
  export const TOOLS_SECTIONS: { id: ToolsSection; label: string }[] = [
    { id: "mixer", label: "Mixer" }, { id: "clipboard", label: "Clipboard" }, { id: "shelf", label: "Shelf" },
    { id: "agents", label: "Agents" }, { id: "system", label: "System" }, { id: "youtube", label: "YouTube" },
  ];
</script>

<script lang="ts">
  import type { Preferences } from "../../island/preferences";
  import Mixer from "./Mixer.svelte";
  import ClipboardPanel from "./ClipboardPanel.svelte";
  import Shelf from "./Shelf.svelte";
  import Agents from "./Agents.svelte";
  import SystemPanel from "./SystemPanel.svelte";
  import YouTube from "./YouTube.svelte";

  /**
   * The Tools tab: everyday desktop tools in one panel, picked with the small tabs at the top.
   * Each section only runs (polls, listens) while it is the one shown.
   */
  let { section, approvals = 0, preferences, onsection, onupdate, onfocuschange, onsettings }: {
    section: ToolsSection;
    /** Claude Code permission prompts waiting (badge on Agents). */
    approvals?: number;
    preferences: Preferences;
    onsection: (section: ToolsSection) => void;
    onupdate: (patch: Partial<Preferences>) => void;
    onfocuschange?: (focused: boolean) => void;
    onsettings: () => void;
  } = $props();
</script>

<section class="tools" aria-label="Tools">
  <nav class="tabs" aria-label="Tools" onwheel={(event) => event.stopPropagation()}>
    {#each TOOLS_SECTIONS as item (item.id)}
      <button type="button" aria-pressed={section === item.id} onclick={() => onsection(item.id)}>
        {item.label}{#if item.id === "agents" && approvals}<span class="badge" aria-label={`${approvals} waiting`}>{approvals}</span>{/if}
      </button>
    {/each}
  </nav>
  <div class="body">
    {#if section === "mixer"}<Mixer />
    {:else if section === "clipboard"}<ClipboardPanel enabled={preferences.clipboardHistory} onenable={() => onupdate({ clipboardHistory: true })} {onfocuschange} />
    {:else if section === "shelf"}<Shelf />
    {:else if section === "agents"}<Agents {onsettings} />
    {:else if section === "system"}<SystemPanel />
    {:else}<YouTube {onfocuschange} />{/if}
  </div>
</section>

<style>
  .tools { position: absolute; inset: 0; display: grid; grid-template-rows: auto minmax(0, 1fr); gap: 8px; padding: 14px 64px 12px 20px; }
  .tabs { display: flex; gap: 2px; min-width: 0; overflow-x: auto; scrollbar-width: none; }
  .tabs button { position: relative; flex: none; height: 24px; padding: 0 8px; border: 0; border-radius: var(--radius-pill); background: transparent; color: var(--text-secondary); font: 600 11.5px/1 var(--font-sans); cursor: pointer; }
  .tabs button:hover { color: var(--text-primary); background: var(--surface-control); }
  .tabs button[aria-pressed="true"] { color: var(--text-primary); background: var(--surface-control-hover); }
  .badge { display: inline-grid; place-items: center; min-width: 15px; height: 15px; margin-left: 4px; padding: 0 4px; border-radius: 8px; background: #ffb340; color: #1b1300; font: 700 10px/1 var(--font-sans); }
  .body { position: relative; min-height: 0; }
  /* Shared look of the sections. */
  .tools :global(.list) { height: 100%; overflow-y: auto; scrollbar-width: thin; display: grid; align-content: start; gap: 2px; }
  .tools :global(.row) { display: flex; align-items: center; gap: 8px; min-width: 0; padding: 5px 8px; border-radius: 10px; }
  .tools :global(.row:hover) { background: #ffffff0d; }
  .tools :global(.grow) { flex: 1; min-width: 0; }
  .tools :global(.title) { font: var(--type-label); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin: 0; }
  .tools :global(.caption) { font: var(--type-caption); color: var(--text-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin: 0; }
  .tools :global(.icon-btn) { flex: none; display: grid; place-items: center; width: 26px; height: 26px; padding: 0; border: 0; border-radius: 50%; background: transparent; color: var(--text-primary); cursor: pointer; opacity: .8; }
  .tools :global(.icon-btn:hover) { background: var(--surface-control-hover); opacity: 1; }
  .tools :global(.icon-btn svg) { width: 15px; height: 15px; }
  .tools :global(.small-btn) { flex: none; height: 24px; padding: 0 10px; border: 0; border-radius: var(--radius-pill); background: var(--surface-control); color: var(--text-primary); font: 600 11.5px/1 var(--font-sans); cursor: pointer; }
  .tools :global(.small-btn:hover) { background: var(--surface-control-hover); }
  .tools :global(.small-btn.accent) { background: var(--accent); color: var(--on-accent); }
  .tools :global(.empty) { height: 100%; display: grid; place-content: center; justify-items: center; gap: 8px; text-align: center; color: var(--text-secondary); font: var(--type-caption); padding: 0 12px; }
  .tools :global(.field) { width: 100%; height: 28px; padding: 0 12px; border: 0; border-radius: var(--radius-pill); background: var(--surface-control); color: var(--text-primary); font: var(--type-label); outline: none; }
  .tools :global(.field:focus) { box-shadow: inset 0 0 0 1px #ffffff40; }
  .tools :global(input[type="range"]) { appearance: none; -webkit-appearance: none; height: 16px; margin: 0; background: transparent; cursor: pointer; }
  .tools :global(input[type="range"]::-webkit-slider-runnable-track) { height: 4px; border-radius: 2px; background: #ffffff33; }
  .tools :global(input[type="range"]::-webkit-slider-thumb) { -webkit-appearance: none; width: 12px; height: 12px; margin-top: -4px; border-radius: 50%; background: #fff; box-shadow: 0 1px 3px #0006; }
</style>
