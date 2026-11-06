import { expect, test } from "@playwright/test";
import { ISLAND_STATES } from "../../src/island/machine";

for (const state of ISLAND_STATES) {
  test(`island ${state}`, async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
    await page.goto(`/?state=${state}&frozen=1&backdrop=fixture`);
    await page.waitForSelector('body[data-island-ready="1"]', { state: "attached" });
    expect(errors).toEqual([]);
    await expect(page).toHaveScreenshot(`${state}.png`);
  });
}
