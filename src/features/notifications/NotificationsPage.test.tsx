import { App as AntdApp, ConfigProvider } from 'antd';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createNotificationFixture } from './mock-notification-data-source';
import { NotificationsPage } from './NotificationsPage';
import type {
  NotificationDataSource,
  NotificationSettingsSnapshot,
  NotificationSettingsUpdate,
} from './notification-types';

function cloneSnapshot(snapshot: NotificationSettingsSnapshot): NotificationSettingsSnapshot {
  return {
    ...snapshot,
    channels: snapshot.channels.map((channel) => ({ ...channel })),
    eventCatalog: snapshot.eventCatalog.map((event) => ({
      ...event,
      supportedChannels: [...event.supportedChannels],
    })),
    rules: snapshot.rules.map((rule) => ({ ...rule, channelIds: [...rule.channelIds] })),
  };
}

function createDataSource({
  snapshot = createNotificationFixture(),
  saveError = false,
}: {
  snapshot?: NotificationSettingsSnapshot;
  saveError?: boolean;
} = {}) {
  let current = cloneSnapshot(snapshot);
  const updates: NotificationSettingsUpdate[] = [];
  const tests: string[] = [];
  const dataSource: NotificationDataSource = {
    async getSettings() {
      return cloneSnapshot(current);
    },
    async updateSettings(settings) {
      updates.push(settings);
      if (saveError) throw new Error('save failed');
      current = {
        ...current,
        revision: current.revision + 1,
        notificationsEnabled: settings.notificationsEnabled,
        rules: settings.rules.map((rule) => ({ ...rule, channelIds: [...rule.channelIds] })),
      };
      return cloneSnapshot(current);
    },
    async sendTestNotification(channelId) {
      tests.push(channelId);
    },
  };
  return { dataSource, updates, tests };
}

function renderPage(dataSource: NotificationDataSource) {
  return render(
    <ConfigProvider>
      <AntdApp>
        <NotificationsPage dataSource={dataSource} />
      </AntdApp>
    </ConfigProvider>,
  );
}

afterEach(cleanup);

describe('NotificationsPage', () => {
  it('renders the dynamic catalog and saves only after the draft changes', async () => {
    const user = userEvent.setup();
    const { dataSource, updates } = createDataSource();
    renderPage(dataSource);

    expect(await screen.findByText('模型探测故障')).toBeVisible();
    expect(screen.getByText('模型探测恢复')).toBeVisible();
    const save = screen.getByRole('button', { name: '保存设置' });
    const discard = screen.getByRole('button', { name: '放弃更改' });
    expect(save).toBeDisabled();
    expect(discard).toBeDisabled();

    const failedRule = screen.getByRole('switch', { name: '启用模型探测故障通知' });
    await user.click(failedRule);
    expect(failedRule).not.toBeChecked();
    expect(save).toBeEnabled();
    expect(discard).toBeEnabled();

    await user.click(save);
    await waitFor(() => expect(updates).toHaveLength(1));
    expect(updates[0]?.rules.find((rule) => rule.eventType === 'probe.model_failed')?.enabled).toBe(
      false,
    );
    expect(updates[0]).not.toHaveProperty('channels');
    expect(screen.queryByRole('switch', { name: '启用系统通知' })).not.toBeInTheDocument();
    expect(await screen.findByText('通知设置已保存')).toBeInTheDocument();
    expect(save).toBeDisabled();
  });

  it('keeps child rules editable while the master switch disables testing', async () => {
    const user = userEvent.setup();
    const { dataSource, tests } = createDataSource();
    renderPage(dataSource);

    const master = await screen.findByRole('switch', { name: '启用消息通知' });
    const testButton = screen.getByRole('button', { name: '发送测试通知' });
    const recoveredRule = screen.getByRole('switch', { name: '启用模型探测恢复通知' });
    expect(testButton).toBeEnabled();

    await user.click(master);
    expect(testButton).toBeDisabled();
    await user.click(recoveredRule);
    expect(recoveredRule).not.toBeChecked();
    expect(tests).toHaveLength(0);
  });

  it('does not test against settings that are only enabled in the unsaved draft', async () => {
    const user = userEvent.setup();
    const snapshot = createNotificationFixture();
    snapshot.notificationsEnabled = false;
    const { dataSource } = createDataSource({ snapshot });
    renderPage(dataSource);

    const master = await screen.findByRole('switch', { name: '启用消息通知' });
    const testButton = screen.getByRole('button', { name: '发送测试通知' });
    expect(testButton).toBeDisabled();

    await user.click(master);
    expect(master).toBeChecked();
    expect(testButton).toBeDisabled();

    await user.click(screen.getByRole('button', { name: '保存设置' }));
    await waitFor(() => expect(testButton).toBeEnabled());
  });

  it('retains the draft and reports an actionable save failure', async () => {
    const user = userEvent.setup();
    const { dataSource } = createDataSource({ saveError: true });
    renderPage(dataSource);

    const failedRule = await screen.findByRole('switch', {
      name: '启用模型探测故障通知',
    });
    await user.click(failedRule);
    await user.click(screen.getByRole('button', { name: '保存设置' }));

    expect(await screen.findByText('通知设置保存失败')).toBeInTheDocument();
    expect(failedRule).not.toBeChecked();
    expect(screen.getByRole('button', { name: '保存设置' })).toBeEnabled();
  });

  it('shows denied permissions without exposing a fake permission action', async () => {
    const { dataSource } = createDataSource({
      snapshot: createNotificationFixture('permission-denied'),
    });
    renderPage(dataSource);

    expect(await screen.findByText('授权已拒绝')).toBeVisible();
    expect(screen.getByRole('button', { name: '发送测试通知' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: /授权|系统设置/ })).not.toBeInTheDocument();
  });

  it.each([
    ['permission-unknown', '状态未知'],
    ['permission-not-requested', '尚未授权'],
    ['permission-unsupported', '当前平台不支持'],
  ] as const)('renders the %s permission state', async (scenario, label) => {
    const { dataSource } = createDataSource({
      snapshot: createNotificationFixture(scenario),
    });
    renderPage(dataSource);

    expect(await screen.findByText(label)).toBeVisible();
    expect(screen.getByRole('button', { name: '发送测试通知' })).toBeDisabled();
    expect(screen.queryByRole('switch', { name: '启用系统通知' })).not.toBeInTheDocument();
  });

  it('renders a stable empty state when the event catalog is empty', async () => {
    const { dataSource } = createDataSource({ snapshot: createNotificationFixture('empty') });
    renderPage(dataSource);

    expect(await screen.findByText('暂无可配置的通知事件')).toBeVisible();
    expect(screen.queryByRole('switch', { name: /模型探测/ })).not.toBeInTheDocument();
  });

  it('loads settings on mount', async () => {
    const snapshot = createNotificationFixture();
    const getSettings = vi.fn(async () => snapshot);
    const dataSource: NotificationDataSource = {
      getSettings,
      updateSettings: vi.fn(),
      sendTestNotification: vi.fn(),
    };
    renderPage(dataSource);

    expect(await screen.findByText('模型可用性')).toBeVisible();
    expect(getSettings).toHaveBeenCalledTimes(1);
  });

  it('renders an actionable load error without inventing catalog entries', async () => {
    const dataSource: NotificationDataSource = {
      async getSettings() {
        throw new Error('load failed');
      },
      updateSettings: vi.fn(),
      sendTestNotification: vi.fn(),
    };
    renderPage(dataSource);

    expect(await screen.findByText('通知设置加载失败')).toBeVisible();
    expect(screen.getByRole('button', { name: /重\s*试/ })).toBeEnabled();
    expect(screen.queryByText('模型探测故障')).not.toBeInTheDocument();
  });
});
