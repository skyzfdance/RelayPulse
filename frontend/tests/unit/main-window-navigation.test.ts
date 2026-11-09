import { beforeEach, expect, it, vi } from 'vitest';

import { subscribeMainWindowNavigation } from '@/api/main-window';

const native = vi.hoisted(() => ({ tauri: false, label: 'main', listen: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => native.tauri }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ label: native.label }) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: native.listen, emit: vi.fn() }));

beforeEach(() => {
  native.tauri = false;
  native.label = 'main';
  native.listen.mockReset();
});

it('浏览器预览和独立小面板不订阅主窗口导航', async () => {
  const navigate = vi.fn();
  await subscribeMainWindowNavigation(navigate);
  native.tauri = true;
  native.label = 'status-panel';
  await subscribeMainWindowNavigation(navigate);
  expect(native.listen).not.toHaveBeenCalled();
});

it('仅接收定向到主窗口的已约定路由，并返回原生取消监听函数', async () => {
  native.tauri = true;
  const stop = vi.fn();
  const navigate = vi.fn();
  native.listen.mockResolvedValue(stop);
  expect(await subscribeMainWindowNavigation(navigate)).toBe(stop);
  expect(native.listen).toHaveBeenCalledWith('main-window:navigate', expect.any(Function), {
    target: { kind: 'WebviewWindow', label: 'main' },
  });
  const receive = native.listen.mock.calls[0]![1];
  for (const route of ['/run/records', '/config/sites', '/config/general', '/panel', null, {}])
    receive({ payload: route });
  expect(navigate.mock.calls).toEqual([['/run/records'], ['/config/sites'], ['/config/general']]);
});

it('系统通知只接受指定 IQ 历史目标和有效 runId，并重建导航对象', async () => {
  native.tauri = true;
  const navigate = vi.fn();
  await subscribeMainWindowNavigation(navigate);
  const receive = native.listen.mock.calls[0]![1];
  const target = { path: '/iq-test/history', query: { runId: 'completed-run_1' } };
  receive({ payload: target });
  expect(navigate).toHaveBeenCalledExactlyOnceWith(target);
  expect(navigate.mock.calls[0]![0]).not.toBe(target);
  expect(navigate.mock.calls[0]![0].query).not.toBe(target.query);
});

it.each([
  '/iq-test/history?runId=completed-run',
  { path: '/iq-test/history' },
  { path: '/config/sites', query: { runId: 'completed-run' } },
  { path: '/iq-test/history', query: { runId: '' } },
  { path: '/iq-test/history', query: { runId: ' completed-run ' } },
  { path: '/iq-test/history', query: { runId: '../other' } },
  { path: '/iq-test/history', query: { runId: 'run?redirect=/config/sites' } },
  { path: '/iq-test/history', query: { runId: ['completed-run'] } },
  { path: '/iq-test/history', query: { runId: 123 } },
  { path: '/iq-test/history', query: { runId: 'completed-run', redirect: '/panel' } },
  { path: '/iq-test/history', query: null },
  { path: '/iq-test/history', query: ['completed-run'] },
  { path: '/iq-test/history', query: { runId: 'completed-run' }, hash: '#other' },
  { path: '/iq-test/history', query: { runId: 'completed-run' }, name: 'panel' },
])('拒绝不符合通知导航契约的目标 %j', async (payload) => {
  native.tauri = true;
  const navigate = vi.fn();
  await subscribeMainWindowNavigation(navigate);
  native.listen.mock.calls[0]![1]({ payload });
  expect(navigate).not.toHaveBeenCalled();
});
