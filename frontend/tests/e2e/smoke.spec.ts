import { expect, test } from '@playwright/test';

test('分组全展开菜单导航与 404 错误页', async ({ page }) => {
  test.setTimeout(60_000);
  await page.goto('/');

  // 默认重定向到配置模块的站点管理，头部标题由路由 meta 驱动
  await expect(page.getByRole('heading', { name: '站点管理' })).toBeVisible();
  await expect(page.getByText('配置多个模型站点，并通过拖动顺序决定故障时的优先级')).toBeVisible();
  await expect(page).toHaveURL(/\/config\/sites$/);

  const menu = page.getByRole('menu');
  await expect(menu.getByRole('menuitem')).toHaveCount(4);
  await expect(menu.getByText('情报', { exact: true })).toHaveCount(0);
  await expect(menu.getByText('测试', { exact: true })).toHaveCount(0);
  for (const name of ['操作记录', '智力天梯', '智商测试', '历史记录', '题库管理']) {
    await expect(menu.getByRole('menuitem', { name })).toHaveCount(0);
  }

  // 分组全展开：菜单项直接点击切换路由
  await page.getByRole('menuitem', { name: '探测记录' }).click();
  await expect(page.getByRole('heading', { name: '探测记录' })).toBeVisible();
  await expect(page).toHaveURL(/\/run\/records$/);

  await page.getByRole('menuitem', { name: '通用设置' }).click();
  await expect(page.getByRole('heading', { name: '通用设置' })).toBeVisible();
  await expect(page).toHaveURL(/\/config\/general$/);

  // 隐藏路由测试详情可直达
  await page.goto('/iq-test/detail/abc');
  await expect(page.getByRole('heading', { name: '测试详情' })).toBeVisible();

  // 未知地址落入 404 独立整页（根记录无布局，错误页不带侧边栏）
  await page.goto('/definitely-not-exist');
  await expect(page.getByText('抱歉，您访问的页面不存在。')).toBeVisible();
  await page.getByRole('button', { name: '返回首页' }).click();
  await expect(page).toHaveURL(/\/config\/sites$/);
});

test('明暗主题默认与持久化', async ({ page }) => {
  // 默认跟随系统，Playwright colorScheme 为 light
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('localStorage 持久化的深色主题在刷新后保持', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('relaypulse-vue-theme-mode', 'dark'));

  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});
