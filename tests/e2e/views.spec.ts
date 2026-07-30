import { expect, test } from '@playwright/test';

const mainViews = [
  ['sites', '站点管理'],
  ['add-site', '添加站点'],
  ['site-advanced', '高级连接'],
  ['model-settings', '模型探针设置'],
  ['general', '通用设置'],
  ['records', '探测记录'],
  ['delete-confirm', '删除“OpenAI 主线路”？'],
  ['empty', '还没有站点'],
] as const;

for (const [view, expectedText] of mainViews) {
  test(`${view} is directly addressable`, async ({ page }) => {
    await page.setViewportSize({ width: 1504, height: 964 });
    await page.goto(`/?view=${view}`);
    await expect(page.getByText(expectedText, { exact: false }).first()).toBeVisible();
  });
}

test('unknown view falls back to sites', async ({ page }) => {
  await page.goto('/?view=unknown');
  await expect(page.getByRole('heading', { name: '站点管理' })).toBeVisible();
});

test('fixture scenarios expose loading, empty, error and long text', async ({ page }) => {
  await page.goto('/?view=sites&scenario=loading');
  await expect(page.getByText('正在加载 Mock 数据')).toBeVisible();

  await page.goto('/?view=sites&scenario=empty');
  await expect(page.getByRole('heading', { name: '还没有站点' })).toBeVisible();

  await page.goto('/?view=sites&scenario=error');
  await expect(page.getByRole('button', { name: /重\s*试/ })).toBeVisible();

  await page.goto('/?view=sites&scenario=long-text');
  await expect(page.getByTitle('OpenAI 企业级多区域故障转移与超长站点名称验证线路')).toBeVisible();
});

test('explicit theme previews override the operating system preference', async ({ page }) => {
  await page.goto('/?view=sites&theme=light');
  await expect(page.locator('[data-theme="light"]')).toBeVisible();

  await page.goto('/?view=sites&theme=dark');
  await expect(page.locator('[data-theme="dark"]')).toBeVisible();
});

test('saving general settings applies the selected theme', async ({ page }) => {
  await page.goto('/?view=general&theme=dark');
  await page.getByText('浅色', { exact: true }).click();
  await page.getByRole('button', { name: '保存设置' }).click();
  await expect(page.locator('[data-theme="light"]')).toBeVisible();
});

test('new sites are enabled by default', async ({ page }) => {
  await page.goto('/?view=add-site');
  await expect(page.getByRole('checkbox', { name: '创建后立即启用站点' })).toBeChecked();
});

test('model settings blocks reserved parameters', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=model-settings&model=gpt-5-4');
  await page.getByRole('button', { name: '添加参数' }).click();
  const parameterRow = page.locator('[class*="paramRow"]').last();
  await parameterRow.locator('input').first().fill('model');
  await expect(page.getByText('保留字段 model 由 RelayPulse 管理，不能覆盖。')).toHaveClass(
    /parameterError/,
  );
  await expect(page.getByRole('button', { name: '保存设置' })).toBeDisabled();
});

test('site defaults open from the strategy section and the duplicate add action is removed', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=sites&theme=light');

  await expect(
    page.locator('[class*="pageHeader"]').getByRole('button', { name: '添加站点', exact: true }),
  ).toHaveCount(0);
  await page.getByRole('button', { name: '编辑默认值' }).click();
  await expect(page.getByText('站点默认探针策略', { exact: true }).first()).toBeVisible();
  await expect(page).toHaveURL(/view=model-settings/);
  await expect(page).toHaveURL(/scope=site-default/);

  await page.getByText('自定义策略', { exact: true }).click();
  await page.getByLabel('探针提示词').fill('ping');
  await page.getByRole('button', { name: '保存设置' }).click();
  await expect(page.getByText('自定义策略', { exact: true })).toBeVisible();
  await expect(page.getByText('使用最低推理 · 最多输出 8 tokens · ping')).toBeVisible();
  await page.getByRole('button', { name: '编辑默认值' }).click();

  await expect(page.locator('.ant-segmented-item-selected')).toContainText('自定义策略');
  await expect(page.getByLabel('探针提示词')).toHaveValue('ping');
});

test('models can be added and configured from icon actions', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=sites&theme=light');

  await expect(page.getByRole('button', { name: '设置模型 gpt-5.4-mini' })).toBeVisible();
  await expect(page.getByText('设置 ›', { exact: true })).toHaveCount(0);

  await page.getByRole('button', { name: /添加模型/ }).click();
  const addModelDialog = page.getByRole('dialog', { name: '添加模型' });
  await expect(addModelDialog).toBeVisible();
  await expect(page.getByRole('checkbox', { name: '添加后立即启用模型' })).toBeChecked();
  await page.getByRole('textbox', { name: '模型名称' }).fill('gpt-new-preview');
  await addModelDialog.getByRole('button', { name: '添加模型', exact: true }).click();

  await expect(page.getByText('gpt-new-preview', { exact: true })).toBeVisible();
  await expect(page.getByText('5 个模型 · 响应偏慢', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '设置模型 gpt-new-preview' }).click();
  await expect(page.getByText('gpt-new-preview · OpenAI 主线路')).toBeVisible();
});

test('site drafts only show actions after edits and guard site switching', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=sites&theme=light');

  await expect(page.getByRole('button', { name: '保存更改' })).toHaveCount(0);
  await page.getByRole('switch', { name: '停用模型 gpt-5.4-mini' }).click();
  await expect(page.getByRole('button', { name: '保存更改' })).toHaveCount(0);

  const siteName = page.getByRole('textbox', { name: '站点名称' });
  await siteName.fill('OpenAI 草稿线路');
  await expect(page.getByText('有未保存的更改')).toBeVisible();
  await expect(page.getByRole('button', { name: '保存更改' })).toBeVisible();

  await page.locator('[data-site-id="claude-backup"]').click();
  const switchDialog = page.getByRole('dialog', { name: '保存当前更改？' });
  await expect(switchDialog).toBeVisible();
  await switchDialog.getByRole('button', { name: '继续编辑' }).click();
  await expect(switchDialog).toBeHidden();
  await expect(siteName).toHaveValue('OpenAI 草稿线路');

  await page.locator('[data-site-id="claude-backup"]').click();
  await switchDialog.getByRole('button', { name: '放弃并切换' }).click();
  await expect(page.getByRole('heading', { name: 'Claude 备用线路' })).toBeVisible();

  await page.locator('[data-site-id="openai-main"]').click();
  await expect(siteName).toHaveValue('OpenAI 主线路');
  await siteName.fill('OpenAI 已保存线路');
  await page.locator('[data-site-id="claude-backup"]').click();
  await switchDialog.getByRole('button', { name: '保存并切换' }).click();
  await expect(page.getByRole('heading', { name: 'Claude 备用线路' })).toBeVisible();

  await page.locator('[data-site-id="openai-main"]').click();
  await expect(siteName).toHaveValue('OpenAI 已保存线路');
  await expect(page.getByRole('button', { name: '保存更改' })).toHaveCount(0);
});

test('individual models can override the shared probe endpoint', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=model-settings&model=gpt-4-1-mini&theme=light');

  await page.getByRole('switch', { name: '使用自定义探针请求' }).click();
  await page.getByRole('textbox', { name: '自定义探测端点' }).fill('/v1/images/generations');
  await page.getByRole('button', { name: '添加参数' }).click();
  const parameterRow = page.locator('[class*="paramRow"]').last();
  const parameterKeyInput = parameterRow.locator('input').first();
  const parameterValueInput = parameterRow.locator('input').nth(1);
  await parameterKeyInput.pressSequentially('prompt');
  await expect(parameterKeyInput).toBeFocused();
  await expect(parameterKeyInput).toHaveValue('prompt');
  await parameterValueInput.pressSequentially('生成一个白色方块');
  await expect(parameterValueInput).toBeFocused();
  await expect(parameterValueInput).toHaveValue('生成一个白色方块');

  await expect(page.locator('pre')).toContainText(
    'POST https://api.openai.com/v1/images/generations',
  );
  await expect(page.locator('pre')).toContainText('"prompt": "生成一个白色方块"');
  await expect(page.locator('pre')).not.toContainText('"stream"');
  await page.getByRole('button', { name: '试探一次' }).click();
  await expect(page.getByText('请求配置校验通过；当前 Mock 阶段未发送网络请求。')).toBeVisible();
  await page.getByRole('button', { name: '保存设置' }).click();

  const modelRow = page.getByRole('row', { name: /gpt-4\.1-mini/ });
  await expect(modelRow).toContainText('自定义探针');
  await expect(modelRow).toContainText('—');
  await page.getByRole('button', { name: '设置模型 gpt-4.1-mini' }).click();
  await expect(page.getByRole('switch', { name: '使用自定义探针请求' })).toBeChecked();
  await expect(page.getByRole('textbox', { name: '自定义探测端点' })).toHaveValue(
    '/v1/images/generations',
  );
});

test('records expose probe details, token usage and safe clearing', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=records&theme=light');

  await expect(page.getByText('共 11 条')).toBeVisible();
  const firstRecord = page.getByRole('row', {
    name: /OpenAI 主线路 gpt-5\.4-mini 278 ms 1\.12 s/,
  });
  await firstRecord.getByRole('button', { name: '查看 gpt-5.4-mini 探测详情' }).click();
  const detailDrawer = page.getByRole('dialog', { name: '探测详情' });
  await expect(detailDrawer).toBeVisible();
  await expect(detailDrawer.getByText('供应商上报')).toBeVisible();
  await expect(detailDrawer.getByText('https://api.openai.com/v1/chat/completions')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(detailDrawer).toBeHidden();

  await page.getByText('Token 统计', { exact: true }).click();
  await expect(page.getByText('129')).toBeVisible();
  await expect(page.getByRole('row', { name: /OpenAI 主线路/ })).toBeVisible();
  await expect(page.getByRole('row', { name: /Claude 备用线路/ })).toContainText('—');

  await page.getByRole('button', { name: '更多记录操作' }).click();
  await page.getByText('清理全部记录', { exact: true }).click();
  const clearDialog = page.getByRole('dialog', { name: '清理全部探测记录？' });
  await expect(clearDialog).toBeVisible();
  await clearDialog.getByRole('button', { name: /取\s*消/ }).click();
  await expect(clearDialog).toBeHidden();
  await expect(page.getByText('129')).toBeVisible();
});

test('tray views keep healthy compact and alerts capped at three', async ({ page }) => {
  await page.setViewportSize({ width: 480, height: 400 });
  await page.goto('/?view=tray-healthy');
  await expect(page.getByRole('heading', { name: '全部正常' })).toBeVisible();
  await expect(page.locator('[class*="trayAlertRow"]')).toHaveCount(0);

  await page.setViewportSize({ width: 520, height: 520 });
  await page.goto('/?view=tray-alerts');
  await expect(page.locator('[class*="trayAlertRow"]')).toHaveCount(3);
  await expect(page.getByText('查看全部 5 个异常')).toBeVisible();
});
