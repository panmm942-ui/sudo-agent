<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";

  type Session = { id: string; agent: string; project: string; cwd: string; status: string; detail: string; updated: number; turnStarted?: number | null };
  type Approval = { id: string; session: string; agent: string; project: string; tool: string; summary: string };
  type State = { sessions: Session[]; approvals: Approval[] };
  type Setup = { claude: boolean; codex: boolean; claudeInstalled: boolean; codexInstalled: boolean };

  /** Live Claude Code / Codex sessions, and Claude Code's permission prompts to answer here. */
  let { onsettings }: { onsettings: () => void } = $props();
  let agents = $state<State>({ sessions: [], approvals: [] });
  let setup = $state<Setup | null>(null);
  let error = $state("");
  let now = $state(Date.now());

  onMount(() => {
    void invoke<State>("agents_state").then((s) => (agents = s)).catch(() => {});
    void invoke<Setup>("agents_setup").then((s) => (setup = s)).catch(() => {});
    let off: (() => void) | undefined, disposed = false;
    listen<State>("agents_changed", (e) => (agents = e.payload)).then((u) => (disposed ? u() : (off = u))).catch(() => {});
    const clock = setInterval(() => (now = Date.now()), 1000);
    return () => { disposed = true; off?.(); clearInterval(clock); };
  });

  async function decide(approval: Approval, allow: boolean): Promise<void> {
    try { await invoke("agents_decide", { id: approval.id, allow }); error = ""; } catch (e) { error = String(e); }
  }
  async function connect(agent: "claude" | "codex"): Promise<void> {
    try { setup = await invoke<Setup>("agents_connect", { agent, connect: true }); error = ""; } catch (e) { error = String(e); }
  }

  const LABEL: Record<string, string> = { working: "Working", approval: "Needs your OK", waiting: "Waiting for you", done: "Finished", idle: "Ready" };
  function elapsed(session: Session): string {
    if (session.status === "working" && session.turnStarted) {
      const s = Math.max(0, Math.floor((now - session.turnStarted) / 1000));
      return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
    }
    const s = Math.max(0, (now - session.updated) / 1000);
    return s < 60 ? "just now" : s < 3600 ? `${Math.floor(s / 60)} min ago` : `${Math.floor(s / 3600)} h ago`;
  }
  const connected = $derived(!!setup && (setup.claude || setup.codex));
</script>

<div class="agents list">
  {#each agents.approvals as approval (approval.id)}
    <div class="approval">
      <p class="caption">{approval.agent} · {approval.project} wants to use <strong>{approval.tool}</strong></p>
      <p class="summary">{approval.summary}</p>
      <div class="buttons">
        <button type="button" class="small-btn" onclick={() => decide(approval, false)}>Deny</button>
        <button type="button" class="small-btn accent" onclick={() => decide(approval, true)}>Allow</button>
      </div>
    </div>
  {/each}
  {#each agents.sessions as session (session.id)}
    <div class="row session">
      <span class="status {session.status}" aria-hidden="true"></span>
      <span class="grow">
        <span class="title">{session.agent} · {session.project}</span>
        <span class="caption">{LABEL[session.status] ?? session.status}{session.detail ? ` — ${session.detail}` : ""}</span>
      </span>
      <span class="caption time tabular">{elapsed(session)}</span>
    </div>
  {:else}
    {#if !agents.approvals.length}
      <div class="empty">
        {#if setup && !connected}
          Connect your coding agents to follow them here{setup.claude || !setup.claudeInstalled ? "" : " and approve Claude Code from the island"}.
          <span class="connect">
            <button type="button" class="small-btn accent" onclick={() => connect("claude")}>Connect Claude Code</button>
            <button type="button" class="small-btn" onclick={() => connect("codex")}>Connect Codex</button>
          </span>
        {:else if setup}
          No agent is running. Start Claude Code{setup.codex ? " or Codex" : ""} and its progress shows here.
          <button type="button" class="small-btn" onclick={onsettings}>Agent settings</button>
        {:else}…{/if}
      </div>
    {/if}
  {/each}
  {#if error}<p class="caption error" role="status">{error}</p>{/if}
</div>

<style>
  .agents { gap: 4px; }
  .approval { display: grid; gap: 4px; padding: 8px 10px; border-radius: 12px; background: #ffb3401a; box-shadow: inset 0 0 0 1px #ffb34040; }
  .approval p { margin: 0; }
  .summary { font: 500 12px/16px ui-monospace, "Cascadia Mono", Consolas, monospace; max-height: 48px; overflow: hidden; overflow-wrap: anywhere; }
  .buttons { display: flex; justify-content: flex-end; gap: 6px; }
  .session .grow { display: grid; }
  .time { flex: none; }
  .status { flex: none; width: 9px; height: 9px; border-radius: 50%; background: #8a8f98; }
  .status.working { background: #6cc4ff; animation: pulse 1.2s ease-in-out infinite; }
  .status.approval { background: #ffb340; }
  .status.waiting { background: #b48cff; }
  .status.done { background: #5fd38a; }
  @keyframes pulse { 50% { opacity: .35; } }
  @media (prefers-reduced-motion: reduce) { .status.working { animation: none; } }
  .connect { display: flex; gap: 6px; justify-content: center; }
  .error { color: #ffb4b4; white-space: normal; }
</style>
