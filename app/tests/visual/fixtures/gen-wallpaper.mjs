// Generates wallpaper.png: a deterministic, photo-like "dusk sky through branches" desktop strip
// (1280×560 = the 640×280 island window at 2×), busy enough to make refraction visible.
// Usage (from app/): node tests/visual/fixtures/gen-wallpaper.mjs
import { chromium } from "@playwright/test";
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const W = 1280;
const H = 560;
const out = join(dirname(fileURLToPath(import.meta.url)), "wallpaper.png");

function paint({ W, H, seed }) {
  let s = seed >>> 0;
  const rand = () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };

  // Value-noise fbm on a seeded lattice.
  const N = 256;
  const lattice = new Float32Array(N * N).map(() => rand());
  const smooth = (t) => t * t * (3 - 2 * t);
  const noise = (x, y) => {
    const xi = Math.floor(x), yi = Math.floor(y);
    const xf = smooth(x - xi), yf = smooth(y - yi);
    const at = (i, j) => lattice[((j & (N - 1)) * N) + (i & (N - 1))];
    const a = at(xi, yi), b = at(xi + 1, yi), c = at(xi, yi + 1), d = at(xi + 1, yi + 1);
    return a + (b - a) * xf + (c - a) * yf + (a - b - c + d) * xf * yf;
  };
  const fbm = (x, y, oct) => {
    let v = 0, amp = 0.5, f = 1;
    for (let i = 0; i < oct; i++) { v += amp * noise(x * f, y * f); f *= 2.03; amp *= 0.5; }
    return v;
  };

  const cv = document.createElement("canvas");
  cv.width = W; cv.height = H;
  const ctx = cv.getContext("2d");
  const mix = (a, b, t) => a + (b - a) * t;
  const ramp = (stops, t) => {
    for (let i = 1; i < stops.length; i++) {
      if (t <= stops[i][0]) {
        const [t0, c0] = stops[i - 1], [t1, c1] = stops[i];
        const k = (t - t0) / (t1 - t0);
        return [mix(c0[0], c1[0], k), mix(c0[1], c1[1], k), mix(c0[2], c1[2], k)];
      }
    }
    return stops[stops.length - 1][1];
  };

  // Sky + clouds, per pixel.
  const sky = [
    [0.0, [14, 30, 48]],
    [0.35, [38, 70, 98]],
    [0.62, [118, 108, 112]],
    [0.82, [214, 128, 82]],
    [1.0, [246, 190, 128]],
  ];
  const sun = [930, 600];
  const img = ctx.createImageData(W, H);
  for (let y = 0; y < H; y++) {
    const ty = y / H;
    for (let x = 0; x < W; x++) {
      let [r, g, b] = ramp(sky, ty);
      // Sun glow.
      const dx = (x - sun[0]) / W, dy = (y - sun[1]) / H;
      const glow = Math.exp(-(dx * dx * 6 + dy * dy * 3.5) * 3);
      r += 120 * glow; g += 70 * glow; b += 30 * glow;
      // Stretched cumulus/altostratus band.
      const n = fbm(x / 260 + 3.1, y / 120 + 7.7, 6);
      const warp = fbm(x / 90, y / 60, 3) * 0.25;
      const density = Math.max(0, Math.min(1, (n + warp - 0.52) * 3.2)) * (0.35 + 0.65 * ty);
      // Lit undersides warm near the sun, tops cool.
      const lit = Math.min(1, 0.45 + glow * 1.2 + ty * 0.4);
      const shade = fbm(x / 70 + 11, y / 40 + 2, 4);
      const cr = mix(70, 255, lit) * (0.75 + shade * 0.5);
      const cg = mix(84, 176, lit) * (0.75 + shade * 0.5);
      const cb = mix(110, 128, lit) * (0.75 + shade * 0.5);
      r = mix(r, cr, density); g = mix(g, cg, density); b = mix(b, cb, density);
      const i = (y * W + x) * 4;
      img.data[i] = r; img.data[i + 1] = g; img.data[i + 2] = b; img.data[i + 3] = 255;
    }
  }
  ctx.putImageData(img, 0, 0);

  // Hazy ridges on the horizon (atmospheric perspective).
  const ridge = (base, amp, scale, color, seedOff) => {
    ctx.beginPath();
    ctx.moveTo(0, H);
    for (let x = 0; x <= W; x += 4) ctx.lineTo(x, base - fbm(x / scale + seedOff, seedOff, 5) * amp);
    ctx.lineTo(W, H);
    ctx.closePath();
    ctx.fillStyle = color;
    ctx.fill();
  };
  ridge(560, 140, 220, "rgba(92, 96, 112, 0.85)", 40);
  ridge(590, 150, 160, "rgba(44, 50, 62, 0.95)", 80);

  // Bokeh from out-of-focus lights near the sun.
  ctx.globalCompositeOperation = "screen";
  for (let i = 0; i < 26; i++) {
    const x = sun[0] + (rand() - 0.5) * 900, y = 260 + rand() * 300, r = 6 + rand() * 26;
    const g = ctx.createRadialGradient(x, y, 0, x, y, r);
    const a = 0.10 + rand() * 0.22;
    g.addColorStop(0, `rgba(255, 196, 140, ${a})`);
    g.addColorStop(0.85, `rgba(255, 170, 110, ${a * 0.8})`);
    g.addColorStop(1, "rgba(255, 170, 110, 0)");
    ctx.fillStyle = g;
    ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fill();
  }
  ctx.globalCompositeOperation = "source-over";

  // Silhouetted branches across the top: high-frequency lines that make lensing obvious.
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const branch = (x, y, ang, len, w, depth) => {
    if (depth === 0 || w < 0.7) {
      // Leaf cluster.
      for (let k = 0; k < 3; k++) {
        const la = ang + (rand() - 0.5) * 2.4, ll = 7 + rand() * 9;
        ctx.save();
        ctx.translate(x, y); ctx.rotate(la);
        ctx.beginPath(); ctx.ellipse(ll * 0.6, 0, ll * 0.6, ll * 0.22, 0, 0, Math.PI * 2);
        ctx.fillStyle = `rgba(${16 + rand() * 14}, ${18 + rand() * 12}, ${18 + rand() * 8}, 0.95)`;
        ctx.fill();
        ctx.restore();
      }
      return;
    }
    const bend = (rand() - 0.5) * 0.5;
    const x2 = x + Math.cos(ang) * len, y2 = y + Math.sin(ang) * len;
    const cx = x + Math.cos(ang + bend) * len * 0.5, cy = y + Math.sin(ang + bend) * len * 0.5;
    ctx.strokeStyle = "rgb(14, 15, 18)";
    ctx.lineWidth = w;
    ctx.beginPath(); ctx.moveTo(x, y); ctx.quadraticCurveTo(cx, cy, x2, y2); ctx.stroke();
    const kids = rand() < 0.35 ? 3 : 2;
    for (let k = 0; k < kids; k++) {
      const spread = (k - (kids - 1) / 2) * (0.5 + rand() * 0.45);
      branch(x2, y2, ang + spread + (rand() - 0.5) * 0.3, len * (0.68 + rand() * 0.14), w * 0.66, depth - 1);
    }
  };
  branch(-20, 40, 0.22, 230, 22, 8);
  branch(W + 20, 110, Math.PI - 0.32, 210, 18, 8);
  branch(560, -30, 1.95, 150, 7, 5);

  // Film grain + gentle vignette.
  const fin = ctx.getImageData(0, 0, W, H);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const i = (y * W + x) * 4;
      const vx = x / W - 0.5, vy = y / H - 0.45;
      const v = 1 - 0.28 * (vx * vx + vy * vy) * 2;
      const g = (rand() - 0.5) * 9;
      for (let c = 0; c < 3; c++) fin.data[i + c] = fin.data[i + c] * v + g;
    }
  }
  ctx.putImageData(fin, 0, 0);
  return cv.toDataURL("image/png");
}

const browser = await chromium.launch();
const page = await browser.newPage();
const dataUrl = await page.evaluate(paint, { W, H, seed: 20261029 });
await browser.close();
writeFileSync(out, Buffer.from(dataUrl.split(",")[1], "base64"));
console.log(`wrote ${out}`);
