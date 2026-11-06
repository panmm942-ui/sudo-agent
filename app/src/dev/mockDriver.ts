/** Scripted IslandEvent playback: stands in for the voice pipeline until it exists. */

import type { IslandEvent } from "../island/events";

/** One scripted event, `at` ms after `run` starts. */
export type MockStep = { at: number; event: IslandEvent };

export type MockScriptName = "demo" | "confirm" | "alarm";

/**
 * Plays `script` through `dispatch` and returns a cancel function. Steps are ordered by `at`
 * (ties keep script order); each is scheduled from the start time, so delays do not drift.
 */
export function run(script: MockStep[], dispatch: (e: IslandEvent) => void): () => void {
  const steps = script.map((step, i) => ({ step, i })).sort((a, b) => a.step.at - b.step.at || a.i - b.i);
  const start = Date.now();
  let next = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const tick = () => {
    timer = undefined;
    const elapsed = Date.now() - start;
    while (next < steps.length && steps[next].step.at <= elapsed) dispatch(steps[next++].step.event);
    if (next < steps.length) timer = setTimeout(tick, steps[next].step.at - elapsed);
  };
  if (steps.length) timer = setTimeout(tick, steps[0].step.at);

  return () => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    next = steps.length;
  };
}

/** Whole-transcript partials revealing `sentence` one word at a time, the last one final. */
function words(sentence: string, from: number, every: number): MockStep[] {
  const parts = sentence.split(" ");
  return parts.map((_, i) => ({
    at: from + i * every,
    event: { type: "transcript", text: parts.slice(0, i + 1).join(" "), final: i === parts.length - 1 },
  }));
}

const demo: MockStep[] = [
  { at: 0, event: { type: "wake" } },
  ...words("What's on my calendar tomorrow?", 450, 230),
  { at: 1700, event: { type: "thinking" } },
  { at: 2500, event: { type: "action", label: "Checking calendar", step: 1, total: 2 } },
  { at: 3300, event: { type: "action", label: "Reading events", step: 2, total: 2 } },
  { at: 4100, event: { type: "reply", text: "Tomorrow you have two meetings:", done: false } },
  { at: 4450, event: { type: "reply", text: "Tomorrow you have two meetings: stand-up at 9:30", done: false } },
  {
    at: 4800,
    event: { type: "reply", text: "Tomorrow you have two meetings: stand-up at 9:30 and design review at 14:00.", done: true },
  },
  { at: 7400, event: { type: "idle_timeout" } },
];

const confirm: MockStep[] = [
  { at: 0, event: { type: "wake" } },
  ...words("Delete the old backups", 450, 240),
  { at: 1500, event: { type: "thinking" } },
  { at: 2200, event: { type: "action", label: "Finding backups", step: 1, total: 2 } },
  {
    at: 3000,
    event: { type: "confirm_request", id: "mock-confirm", title: "Delete 3 backups?", body: "backup-2026-08.zip and 2 more, 4.2 GB" },
  },
  { at: 6000, event: { type: "confirm_result", id: "mock-confirm", ok: true } },
  { at: 6600, event: { type: "action", label: "Deleting backups", step: 2, total: 2 } },
  { at: 7600, event: { type: "reply", text: "Deleted 3 backups and freed 4.2 GB.", done: true } },
  { at: 10_000, event: { type: "idle_timeout" } },
];

const alarm: MockStep[] = [
  { at: 0, event: { type: "wake" } },
  ...words("Summarise my unread email", 450, 230),
  { at: 1600, event: { type: "thinking" } },
  { at: 2400, event: { type: "alarm_ring", id: "mock-alarm", label: "Stand-up", time: "09:30" } },
  { at: 7400, event: { type: "alarm_stop", id: "mock-alarm" } },
  { at: 8200, event: { type: "reply", text: "You have 4 unread emails; one from Dana needs a reply today.", done: true } },
  { at: 11_000, event: { type: "idle_timeout" } },
];

export const SCRIPTS: Record<MockScriptName, MockStep[]> = { demo, confirm, alarm };
