import { defineConfig } from "@playwright/test";

// Glass lab: renders the glass shader over the dusk and a bright fixture for visual review
// (tests/visual/glass-lab.html via a plain Vite server rooted at tests/visual). Writes PNGs to
// docs/perf/glass-lab/; nothing is asserted against goldens.
// Run: npx playwright test -c playwright.glass.config.ts
const PORT = 4393;

export default defineConfig({
  testDir: "tests/visual",
  testMatch: "glass-lab.spec.ts",
  timeout: 180_000,
  workers: 1,
  reporter: "list",
  metadata: { glassLab: true },
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    browserName: "chromium",
    viewport: { width: 640, height: 280 },
    deviceScaleFactor: 1,
    launchOptions: { args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] },
  },
  webServer: {
    command: `node node_modules/vite/bin/vite.js tests/visual --host 127.0.0.1 --port ${PORT} --strictPort`,
    url: `http://127.0.0.1:${PORT}/glass-lab.html`,
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
