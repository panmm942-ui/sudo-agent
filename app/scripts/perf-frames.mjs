// Frame-time evidence for the Phase 1 perf gate (docs/perf/phase1.md), measured in headless
// Chromium against a running `vite preview` (WebView2 devtools cannot be driven from here).
//   node scripts/perf-frames.mjs [baseUrl] [--gl=swiftshader|d3d11] [--script=demo|confirm|alarm]
// Per app rAF callback it records: callback time (scripting + GL command submission), the same
// plus gl.finish() in a second pass (CPU-visible GPU completion), GPU time from
// EXT_disjoint_timer_query_webgl2 when the browser exposes it, frame-to-frame interval,
// long tasks / long animation frames, and rAF callbacks during a 5 s idle at rest.
import { chromium } from "@playwright/test";

const args = process.argv.slice(2);
const base = args.find((a) => !a.startsWith("--")) ?? "http://127.0.0.1:4392";
const gl = args.find((a) => a.startsWith("--gl="))?.slice(5) ?? "swiftshader";
const scriptName = args.find((a) => a.startsWith("--script="))?.slice(9) ?? "demo";
const KEYS = { demo: "d", confirm: "c", alarm: "a" };

const GL_ARGS = {
  swiftshader: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"],
  d3d11: ["--use-angle=d3d11", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-webgl-draft-extensions"],
};

/** Installed before the page's own scripts: wraps rAF and getContext to time app frames. */
function instrument(finish) {
  const P = (window.__perf = { frames: [], idleCalls: 0, idle: false, longtasks: [], loaf: [], gpu: [], timer: false });
  const contexts = [];
  const getContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function (type, opts) {
    const ctx = getContext.call(this, type, opts);
    if (ctx && type === "webgl2" && !contexts.includes(ctx)) {
      contexts.push(ctx);
      ctx.__timer = ctx.getExtension("EXT_disjoint_timer_query_webgl2");
      if (ctx.__timer) P.timer = true;
    }
    return ctx;
  };
  const pending = [];
  function pollQueries() {
    for (let i = pending.length - 1; i >= 0; i--) {
      const { ctx, q } = pending[i];
      const ext = ctx.__timer;
      if (!ctx.getQueryParameter(q, ctx.QUERY_RESULT_AVAILABLE)) continue;
      if (!ctx.getParameter(ext.GPU_DISJOINT_EXT)) P.gpu.push(ctx.getQueryParameter(q, ctx.QUERY_RESULT) / 1e6);
      ctx.deleteQuery(q);
      pending.splice(i, 1);
    }
  }
  const raf = window.requestAnimationFrame.bind(window);
  let lastT = 0;
  window.requestAnimationFrame = (cb) =>
    raf((t) => {
      if (P.idle) P.idleCalls++;
      pollQueries();
      const queries = contexts
        .filter((c) => c.__timer)
        .map((ctx) => {
          const q = ctx.createQuery();
          ctx.beginQuery(ctx.__timer.TIME_ELAPSED_EXT, q);
          return { ctx, q };
        });
      const t0 = performance.now();
      cb(t);
      if (finish) contexts.forEach((c) => c.finish());
      const ms = performance.now() - t0;
      for (const { ctx, q } of queries) {
        ctx.endQuery(ctx.__timer.TIME_ELAPSED_EXT);
        pending.push({ ctx, q });
      }
      P.frames.push({ ms, dt: lastT ? t - lastT : 0, state: document.querySelector(".content")?.getAttribute("data-state") });
      lastT = t;
    });
  new PerformanceObserver((l) => l.getEntries().forEach((e) => P.longtasks.push(e.duration))).observe({ type: "longtask", buffered: true });
  try {
    new PerformanceObserver((l) => l.getEntries().forEach((e) => P.loaf.push(e.duration))).observe({ type: "long-animation-frame", buffered: true });
  } catch {}
}

const stats = (xs) => {
  if (!xs.length) return "n/a";
  const s = [...xs].sort((a, b) => a - b);
  const q = (p) => s[Math.min(s.length - 1, Math.floor(p * s.length))];
  return `n=${s.length} p50=${q(0.5).toFixed(2)} p95=${q(0.95).toFixed(2)} p99=${q(0.99).toFixed(2)} max=${s.at(-1).toFixed(2)} ms`;
};

async function pass(browser, finish) {
  const page = await browser.newPage({ viewport: { width: 640, height: 280 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(instrument, finish);
  await page.goto(`${base}/?mock=voice&backdrop=fixture`);
  await page.waitForSelector('body[data-island-ready="1"]', { state: "attached", timeout: 120_000 });
  const renderer = await page.evaluate(() => {
    const c = document.createElement("canvas").getContext("webgl2");
    const ext = c?.getExtension("WEBGL_debug_renderer_info");
    return ext ? c.getParameter(ext.UNMASKED_RENDERER_WEBGL) : c?.getParameter(c.RENDERER);
  });
  await page.waitForTimeout(1500);
  await page.evaluate(() => {
    const P = window.__perf;
    P.frames.length = 0; P.longtasks.length = 0; P.loaf.length = 0; P.gpu.length = 0;
    const el = document.querySelector(".content");
    P.seen = [];
    new MutationObserver(() => P.seen.push(el.getAttribute("data-state"))).observe(el, { attributeFilter: ["data-state"] });
  });
  const t0 = Date.now();
  await page.keyboard.press(KEYS[scriptName]);
  await page.waitForFunction(() => window.__perf.seen.length > 2 && window.__perf.seen.at(-1) === "compact", undefined, { timeout: 60_000 });
  const scriptMs = Date.now() - t0;
  await page.waitForTimeout(2000); // let the compact shrink spring settle
  const run = await page.evaluate(() => {
    const P = window.__perf;
    return { frames: P.frames.slice(), longtasks: P.longtasks.slice(), loaf: P.loaf.slice(), gpu: P.gpu.slice(), timer: P.timer, seen: P.seen };
  });
  await page.evaluate(() => (window.__perf.idle = true));
  await page.waitForTimeout(5000);
  const idleCalls = await page.evaluate(() => window.__perf.idleCalls);
  await page.close();
  return { renderer, scriptMs, idleCalls, errors, ...run };
}

const browser = await chromium.launch({ args: GL_ARGS[gl] });
console.log(`gl=${gl} script=${scriptName} base=${base}`);
for (const finish of [false, true]) {
  const r = await pass(browser, finish);
  const ms = r.frames.map((f) => f.ms);
  const dts = r.frames.map((f) => f.dt).filter((d) => d > 0);
  console.log(`\n--- pass ${finish ? "B: callback + gl.finish()" : "A: callback only"} ---`);
  console.log(`renderer: ${r.renderer}`);
  console.log(`script ran ${r.scriptMs} ms; states: ${r.seen.join(" > ")}`);
  console.log(`app frames: ${r.frames.length}`);
  console.log(`frame cost      ${stats(ms)}  (>4 ms: ${ms.filter((x) => x > 4).length}, >16 ms: ${ms.filter((x) => x > 16).length})`);
  console.log(`frame interval  ${stats(dts)}  (>16.7 ms: ${dts.filter((x) => x > 16.7).length}, >33 ms: ${dts.filter((x) => x > 33.4).length})`);
  console.log(`gpu timer query ${r.timer ? stats(r.gpu) : "EXT_disjoint_timer_query_webgl2 unavailable"}`);
  console.log(`long tasks (>50 ms): ${r.longtasks.length} ${r.longtasks.length ? stats(r.longtasks) : ""}`);
  console.log(`long animation frames (>50 ms): ${r.loaf.length} ${r.loaf.length ? stats(r.loaf) : ""}`);
  console.log(`rAF callbacks during 5 s idle at rest: ${r.idleCalls}`);
  if (r.errors.length) console.log(`page errors: ${r.errors.join(" | ")}`);
}
await browser.close();
