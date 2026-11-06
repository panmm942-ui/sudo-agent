import { expect, test } from "@playwright/test";

// Live (unfrozen) run of the mock demo script (dev key D) with the mock voice: every state transition,
// content crossfade and the orb pass must run without page errors and end back at compact.
test("demo script runs end to end", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto("/?mock=voice&backdrop=fixture");
  await page.waitForSelector('body[data-island-ready="1"]', { state: "attached" });
  // Record every state in-page (polling from the test misses short states under SwiftShader).
  await page.evaluate(() => {
    const el = document.querySelector(".content")!;
    const seen: string[] = ((window as any).__seen = []);
    new MutationObserver(() => seen.push(el.getAttribute("data-state") ?? "")).observe(el, {
      attributeFilter: ["data-state"],
    });
  });
  // Start the demo only once the island is up (SwiftShader makes the first frame slow).
  await page.keyboard.press("d");
  await page.waitForFunction(
    () => {
      const seen: string[] = (window as any).__seen;
      return seen.includes("responding") && seen.at(-1) === "compact";
    },
    undefined,
    { timeout: 30_000 },
  );
  const seen: string[] = await page.evaluate(() => (window as any).__seen);
  for (const s of ["listening", "thinking", "acting", "responding", "compact"]) expect(seen).toContain(s);
  expect(errors).toEqual([]);
});
