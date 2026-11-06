import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

// Unit tests only; Playwright visual specs under tests/ are run by `npx playwright test`.
// The svelte plugin compiles components for the SSR content tests.
export default defineConfig({
  plugins: [svelte()],
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
