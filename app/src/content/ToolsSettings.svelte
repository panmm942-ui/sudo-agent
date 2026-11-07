<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { CalendarFeed, Preferences } from "../island/preferences";

  /** Settings → Tools: clipboard history, coding agents and calendars. */
  let { preferences, onupdate, calendarStatus = {}, onrefreshcalendars }: {
    preferences: Preferences;
    onupdate: (patch: Partial<Preferences>) => void;
    calendarStatus?: Record<string, string>;
    onrefreshcalendars?: () => void;
  } = $props();

  type Setup = { claude: boolean; codex: boolean; claudeInstalled: boolean; codexInstalled: boolean };
  let setup = $state<Setup | null>(null);
  let agentError = $state("");
  let feedName = $state("");
  let feedUrl = $state("");
  let feedError = $state("");

  onMount(() => { void invoke<Setup>("agents_setup").then((s) => (setup = s)).catch(() => {}); });

  async function connect(agent: "claude" | "codex", on: boolean): Promise<void> {
    agentError = "";
    try { setup = await invoke<Setup>("agents_connect", { agent, connect: on }); } catch (e) { agentError = String(e); }
  }

  function addFeed(event: SubmitEvent): void {
    event.preventDefault();
    const url = feedUrl.trim();
    if (!/^(https?|webcal):\/\/\S+$/i.test(url)) { feedError = "Paste the calendar's address that starts with https:// or webcal://."; return; }
    if (preferences.calendars.some((c) => c.url === url)) { feedError = "That calendar is already added."; return; }
    if (preferences.calendars.length >= 10) { feedError = "Up to 10 calendars."; return; }
    const feed: CalendarFeed = { id: crypto.randomUUID(), name: feedName.trim() || "Calendar", url };
    onupdate({ calendars: [...preferences.calendars, feed] });
    feedName = ""; feedUrl = ""; feedError = "";
  }
  const removeFeed = (id: string) => onupdate({ calendars: preferences.calendars.filter((c) => c.id !== id) });
</script>

{#snippet toggle(key: "clipboardHistory" | "agentApprovals" | "agentNotices" | "calendarNotices", label: string, help?: string)}
  <label class="row switch">
    <span><strong>{label}</strong>{#if help}<small>{help}</small>{/if}</span>
    <input type="checkbox" role="switch" checked={preferences[key]} onchange={(e) => onupdate({ [key]: e.currentTarget.checked })} />
  </label>
{/snippet}

<p class="intro">Everyday tools on the island's fourth tab: Mixer, Clipboard, Shelf, Agents, System and YouTube.</p>

<h3 class="group-label">Clipboard</h3>
{@render toggle("clipboardHistory", "Clipboard history", "Keeps the last 50 things you copied (text, images, files) on this PC. Anything a password manager marks private is never saved.")}

<h3 class="group-label">Coding agents</h3>
<div class="row">
  <span><strong>Claude Code</strong><small>{setup?.claude ? "Connected: progress and permission prompts come to Sudo." : setup?.claudeInstalled === false ? "Claude Code isn't set up on this PC yet." : "Adds Sudo as a hook in your Claude Code settings (a backup is kept)."}</small></span>
  <button type="button" class="pill-btn" onclick={() => connect("claude", !setup?.claude)}>{setup?.claude ? "Disconnect" : "Connect"}</button>
</div>
<div class="row">
  <span><strong>Codex</strong><small>{setup?.codex ? "Connected: Sudo shows when Codex finishes a turn." : "Adds Sudo as Codex's notify program (Codex can't ask Sudo for approvals)."}</small></span>
  <button type="button" class="pill-btn" onclick={() => connect("codex", !setup?.codex)}>{setup?.codex ? "Disconnect" : "Connect"}</button>
</div>
{#if agentError}<p class="note" role="status">{agentError}</p>{/if}
{@render toggle("agentApprovals", "Answer Claude Code's prompts on the island", "Allow or Deny without the terminal. Not answered within 45 seconds? The terminal asks as usual.")}
{@render toggle("agentNotices", "Tell me when an agent finishes", "A short notice when a turn that took longer than 15 seconds is done.")}

<h3 class="group-label">Calendar</h3>
<p class="note">Paste a calendar's private iCal address: Google Calendar → Settings → your calendar → “Secret address in iCal format”; Outlook → Settings → Shared calendars → Publish → ICS; iCloud → Share → Public calendar.</p>
{#each preferences.calendars as feed (feed.id)}
  <div class="row">
    <span><strong>{feed.name}</strong><small>{calendarStatus[feed.id] ?? "Loading…"}</small></span>
    <button type="button" class="pill-btn" onclick={() => removeFeed(feed.id)}>Remove</button>
  </div>
{/each}
<form class="feed" onsubmit={addFeed}>
  <input class="text-input" type="text" maxlength="60" placeholder="Name (e.g. Work)" aria-label="Calendar name" bind:value={feedName} />
  <input class="text-input url" type="url" placeholder="https://… or webcal://…" aria-label="Calendar address" bind:value={feedUrl} />
  <button type="submit" class="pill-btn">Add</button>
</form>
{#if feedError}<p class="note" role="status">{feedError}</p>{/if}
{#if preferences.calendars.length}
  <div class="row"><span><strong>Refresh</strong><small>Calendars update every 15 minutes.</small></span><button type="button" class="pill-btn" onclick={onrefreshcalendars}>Refresh now</button></div>
{/if}
{@render toggle("calendarNotices", "Event notices", "Shows each event on the island before it starts. Upcoming events also appear in Home's “Up next”.")}
<label class="row"><strong>Notice before an event</strong>
  <select aria-label="Notice before an event" value={String(preferences.calendarLead)} onchange={(e) => onupdate({ calendarLead: +e.currentTarget.value })}>
    {#each [[0, "When it starts"], [5, "5 minutes"], [10, "10 minutes"], [15, "15 minutes"], [30, "30 minutes"]] as [value, label] (value)}<option value={String(value)}>{label}</option>{/each}
  </select>
</label>

<style>
  .intro { margin: 10px 0 2px; font: var(--type-caption); color: var(--text-secondary); line-height: 18px; }
  .group-label { font: 600 11px/16px var(--font-sans); letter-spacing: .065em; text-transform: uppercase; opacity: .62; margin: 20px 0 3px; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); min-height: 48px; border-bottom: 1px solid oklch(0.98 0 0 / 0.07); padding: 9px 0; font: var(--type-body); }
  .row > span { min-width: 0; }
  strong { font-weight: 560; }
  small, .note { display: block; margin: 3px 0 0; color: var(--text-primary); opacity: 0.8; font: var(--type-caption); }
  input, select { color-scheme: dark; accent-color: var(--accent); }
  .text-input { min-height: 32px; padding: 4px 10px; border: 1px solid #ffffff24; border-radius: 9px; background: var(--surface-control); color: inherit; font: var(--type-body); }
  select { background: var(--surface-control); color: inherit; border: 1px solid var(--surface-control-hover); border-radius: 9px; min-height: 32px; padding: 3px 7px; max-width: 150px; flex-shrink: 0; }
  .row .pill-btn { flex: none; }
  .switch input { appearance: none; flex: none; width: 34px; height: 20px; margin: 0; border: 0; border-radius: 20px; background: oklch(0.55 0.01 255); position: relative; cursor: pointer; transition: background-color 160ms ease; }
  .switch input::before { content: ""; position: absolute; width: 14px; height: 14px; top: 3px; left: 3px; border-radius: 50%; background: var(--text-primary); transition: transform 160ms ease; }
  .switch input:checked { background: var(--accent); }
  .switch input:checked::before { transform: translateX(14px); background: var(--on-accent); }
  @media (prefers-reduced-motion: reduce) { .switch input, .switch input::before { transition: none; } }
  .feed { display: flex; gap: 6px; align-items: center; padding: 9px 0; }
  .feed .text-input { min-width: 0; width: 120px; }
  .feed .url { flex: 1; }
</style>
