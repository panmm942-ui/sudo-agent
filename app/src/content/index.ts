/** Island content layers: one component per state, plus the payload → props mapping. */
import type { Component, Snippet } from "svelte";
import Acting from "./Acting.svelte";
import Alarm from "./Alarm.svelte";
import Compact from "./Compact.svelte";
import Confirm from "./Confirm.svelte";
import Expanded from "./Expanded.svelte";
import Listening from "./Listening.svelte";
import Responding from "./Responding.svelte";
import Thinking from "./Thinking.svelte";

export { default as Crossfade } from "./Crossfade.svelte";

/** Mirrors `IslandState` in `island/machine.ts` (kept local so content has no machine dependency). */
export type ContentState =
  | "dormant"
  | "compact"
  | "listening"
  | "thinking"
  | "acting"
  | "responding"
  | "confirm"
  | "expanded"
  | "alarm";

/** Payload shapes written by `reduce` (plan Task 2). */
export interface ActionPayload {
  label: string;
  step?: number;
  total?: number;
}
export interface ConfirmPayload {
  id: string;
  title: string;
  body: string;
}
export interface AlarmPayload {
  id: string;
  label: string;
  detail?: string;
  /** Preformatted ("07:30") or an instant (epoch ms / Date) formatted as local HH:MM. */
  time: string | number | Date;
}

/** User intents raised by content; the integrator maps them to `confirm_result` / `alarm_stop`. */
export interface ContentHandlers {
  onconfirm?: (id: string) => void;
  oncancel?: (id: string) => void;
  onsnooze?: (id: string) => void;
  onstop?: (id: string) => void;
  /** Responding only: natural content height in logical px (feeds the 96–180 px pill height). */
  onresize?: (height: number) => void;
}

/** `null` = no content (dormant shows the bare pill). */
export const CONTENT: Record<ContentState, Component<any> | null> = {
  dormant: null,
  compact: Compact,
  listening: Listening,
  thinking: Thinking,
  acting: Acting,
  responding: Responding,
  confirm: Confirm,
  expanded: Expanded,
  alarm: Alarm,
};

const str = (v: unknown): string => (typeof v === "string" ? v : "");

/** Props for `CONTENT[state]` from `MachineState.payload`, handlers and the orb snippet. */
export function contentProps(
  state: ContentState,
  payload: Record<string, unknown>,
  handlers: ContentHandlers = {},
  orb?: Snippet,
): Record<string, unknown> {
  switch (state) {
    case "listening":
      return { transcript: str(payload.transcript), orb };
    case "thinking":
      return { transcript: str(payload.transcript), orb };
    case "acting":
      return { action: (payload.action as ActionPayload | undefined) ?? { label: "" } };
    case "responding":
      return { reply: str(payload.reply), onresize: handlers.onresize, orb };
    case "confirm":
      return { confirm: payload.confirm as ConfirmPayload | undefined, onconfirm: handlers.onconfirm, oncancel: handlers.oncancel };
    case "alarm":
      return { alarm: payload.alarm as AlarmPayload | undefined, onsnooze: handlers.onsnooze, onstop: handlers.onstop };
    default:
      return {};
  }
}
