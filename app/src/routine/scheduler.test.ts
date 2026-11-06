import { beforeEach, describe, expect, it } from "vitest";
import { occurrencesFor } from "./program";
import { DEFAULTS, type ProgramEntry } from "../island/preferences";
import { createRoutine } from "./scheduler";

const start = new Date(2026, 10, 2); // Monday
const at = (hour: number, minute: number) => new Date(2026, 10, 2, hour, minute);
const everyDay = [0, 1, 2, 3, 4, 5, 6];
const entry = (id: string, time: string, label: string, extra: Partial<ProgramEntry> = {}): ProgramEntry =>
  ({ id, label, detail: "", time, days: everyDay, enabled: true, ...extra });
const program = [entry("wake", "10:30", "Wake up"), entry("breakfast", "11:05", "Breakfast"), entry("walk", "11:40", "Walk")];
const options = { ...DEFAULTS, program };

describe("program alarms", () => {
  let storage: Pick<Storage, "getItem" | "setItem">;
  beforeEach(() => {
    const data = new Map<string, string>();
    storage = {
      getItem: (key) => data.get(key) ?? null,
      setItem: (key, value) => { data.set(key, value); },
    };
  });

  it("turns the weekly program into each day's reminders in time order", () => {
    const monday = occurrencesFor(start, start, { ...options, program: [...program].reverse() });
    expect(monday.map(({ time, label }) => [time, label])).toEqual([["10:30", "Wake up"], ["11:05", "Breakfast"], ["11:40", "Walk"]]);
    expect(monday[0].id).toBe("routine:2026-11-02:custom-wake");
    expect(occurrencesFor(start, start, { ...options, program: [entry("tue", "09:00", "Tuesday only", { days: [2] })] })).toEqual([]);
  });

  it("keeps an unacknowledged alarm through restart and snoozes for five minutes", () => {
    const routine = createRoutine(storage, start);
    expect(routine.tick(at(10, 29), options).due).toBeUndefined();
    const first = routine.tick(at(10, 30), options).due!;
    expect(first.label).toBe("Wake up");
    const reopened = createRoutine(storage, at(10, 33));
    expect(reopened.tick(at(10, 33), options).due?.id).toBe(first.id);
    reopened.acknowledge(first.id, true, at(10, 33).getTime());
    expect(reopened.tick(at(10, 37), options).due).toBeUndefined();
    expect(reopened.tick(at(10, 38), options).due?.id).toBe(first.id);
    reopened.acknowledge(first.id, false, at(10, 38).getTime());
    expect(createRoutine(storage, start).tick(at(10, 38), options).due).toBeUndefined();
  });

  it("queues the next due alarm behind the one already ringing and skips older misses", () => {
    const routine = createRoutine(storage, at(10, 40));
    const later = { ...options, program: [entry("early", "10:35", "Missed"), entry("wake", "10:45", "Wake up"), entry("breakfast", "11:05", "Breakfast")] };
    expect(routine.tick(at(10, 40), later).due).toBeUndefined();
    const first = routine.tick(at(10, 45), later).due!;
    expect(routine.tick(at(11, 5), later).due?.id).toBe(first.id);
    routine.acknowledge(first.id, false, at(11, 5).getTime());
    expect(routine.tick(at(11, 6), later).due?.label).toBe("Breakfast");
  });

  it("skips disabled reminders, respects alarms off and uses the chosen snooze duration", () => {
    const routine = createRoutine(storage, start);
    const mixed = { ...options, program: [entry("eight", "08:00", "Start"), entry("off", "09:15", "Disabled", { enabled: false }), ...program.slice(1)] };
    expect(routine.tick(at(7, 59), mixed).next.map(({ time }) => time)).toEqual(["08:00", "11:05", "11:40"]);
    expect(routine.tick(at(8, 0), { ...mixed, alarms: false }).due).toBeUndefined();
    const alarm = routine.tick(at(8, 0), mixed).due!;
    expect(alarm.id).toMatch(/:custom-eight$/);
    routine.acknowledge(alarm.id, true, at(8, 0).getTime(), 3);
    expect(routine.tick(at(8, 2), mixed).due).toBeUndefined();
    expect(routine.tick(at(8, 3), mixed).due?.id).toBe(alarm.id);
  });
});
