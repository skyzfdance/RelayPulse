import { expect, test } from '@playwright/test';

test('notification settings support draft, discard, save and test feedback', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=notifications&theme=light');

  await expect(page.getByRole('heading', { name: '消息通知' })).toBeVisible();
  await expect(
    page.getByTestId('sidebar').getByRole('button', { name: '消息通知' }),
  ).toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('switch', { name: '启用系统通知' })).toHaveCount(0);

  const failedRule = page.getByRole('switch', { name: '启用模型探测故障通知' });
  const save = page.getByRole('button', { name: '保存设置' });
  const discard = page.getByRole('button', { name: '放弃更改' });
  const testNotification = page.getByRole('button', { name: '发送测试通知' });
  await expect(save).toBeDisabled();
  await expect(discard).toBeDisabled();

  await failedRule.click();
  await expect(failedRule).not.toBeChecked();
  await expect(save).toBeEnabled();
  await discard.click();
  await expect(failedRule).toBeChecked();
  await expect(save).toBeDisabled();

  await failedRule.click();
  await save.click();
  await expect(page.getByText('通知设置已保存', { exact: true })).toBeVisible();
  await expect(save).toBeDisabled();

  await testNotification.click();
  await expect(page.getByText('测试通知已发送', { exact: true })).toBeVisible();
});

test('master switch disables testing while preserving editable child rules', async ({ page }) => {
  await page.goto('/?view=notifications&theme=light');

  const master = page.getByRole('switch', { name: '启用消息通知' });
  const recoveredRule = page.getByRole('switch', { name: '启用模型探测恢复通知' });
  const testNotification = page.getByRole('button', { name: '发送测试通知' });
  await expect(testNotification).toBeEnabled();

  await master.click();
  await expect(testNotification).toBeDisabled();
  await recoveredRule.click();
  await expect(recoveredRule).not.toBeChecked();
});

test('permission, empty and failure fixtures expose truthful UI states', async ({ page }) => {
  await page.goto('/?view=notifications&notificationScenario=permission-denied');
  await expect(page.getByText('授权已拒绝', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '发送测试通知' })).toBeDisabled();

  await page.goto('/?view=notifications&notificationScenario=empty');
  await expect(page.getByText('暂无可配置的通知事件', { exact: true })).toBeVisible();

  await page.goto('/?view=notifications&notificationScenario=save-error');
  const failedRule = page.getByRole('switch', { name: '启用模型探测故障通知' });
  await failedRule.click();
  await page.getByRole('button', { name: '保存设置' }).click();
  await expect(page.getByText('通知设置保存失败', { exact: true })).toBeVisible();
  await expect(failedRule).not.toBeChecked();
});

test('long event text stays inside the table at the minimum application width', async ({
  page,
}) => {
  await page.setViewportSize({ width: 960, height: 640 });
  await page.goto('/?view=notifications&theme=light&notificationScenario=long-text');

  const content = page.getByTestId('notifications-content');
  await expect(
    page.getByText('跨区域模型探测连续失败并达到稳定故障判定阈值的通知事件'),
  ).toBeVisible();
  await expect
    .poll(() => content.evaluate((element) => element.scrollWidth <= element.clientWidth))
    .toBe(true);

  const eventCell = page.locator('.notification-rule-table tbody tr').first().locator('td').first();
  const channelCell = page
    .locator('.notification-rule-table tbody tr')
    .first()
    .locator('td')
    .nth(1);
  const boxes = await Promise.all([eventCell.boundingBox(), channelCell.boundingBox()]);
  expect(boxes[0]).not.toBeNull();
  expect(boxes[1]).not.toBeNull();
  expect(boxes[0]!.x + boxes[0]!.width).toBeLessThanOrEqual(boxes[1]!.x);
});
