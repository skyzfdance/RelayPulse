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
  await expect(page.getByText('测试通知已提交到系统通知中心', { exact: true })).toBeVisible();
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

test('notification template editor keeps cursor insertion and page draft boundaries', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=notifications&theme=light');

  await page.getByRole('button', { name: '编辑 模型探测故障 通知模板' }).click();
  const editor = page.getByTestId('notification-template-editor');
  await expect(editor).toBeVisible();
  await expect(page.getByRole('textbox', { name: '通知标题' })).toBeDisabled();

  await page.getByText('自定义模板', { exact: true }).click();
  const body = page.getByRole('textbox', { name: '通知详情' });
  const bodyEditor = page.getByTestId('notification-template-body-editor');
  await expect(bodyEditor.getByRole('button', { name: '插入变量' })).toBeVisible();
  await expect(bodyEditor.getByRole('textbox', { name: '通知详情' })).toBeVisible();
  await body.fill('前后');
  await body.evaluate((element) => {
    const textArea = element as HTMLTextAreaElement;
    textArea.focus();
    textArea.setSelectionRange(1, 1);
  });
  await bodyEditor.getByRole('button', { name: '插入变量' }).click();
  await page.getByText('模型名称', { exact: true }).last().click();
  await expect(body).toHaveValue('前{{model_name}}后');

  const undoModifier = await page.evaluate(() =>
    navigator.platform.toLowerCase().includes('mac') ? 'Meta' : 'Control',
  );
  await body.press(`${undoModifier}+z`);
  await expect(body).toHaveValue('前后');
  await body.press(`${undoModifier}+Shift+z`);
  await expect(body).toHaveValue('前{{model_name}}后');

  await body.evaluate((element) =>
    element.dispatchEvent(
      new CompositionEvent('compositionstart', { bubbles: true, data: '恢复' }),
    ),
  );
  await body.fill('{{recovery_state}}');
  await page.waitForTimeout(220);
  await expect(page.getByText(/不适用于当前事件/)).toHaveCount(0);
  await body.evaluate((element) =>
    element.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '恢复' })),
  );
  await expect(page.getByText(/不适用于当前事件/)).toBeVisible();
  await expect(page.getByRole('button', { name: /应\s*用/ })).toBeDisabled();

  await body.fill('{{site_name}} · {{model_name}}：{{failure_reason}}');
  await expect(page.getByText('东京主站 · GPT-5：探测请求超时', { exact: true })).toBeVisible();
  await page.getByText('默认模板', { exact: true }).click();
  await expect(body).toBeDisabled();
  await page.getByText('自定义模板', { exact: true }).click();
  await expect(body).toHaveValue('{{site_name}} · {{model_name}}：{{failure_reason}}');

  await page.getByRole('button', { name: '恢复默认模板' }).click();
  await page.getByRole('button', { name: /恢\s*复\s*默\s*认$/, exact: true }).click();
  await expect(body).toBeDisabled();
  await page.getByRole('button', { name: '撤销恢复' }).click();
  await expect(body).toHaveValue('{{site_name}} · {{model_name}}：{{failure_reason}}');

  await page.getByRole('button', { name: '测试当前模板' }).click();
  await expect(page.getByText('当前模板已提交到系统通知中心', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: /应\s*用/ }).click();

  await expect(page.getByText('自定义', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '保存设置' })).toBeEnabled();

  page.once('dialog', async (dialog) => dialog.dismiss());
  await page.getByTestId('sidebar').getByRole('button', { name: '站点管理' }).click();
  await expect(page.getByRole('heading', { name: '消息通知' })).toBeVisible();

  await page.getByRole('button', { name: '保存设置' }).click();
  await expect(page.getByText('通知设置已保存', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: '编辑 模型探测恢复 通知模板' }).click();
  await page.getByText('自定义模板', { exact: true }).click();
  await page.getByRole('button', { name: '插入变量' }).click();
  await expect(page.getByText('恢复状态', { exact: true })).toBeVisible();
  await expect(page.getByText('故障原因', { exact: true })).toHaveCount(0);
});

test('permission, empty and failure fixtures expose truthful UI states', async ({ page }) => {
  await page.goto('/?view=notifications&notificationScenario=permission-not-requested');
  await expect(page.getByText('尚未授权', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '请求授权' }).click();
  await expect(page.getByText('系统通知权限已授权', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '发送测试通知' })).toBeEnabled();

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
