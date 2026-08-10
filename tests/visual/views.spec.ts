import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { expect, test } from '@playwright/test';

const cases = [
  ['sites', 1504, 964],
  ['add-site', 1504, 964],
  ['site-advanced', 1504, 964],
  ['model-settings', 1504, 964],
  ['general', 1504, 964],
  ['intelligence-ladder', 1504, 964],
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
    if (view.startsWith('tray-')) {
      await expect(page.getByTestId('tray-panel')).toHaveAttribute('data-motion', 'open');
      await expect(page.getByTestId('tray-panel')).toHaveCSS('opacity', '1');
    }

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

test('intelligence-ladder matches the narrow approved design', async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 640 });
  await page.goto('/?view=intelligence-ladder&theme=light');
  await page.locator('body').waitFor();
  await page.evaluate(() => document.fonts.ready);

  const outputDirectory = path.resolve('artifacts/ui-candidate');
  await mkdir(outputDirectory, { recursive: true });
  await page.screenshot({ path: path.join(outputDirectory, 'intelligence-ladder-narrow.png') });
  await expect(page).toHaveScreenshot('intelligence-ladder-narrow.png', {
    animations: 'disabled',
    caret: 'hide',
    maxDiffPixelRatio: 0.08,
  });
});

test('intelligence-ladder drawer matches the approved design', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=intelligence-ladder&theme=light');
  await page.getByRole('button', { name: '打开自动获取设置' }).click();
  const drawer = page.getByRole('dialog', { name: '自动获取设置' });
  await expect(drawer).toBeVisible();
  await expect
    .poll(() => drawer.evaluate((element) => Math.round(element.getBoundingClientRect().right)))
    .toBe(1504);
  await page.evaluate(() => document.fonts.ready);

  const outputDirectory = path.resolve('artifacts/ui-candidate');
  await mkdir(outputDirectory, { recursive: true });
  await page.screenshot({ path: path.join(outputDirectory, 'intelligence-ladder-drawer.png') });
  await expect(page).toHaveScreenshot('intelligence-ladder-drawer.png', {
    animations: 'disabled',
    caret: 'hide',
    maxDiffPixelRatio: 0.08,
  });
});
