// Records a mock script (demo/confirm/alarm) over a bright, busy backdrop as timestamped PNG
// frames via the CDP screencast; scripts/frames-to-gif.py turns them into a 20 fps GIF.
//   node scripts/record-gif.mjs <script> <backdrop.png> <outDir> [baseUrl] [--gl=swiftshader|d3d11]
// The backdrop replaces the fixture wallpaper (route interception), so no app code changes.
import { chromium } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const args = process.argv.slice(2);
const pos = args.filter((a) => !a.startsWith("--"));
const [name, backdropPath, outDir] = pos;
const base = pos[3] ?? "http://127.0.0.1:4392";
const gl = args.find((a) => a.startsWith("--gl="))?.slice(5) ?? "swiftshader";
const KEYS = { demo: "d", confirm: "c", alarm: "a" };
const GL_ARGS = {
  swiftshader: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"],
  d3d11: ["--use-angle=d3d11", "--enable-gpu", "--ignore-gpu-blocklist"],
};
mkdirSync(outDir, { recursive: true });

const browser = await chromium.launch({ args: GL_ARGS[gl] });
const page = await browser.newPage({ viewport: { width: 640, height: 280 }, deviceScaleFactor: 1 });
const png = readFileSync(backdropPath);
await page.route(/wallpaper[^/]*\.png/, (route) => route.fulfill({ body: png, contentType: "image/png" }));
await page.goto(`${base}/?mock=voice&backdrop=fixture`);
await page.waitForSelector('body[data-island-ready="1"]', { state: "attached", timeout: 120_000 });
await page.waitForTimeout(1000);
await page.evaluate(() => {
  const el = document.querySelector(".content");
  window.__seen = [];
  new MutationObserver(() => window.__seen.push(el.getAttribute("data-state"))).observe(el, { attributeFilter: ["data-state"] });
});

const cdp = await page.context().newCDPSession(page);
const frames = [];
cdp.on("Page.screencastFrame", ({ data, metadata, sessionId }) => {
  const file = `f${String(frames.length).padStart(5, "0")}.png`;
  writeFileSync(join(outDir, file), Buffer.from(data, "base64"));
  frames.push({ file, t: metadata.timestamp });
  cdp.send("Page.screencastFrameAck", { sessionId }).catch(() => {});
});
await cdp.send("Page.startScreencast", { format: "png", maxWidth: 640, maxHeight: 280, everyNthFrame: 1 });
await page.waitForTimeout(500);
const tKey = Date.now() / 1000;
await page.keyboard.press(KEYS[name]);
await page.waitForFunction(() => window.__seen.length > 2 && window.__seen.at(-1) === "compact", undefined, { timeout: 60_000 });
await page.waitForTimeout(1200);
// A final frame at rest even if nothing repainted since the last screencast frame.
await page.screenshot({ path: join(outDir, "last.png") });
const tEnd = Date.now() / 1000;
await cdp.send("Page.stopScreencast");
writeFileSync(join(outDir, "frames.json"), JSON.stringify({ frames, tKey, tEnd, last: "last.png" }, null, 1));
console.log(`${name}: ${frames.length} frames over ${(tEnd - tKey).toFixed(1)} s (${gl})`);
await browser.close();
