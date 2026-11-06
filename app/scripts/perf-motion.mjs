// Reduced-motion check for the Phase 1 perf gate: samples every frame of a few state transitions
// with `prefers-reduced-motion` off and on, and reports what the content layers and pill did.
//   node scripts/perf-motion.mjs [baseUrl]
// Expected with reduce: layers animate opacity only (no blur, no scale) and the pill reaches its
// target in ~120 ms with no overshoot (springs replaced by an ease-out tween).
import { chromium } from "@playwright/test";

const base = process.argv[2] ?? "http://127.0.0.1:4392";
const STEPS = [
  ["2", "compact"],
  ["3", "listening"],
  ["6", "responding"],
  ["7", "confirm"],
  ["2", "compact"],
];

async function run(browser, reducedMotion) {
  const ctx = await browser.newContext({ viewport: { width: 640, height: 280 }, deviceScaleFactor: 1, reducedMotion });
  const page = await ctx.newPage();
  await page.goto(`${base}/?backdrop=fixture`);
  await page.waitForSelector('body[data-island-ready="1"]', { state: "attached", timeout: 120_000 });
  await page.waitForTimeout(1000);
  const out = [];
  for (const [key, state] of STEPS) {
    await page.evaluate(() => {
      const S = (window.__s = { t0: performance.now(), rows: [], on: true });
      const tick = () => {
        if (!S.on) return;
        const c = document.querySelector(".content");
        const layers = [...document.querySelectorAll(".content .layer")].map((l) => {
          const cs = getComputedStyle(l);
          return { o: +cs.opacity, f: cs.filter, tr: cs.transform };
        });
        S.rows.push({ t: performance.now() - S.t0, w: c.offsetWidth, h: c.offsetHeight, layers });
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
    await page.keyboard.press(key);
    await page.waitForTimeout(1500);
    const rows = await page.evaluate(() => ((window.__s.on = false), window.__s.rows));
    const final = rows.at(-1);
    const settle = rows.find((r, i) => rows.slice(i).every((q) => Math.abs(q.w - final.w) < 0.5 && Math.abs(q.h - final.h) < 0.5));
    const start = rows[0];
    const over = (k) => {
      const up = final[k] >= start[k];
      const peak = up ? Math.max(...rows.map((r) => r[k])) : Math.min(...rows.map((r) => r[k]));
      return Math.abs(peak - final[k]);
    };
    let maxBlur = 0, minScale = 1, multi = 0;
    const opac = new Set();
    for (const r of rows) {
      if (r.layers.length > 1) multi++;
      for (const l of r.layers) {
        const b = /blur\(([\d.]+)px\)/.exec(l.f);
        if (b) maxBlur = Math.max(maxBlur, +b[1]);
        const m = /matrix\(([-\d.e]+)/.exec(l.tr);
        if (m) minScale = Math.min(minScale, +m[1]);
        opac.add(l.o.toFixed(2));
      }
    }
    out.push({
      to: state,
      frames: rows.length,
      size: `${start.w}x${start.h} -> ${final.w}x${final.h}`,
      settleMs: settle ? Math.round(settle.t) : null,
      overshootPx: Math.max(over("w"), over("h")).toFixed(1),
      crossfadeFrames: multi,
      maxBlurPx: maxBlur.toFixed(2),
      minScale: minScale.toFixed(3),
      opacitySteps: opac.size,
    });
  }
  await ctx.close();
  return out;
}

const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
for (const rm of ["no-preference", "reduce"]) {
  console.log(`\n--- reducedMotion: ${rm} ---`);
  console.table(await run(browser, rm));
}
await browser.close();
