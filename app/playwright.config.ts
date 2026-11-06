import { defineConfig } from "@playwright/test";

const PORT = 4391;

// Visual goldens of the island over a fixture wallpaper, rendered with SwiftShader so they do
// not depend on the host GPU.
export default defineConfig({
  testDir: "tests/visual",
  snapshotPathTemplate: "{testDir}/__screenshots__/{arg}{ext}",
  // SwiftShader is CPU-bound: a couple of workers and generous timeouts keep runs stable.
  workers: 2,
  reporter: "list",
  timeout: 180_000,
  expect: { timeout: 60_000, toHaveScreenshot: { maxDiffPixelRatio: 0.01 } },
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    viewport: { width: 640, height: 280 },
    deviceScaleFactor: 1,
    browserName: "chromium",
    launchOptions: { args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] },
  },
  webServer: {
    command: `npx vite build && npx vite preview --host 127.0.0.1 --port ${PORT} --strictPort`,
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 600_000,
  },
});
