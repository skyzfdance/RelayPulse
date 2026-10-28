import type { NotificationPermission, NotificationSettings } from '@/types/domain/notifications';

import { invoke } from '@tauri-apps/api/core';
import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext from 'antdv-next';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import NotificationsView from '@/views/config/notifications/NotificationsView.vue';

const feedback = vi.hoisted(() => ({ success: vi.fn(), error: vi.fn() }));
vi.mock('@/composables/useFeedback', () => ({ useFeedback: () => feedback }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));

function settings(authorization: NotificationPermission = 'not_requested'): NotificationSettings {
  return {
    notificationsEnabled: false,
    authorization,
    rules: [
      {
        id: 'probe.model_failed',
        title: '模型探测故障',
        subTitle: '探测故障通知',
        status: true,
        templateType: 'default',
        channel: '系统通知',
      },
    ],
  };
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === 'get_notification_settings') return settings();
    throw new Error(`Unexpected command: ${command}`);
  });
});

async function mountPage(errorHandler = vi.fn()) {
  const wrapper = mount(NotificationsView, {
    global: {
      plugins: [AntdvNext],
      stubs: { ATable: true, NotificationTemplateDrawer: true, AppIcon: true },
      config: { errorHandler },
    },
  });
  await flushPromises();
  return wrapper;
}

function button(wrapper: Awaited<ReturnType<typeof mountPage>>, text: string) {
  return wrapper.findAll('button').find((item) => item.text().replace(/\s/gu, '') === text);
}

describe('通知授权提示', () => {
  it.each([
    ['granted', '已授权'],
    ['denied', '已拒绝'],
  ] as const)('请求返回 %s 后显示重启 modal，保留真实授权状态', async (result, label) => {
    const wrapper = await mountPage();
    vi.mocked(invoke).mockResolvedValueOnce(result);
    expect(feedback.success).not.toHaveBeenCalled();

    await button(wrapper, '请求授权')!.trigger('click');
    await flushPromises();

    expect(vi.mocked(invoke).mock.calls.map(([command]) => command)).toEqual([
      'get_notification_settings',
      'request_notification_permission',
    ]);
    expect(feedback.success).toHaveBeenCalledExactlyOnceWith('完成授权后，请重启软件。', 'modal');
    expect(wrapper.text()).toContain(label);
    wrapper.unmount();
  });

  it('等待系统授权请求返回后才显示提示', async () => {
    const wrapper = await mountPage();
    let resolve!: (value: NotificationPermission) => void;
    vi.mocked(invoke).mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    await button(wrapper, '请求授权')!.trigger('click');
    await flushPromises();
    expect(feedback.success).not.toHaveBeenCalled();

    resolve('granted');
    await flushPromises();
    expect(feedback.success).toHaveBeenCalledExactlyOnceWith('完成授权后，请重启软件。', 'modal');
    wrapper.unmount();
  });

  it('请求失败时不显示重启提示，保留原状态及错误处理流程', async () => {
    const errorHandler = vi.fn();
    const wrapper = await mountPage(errorHandler);
    const error = {
      code: 'notification_platform_error',
      category: 'platform',
      message: '无法申请系统通知授权',
      retryable: true,
    };
    vi.mocked(invoke).mockRejectedValueOnce(error);

    await button(wrapper, '请求授权')!.trigger('click');
    await flushPromises();

    expect(errorHandler.mock.calls[0]?.[0]).toMatchObject({ message: error.message });
    expect(feedback.error).toHaveBeenCalledWith(error.message, 'modal');
    expect(feedback.success).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain('尚未授权');
    wrapper.unmount();
  });
});
