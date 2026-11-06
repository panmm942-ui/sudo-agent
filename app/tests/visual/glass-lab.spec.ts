import { expect, test } from "@playwright/test";

// The app has no @types/node; this spec only needs env vars.
declare const process: { env: Record<string, string | undefined> };

// Runs only under playwright.glass.config.ts (it serves the standalone lab page).
test.beforeEach(({}, info) => test.skip(!info.config.metadata?.glassLab, "run with -c playwright.glass.config.ts"));

const OUT = "../docs/perf/glass-lab";
const STATES = (process.env.GLASS_STATES ?? "compact,listening,confirm,alarm").split(",");
const BACKDROPS = (process.env.GLASS_BG ?? "dusk,bright").split(",");

// GLASS_DPR=2 renders a HiDPI close-up crop of the pill alongside the full window.
const DPR = Number(process.env.GLASS_DPR ?? 1);
test.use({ deviceScaleFactor: DPR });

for (const bg of BACKDROPS) {
  for (const state of STATES) {
    test(`glass ${state} over ${bg}`, async ({ page }) => {
      const errors: string[] = [];
      page.on("pageerror", (e) => errors.push(e.message));
      page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
      await page.goto(`/glass-lab.html?state=${state}&bg=${bg}`);
      await page.waitForFunction(() => (window as any).glassLab?.ready === true);
      expect(errors).toEqual([]);
      if (DPR === 1) await page.screenshot({ path: `${OUT}/${state}-${bg}.png` });
      else {
        const pill = await page.evaluate(() => document.getElementById("content")!.getBoundingClientRect().toJSON());
        const clip = { x: Math.max(0, pill.x - 24), y: 0, width: pill.width + 48, height: pill.y + pill.height + 30 };
        await page.screenshot({ path: `${OUT}/${state}-${bg}@${DPR}x.png`, clip });
      }
    });
  }
}
