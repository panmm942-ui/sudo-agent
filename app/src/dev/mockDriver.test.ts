import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { IslandEvent } from "../island/events";
import { initialState, reduce } from "../island/machine";
import { SCRIPTS, run, type MockStep } from "./mockDriver";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** Runs `script` and records each dispatched event with its fake-clock time. */
function record(script: MockStep[]) {
  const seen: { at: number; event: IslandEvent }[] = [];
  const start = Date.now();
  const cancel = run(script, (event) => seen.push({ at: Date.now() - start, event }));
  return { seen, cancel };
}

describe("mockDriver.run", () => {
  it("dispatches the demo script in order at the scripted times", () => {
    const { seen } = record(SCRIPTS.demo);
    vi.advanceTimersByTime(60_000);
    expect(seen).toEqual(SCRIPTS.demo.map((s) => ({ at: s.at, event: s.event })));
  });

  it("dispatches nothing before a step's time", () => {
    const { seen } = record(SCRIPTS.demo);
    const second = SCRIPTS.demo[1].at;
    vi.advanceTimersByTime(second - 1);
    expect(seen).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(seen).toHaveLength(2);
  });

  it("the cancel function stops further dispatches", () => {
    const { seen, cancel } = record(SCRIPTS.demo);
    vi.advanceTimersByTime(SCRIPTS.demo[2].at);
    const count = seen.length;
    cancel();
    vi.advanceTimersByTime(60_000);
    expect(seen).toHaveLength(count);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("sorts out-of-order steps and keeps equal-time steps in script order", () => {
    const script: MockStep[] = [
      { at: 200, event: { type: "thinking" } },
      { at: 0, event: { type: "wake" } },
      { at: 200, event: { type: "idle_timeout" } },
    ];
    const { seen } = record(script);
    vi.advanceTimersByTime(1000);
    expect(seen.map((s) => s.event.type)).toEqual(["wake", "thinking", "idle_timeout"]);
  });

  it("an empty script dispatches nothing and cancel is safe", () => {
    const { seen, cancel } = record([]);
    vi.advanceTimersByTime(1000);
    cancel();
    cancel();
    expect(seen).toEqual([]);
  });
});

describe("built-in scripts", () => {
  const fold = (script: MockStep[]) => {
    let s = initialState();
    const states: string[] = [];
    for (const step of script) {
      s = reduce(s, step.event);
      if (states.at(-1) !== s.state) states.push(s.state);
    }
    return { final: s, states };
  };

  it("demo walks wake → listening → thinking → acting → responding → compact in 6–8 s", () => {
    const { states } = fold(SCRIPTS.demo);
    expect(states).toEqual(["listening", "thinking", "acting", "responding", "compact"]);
    const end = SCRIPTS.demo.at(-1)!.at;
    expect(end).toBeGreaterThanOrEqual(6000);
    expect(end).toBeLessThanOrEqual(8000);
    const partials = SCRIPTS.demo.filter((s) => s.event.type === "transcript");
    expect(partials.length).toBeGreaterThanOrEqual(4);
  });

  it("transcript partials grow word by word and only the last is final", () => {
    const t = SCRIPTS.demo.flatMap((s) => (s.event.type === "transcript" ? [s.event] : []));
    t.forEach((e, i) => {
      expect(e.final).toBe(i === t.length - 1);
      if (i > 0) expect(e.text.startsWith(t[i - 1].text)).toBe(true);
    });
  });

  it("confirm reaches confirm and resolves back through thinking to compact", () => {
    const { states } = fold(SCRIPTS.confirm);
    expect(states).toContain("confirm");
    expect(states.indexOf("thinking", states.indexOf("confirm"))).toBeGreaterThan(0);
    expect(states.at(-1)).toBe("compact");
  });

  it("alarm preempts the conversation and returns to it after alarm_stop", () => {
    const { states } = fold(SCRIPTS.alarm);
    const i = states.indexOf("alarm");
    expect(i).toBeGreaterThan(0);
    expect(states[i + 1]).toBe(states[i - 1]);
    expect(states.at(-1)).toBe("compact");
  });

  it("every script is sorted by time", () => {
    for (const script of Object.values(SCRIPTS)) {
      const times = script.map((s) => s.at);
      expect(times).toEqual([...times].sort((a, b) => a - b));
    }
  });
});
