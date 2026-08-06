import { expect, test } from '@playwright/test';

const mainViews = [
  ['sites', '站点管理'],
  ['add-site', '添加站点'],
  ['site-advanced', '高级连接'],
  ['model-settings', '模型探针设置'],
  ['general', '通用设置'],
  ['notifications', '消息通知'],
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

test('main application fills the available window without a surrounding stage', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=sites&theme=light');

  await expect(page.locator('main')).toHaveCSS('width', '1504px');
  await expect(page.locator('main')).toHaveCSS('height', '964px');
  await expect(page.locator('main')).toHaveCSS('border-top-width', '0px');
  await expect(page.locator('main')).toHaveCSS('box-shadow', 'none');
});

test('brand lives in the sidebar without a duplicate application titlebar', async ({ page }) => {
  await page.goto('/?view=sites&theme=light');

  const sidebar = page.locator('aside');
  await expect(sidebar.getByText('RelayPulse', { exact: true })).toBeVisible();
  await expect(page.locator('[class*="titlebar"]')).toHaveCount(0);
  await expect(page.getByText('模型可用性探针', { exact: true })).toHaveCount(0);
});

test('sidebar keeps the current page selected after navigation', async ({ page }) => {
  await page.goto('/?view=general&theme=dark');

  const sidebar = page.getByTestId('sidebar');
  const general = sidebar.getByRole('button', { name: '通用设置' });
  await expect(general).toHaveAttribute('aria-current', 'page');
  await expect(general).not.toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');

  const records = sidebar.getByRole('button', { name: '探测记录' });
  await records.click();
  await expect(records).toHaveAttribute('aria-current', 'page');
  await expect(general).not.toHaveAttribute('aria-current', 'page');
});

test('main application leaves the titlebar and window dragging to native Tauri chrome', async ({
  page,
}) => {
  await page.goto('/?view=sites&theme=light');

  await expect(page.locator('[data-titlebar-overlay]')).toHaveCount(0);
  await expect(page.locator('[data-tauri-drag-region]')).toHaveCount(0);
});

test('fixture scenarios expose loading, empty, error and long text', async ({ page }) => {
  await page.goto('/?view=sites&scenario=loading');
  await expect(page.getByText('正在加载应用数据')).toBeVisible();

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

test('general settings apply the selected theme immediately', async ({ page }) => {
  await page.goto('/?view=general&theme=dark');
  await page.getByText('浅色', { exact: true }).click();
  await expect(page.locator('[data-theme="light"]')).toBeVisible();
  await expect(page.getByText('主题已更新', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '保存设置' })).toHaveCount(0);
});

test('general settings report and clear bounded diagnostic logs', async ({ page }) => {
  await page.goto('/?view=general&theme=light');

  await expect(page.getByText('2 个文件 · 1.75 MB / 10 MB', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '清理日志' }).click();
  await expect(page.getByText('只会删除日志文件，不会删除探测记录或站点配置。')).toBeVisible();
  await page
    .getByRole('tooltip')
    .getByRole('button', { name: /清\s*理/ })
    .click();

  await expect(page.getByText('1 个文件 · 192 B / 10 MB', { exact: true })).toBeVisible();
  await expect(page.getByText('已清理 1.75 MB 日志', { exact: true })).toBeVisible();
});

test('failed immediate settings restore the previous value', async ({ page }) => {
  await page.goto('/?view=general&theme=light&scenario=action-error');
  await page.getByText('深色', { exact: true }).click();

  await expect(page.getByText('主题更新失败', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '通用设置' })).toBeVisible();
  await expect(page.locator('[data-theme="light"]')).toBeVisible();
  await expect(page.getByText('应用数据加载失败', { exact: true })).toHaveCount(0);
});

test('lightweight backend failures use a message and keep the previous state', async ({ page }) => {
  await page.goto('/?view=sites&theme=light&scenario=action-error');
  const siteSwitch = page.getByRole('switch', { name: '停用 OpenAI 主线路' });
  await siteSwitch.click();

  await expect(page.getByText('OpenAI 主线路 停用失败', { exact: true })).toBeVisible();
  await expect(siteSwitch).toBeChecked();
  await expect(page.getByRole('heading', { name: '站点管理' })).toBeVisible();
});

test('primary content remains scrollable without a persistent scrollbar', async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 560 });
  await page.goto('/?view=general&theme=light');

  const content = page.getByTestId('general-content');
  await expect(content).toHaveCSS('overflow-y', 'auto');
  await expect(content).toHaveCSS('scrollbar-width', 'none');

  const before = await content.evaluate((element) => element.scrollTop);
  await content.hover();
  await page.mouse.wheel(0, 600);
  await expect.poll(() => content.evaluate((element) => element.scrollTop)).toBeGreaterThan(before);
});

test('new sites are enabled by default', async ({ page }) => {
  await page.goto('/?view=add-site');
  const addSiteDialog = page.getByRole('dialog', { name: '添加站点' });
  await expect(addSiteDialog.getByRole('checkbox', { name: '创建后立即启用站点' })).toBeChecked();
  await addSiteDialog.getByRole('textbox', { name: '站点名称' }).fill('测试线路');
  await addSiteDialog
    .getByRole('textbox', { name: 'Base URL' })
    .fill('https://relay.example.com/v1');
  await addSiteDialog.getByLabel('API Key').fill('fixture-key');
  await addSiteDialog.getByRole('button', { name: '连接并获取模型' }).click();
  await expect(page.getByText('站点已添加', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '测试线路' })).toBeVisible();
});

test('model settings blocks reserved parameters', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=model-settings&model=gpt-5-4');

  const settingsDrawer = page.getByRole('dialog', { name: '模型探针设置' });
  const settingsBody = settingsDrawer.locator('.ant-drawer-body');
  const requestPreview = settingsDrawer.getByTestId('request-preview');
  const customProbeSwitch = settingsDrawer.getByRole('switch', {
    name: '使用自定义探针请求',
  });
  const siteSwitch = page.getByRole('switch', { name: '停用 OpenAI 主线路' });
  await expect(settingsBody).toHaveCSS('scrollbar-width', 'none');
  await expect(customProbeSwitch).toHaveClass(/ant-switch-small/);
  await expect(siteSwitch).toHaveClass(/ant-switch-small/);
  const customSwitchSize = await customProbeSwitch.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return { width: rect.width, height: rect.height };
  });
  const siteSwitchSize = await siteSwitch.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return { width: rect.width, height: rect.height };
  });
  expect(customSwitchSize).toEqual(siteSwitchSize);
  await expect(requestPreview).toHaveCSS('max-height', 'none');
  await expect(requestPreview).toHaveCSS('overflow-y', 'visible');
  await expect(
    requestPreview.evaluate((element) => element.scrollHeight <= element.clientHeight),
  ).resolves.toBe(true);
  await expect(
    requestPreview.evaluate((element) => element.scrollWidth <= element.clientWidth),
  ).resolves.toBe(true);

  await page.getByRole('button', { name: '添加参数' }).click();
  const parameterRow = page.getByTestId('parameter-row').last();
  await parameterRow.locator('input').first().fill('model');
  await expect(page.getByText('保留字段 model 由 RelayPulse 管理，不能覆盖。')).toBeVisible();
  await expect(page.getByRole('button', { name: '保存设置' })).toBeDisabled();

  await settingsDrawer.getByRole('button', { name: '关闭' }).click();
  expect(page.url()).toContain('view=model-settings');
  await expect(settingsDrawer).toBeHidden();
  await expect(page).toHaveURL(/view=sites/);
});

test('site defaults open from the strategy section and the duplicate add action is removed', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=sites&theme=light');

  await expect(
    page.locator('.page-header').getByRole('button', { name: '添加站点', exact: true }),
  ).toHaveCount(0);
  await page.getByRole('button', { name: '编辑默认值' }).click();
  await expect(page.getByText('站点默认探针策略', { exact: true }).first()).toBeVisible();
  await expect(page).toHaveURL(/view=model-settings/);
  await expect(page).toHaveURL(/scope=site-default/);

  await page.getByText('自定义策略', { exact: true }).click();
  await page.getByLabel('探针提示词').fill('ping');
  await page.getByRole('button', { name: '保存设置' }).click();
  await expect(page.getByText('站点默认策略已保存', { exact: true })).toBeVisible();
  await expect(page).toHaveURL(/view=sites/);
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

  await page.getByRole('button', { name: '同步模型' }).click();
  await expect(page.getByText('模型列表已同步', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: /添加模型/ }).click();
  const addModelDialog = page.getByRole('dialog', { name: '添加模型' });
  await expect(addModelDialog).toBeVisible();
  await expect(page.getByRole('checkbox', { name: '添加后立即启用模型' })).toBeChecked();
  await page.getByRole('textbox', { name: '模型名称' }).fill('gpt-new-preview');
  await addModelDialog.getByRole('button', { name: '添加模型', exact: true }).click();

  await expect(page.getByText('模型已添加', { exact: true })).toBeVisible();
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
  await expect(page.getByText('已停用 gpt-5.4-mini', { exact: true })).toBeVisible();
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

test('advanced site settings persist a custom probe frequency', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=site-advanced&theme=light');

  const advancedDrawer = page.getByRole('dialog', { name: '站点高级设置' });
  await expect(advancedDrawer).toBeVisible();
  await expect(page.getByRole('heading', { name: '站点管理' })).toBeVisible();
  const interval = advancedDrawer.getByRole('spinbutton', { name: '探测频率' });
  await expect(interval).toHaveValue('10');
  await interval.fill('15');
  await advancedDrawer.getByRole('button', { name: '保存更改' }).click();
  await expect(page.getByText('站点高级设置已保存', { exact: true })).toBeVisible();
  await expect(advancedDrawer).toBeHidden();
  await expect(page).toHaveURL(/view=sites/);
});

test('saving basic site fields preserves advanced request headers', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=sites&theme=light');

  await page.getByRole('textbox', { name: '站点名称' }).fill('OpenAI 已保存线路');
  await page.getByRole('button', { name: '保存更改' }).click();
  await expect(page.getByText('站点配置已保存', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: '高级设置' }).click();
  const advancedDrawer = page.getByRole('dialog', { name: '站点高级设置' });
  await expect(advancedDrawer).toBeVisible();
  await expect(advancedDrawer.getByRole('heading', { name: '高级连接' })).toBeVisible();
  await expect(advancedDrawer.locator('input[value="X-Workspace"]')).toBeVisible();
  await expect(advancedDrawer.locator('input[value="relaypulse-prod"]')).toBeVisible();
  await advancedDrawer.getByRole('button', { name: /取\s*消/ }).click();
  await expect(advancedDrawer).toBeHidden();
  await expect(page).toHaveURL(/view=sites/);
});

test('individual models can override the shared probe endpoint', async ({ page }) => {
  await page.setViewportSize({ width: 1504, height: 964 });
  await page.goto('/?view=model-settings&model=gpt-4-1-mini&theme=light');

  await page.getByRole('switch', { name: '使用自定义探针请求' }).click();
  await page.getByRole('textbox', { name: '自定义探测端点' }).fill('/v1/images/generations');
  await page.getByRole('button', { name: '添加参数' }).click();
  const parameterRow = page.getByTestId('parameter-row').last();
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
  await expect(page.getByText('模型探针设置已保存', { exact: true })).toBeVisible();
  await expect(page).toHaveURL(/view=sites/);

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

  await page.getByRole('button', { name: '刷新探测记录' }).click();
  await expect(page.getByText('探测记录已刷新', { exact: true })).toBeVisible();

  await expect(page.getByText('共 11 条')).toBeVisible();
  const firstRecord = page.getByRole('row', {
    name: /OpenAI 主线路 gpt-5\.4-mini 278 ms 1\.12 s/,
  });
  await firstRecord.getByRole('button', { name: '查看 gpt-5.4-mini 探测详情' }).click();
  const detailDrawer = page.getByRole('dialog', { name: '探测详情' });
  await expect(detailDrawer).toBeVisible();
  const detailBody = detailDrawer.locator('.ant-drawer-body');
  await expect(detailBody).toHaveCSS('overflow-y', 'auto');
  await expect(detailBody).toHaveCSS('scrollbar-width', 'none');
  await expect(detailDrawer.getByText('供应商上报')).toBeVisible();
  await expect(detailDrawer.getByText('https://api.openai.com/v1/chat/completions')).toBeVisible();
  await detailDrawer.getByText('请求详情', { exact: true }).click();
  await expect(detailDrawer.getByText('Bearer ••••••••••••••••')).toBeVisible();
  await expect(detailDrawer.locator('pre')).toContainText('"stream": true');
  await detailDrawer.getByText('原始返回', { exact: true }).click();
  await expect(detailDrawer.getByText('流式', { exact: true })).toBeVisible();
  await expect(detailDrawer.getByText('分片 1', { exact: false })).toBeVisible();
  await expect(detailDrawer.locator('pre').last()).toContainText('data:');
  await page.keyboard.press('Escape');
  await expect(detailDrawer).toBeHidden();

  const synchronousRecord = page.getByRole('row', {
    name: /Gemini 亚洲线路 gemini-2\.5-pro 1\.82 s 6\.41 s/,
  });
  await synchronousRecord.getByRole('button', { name: '查看 gemini-2.5-pro 探测详情' }).click();
  const synchronousDrawer = page.getByRole('dialog', { name: '探测详情' });
  await synchronousDrawer.getByText('原始返回', { exact: true }).click();
  await expect(synchronousDrawer.getByText('同步', { exact: true })).toBeVisible();
  await expect(synchronousDrawer.locator('pre')).toContainText('"choices"');
  await page.keyboard.press('Escape');
  await expect(synchronousDrawer).toBeHidden();

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

  await page.getByRole('button', { name: '更多记录操作' }).click();
  await page.getByText('清理全部记录', { exact: true }).click();
  await clearDialog.getByRole('button', { name: '清理记录' }).click();
  await expect(page.getByText('探测记录已清理', { exact: true })).toBeVisible();
  await expect(page.getByText('当前周期没有可统计的探测记录')).toBeVisible();
});

test('site deletion reports its backend result', async ({ page }) => {
  await page.goto('/?view=delete-confirm&theme=light');
  await page.getByRole('dialog').getByRole('button', { name: '删除站点' }).click();

  await expect(page.getByText('站点已删除', { exact: true })).toBeVisible();
  await expect(page.locator('[data-site-id="openai-main"]')).toHaveCount(0);
  await expect(page).toHaveURL(/view=sites/);
});

test('records keep the table header and pagination fixed while rows scroll', async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 560 });
  await page.goto('/?view=records&theme=light');

  const tableBody = page.locator('.ant-table-body');
  const tableHeader = page.locator('.ant-table-header');
  const pagination = page.locator('.ant-pagination');
  const detailHeader = page.getByRole('columnheader', { name: '详情' });
  const headerTop = await tableHeader.evaluate((element) => element.getBoundingClientRect().top);
  const paginationTop = await pagination.evaluate((element) => element.getBoundingClientRect().top);
  const detailHeaderRight = await detailHeader.evaluate(
    (element) => element.getBoundingClientRect().right,
  );

  await expect(tableBody).toHaveCSS('scrollbar-width', 'thin');
  await expect(
    tableBody.evaluate((element) => {
      const scrollbar = window.getComputedStyle(element, '::-webkit-scrollbar');
      return { height: scrollbar.height, width: scrollbar.width };
    }),
  ).resolves.toEqual({ height: '4px', width: '4px' });
  await expect(pagination).toContainText('第 1-10 条，共 11 条');
  await expect(pagination.locator('.ant-pagination-options-size-changer')).toBeVisible();
  await expect(pagination.locator('.ant-pagination-options-quick-jumper')).toBeVisible();
  await expect(
    pagination.evaluate((element) => window.innerHeight - element.getBoundingClientRect().bottom),
  ).resolves.toBeGreaterThanOrEqual(12);

  await expect(
    tableBody.evaluate((element) => element.scrollHeight > element.clientHeight),
  ).resolves.toBe(true);
  await tableBody.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });

  await expect.poll(() => tableBody.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
  await expect
    .poll(() => tableHeader.evaluate((element) => element.getBoundingClientRect().top))
    .toBe(headerTop);
  await expect
    .poll(() => pagination.evaluate((element) => element.getBoundingClientRect().top))
    .toBe(paginationTop);

  await expect(detailHeader).toHaveClass(/ant-table-cell-fix-end/);
  await tableBody.evaluate((element) => {
    element.scrollLeft = element.scrollWidth;
  });
  await expect.poll(() => tableBody.evaluate((element) => element.scrollLeft)).toBeGreaterThan(0);
  await expect
    .poll(() => detailHeader.evaluate((element) => element.getBoundingClientRect().right))
    .toBe(detailHeaderRight);

  const quickJumper = pagination.locator('.ant-pagination-options-quick-jumper input');
  await quickJumper.fill('2');
  await quickJumper.press('Enter');
  await expect(pagination.locator('.ant-pagination-item-active')).toHaveText('2');

  await pagination.locator('.ant-pagination-options-size-changer').click();
  await page.getByText('20 条/页', { exact: true }).click();
  await expect(pagination.locator('.ant-pagination-options-size-changer')).toContainText(
    '20 条/页',
  );
});

test('tray views keep healthy compact and alerts capped at three', async ({ page }) => {
  await page.setViewportSize({ width: 480, height: 400 });
  await page.goto('/?view=tray-healthy');
  await expect(page.getByRole('heading', { name: '全部正常' })).toBeVisible();
  await expect(page.getByTestId('tray-alert-row')).toHaveCount(0);
  await page.getByRole('button', { name: '立即探测' }).click();
  await expect(page.getByText('探测状态已刷新', { exact: true })).toBeVisible();

  await page.setViewportSize({ width: 520, height: 520 });
  await page.goto('/?view=tray-alerts');
  await expect(page.getByTestId('tray-alert-row')).toHaveCount(3);
  await expect(page.getByText('查看全部 5 个异常')).toBeVisible();
});
