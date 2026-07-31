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

test.use({ colorScheme: 'dark', reducedMotion: 'reduce' });

for (const [view, width, height] of cases) {
  test(`${view} uses a coherent dark surface`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await page.goto(`/?view=${view}&theme=dark`);
    await expect(page.locator('[data-theme="dark"]')).toBeVisible();
    await page.evaluate(() => document.fonts.ready);

    const customSurfaces = await page.evaluate(() => {
      const selectors = [
        '[data-testid="desktop-frame"]',
        '[data-testid="sidebar"]',
        '[data-testid="content"]',
        '[data-testid="site-detail"]',
        '[data-testid="tray-panel"]',
      ];
      return selectors.flatMap((selector) => {
        const element = document.querySelector(selector);
        return element ? [getComputedStyle(element).backgroundColor] : [];
      });
    });

    expect(customSurfaces.length).toBeGreaterThan(0);
    expect(customSurfaces).not.toContain('rgb(255, 255, 255)');

    const outputDirectory = path.resolve('artifacts/ui-candidate-dark');
    await mkdir(outputDirectory, { recursive: true });
    await page.screenshot({ path: path.join(outputDirectory, `${view}.png`) });
  });
}
