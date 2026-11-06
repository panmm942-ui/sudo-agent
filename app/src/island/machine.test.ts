import { describe, expect, it } from "vitest";
import type { IslandEvent } from "./events";
import {
  ISLAND_STATES,
  SIZES,
  heightFor,
  initialState,
  reduce,
  springFor,
  type IslandState,
  type MachineState,
} from "./machine";

const at = (state: IslandState, payload: Record<string, unknown> = {}): MachineState => ({
  state,
  prev: null,
  payload,
});

const ring: IslandEvent = { type: "alarm_ring", id: "a1", label: "Wake up", time: "07:00" };
const ask: IslandEvent = { type: "confirm_request", id: "c1", title: "Delete file?", body: "report.pdf" };
const ask2: IslandEvent = { type: "confirm_request", id: "c2", title: "Send email?", body: "to Ana" };
const confirmId = (s: MachineState) => (s.payload.confirm as { id: string } | undefined)?.id;

describe("reduce", () => {
  it("starts dormant", () => {
    expect(initialState()).toEqual({ state: "dormant", prev: null, payload: {} });
  });

  it("dormant + hotkey_down → listening", () => {
    expect(reduce(at("dormant"), { type: "hotkey_down" }).state).toBe("listening");
  });

  it("compact + wake → listening, clearing the previous transcript and reply", () => {
    const s = reduce(at("compact", { transcript: "old", reply: "old" }), { type: "wake" });
    expect(s.state).toBe("listening");
    expect(s.prev).toBe("compact");
    expect(s.payload.transcript).toBe("");
    expect(s.payload.reply).toBeUndefined();
  });

  it("listening + partial transcript stays listening and records the text", () => {
    const s = reduce(at("listening"), { type: "transcript", text: "set a tim", final: false });
    expect(s.state).toBe("listening");
    expect(s.payload.transcript).toBe("set a tim");
  });

  it("listening + transcript{final} → thinking", () => {
    const s = reduce(at("listening"), { type: "transcript", text: "set a timer", final: true });
    expect(s.state).toBe("thinking");
    expect(s.prev).toBe("listening");
    expect(s.payload.transcript).toBe("set a timer");
  });

  it("thinking + action → acting with the action payload", () => {
    const s = reduce(at("thinking"), { type: "action", label: "Opening Spotify", step: 1, total: 3 });
    expect(s.state).toBe("acting");
    expect(s.payload.action).toEqual({ label: "Opening Spotify", step: 1, total: 3 });
  });

  it("acting + reply → responding with the reply text", () => {
    const s = reduce(at("acting"), { type: "reply", text: "Done.", done: true });
    expect(s.state).toBe("responding");
    expect(s.payload.reply).toBe("Done.");
  });

  it("responding + idle_timeout → compact", () => {
    expect(reduce(at("responding"), { type: "idle_timeout" }).state).toBe("compact");
  });

  it.each(ISLAND_STATES)("%s + alarm_ring → alarm", (state) => {
    const s = reduce(at(state), ring);
    expect(s.state).toBe("alarm");
    expect(s.payload.alarm).toEqual({ id: "a1", label: "Wake up", time: "07:00" });
  });

  it("alarm ignores everything but alarm_stop (confirm_request only queues)", () => {
    const alarm = reduce(at("responding"), ring);
    const others: IslandEvent[] = [
      { type: "hotkey_down" },
      { type: "wake" },
      { type: "thinking" },
      { type: "idle_timeout" },
      { type: "hover", on: true },
      { type: "reply", text: "x", done: true },
    ];
    for (const e of others) expect(reduce(alarm, e)).toBe(alarm);
  });

  it.each(["dormant", "compact", "thinking", "acting", "responding", "confirm"] as const)(
    "alarm + alarm_stop → prev (%s)",
    (state) => {
      const alarm = reduce(at(state), ring);
      const s = reduce(alarm, { type: "alarm_stop", id: "a1" });
      expect(s.state).toBe(state);
      expect(s.prev).toBe("alarm");
      expect(s.payload.alarm).toBeUndefined();
    },
  );

  it.each(["listening", "expanded"] as const)(
    "alarm_stop does not resume transient %s, it settles to compact",
    (state) => {
      const alarm = reduce(at(state), ring);
      expect(reduce(alarm, { type: "alarm_stop", id: "a1" }).state).toBe("compact");
    },
  );

  it("hold hotkey → alarm rings → release → stop does not leave the island listening", () => {
    let s = reduce(at("dormant"), { type: "hotkey_down" });
    s = reduce(s, ring);
    s = reduce(s, { type: "hotkey_up" });
    s = reduce(s, { type: "transcript", text: "hi", final: true });
    s = reduce(s, { type: "alarm_stop", id: "a1" });
    expect(s.state).toBe("compact");
  });

  it("a confirm_request during alarm is kept and shown after alarm_stop", () => {
    let s = reduce(at("listening"), ring);
    s = reduce(s, ask);
    expect(s.state).toBe("alarm");
    expect(s.payload.pendingConfirm).toEqual([{ id: "c1", title: "Delete file?", body: "report.pdf" }]);
    s = reduce(s, { type: "alarm_stop", id: "a1" });
    expect(s.state).toBe("confirm");
    expect(s.prev).toBe("compact");
    expect(s.payload.confirm).toEqual({ id: "c1", title: "Delete file?", body: "report.pdf" });
    expect(s.payload.pendingConfirm).toBeUndefined();
  });

  it("a confirm queued during an alarm that interrupted thinking is shown on resuming thinking", () => {
    let s = reduce(at("thinking"), ring);
    s = reduce(s, ask);
    s = reduce(s, { type: "alarm_stop", id: "a1" });
    expect(s.state).toBe("confirm");
    expect(s.prev).toBe("thinking");
  });

  it("an alarm interrupting a confirm returns to that confirm, with requests queued behind it", () => {
    let s = reduce(at("thinking"), ask);
    s = reduce(s, ring);
    s = reduce(s, ask2);
    s = reduce(s, { type: "alarm_stop", id: "a1" });
    expect(s.state).toBe("confirm");
    expect((s.payload.confirm as { id: string }).id).toBe("c1");
    s = reduce(s, { type: "confirm_result", id: "c1", ok: true });
    expect(s.state).toBe("confirm");
    expect((s.payload.confirm as { id: string }).id).toBe("c2");
  });

  it("alarm_stop for another alarm id leaves the ringing alarm alone", () => {
    const alarm = reduce(at("compact"), ring);
    expect(reduce(alarm, { type: "alarm_stop", id: "other" })).toBe(alarm);
  });

  it("a second alarm_ring while ringing keeps the pre-alarm state as prev", () => {
    const first = reduce(at("compact"), ring);
    const second = reduce(first, { type: "alarm_ring", id: "a2", label: "Timer", time: "07:05" });
    expect(second.state).toBe("alarm");
    expect(second.prev).toBe("compact");
    expect(reduce(second, { type: "alarm_stop", id: "a2" }).state).toBe("compact");
  });

  it.each(["responding", "acting", "thinking"] as const)("%s + confirm_request → confirm", (state) => {
    const s = reduce(at(state), ask);
    expect(s.state).toBe("confirm");
    expect(s.payload.confirm).toEqual({ id: "c1", title: "Delete file?", body: "report.pdf" });
  });

  it("confirm + confirm_result{ok} → thinking", () => {
    const c = reduce(at("responding"), ask);
    expect(reduce(c, { type: "confirm_result", id: "c1", ok: true }).state).toBe("thinking");
  });

  it("confirm + confirm_result{not ok} → compact", () => {
    const c = reduce(at("responding"), ask);
    expect(reduce(c, { type: "confirm_result", id: "c1", ok: false }).state).toBe("compact");
  });

  it("confirm_result for another id is ignored", () => {
    const c = reduce(at("responding"), ask);
    expect(reduce(c, { type: "confirm_result", id: "zzz", ok: true })).toBe(c);
  });

  it("a second confirm_request during confirm queues FIFO and shows after the current result", () => {
    const ask3: IslandEvent = { type: "confirm_request", id: "c3", title: "Pay?", body: "$5" };
    let s = reduce(at("acting"), ask);
    s = reduce(s, ask2);
    s = reduce(s, ask3);
    expect(confirmId(s)).toBe("c1");
    s = reduce(s, { type: "confirm_result", id: "c1", ok: false });
    expect(s.state).toBe("confirm");
    expect(confirmId(s)).toBe("c2");
    s = reduce(s, { type: "confirm_result", id: "c2", ok: true });
    expect(confirmId(s)).toBe("c3");
    s = reduce(s, { type: "confirm_result", id: "c3", ok: true });
    expect(s.state).toBe("thinking");
    expect(s.payload.confirm).toBeUndefined();
    expect(s.payload.pendingConfirm).toBeUndefined();
  });

  it.each(["dormant", "compact"] as const)("%s + confirm_request → confirm (nothing to interrupt)", (state) => {
    const s = reduce(at(state), ask);
    expect(s.state).toBe("confirm");
    expect(s.prev).toBe(state);
  });

  it("listening + confirm_request keeps listening and surfaces the confirm at thinking", () => {
    let s = reduce(at("dormant"), { type: "hotkey_down" });
    s = reduce(s, { type: "transcript", text: "turn off the", final: false });
    s = reduce(s, ask);
    expect(s.state).toBe("listening");
    expect(s.payload.confirm).toBeUndefined();
    s = reduce(s, { type: "transcript", text: "turn off the lights", final: true });
    expect(s.state).toBe("confirm");
    expect(s.prev).toBe("thinking");
    expect(s.payload.transcript).toBe("turn off the lights");
    expect(confirmId(s)).toBe("c1");
  });

  it("listening + confirm_request surfaces on idle_timeout → compact", () => {
    const s = reduce(reduce(at("listening"), ask), { type: "idle_timeout" });
    expect(s.state).toBe("confirm");
    expect(s.prev).toBe("compact");
  });

  it("expanded + confirm_request waits for hover-off, and survives a wake in between", () => {
    let s = reduce(at("expanded"), ask);
    expect(s.state).toBe("expanded");
    expect(reduce(s, { type: "hover", on: false }).state).toBe("confirm");
    s = reduce(s, { type: "wake" });
    expect(s.state).toBe("listening");
    s = reduce(s, { type: "transcript", text: "ok", final: true });
    expect(s.state).toBe("confirm");
    expect(confirmId(s)).toBe("c1");
  });

  it("confirm holds against hover, wake and replies", () => {
    const c = reduce(at("acting"), ask);
    for (const e of [{ type: "hover", on: true }, { type: "wake" }, { type: "reply", text: "x", done: false }] as IslandEvent[])
      expect(reduce(c, e)).toBe(c);
  });

  it("compact + hover{on} → expanded, expanded + hover{off} → compact", () => {
    const e = reduce(at("compact"), { type: "hover", on: true });
    expect(e.state).toBe("expanded");
    expect(reduce(e, { type: "hover", on: false }).state).toBe("compact");
  });

  it("unknown pairs return the same state object", () => {
    const s = at("dormant");
    expect(reduce(s, { type: "hotkey_up" })).toBe(s);
    expect(reduce(s, { type: "hover", on: false })).toBe(s);
    const r = at("responding");
    expect(reduce(r, { type: "confirm_result", id: "c1", ok: true })).toBe(r);
  });

  it("survives rapid listening → thinking → confirm with consistent prev links", () => {
    let s = reduce(at("dormant"), { type: "hotkey_down" });
    s = reduce(s, { type: "transcript", text: "delete it", final: true });
    s = reduce(s, ask);
    expect(s.state).toBe("confirm");
    expect(s.prev).toBe("thinking");
    expect(s.payload.transcript).toBe("delete it");
  });
});

describe("SIZES", () => {
  it("covers every state with the Global Constraints sizes", () => {
    expect(Object.keys(SIZES).sort()).toEqual([...ISLAND_STATES].sort());
    expect(SIZES.dormant).toEqual({ w: 126, h: 34, r: 17 });
    expect(SIZES.compact).toEqual({ w: 220, h: 36, r: 18 });
    expect(SIZES.listening).toEqual({ w: 360, h: 84, r: 42 });
    expect(SIZES.thinking).toEqual({ w: 360, h: 84, r: 42 });
    expect(SIZES.acting).toEqual({ w: 380, h: 72, r: 36 });
    expect(SIZES.responding).toEqual({ w: 420, h: [96, 180], r: 36 });
    expect(SIZES.confirm).toEqual({ w: 420, h: 150, r: 40 });
    expect(SIZES.expanded).toEqual({ w: 420, h: 190, r: 44 });
    expect(SIZES.alarm).toEqual({ w: 440, h: 210, r: 48 });
  });
});

describe("heightFor", () => {
  it("clamps content height into the responding range", () => {
    expect(heightFor("responding", 50)).toBe(96);
    expect(heightFor("responding", 120)).toBe(120);
    expect(heightFor("responding", 400)).toBe(180);
    expect(heightFor("confirm", 400)).toBe(150);
  });

  it.each([NaN, Infinity, -Infinity])("treats a %s content height as unknown (minimum)", (bad) => {
    expect(heightFor("responding", bad)).toBe(96);
  });
});

describe("springFor", () => {
  it("growing → bouncy", () => {
    expect(springFor("dormant", "listening")).toBe("bouncy");
    expect(springFor("compact", "alarm")).toBe("bouncy");
  });
  it("shrinking → gentle", () => {
    expect(springFor("responding", "compact")).toBe("gentle");
    expect(springFor("alarm", "dormant")).toBe("gentle");
  });
  it("same size → snappy", () => {
    expect(springFor("listening", "thinking")).toBe("snappy");
    expect(springFor("dormant", "dormant")).toBe("snappy");
  });
});
