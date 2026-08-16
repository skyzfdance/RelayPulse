import { App as AntdApp, ConfigProvider } from 'antd';
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { IntelligenceLadderPage } from './IntelligenceLadderPage';
import { MockIntelligenceLadderDataSource } from './mock-intelligence-ladder-data-source';
import { UnavailableIntelligenceLadderDataSource } from './unavailable-intelligence-ladder-data-source';
import type { IntelligenceLadderDataSource } from './intelligence-ladder-types';

function renderPage(dataSource: IntelligenceLadderDataSource) {
  return render(
    <ConfigProvider>
      <AntdApp>
        <div id="relaypulse-overlay-root">
          <IntelligenceLadderPage dataSource={dataSource} />
        </div>
      </AntdApp>
    </ConfigProvider>,
  );
}

afterEach(cleanup);

describe('IntelligenceLadderPage', () => {
  it('renders the fixed header, 2-1-3 podium and ranking card grid', async () => {
    renderPage(new MockIntelligenceLadderDataSource());

    expect(await screen.findByRole('heading', { name: '智力天梯' })).toBeVisible();
    expect(screen.getByRole('button', { name: '刷新智力天梯' })).toBeVisible();
    expect(screen.getByRole('button', { name: '打开自动获取设置' })).toBeVisible();

    const podium = screen.getByRole('region', { name: '前三名' });
    expect(
      within(podium)
        .getAllByRole('article')
        .map((article) => article.dataset.testid),
    ).toEqual(['podium-rank-1', 'podium-rank-2', 'podium-rank-3']);
    expect(screen.getByTestId('podium-rank-2')).toHaveClass('order-1');
    expect(screen.getByTestId('podium-rank-1')).toHaveClass('order-2');
    expect(screen.getByTestId('podium-rank-3')).toHaveClass('order-3');
    expect(podium.querySelectorAll('.intelligence-podium-wreath')).toHaveLength(3);
    expect(within(podium).queryByText('XHIGH')).toBeNull();
    expect(within(podium).queryByText('ULTRA')).toBeNull();
    expect(within(podium).queryByText('MAX')).toBeNull();

    expect(screen.getByRole('heading', { name: '后续排名' })).toBeVisible();
    expect(screen.getByText('7 个模型')).toBeVisible();
    expect(screen.queryByRole('table')).toBeNull();
    expect(screen.getByTestId('ranking-entry-4')).toBeVisible();
    expect(screen.getByTestId('ranking-entry-10')).toBeVisible();
    expect(screen.getAllByRole('article')).toHaveLength(10);
    const rankingList = screen.getByRole('region', { name: '第四名及以后' });
    expect(within(rankingList).getAllByText('xhigh')).toHaveLength(3);
    expect(within(rankingList).queryByText('XHIGH')).toBeNull();
    expect(screen.getByText('数据来源：Codex Radar')).toBeVisible();
    expect(screen.queryByText(/图表|历史趋势|搜索|筛选/)).toBeNull();
  });

  it('keeps dependent drafts while auto fetch is disabled and persists saved settings', async () => {
    const user = userEvent.setup();
    renderPage(new MockIntelligenceLadderDataSource());

    await user.click(await screen.findByRole('button', { name: '打开自动获取设置' }));
    const drawer = screen.getByRole('dialog', { name: '自动获取设置' });
    expect(drawer).toBeVisible();
    expect(within(drawer).queryByText(/URL|来源地址|路由|选择器|请求头/)).toBeNull();

    const enabled = within(drawer).getByRole('switch', { name: '后台自动获取' });
    const interval = within(drawer).getByRole('combobox', { name: '获取间隔' });
    const appStart = within(drawer).getByRole('switch', { name: '应用启动时立即获取' });
    const networkRestore = within(drawer).getByRole('switch', {
      name: '网络恢复后立即获取',
    });

    expect(enabled).toBeChecked();
    expect(appStart).toBeChecked();
    expect(networkRestore).toBeChecked();
    await user.click(enabled);
    expect(interval).toBeDisabled();
    expect(appStart).toBeDisabled();
    expect(networkRestore).toBeDisabled();
    expect(appStart).toBeChecked();
    expect(networkRestore).toBeChecked();

    await user.click(enabled);
    await user.click(networkRestore);
    await user.click(within(drawer).getByRole('button', { name: /保\s*存/ }));
    expect(await screen.findByText('自动获取设置已保存')).toBeInTheDocument();
    await waitFor(() => expect(drawer).not.toBeVisible());

    await user.click(screen.getByRole('button', { name: '打开自动获取设置' }));
    expect(
      within(screen.getByRole('dialog', { name: '自动获取设置' })).getByRole('switch', {
        name: '网络恢复后立即获取',
      }),
    ).not.toBeChecked();
  }, 10_000);

  it('retains the drawer draft after a settings save failure', async () => {
    const user = userEvent.setup();
    renderPage(new MockIntelligenceLadderDataSource('save-error'));

    await user.click(await screen.findByRole('button', { name: '打开自动获取设置' }));
    const drawer = screen.getByRole('dialog', { name: '自动获取设置' });
    const networkRestore = within(drawer).getByRole('switch', {
      name: '网络恢复后立即获取',
    });
    await user.click(networkRestore);
    await user.click(within(drawer).getByRole('button', { name: /保\s*存/ }));

    expect(await screen.findByText('自动获取设置保存失败')).toBeInTheDocument();
    expect(drawer).toBeVisible();
    expect(networkRestore).not.toBeChecked();
    expect(within(drawer).getByRole('button', { name: /保\s*存/ })).toBeEnabled();
  });

  it('shows the busy message without clearing the previous ranking', async () => {
    const user = userEvent.setup();
    renderPage(new MockIntelligenceLadderDataSource('refreshing'));

    const refresh = await screen.findByRole('button', { name: '刷新智力天梯' });
    expect(refresh).toHaveAttribute('aria-busy', 'true');
    await user.click(refresh);
    await waitFor(() => expect(screen.getAllByText('数据刷新中...').length).toBeGreaterThan(1));
    expect(screen.getByTestId('podium-rank-1')).toBeVisible();
    expect(screen.getByText('10')).toBeVisible();
  });

  it('keeps last-known-good entries visible for failure and circuit-open states', async () => {
    const { unmount } = renderPage(new MockIntelligenceLadderDataSource('stale-error'));
    expect(await screen.findByText('上次刷新失败')).toBeVisible();
    expect(screen.getByTestId('podium-rank-1')).toBeVisible();
    unmount();

    renderPage(new MockIntelligenceLadderDataSource('scraper-unavailable'));
    expect(await screen.findByText('采集服务不可用')).toBeVisible();
    expect(screen.getByTestId('podium-rank-1')).toBeVisible();
  });

  it('distinguishes first empty, no-data failure and production not-connected states', async () => {
    const empty = renderPage(new MockIntelligenceLadderDataSource('empty'));
    expect(await screen.findByText('暂无排名数据')).toBeVisible();
    empty.unmount();

    const failed = renderPage(new MockIntelligenceLadderDataSource('load-error'));
    expect(await screen.findByText('暂时无法获取排名')).toBeVisible();
    expect(screen.getByRole('button', { name: '打开自动获取设置' })).toBeEnabled();
    failed.unmount();

    const user = userEvent.setup();
    renderPage(new UnavailableIntelligenceLadderDataSource());
    expect(await screen.findByText('智力天梯数据源尚未接入')).toBeVisible();
    expect(screen.getByRole('button', { name: '刷新智力天梯' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: '打开自动获取设置' }));
    const drawer = screen.getByRole('dialog', { name: '自动获取设置' });
    expect(within(drawer).getByRole('switch', { name: '后台自动获取' })).toBeDisabled();
    expect(within(drawer).getByRole('button', { name: /保\s*存/ })).toBeDisabled();
  });

  it('renders untrusted long text as text and never creates injected elements', async () => {
    renderPage(new MockIntelligenceLadderDataSource('long-text'));
    const malicious = '<img src=x onerror=alert(1)> GPT-5.6 跨区域超长模型名称与字符安全显示验证';

    expect(await screen.findByTitle(malicious)).toHaveTextContent(malicious);
    expect(document.querySelector('img')).toBeNull();
    expect(document.querySelectorAll('img, [onerror]')).toHaveLength(0);
  });
});
