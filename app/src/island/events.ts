/** Typed event bus payloads that drive the island state machine. */

export type ActionInfo = { label: string; step?: number; total?: number };
export type ConfirmInfo = { id: string; title: string; body: string };
export type AlarmInfo = { id: string; label: string; time: string; detail?: string };

export type IslandEvent =
  | { type: "wake" }
  | { type: "hotkey_down" }
  | { type: "hotkey_up" }
  /** `text` is the whole transcript so far (not a delta). */
  | { type: "transcript"; text: string; final: boolean }
  | { type: "thinking" }
  | ({ type: "action" } & ActionInfo)
  /** `text` is the whole reply so far (not a delta); `done` once streaming ends. */
  | { type: "reply"; text: string; done: boolean }
  | ({ type: "confirm_request" } & ConfirmInfo)
  | { type: "confirm_result"; id: string; ok: boolean }
  | { type: "hover"; on: boolean }
  | ({ type: "alarm_ring" } & AlarmInfo)
  | { type: "alarm_stop"; id: string }
  | { type: "idle_timeout" };

export type IslandEventType = IslandEvent["type"];
