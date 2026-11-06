import { describe, expect, it, vi } from "vitest";
import type { IslandEvent } from "../island/events";
import { BRIDGE, inTauri, installHotkeyBridge, type ListenFn } from "./hotkeyBridge";

/** A stand-in for Tauri's `listen` that lets the test emit events synchronously. */
function fakeListen() {
  const handlers = new Map<string, (payload: unknown) => void>();
  const unlistened: string[] = [];
  const listen: ListenFn = (name, handler) => {
    handlers.set(name, (payload) => handler({ event: name, id: 0, payload }));
    return Promise.resolve(() => {
      handlers.delete(name);
      unlistened.push(name);
    });
  };
  return { listen, emit: (name: string, payload: unknown = null) => handlers.get(name)?.(payload), handlers, unlistened };
}

function install() {
  const f = fakeListen();
  const seen: IslandEvent[] = [];
  const uninstall = installHotkeyBridge((e) => seen.push(e), f.listen);
  return { f, seen, uninstall };
}

describe("installHotkeyBridge", () => {
  it("is a no-op outside Tauri", () => {
    expect(inTauri()).toBe(false);
    const dispatch = vi.fn();
    const uninstall = installHotkeyBridge(dispatch);
    uninstall();
    expect(dispatch).not.toHaveBeenCalled();
  });

  it("drives a full voice turn: hold, transcript, thinking, streamed reply", () => {
    const { f, seen } = install();
    f.emit("jarvis_listening");
    f.emit("jarvis_transcript", { text: "Τι ώρα είναι;", final: true });
    f.emit("jarvis_thinking");
    f.emit("jarvis_reply", { text: "Είναι", done: false });
    f.emit("jarvis_reply", { text: "Είναι τρεις.", done: true });
    expect(seen).toEqual([
      { type: "hotkey_down" },
      { type: "transcript", text: "Τι ώρα είναι;", final: true },
      { type: "thinking" },
      { type: "reply", text: "Είναι", done: false },
      { type: "reply", text: "Είναι τρεις.", done: true },
    ]);
  });

  it("ignores raw hotkey events: Rust decides whether Sudo AI listens", () => {
    const { f, seen } = install();
    f.emit("hotkey_down");
    f.emit("hotkey_up");
    expect(seen).toEqual([]);
  });

  it("maps wake word, no speech and cancel", () => {
    const { f, seen } = install();
    f.emit("jarvis_wake");
    f.emit("jarvis_idle");
    f.emit("hotkey_cancel");
    expect(seen).toEqual([{ type: "wake" }, { type: "idle_timeout" }, { type: "idle_timeout" }]);
  });

  it("tolerates malformed payloads", () => {
    expect(BRIDGE.jarvis_reply(null)).toEqual({ type: "reply", text: "", done: false });
    expect(BRIDGE.jarvis_transcript({ text: 3 })).toEqual({ type: "transcript", text: "", final: true });
  });

  it("uninstall unlistens every event and ignores late events", async () => {
    const { f, seen, uninstall } = install();
    await Promise.resolve();
    const emitLate = f.handlers.get("jarvis_reply");
    uninstall();
    await Promise.resolve();
    emitLate?.({ text: "late", done: true });
    expect(seen).toHaveLength(0);
    expect(f.unlistened.sort()).toEqual(Object.keys(BRIDGE).sort());
  });
});
