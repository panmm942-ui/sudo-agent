/** Dev keyboard shortcuts while the WebView is focused: `1`–`9` jump to states, `D`/`C`/`A` run scripts. */

import { ISLAND_STATES, type IslandState, type MachineState } from "../island/machine";
import type { MockScriptName } from "./mockDriver";

export type DevKeyHandlers = {
  jump: (state: IslandState) => void;
  script: (name: MockScriptName) => void;
};

const SCRIPT_KEYS: Record<string, MockScriptName> = { d: "demo", c: "confirm", a: "alarm" };

/** `"1"`–`"9"` → the states in machine order (dormant … alarm). */
export function stateForKey(key: string): IslandState | null {
  if (!/^[1-9]$/.test(key)) return null;
  return ISLAND_STATES[Number(key) - 1] ?? null;
}

/** Sample payload per state, so a jumped-to state renders real-looking content. */
const SAMPLE: Partial<Record<IslandState, Record<string, unknown>>> = {
  listening: { transcript: "Remind me to call" },
  thinking: { transcript: "Remind me to call Dana at five" },
  acting: { action: { label: "Creating reminder", step: 1, total: 2 } },
  responding: { reply: "Done. I'll remind you to call Dana at 17:00." },
  confirm: { confirm: { id: "dev-confirm", title: "Empty the recycle bin?", body: "38 items, 1.1 GB" } },
  alarm: { alarm: { id: "dev-alarm", label: "Stand-up", time: "09:30" } },
};

/** A MachineState sitting in `state`; alarm/confirm are leavable via their `dev-*` ids. */
export function jumpTo(state: IslandState): MachineState {
  return { state, prev: state === "alarm" ? "compact" : null, payload: { ...SAMPLE[state] } };
}

function isTyping(t: EventTarget | null): boolean {
  const el = t as { tagName?: string; isContentEditable?: boolean } | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable === true);
}

/** Installs the shortcuts on `target` (default `window`) and returns an uninstall function. */
export function installDevKeys(h: DevKeyHandlers, target: EventTarget = window): () => void {
  const onKey = (ev: Event) => {
    const e = ev as KeyboardEvent;
    if (e.repeat || e.ctrlKey || e.altKey || e.metaKey || isTyping(e.target)) return;
    const state = stateForKey(e.key);
    if (state) return h.jump(state);
    const name = SCRIPT_KEYS[e.key?.toLowerCase()];
    if (name) h.script(name);
  };
  target.addEventListener("keydown", onKey);
  return () => target.removeEventListener("keydown", onKey);
}
