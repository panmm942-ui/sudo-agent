import { defineConfig } from "@playwright/test";

// Standalone orb preview (tests/visual/orb-preview.html) served by a plain Vite server rooted at
// tests/visual (no config file there), so neither the SvelteKit app nor the main
// playwright.config.ts is involved.
// Run: npx playwright test -c playwright.orb.config.ts
const PORT = 1431;

export default defineConfig({
  testDir: "tests/visual",
  testMatch: "orb.spec.ts",
  timeout: 300_000,
  workers: 1,
  reporter: "list",
  metadata: { orbPreview: true },
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    browserName: "chromium",
    viewport: { width: 560, height: 400 },
    deviceScaleFactor: 2,
    launchOptions: { args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] },
  },
  webServer: {
    command: `node node_modules/vite/bin/vite.js tests/visual --host 127.0.0.1 --port ${PORT} --strictPort`,
    url: `http://127.0.0.1:${PORT}/orb-preview.html`,
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
