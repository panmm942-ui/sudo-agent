// Deterministic render-clock regression, independent of native capture / desktop state.
// Run from app/: node tests/visual/glass-motion.mjs
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from '@playwright/test';

const root = fileURLToPath(new URL('../../', import.meta.url));
const server = await createServer({ root, configFile: false, cacheDir: 'node_modules/.vite-glass-motion', server: { host: '127.0.0.1', port: 0 } });
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true, args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const results = [];
  for (const dpr of [1, 1.25, 1.5, 2]) {
    const page = await browser.newPage({ viewport: { width: 640, height: 280 }, deviceScaleFactor: dpr });
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.route('**/motion-fixture.html', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><style>body{margin:0}</style><canvas style="width:640px;height:280px"></canvas>' }));
    await page.goto(`${server.resolvedUrls.local[0]}motion-fixture.html`);
    const result = await page.evaluate(async () => {
      const { GlassRenderer } = await import('/src/gl/renderer.ts');
      const { Spring, SPRINGS } = await import('/src/island/spring.ts');
      const canvas = document.querySelector('canvas');
      const renderer = new GlassRenderer(canvas);
      let layoutReads = 0;
      for (const key of ['clientWidth','clientHeight']) {
        const descriptor=Object.getOwnPropertyDescriptor(Element.prototype,key);
        Object.defineProperty(canvas,key,{configurable:true,get(){layoutReads++;return descriptor.get.call(this);}});
      }
      renderer.setAppearance('glass', 0, 1, 0);
      renderer.setShape(246, 44, 22);
      const gl = canvas.getContext('webgl2');
      const pixels = () => {
        const data = new Uint8Array(canvas.width * canvas.height * 4);
        gl.readPixels(0, 0, canvas.width, canvas.height, gl.RGBA, gl.UNSIGNED_BYTE, data);
        return data;
      };
      renderer.render(0);
      const initial = pixels();
      const scoped = gl.scissor.bind(gl);
      gl.scissor = () => scoped(0,0,canvas.width,canvas.height);
      renderer.render(0);
      const full = pixels();
      const scissorMatchesFull = initial.every((value,index)=>value===full[index]);
      gl.scissor = scoped;
      renderer.render(9312);
      const still = pixels();
      const stationaryUnchanged = initial.every((v, i) => v === still[i]);
      const width = new Spring(246, SPRINGS.snappy);
      width.setTarget(440);
      let previous = initial, repeatedFrames = 0;
      const widths = [];
      for (let frame = 0; frame < 8; frame++) {
        width.step(0.002);
        widths.push(width.x);
        renderer.setShape(width.x, 44, 22);
        renderer.render((frame + 1) * 2);
        const next = pixels();
        if (next.every((v, i) => v === previous[i])) repeatedFrames++;
        previous = next;
      }
      const error = gl.getError();
      renderer.dispose();
      return { layoutReads, scissorMatchesFull, stationaryUnchanged, repeatedFrames, widths, error };
    });
    results.push({ dpr, ...result });
    assert.deepEqual(errors, [], 'browser/shader errors');
    assert.ok(result.scissorMatchesFull, 'scissor must preserve every rendered pixel');
    assert.equal(result.error, 0, 'WebGL error');
    assert.equal(result.layoutReads, 0, 'drawing must not read DOM layout dimensions');
    assert.ok(result.stationaryUnchanged, 'a stationary glass border must not depend on animation time');
    assert.equal(result.repeatedFrames, 0, `DPR ${dpr}: short display frames must advance the rendered border`);
    await page.close();
  }
  console.log(JSON.stringify(results, null, 2));
} finally {
  await browser?.close();
  await server.close();
}
