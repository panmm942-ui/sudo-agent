import type { AlarmInfo } from "../island/events";
import type { Preferences } from "../island/preferences";
import { localDay, nearbyOccurrences } from "./program";

export const STORAGE_KEY = "sudo.routine.v1";
const GRACE_MS = 60_000;

type Progress = { start: string; alarms: Record<string, number> };

/** Positive alarm times stay pending until acknowledged. Zero marks a dismissed occurrence. */
export function createRoutine(storage: Pick<Storage, "getItem" | "setItem">, now: Date) {
  const saved = storage.getItem(STORAGE_KEY);
  const progress: Progress = saved ? JSON.parse(saved) : { start: localDay(now), alarms: {} };
  const save = () => storage.setItem(STORAGE_KEY, JSON.stringify(progress));
  const start = new Date(`${progress.start}T00:00:00`);
  if (!saved) save();

  return {
    tick(now: Date, preferences?: Pick<Preferences, "alarms" | "gradualSleep" | "reminders"> & Partial<Pick<Preferences,'program'>>): { due?: AlarmInfo; next: AlarmInfo[] } {
      if (preferences?.alarms === false) return { next: [] };
      const at = now.getTime();
      const occurrences = nearbyOccurrences(now, start, preferences);
      let changed = false;
      for (const alarm of occurrences) {
        if (progress.alarms[alarm.id] === undefined && at - alarm.at >= 0 && at - alarm.at < GRACE_MS) {
          progress.alarms[alarm.id] = alarm.at;
          changed = true;
        }
      }
      if (changed) save();

      const pending = occurrences.flatMap((alarm) => {
        const scheduled = progress.alarms[alarm.id];
        if (scheduled === 0 || (scheduled === undefined && alarm.at <= at)) return [];
        return [{ ...alarm, at: scheduled ?? alarm.at }];
      }).sort((a, b) => a.at - b.at);

      return {
        due: pending.find((alarm) => alarm.at <= at),
        next: pending.filter((alarm) => alarm.at > at).slice(0, 3).map((alarm) => ({
          ...alarm,
          time: new Date(alarm.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hourCycle: "h23" }),
        })),
      };
    },
    acknowledge(id: string, snooze: boolean, now: number, snoozeMinutes = 5): void {
      if (!progress.alarms[id]) return;
      progress.alarms[id] = snooze ? now + snoozeMinutes * 60_000 : 0;
      save();
    },
  };
}
