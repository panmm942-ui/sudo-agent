// Actual WebGL regression, independent of Tauri and the visual screenshot suite.
// Run from app/: node tests/visual/glass-blur.mjs
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from '@playwright/test';

const root = fileURLToPath(new URL('../../', import.meta.url));
const server = await createServer({ root, configFile: false, server: { host: '127.0.0.1', port: 0 } });
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true, args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const page = await browser.newPage({ viewport: { width: 640, height: 280 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.route('**/blur-fixture.html', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><canvas style="width:640px;height:280px"></canvas>' }));
  await page.goto(`${server.resolvedUrls.local[0]}blur-fixture.html`);
  const result = await page.evaluate(async () => {
    const { GlassRenderer } = await import('/src/gl/renderer.ts');
    const canvas = document.querySelector('canvas');
    const renderer = new GlassRenderer(canvas);
    renderer.setShape(600, 240, 32, 20);
    const width = 640, height = 280;
    const data = new Uint8Array(width * height * 4);
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      // Fine desktop/text detail plus a broad gradient that should survive blur.
      const value = ((Math.floor(x / 5) + Math.floor(y / 5)) % 2 ? 210 : 25) + Math.sin(x / 45) * 20;
      data[i] = data[i + 1] = data[i + 2] = value;
      data[i + 3] = 255;
    }
    renderer.setBackdrop({ width, height, data });
    const gl = canvas.getContext('webgl2');
    const sample = (blur, darkMaterial = false) => {
      renderer.setTintAlpha(darkMaterial ? 0.98 : 0.55);
      renderer.setTintColor(darkMaterial ? [0.1, 0.12, 0.16] : undefined);
      renderer.setAppearance('glass', darkMaterial ? 0.85 : 0, 1, blur);
      renderer.render(0);
      const pixels = new Uint8Array(width * height * 4);
      gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      const values = [];
      let edge = 0;
      for (let y = 90; y < 190; y++) for (let x = 100; x < 540; x++) {
        const i = (y * width + x) * 4;
        values.push(pixels[i]);
        edge += Math.abs(pixels[i] - pixels[i + 4]);
      }
      const mean = values.reduce((s, value) => s + value, 0) / values.length;
      return { pixels, blur, mean, variance: values.reduce((s, value) => s + (value - mean) ** 2, 0) / values.length, edge: edge / values.length, error: gl.getError() };
    };
    const samples = [0, 25, 50, 75, 100, 0].map(blur => sample(blur));
    const zeroUnchanged = samples[0].pixels.every((value, i) => value === samples[5].pixels[i]);
    // Native user's saved appearance: darkness 100, song-color tint, blur 97.
    // The tint used to replace 98% of the backdrop, concealing the working blur.
    const darkZero = sample(0, true);
    const darkBlur = sample(97, true);
    const darkReset = sample(0, true);
    let darkDifference = 0;
    for (let y = 90; y < 190; y++) for (let x = 100; x < 540; x++) {
      const i = (y * width + x) * 4;
      darkDifference += Math.abs(darkZero.pixels[i] - darkBlur.pixels[i]);
    }
    darkDifference /= 44000;
    const darkZeroUnchanged = darkZero.pixels.every((value, i) => value === darkReset.pixels[i]);
    // Same-size raw capture updates use texSubImage2D: cached mipmaps must be invalidated.
    for (let i = 0; i < data.length; i += 4) data[i] = data[i + 1] = data[i + 2] = 25;
    renderer.setBackdrop({ width, height, data });
    const refreshed = sample(100);
    // ImageBitmap upload and non-power-of-two/resized storage need fresh mipmaps too.
    const source = document.createElement('canvas');
    source.width = 777; source.height = 333;
    source.getContext('2d').fillRect(0, 0, source.width, source.height);
    const bitmap = await createImageBitmap(source);
    renderer.setBackdrop(bitmap);
    const resized = sample(100);
    renderer.dispose(); bitmap.close();
    return { zeroUnchanged, darkZeroUnchanged, darkDifference, darkBlurMean: darkBlur.mean, samples: samples.map(({ pixels, ...stats }) => stats), refreshed: { mean: refreshed.mean, error: refreshed.error }, resized: { mean: resized.mean, error: resized.error } };
  });
  assert.deepEqual(errors, [], 'shader/browser errors');
  assert.ok(result.zeroUnchanged, '0% glass must return pixel-for-pixel after enabling blur');
  assert.ok(result.darkZeroUnchanged, '0% dark material must return pixel-for-pixel after enabling blur');
  assert.ok(result.darkDifference > 5, 'blur must remain visible beneath the native darkness-100 song tint');
  assert.ok(result.darkBlurMean < 70, 'the blurred material must retain its dark appearance');
  for (const sample of [...result.samples, result.refreshed, result.resized]) assert.equal(sample.error, 0, 'WebGL error');
  const [zero, low, mid, high, full] = result.samples;
  assert.ok(full.edge < zero.edge * 0.1, '100% must remove fine detail rather than spread sharp copies');
  assert.ok(full.variance < zero.variance * 0.3, '100% must substantially smooth the sampled backdrop');
  assert.ok(low.variance > mid.variance && mid.variance > high.variance && high.variance > full.variance, 'increasing blur must progressively smooth detail');
  assert.ok(Math.abs(result.refreshed.mean - full.mean) > 10, 'same-size capture update must invalidate cached mipmaps');
  assert.ok(Math.abs(result.resized.mean - result.refreshed.mean) > 2, 'resized ImageBitmap must replace cached backdrop');
  console.log(JSON.stringify(result, null, 2));
} finally {
  await browser?.close();
  await server.close();
}
