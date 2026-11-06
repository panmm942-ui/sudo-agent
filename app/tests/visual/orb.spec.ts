import { expect, test, type Page } from "@playwright/test";

// The app has no @types/node; this spec only needs env vars.
declare const process: { env: Record<string, string | undefined> };

// Runs only under playwright.orb.config.ts (it serves the standalone preview page).
test.beforeEach(({}, info) => test.skip(!info.config.metadata?.orbPreview, "run with -c playwright.orb.config.ts"));

async function open(page: Page, query: string) {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto(`/orb-preview.html?frozen=1&${query}`);
  await page.waitForFunction(() => (window as any).orbPreview?.ready === true);
  return errors;
}

/** Alpha at a few canvas points, read back through a 2D canvas. */
async function sampleAlpha(page: Page, id: string) {
  return page.evaluate((canvasId) => {
    const src = document.getElementById(canvasId) as HTMLCanvasElement;
    const c = document.createElement("canvas");
    c.width = src.width;
    c.height = src.height;
    const ctx = c.getContext("2d")!;
    ctx.drawImage(src, 0, 0);
    const at = (x: number, y: number) => ctx.getImageData(Math.floor(x), Math.floor(y), 1, 1).data[3];
    return {
      width: src.width,
      cssWidth: src.clientWidth,
      center: at(src.width / 2, src.height / 2),
      corner: at(1, 1),
      edge: at(src.width - 2, src.height / 2),
    };
  }, id);
}

test("orb draws a DPR-correct body on a transparent background", async ({ page }) => {
  const errors = await open(page, "t=2");
  const hero = await sampleAlpha(page, "hero");
  expect(hero.width).toBe(hero.cssWidth * 2);
  expect(hero.center).toBeGreaterThan(220);
  expect(hero.corner).toBe(0);
  expect(hero.edge).toBe(0);
  const mini = await sampleAlpha(page, "mini");
  expect(mini.width).toBe(mini.cssWidth * 2);
  expect(mini.center).toBeGreaterThan(220);
  expect(errors).toEqual([]);
});

test("every mode renders; stills saved for review", async ({ page }, info) => {
  for (const mode of ["listening", "thinking", "speaking"]) {
    const errors = await open(page, `mode=${mode}`);
    for (const t of [1.3, 2.6, 4.1]) {
      await page.evaluate((time) => (window as any).orbPreview.renderAt(time), t);
      await page.locator("#stage").screenshot({ path: info.outputPath(`${mode}-${t}.png`) });
    }
    expect(errors).toEqual([]);
  }
});

// ORB_FRAMES_DIR=<dir> captures 10 s of the default timeline at ORB_FPS (default 20) for the review GIF.
test("capture frame sequence", async ({ page }) => {
  const dir = process.env.ORB_FRAMES_DIR;
  test.skip(!dir, "set ORB_FRAMES_DIR to capture");
  const fps = Number(process.env.ORB_FPS ?? 20);
  await open(page, "t=0");
  for (let i = 0; i < 10 * fps; i++) {
    await page.evaluate((time) => (window as any).orbPreview.renderAt(time), i / fps);
    await page.locator("#stage").screenshot({ path: `${dir}/frame-${String(i).padStart(4, "0")}.png` });
  }
});
