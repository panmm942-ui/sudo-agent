// Records the README demo: the production Svelte build of the island, driven by a scripted demo
// track (original "Blue Hour" artwork, fictional metadata and original lyric lines) fed through a
// mocked Tauri IPC layer. Native capture, Spotify and audio are simulated; the UI code is the
// shipping code. Playwright records a video; to-gif.py converts it with FFmpeg.
//
//   npm run build && npx vite preview --host 127.0.0.1 --port 4393 --strictPort
//   node scripts/readme-demo/record.mjs <outDir> [baseUrl]
//   python scripts/readme-demo/to-gif.py <outDir> ../docs/images/demo.gif
import { chromium } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const [outDir = "readme-demo-frames", base = "http://127.0.0.1:4393"] = process.argv.slice(2);
mkdirSync(outDir, { recursive: true });
const W = 720, H = 300;

const cover = `data:image/png;base64,${readFileSync(join(here, "cover.png")).toString("base64")}`;
const wallpaper = readFileSync(join(here, "..", "..", "tests", "visual", "fixtures", "wallpaper.png"));
const LYRICS = [
  "Streetlights hum a quiet tune",
  "The city folds into the blue",
  "I keep the window open wide",
  "So the evening can come inside",
  "Hold the hour, let it stay",
  "Blue light carries me away",
  "Every echo finds its way home",
  "Under skies of cobalt and chrome",
];
const synced = LYRICS.map((line, i) => {
  const t = 4 + i * 3.2;
  return `[${String(Math.floor(t / 60)).padStart(2, "0")}:${(t % 60).toFixed(2).padStart(5, "0")}]${line}`;
}).join("\n");

const browser = await chromium.launch({ args: ["--use-angle=d3d11", "--enable-gpu", "--ignore-gpu-blocklist"] });
const context = await browser.newContext({ viewport: { width: W, height: H }, deviceScaleFactor: 2, recordVideo: { dir: outDir, size: { width: W, height: H } } });
const page = await context.newPage();
const t0 = Date.now();

// Simulated native layer: just enough of Tauri's IPC for the island to run as it does on Windows.
await page.addInitScript(({ cover }) => {
  const callbacks = new Map();
  let next = 1;
  const startedAt = Date.now() - 2500;
  const track = () => ({
    key: "demo-blue-hour", title: "Blue Hour", artist: "Sudo Sessions", album: "Evening Studies",
    artwork: cover, playing: true, canToggle: true, canPrevious: true, canNext: true,
    shuffle: false, repeatMode: "off", canShuffle: true, canRepeat: true,
    duration: 214, position: (Date.now() - startedAt) / 1000, positionUpdatedAt: Date.now(),
  });
  const spectrum = (t) => Array.from({ length: 7 }, (_, i) => Math.max(0.05, Math.min(1, 0.55 + 0.4 * Math.sin(t * (2.1 + i * 0.7) + i) * Math.sin(t * 0.9 + i * 1.3))));
  const handlers = {
    spotify_state: () => track(),
    start_spotify_meter: ({ channel }) => {
      const id = channel.id ?? channel;
      setInterval(() => callbacks.get(id)?.({ message: spectrum(performance.now() / 1000), index: 0 }), 50);
      return 1;
    },
    start_backdrop: () => { throw new Error("demo: no desktop capture"); },
  };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "island" }, currentWebview: { windowLabel: "island", label: "island" } },
    transformCallback(cb) { const id = next++; callbacks.set(id, cb); return id; },
    unregisterCallback(id) { callbacks.delete(id); },
    convertFileSrc: (p) => p,
    async invoke(cmd, args = {}) {
      if (cmd in handlers) return handlers[cmd](args);
      if (cmd.startsWith("plugin:event|")) return next++;
      return null;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.isTauri = true;
  // Neutral example reminders instead of the built-in personal routine.
  const all = [0, 1, 2, 3, 4, 5, 6];
  const program = [["20:30", "Evening walk", "30 min outside"], ["21:15", "Read", "Two chapters"], ["22:40", "Wind down", "Screens off"], ["07:30", "Morning run", "5 km"]]
    .map(([time, label, detail], i) => ({ id: `demo-${i}`, label, detail, time, days: all, enabled: true }));
  try { localStorage.setItem("sudo.preferences.v1", JSON.stringify({ program })); } catch {}
}, { cover });

// Demo lyrics instead of a real provider; every other external request is blocked.
const cors = { "access-control-allow-origin": "*", "access-control-allow-headers": "*" };
await page.route(/^https?:\/\/(?!127\.0\.0\.1)/, (route) => {
  if (!/lrclib\.net/.test(route.request().url())) return route.abort();
  if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
  return route.fulfill({
    headers: cors, contentType: "application/json",
    body: JSON.stringify({ trackName: "Blue Hour", artistName: "Sudo Sessions", albumName: "Evening Studies", duration: 214, instrumental: false, plainLyrics: LYRICS.join("\n"), syncedLyrics: synced }),
  });
});

await page.goto(`${base}/`);
// Desktop behind the transparent island window.
await page.addStyleTag({ content: `html{min-height:100%;background:#0b1020 url(data:image/png;base64,${wallpaper.toString("base64")}) center top/cover no-repeat fixed !important} body{background:transparent !important}` });
await page.waitForTimeout(3000);

const tStart = (Date.now() - t0) / 1000;  // video time where the demo begins
// 2× stills of each state, used for the social preview image.
const still = (name) => page.screenshot({ path: join(outDir, `still-${name}.png`), clip: { x: 100, y: 0, width: 520, height: 236 } });
const click = async (label) => { await page.getByRole("button", { name: label, exact: true }).first().click(); };
await page.mouse.move(W / 2, H - 10);
await page.waitForTimeout(1500);                       // compact island at rest
await page.mouse.move(W / 2, 30, { steps: 10 });      // hover: island expands to the Spotify panel
await page.waitForTimeout(3000);
await still("spotify");
await click("Show lyrics");                            // synced lyric reader
await page.waitForTimeout(4500);
await still("lyrics");
await page.waitForTimeout(2500);
await click("Home");                                   // Home: clock, reminders, focus, note
await page.waitForTimeout(2000);
await still("home");
await page.waitForTimeout(1000);
await click("Spotify");
await page.waitForTimeout(1500);
await page.mouse.move(W / 2, H - 5, { steps: 10 });   // leave: island settles back
await page.waitForTimeout(2200);
const tEnd = (Date.now() - t0) / 1000;
await context.close();
const video = await page.video().path();
writeFileSync(join(outDir, "timing.json"), JSON.stringify({ video, tStart, tEnd }, null, 2));
await browser.close();
console.log(`video ${video}: demo from ${tStart.toFixed(1)} s to ${tEnd.toFixed(1)} s`);
