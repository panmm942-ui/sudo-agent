import { describe, expect, it, vi } from "vitest";
import { ISLAND_STATES, reduce } from "../island/machine";
import { installDevKeys, jumpTo, stateForKey } from "./devKeys";

function press(target: EventTarget, key: string, extra: Record<string, unknown> = {}) {
  target.dispatchEvent(Object.assign(new Event("keydown"), { key, ...extra }));
}

function setup() {
  const target = new EventTarget();
  const jump = vi.fn();
  const script = vi.fn();
  const uninstall = installDevKeys({ jump, script }, target);
  return { target, jump, script, uninstall };
}

describe("installDevKeys", () => {
  it("1–9 jump to the states in machine order", () => {
    const { target, jump } = setup();
    for (let i = 1; i <= 9; i++) press(target, String(i));
    expect(jump.mock.calls.map((c) => c[0])).toEqual([...ISLAND_STATES]);
  });

  it("D, C and A run the demo, confirm and alarm scripts (any case)", () => {
    const { target, script } = setup();
    for (const k of ["d", "D", "c", "a"]) press(target, k);
    expect(script.mock.calls.map((c) => c[0])).toEqual(["demo", "demo", "confirm", "alarm"]);
  });

  it("ignores modified keys, auto-repeat, other keys and typing in fields", () => {
    const { target, jump, script } = setup();
    press(target, "1", { ctrlKey: true });
    press(target, "2", { altKey: true });
    press(target, "3", { metaKey: true });
    press(target, "d", { repeat: true });
    press(target, "0");
    press(target, "x");
    expect(jump).not.toHaveBeenCalled();
    expect(script).not.toHaveBeenCalled();
  });

  it("ignores keys typed into text fields", () => {
    for (const field of [{ tagName: "INPUT" }, { tagName: "TEXTAREA" }, { tagName: "DIV", isContentEditable: true }]) {
      const target = Object.assign(new EventTarget(), field);
      const jump = vi.fn();
      installDevKeys({ jump, script: vi.fn() }, target);
      press(target, "4");
      expect(jump).not.toHaveBeenCalled();
    }
  });

  it("uninstall removes the listener", () => {
    const { target, jump, uninstall } = setup();
    uninstall();
    press(target, "1");
    expect(jump).not.toHaveBeenCalled();
  });
});

describe("stateForKey / jumpTo", () => {
  it("maps only 1–9", () => {
    expect(stateForKey("1")).toBe("dormant");
    expect(stateForKey("9")).toBe("alarm");
    expect(stateForKey("0")).toBeNull();
    expect(stateForKey("10")).toBeNull();
  });

  it("jumpTo gives every state sample payload its content layer reads", () => {
    for (const s of ISLAND_STATES) expect(jumpTo(s).state).toBe(s);
    expect(typeof jumpTo("listening").payload.transcript).toBe("string");
    expect(jumpTo("acting").payload.action).toMatchObject({ label: expect.any(String) });
    expect(typeof jumpTo("responding").payload.reply).toBe("string");
    expect(jumpTo("confirm").payload.confirm).toMatchObject({ id: expect.any(String) });
    expect(jumpTo("alarm").payload.alarm).toMatchObject({ id: expect.any(String) });
  });

  it("jumped-to confirm and alarm states can be left with the matching events", () => {
    const c = jumpTo("confirm");
    const id = (c.payload.confirm as { id: string }).id;
    expect(reduce(c, { type: "confirm_result", id, ok: false }).state).toBe("compact");
    const a = jumpTo("alarm");
    const aid = (a.payload.alarm as { id: string }).id;
    expect(reduce(a, { type: "alarm_stop", id: aid }).state).toBe("compact");
  });
});
