import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { expect, test } from '@playwright/test';

const cases = [
  ['sites', 1504, 964],
  ['add-site', 1504, 964],
  ['site-advanced', 1504, 964],
  ['model-settings', 1504, 964],
  ['general', 1504, 964],
  ['delete-confirm', 1504, 964],
  ['empty', 1504, 964],
  ['tray-healthy', 480, 400],
  ['tray-alerts', 520, 520],
] as const;

for (const [view, width, height] of cases) {
  test(`${view} matches the approved design`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await page.goto(`/?view=${view}`);
    await page.locator('body').waitFor();
    await page.evaluate(() => document.fonts.ready);

    const outputDirectory = path.resolve('artifacts/ui-candidate');
    await mkdir(outputDirectory, { recursive: true });
    await page.screenshot({ path: path.join(outputDirectory, `${view}.png`) });
    await expect(page).toHaveScreenshot(`${view}.png`, {
      animations: 'disabled',
      caret: 'hide',
      maxDiffPixelRatio: 0.08,
    });
  });
}
