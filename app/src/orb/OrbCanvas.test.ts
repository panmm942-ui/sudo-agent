import { afterEach, describe, expect, it, vi } from "vitest";
import { OrbCanvas } from "./OrbCanvas";

/** WebGL2 stand-in: every method returns a truthy dummy, enough for compile/link to "succeed". */
function fakeCanvas(clientWidth: number, clientHeight: number) {
  const gl = new Proxy({}, { get: () => () => ({}) });
  return { clientWidth, clientHeight, width: 0, height: 0, getContext: () => gl } as unknown as HTMLCanvasElement;
}

describe("OrbCanvas.resize", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("re-reads devicePixelRatio so a display-scale change resizes the backing store", () => {
    vi.stubGlobal("window", { devicePixelRatio: 1 });
    const canvas = fakeCanvas(100, 50);
    const orb = new OrbCanvas(canvas);
    expect([canvas.width, canvas.height]).toEqual([100, 50]);
    vi.stubGlobal("window", { devicePixelRatio: 1.5 });
    expect(orb.resize()).toBe(true);
    expect([canvas.width, canvas.height]).toEqual([150, 75]);
    expect(orb.resize()).toBe(false);
  });

  it("uses the dpr option as a fixed override", () => {
    vi.stubGlobal("window", { devicePixelRatio: 1 });
    const canvas = fakeCanvas(100, 50);
    const orb = new OrbCanvas(canvas, { dpr: 2 });
    vi.stubGlobal("window", { devicePixelRatio: 3 });
    orb.resize();
    expect([canvas.width, canvas.height]).toEqual([200, 100]);
  });
});
