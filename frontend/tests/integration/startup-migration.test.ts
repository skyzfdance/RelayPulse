import type { StartupMigrationResult, StartupStatus } from '@/types/domain/startup';

import { flushPromises, mount } from '@vue/test-utils';
import AntdvNext, { Button } from 'antdv-next';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { defineComponent } from 'vue';

import * as api from '@/api/startup';
import { RelayPulseBackendError } from '@/utils/backend-error';

import { deferred } from '../support/deferred';

import StartupGate from '@/components/StartupGate.vue';

const cleanup: (() => void)[] = [];
afterEach(() => {
  cleanup.splice(0).forEach((stop) => stop());
  vi.restoreAllMocks();
  vi.useRealTimers();
});
function setup() {
  const businessMounted = vi.fn();
  const wrapper = mount(StartupGate, {
    global: { plugins: [AntdvNext] },
    slots: {
      default: defineComponent({ setup: businessMounted, template: '<div>业务页面</div>' }),
    },
  });
  cleanup.push(() => wrapper.unmount());
  return { wrapper, businessMounted };
}
const legacy: StartupStatus = {
  status: 'migration_required',
  message: '检测到旧版本数据，原数据会保留为备份。',
};
function failure(message: string) {
  return new RelayPulseBackendError({
    code: 'startup_migration_failed',
    category: 'startup',
    message,
    retryable: true,
  });
}

describe('应用启动数据迁移', () => {
  it('启动状态确认前不挂载业务页面，就绪后正常进入', async () => {
    const pending = deferred<StartupStatus>();
    vi.spyOn(api, 'fetchStartupStatus').mockReturnValue(pending.promise);
    const page = setup();
    expect(page.businessMounted).not.toHaveBeenCalled();
    expect(page.wrapper.text()).toContain('正在检查启动状态');
    pending.resolve({ status: 'ready', message: '' });
    await flushPromises();
    expect(page.businessMounted).toHaveBeenCalledOnce();
    expect(page.wrapper.text()).toBe('业务页面');
  });

  it('用户点击一次完成迁移和重启，等待期间禁用重复操作', async () => {
    vi.spyOn(api, 'fetchStartupStatus').mockResolvedValue(legacy);
    const pending = deferred<StartupMigrationResult>();
    const migrate = vi.spyOn(api, 'migrateStartupData').mockReturnValue(pending.promise);
    const restart = vi.spyOn(api, 'restartAfterDataMigration').mockResolvedValue();
    const page = setup();
    await flushPromises();
    expect(migrate).not.toHaveBeenCalled();
    await page.wrapper.getComponent(Button).trigger('click');
    await flushPromises();
    expect(page.wrapper.getComponent(Button).props('loading')).toBe(true);
    expect(restart).not.toHaveBeenCalled();
    await page.wrapper.getComponent(Button).trigger('click');
    expect(migrate).toHaveBeenCalledOnce();
    pending.resolve({ backupDirectory: '/data/retained-backup' });
    await flushPromises();
    expect(restart).toHaveBeenCalledOnce();
    expect(page.wrapper.text()).toContain('正在重启应用');
    expect(page.wrapper.text()).toContain('/data/retained-backup');
    expect(page.businessMounted).not.toHaveBeenCalled();
  });

  it('迁移失败显示具体原因并允许重试，不重启、不进入业务页面', async () => {
    vi.spyOn(api, 'fetchStartupStatus').mockResolvedValue(legacy);
    const migrate = vi
      .spyOn(api, 'migrateStartupData')
      .mockRejectedValueOnce(failure('数据转换未通过校验，原数据保持不变。'))
      .mockResolvedValueOnce({ backupDirectory: '/backup' });
    const restart = vi.spyOn(api, 'restartAfterDataMigration').mockResolvedValue();
    const page = setup();
    await flushPromises();
    await page.wrapper.getComponent(Button).trigger('click');
    await flushPromises();
    expect(page.wrapper.get('[role="alert"]').text()).toContain('数据转换未通过校验');
    expect(page.wrapper.getComponent(Button).props('loading')).toBe(false);
    expect(restart).not.toHaveBeenCalled();
    expect(page.businessMounted).not.toHaveBeenCalled();
    await page.wrapper.getComponent(Button).trigger('click');
    await flushPromises();
    expect(migrate).toHaveBeenCalledTimes(2);
    expect(restart).toHaveBeenCalledOnce();
  });

  it('重启失败后只重试重启，不重复迁移数据', async () => {
    vi.spyOn(api, 'fetchStartupStatus').mockResolvedValue(legacy);
    const migrate = vi
      .spyOn(api, 'migrateStartupData')
      .mockResolvedValue({ backupDirectory: '/backup' });
    const restart = vi
      .spyOn(api, 'restartAfterDataMigration')
      .mockRejectedValueOnce(failure('重启失败'))
      .mockResolvedValueOnce();
    const page = setup();
    await flushPromises();
    await page.wrapper.getComponent(Button).trigger('click');
    await flushPromises();
    expect(page.wrapper.getComponent(Button).text()).toBe('重启应用');
    expect(page.wrapper.get('[role="alert"]').text()).toContain('重启失败');
    await page.wrapper.getComponent(Button).trigger('click');
    await flushPromises();
    expect(restart).toHaveBeenCalledTimes(2);
    expect(migrate).toHaveBeenCalledOnce();
  });

  it('不支持的数据格式只显示启动原因，不提供迁移按钮', async () => {
    vi.spyOn(api, 'fetchStartupStatus').mockResolvedValue({
      status: 'blocked',
      message: '旧数据格式不受支持。',
    });
    const migrate = vi.spyOn(api, 'migrateStartupData');
    const page = setup();
    await flushPromises();
    expect(page.wrapper.text()).toContain('旧数据格式不受支持');
    expect(page.wrapper.getComponent(Button).text()).toBe('重新检查');
    expect(migrate).not.toHaveBeenCalled();
    expect(page.businessMounted).not.toHaveBeenCalled();
  });

  it('启动查询失败后可重新检查恢复进入页面', async () => {
    vi.spyOn(api, 'fetchStartupStatus')
      .mockRejectedValueOnce(failure('启动状态读取失败'))
      .mockResolvedValueOnce({ status: 'ready', message: '' });
    const page = setup();
    await flushPromises();
    expect(page.wrapper.text()).toContain('启动状态读取失败');
    expect(page.businessMounted).not.toHaveBeenCalled();
    await page.wrapper.getComponent(Button).trigger('click');
    await flushPromises();
    expect(page.wrapper.text()).toBe('业务页面');
  });

  it('界面重新加载后继续等待后端迁移，完成时自动重启', async () => {
    vi.useFakeTimers();
    vi.spyOn(api, 'fetchStartupStatus')
      .mockResolvedValueOnce({ status: 'migrating', message: '正在迁移' })
      .mockResolvedValueOnce({ status: 'migration_completed', message: '已完成' });
    const restart = vi.spyOn(api, 'restartAfterDataMigration').mockResolvedValue();
    const migrate = vi.spyOn(api, 'migrateStartupData');
    const page = setup();
    await flushPromises();
    expect(page.wrapper.text()).toContain('正在备份并迁移数据');
    await vi.advanceTimersByTimeAsync(1000);
    await flushPromises();
    expect(restart).toHaveBeenCalledOnce();
    expect(migrate).not.toHaveBeenCalled();
    expect(page.businessMounted).not.toHaveBeenCalled();
  });

  it('离开启动界面后停止轮询，避免遗留后台查询', async () => {
    vi.useFakeTimers();
    const status = vi
      .spyOn(api, 'fetchStartupStatus')
      .mockResolvedValue({ status: 'migrating', message: '正在迁移' });
    const page = setup();
    await flushPromises();
    page.wrapper.unmount();
    await vi.advanceTimersByTimeAsync(3000);
    expect(status).toHaveBeenCalledOnce();
  });
});
