import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import { CONTENT, contentProps, type ContentState } from "./index";

const payload = {
  transcript: "set a timer for ten minutes",
  action: { label: "Opening Calendar", step: 2, total: 4 },
  reply: "Done. Your timer ends at 10:42.",
  confirm: { id: "c1", title: "Delete 3 files?", body: "They go to the Recycle Bin." },
  alarm: { id: "a1", label: "Morning run", time: "06:30" },
};

const html = (state: ContentState, extra: Record<string, unknown> = {}) => {
  const C = CONTENT[state];
  if (!C) throw new Error(`no content for ${state}`);
  return render(C, { props: { ...contentProps(state, payload), ...extra } }).body;
};

describe("content components", () => {
  it("dormant has no content layer", () => {
    expect(CONTENT.dormant).toBeNull();
  });

  it("every other state renders from the reduce payload", () => {
    for (const s of Object.keys(CONTENT) as ContentState[]) if (s !== "dormant") expect(html(s)).toBeTruthy();
  });

  it("listening shows the transcript, last word separately, and a centred orb slot", () => {
    const out = html("listening");
    expect(out).toContain("data-orb-slot");
    expect(out).toContain("set a timer for ten");
    expect(out).toMatch(/class="fresh[^"]*">minutes</);
    expect(html("listening", { transcript: "" })).toContain("Listening");
  });

  it("acting announces the step position", () => {
    expect(html("acting")).toContain("Step 2 of 4");
    expect(html("acting", { action: { label: "Searching" } })).toContain("Working");
  });

  it("responding keeps the full reply for screen readers", () => {
    expect(html("responding")).toContain("Done. Your timer ends at 10:42.");
  });

  it("confirm and alarm show their copy and both actions", () => {
    const c = html("confirm");
    for (const s of ["Delete 3 files?", "They go to the Recycle Bin.", "Cancel", "Confirm"]) expect(c).toContain(s);
    const a = html("alarm");
    for (const s of ["06:30", "Morning run", "Snooze", "Stop"]) expect(a).toContain(s);
    expect(a).toMatch(/class="time tabular/);
  });

  it("expanded shows a fixed clock when given one", () => {
    expect(html("expanded", { now: new Date(2026, 9, 30, 9, 5) })).toMatch(/09:05|9:05/);
  });
});
