import { isTime, type Preferences, type ProgramEntry } from "../island/preferences";

// Sudo ships no built-in schedule: reminders come only from the program each person builds or imports.
export type Occurrence = { id: string; at: number; label: string; detail: string; time: string };
type Timing = Pick<Preferences, "gradualSleep" | "reminders"> & Partial<Pick<Preferences,'program'>>;

/** The person's own weekly program; empty until they add or import reminders. */
export function editableProgram(preferences:Pick<Preferences,'program'>):ProgramEntry[] {
  return preferences.program ?? [];
}

export function localDay(date: Date): string {
  return [date.getFullYear(), date.getMonth() + 1, date.getDate()]
    .map((part) => String(part).padStart(2, "0")).join("-");
}

/** One day's reminders from the person's weekly program, in time order. */
export function occurrencesFor(day: Date, _start: Date, timing?: Timing): Occurrence[] {
  return (timing?.program ?? []).filter(entry=>entry.enabled&&entry.days.includes(day.getDay())&&isTime(entry.time)).map(entry=>{
    const [hour,minute]=entry.time.split(':').map(Number);
    return {id:`routine:${localDay(day)}:custom-${entry.id}`,at:new Date(day.getFullYear(),day.getMonth(),day.getDate(),hour,minute).getTime(),label:entry.label,detail:entry.detail,time:entry.time};
  }).sort((a,b)=>a.at-b.at);
}

export function nearbyOccurrences(now: Date, start: Date, timing?: Timing): Occurrence[] {
  return [-1, 0, 1].flatMap((offset) =>
    occurrencesFor(new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset), start, timing),
  );
}
