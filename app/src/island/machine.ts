/** The island state machine: the single source of truth for states, sizes and transitions. */

import type { ConfirmInfo, IslandEvent } from "./events";
import type { SpringName } from "./spring";

export const ISLAND_STATES = [
  "dormant",
  "compact",
  "listening",
  "thinking",
  "acting",
  "responding",
  "confirm",
  "expanded",
  "alarm",
] as const;

export type IslandState = (typeof ISLAND_STATES)[number];

/**
 * Payload keys written by `reduce`: `transcript: string`, `action: ActionInfo`, `reply: string`,
 * `confirm: ConfirmInfo`, `alarm: AlarmInfo` (see events.ts), and `pendingConfirm: ConfirmInfo[]`
 * (FIFO of requests waiting to be shown; absent when empty).
 */
export type MachineState = {
  state: IslandState;
  prev: IslandState | null;
  payload: Record<string, unknown>;
};

/** Target pill geometry in logical px; `h` is a [min, max] range when it follows content. */
export type IslandSize = { w: number; h: number | [number, number]; r: number };

export const SIZES: Record<IslandState, IslandSize> = {
  dormant: { w: 126, h: 34, r: 17 },
  compact: { w: 220, h: 36, r: 18 },
  listening: { w: 360, h: 84, r: 42 },
  thinking: { w: 360, h: 84, r: 42 },
  acting: { w: 380, h: 72, r: 36 },
  responding: { w: 420, h: [96, 180], r: 36 },
  confirm: { w: 420, h: 150, r: 40 },
  expanded: { w: 420, h: 190, r: 44 },
  alarm: { w: 440, h: 210, r: 48 },
};

/** Resolves a state's height, clamping `contentH` into the range for content-sized states. */
export function heightFor(state: IslandState, contentH?: number): number {
  const h = SIZES[state].h;
  if (typeof h === "number") return h;
  const c = contentH !== undefined && Number.isFinite(contentH) ? contentH : h[0];
  return Math.min(h[1], Math.max(h[0], c));
}

export function initialState(): MachineState {
  return { state: "dormant", prev: null, payload: {} };
}

/** Growing → bouncy, shrinking → gentle, same footprint → snappy (compared by minimum area). */
export function springFor(from: IslandState, to: IslandState): SpringName {
  const area = (s: IslandState) => SIZES[s].w * heightFor(s);
  const a = area(from);
  const b = area(to);
  if (b > a) return "bouncy";
  if (b < a) return "gentle";
  return "snappy";
}

function without(payload: Record<string, unknown>, key: string): Record<string, unknown> {
  const { [key]: _, ...rest } = payload;
  return rest;
}

function go(s: MachineState, state: IslandState, payload = s.payload): MachineState {
  return { state, prev: state === s.state ? s.prev : s.state, payload };
}

/** Transient states `alarm_stop` must not return to: the events that end them were swallowed by the alarm. */
const TRANSIENT = new Set<IslandState>(["listening", "expanded"]);

/** States in which a queued confirm is shown at once: nothing the user is doing gets cut off. */
const SURFACES_CONFIRM = new Set<IslandState>(["dormant", "compact", "thinking", "acting", "responding"]);

function pendingOf(payload: Record<string, unknown>): ConfirmInfo[] {
  return (payload.pendingConfirm as ConfirmInfo[] | undefined) ?? [];
}

function withPending(payload: Record<string, unknown>, queue: ConfirmInfo[]): Record<string, unknown> {
  return queue.length ? { ...payload, pendingConfirm: queue } : without(payload, "pendingConfirm");
}

/** Shows the oldest queued confirm if `s` has just reached a state that allows it. */
function surface(s: MachineState): MachineState {
  const [next, ...rest] = pendingOf(s.payload);
  if (!next || !SURFACES_CONFIRM.has(s.state)) return s;
  return go(s, "confirm", withPending({ ...s.payload, confirm: next }, rest));
}

/**
 * Pure transition function. Unknown (state, event) pairs return `s` itself.
 *
 * - `alarm` preempts everything and only `alarm_stop` leaves it. It swallows every other event, so
 *   on stop it resumes `prev` (the conversation carries on) unless `prev` is listening/expanded:
 *   their hotkey_up / transcript / hover-off were swallowed, so it settles to compact instead.
 * - `confirm` preempts dormant/compact/thinking/acting/responding and only `confirm_result` leaves it.
 *   Requests arriving during alarm, confirm, listening (the user is speaking) or expanded (the user
 *   is reading) are queued FIFO in `pendingConfirm` and shown as soon as the machine reaches one of
 *   the preemptible states (e.g. listening → thinking, expanded → compact, after `confirm_result`).
 */
export function reduce(s: MachineState, e: IslandEvent): MachineState {
  const next = step(s, e);
  return next === s ? s : surface(next);
}

function step(s: MachineState, e: IslandEvent): MachineState {
  if (e.type === "alarm_ring") {
    const alarm = { id: e.id, label: e.label, time: e.time, ...(e.detail ? { detail: e.detail } : {}) };
    const payload = { ...s.payload, alarm };
    return s.state === "alarm" ? { ...s, payload } : go(s, "alarm", payload);
  }

  if (e.type === "confirm_request") {
    const info = { id: e.id, title: e.title, body: e.body };
    if (SURFACES_CONFIRM.has(s.state)) return go(s, "confirm", { ...s.payload, confirm: info });
    return { ...s, payload: withPending(s.payload, [...pendingOf(s.payload), info]) };
  }

  if (s.state === "alarm") {
    const ringing = s.payload.alarm as { id: string } | undefined;
    if (e.type !== "alarm_stop" || ringing?.id !== e.id) return s;
    const back = s.prev && !TRANSIENT.has(s.prev) ? s.prev : "compact";
    return { state: back, prev: "alarm", payload: without(s.payload, "alarm") };
  }

  if (s.state === "confirm") {
    const pending = s.payload.confirm as { id: string } | undefined;
    if (e.type !== "confirm_result" || pending?.id !== e.id) return s;
    return go(s, e.ok ? "thinking" : "compact", without(s.payload, "confirm"));
  }

  switch (e.type) {
    case "wake":
    case "hotkey_down":
      if (s.state === "listening") return s;
      return go(s, "listening", withPending({ transcript: "" }, pendingOf(s.payload)));

    case "transcript":
      if (s.state !== "listening") return s;
      return go(s, e.final ? "thinking" : "listening", { ...s.payload, transcript: e.text });

    case "thinking":
      if (s.state === "thinking") return s;
      return go(s, "thinking");

    case "action": {
      const action = { label: e.label, step: e.step, total: e.total };
      return go(s, "acting", { ...s.payload, action });
    }

    case "reply":
      return go(s, "responding", { ...s.payload, reply: e.text });

    case "idle_timeout":
      if (s.state === "dormant" || s.state === "compact") return s;
      return go(s, "compact");

    case "hover":
      if (e.on && (s.state === "compact" || s.state === "dormant")) return go(s, "expanded");
      if (!e.on && s.state === "expanded") return go(s, "compact");
      return s;

    case "hotkey_up":
    case "confirm_result":
    case "alarm_stop":
      return s;
  }
}
