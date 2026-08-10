import { Alert, Button, Drawer, Select, Switch } from 'antd';
import { useMemo, useState } from 'react';
import { formatLocalTimestamp } from '../../domain/time';
import type {
  IntelligenceAutoFetchSettings,
  IntelligenceLadderSnapshot,
  IntelligenceRefreshState,
} from './intelligence-ladder-types';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;

const settingRowClass =
  'flex min-h-14 items-center justify-between gap-6 border-b border-rp-line py-3 last:border-b-0';

function runtimePresentation(state: IntelligenceRefreshState, hasSuccess: boolean) {
  if (state === 'running') {
    return { type: 'info' as const, title: '正在获取', description: '排名数据正在后台更新。' };
  }
  if (state === 'failed') {
    return {
      type: 'warning' as const,
      title: '最近获取失败',
      description: hasSuccess ? '已保留上次成功数据。' : '尚无可用排名数据。',
    };
  }
  if (state === 'scraper_unavailable') {
    return {
      type: 'error' as const,
      title: '采集服务不可用',
      description: hasSuccess ? '已保留上次成功数据。' : '暂时无法获取排名。',
    };
  }
  if (state === 'not_connected') {
    return {
      type: 'info' as const,
      title: '数据源尚未接入',
      description: '生产数据接入将在 TASK-16 契约稳定后完成。',
    };
  }
  if (!hasSuccess) {
    return { type: 'info' as const, title: '尚未成功获取', description: '等待首次完整排名快照。' };
  }
  return { type: 'success' as const, title: '运行正常', description: '后台自动获取可用。' };
}

export function IntelligenceSettingsDrawer({
  disabled,
  open,
  saving,
  snapshot,
  onClose,
  onSave,
}: {
  disabled: boolean;
  open: boolean;
  saving: boolean;
  snapshot: IntelligenceLadderSnapshot;
  onClose(): void;
  onSave(settings: IntelligenceAutoFetchSettings): Promise<void>;
}) {
  const [draft, setDraft] = useState<IntelligenceAutoFetchSettings>(() => ({
    ...snapshot.settings,
  }));

  const dirty = useMemo(
    () =>
      draft.enabled !== snapshot.settings.enabled ||
      draft.intervalMinutes !== snapshot.settings.intervalMinutes ||
      draft.fetchOnAppStart !== snapshot.settings.fetchOnAppStart ||
      draft.fetchOnNetworkRestore !== snapshot.settings.fetchOnNetworkRestore,
    [draft, snapshot.settings],
  );
  const runtime = runtimePresentation(snapshot.runtime.state, Boolean(snapshot.fetchedAt));
  const dependentDisabled = disabled || saving || !draft.enabled;

  return (
    <Drawer
      afterOpenChange={(nextOpen) => {
        if (!nextOpen) setDraft({ ...snapshot.settings });
      }}
      closable={{ placement: 'end', disabled: saving }}
      destroyOnHidden
      footer={
        <div className="flex justify-end gap-2">
          <Button disabled={saving} onClick={onClose}>
            取消
          </Button>
          <Button
            disabled={disabled || !dirty}
            form="intelligence-settings-form"
            htmlType="submit"
            loading={saving}
            type="primary"
          >
            保存
          </Button>
        </div>
      }
      getContainer={overlayContainer}
      mask={{ closable: !saving }}
      onClose={onClose}
      open={open}
      rootStyle={{ position: 'absolute' }}
      size={520}
      title="自动获取设置"
    >
      <form
        id="intelligence-settings-form"
        onSubmit={(event) => {
          event.preventDefault();
          void onSave(draft);
        }}
      >
        <div className={settingRowClass}>
          <div>
            <strong className="block text-sm font-medium">后台自动获取</strong>
            <span className="mt-0.5 block text-xs leading-5 text-rp-tertiary">
              定期更新智力天梯快照
            </span>
          </div>
          <Switch
            aria-label="后台自动获取"
            checked={draft.enabled}
            disabled={disabled || saving}
            onChange={(enabled) => setDraft((current) => ({ ...current, enabled }))}
          />
        </div>

        <label className="block border-b border-rp-line py-3">
          <span className="mb-2 block text-sm font-medium">获取间隔</span>
          <Select<number>
            aria-label="获取间隔"
            className="w-full"
            disabled={dependentDisabled}
            onChange={(intervalMinutes) => setDraft((current) => ({ ...current, intervalMinutes }))}
            options={snapshot.allowedIntervalMinutes.map((minutes) => ({
              value: minutes,
              label: `${minutes} 分钟`,
            }))}
            value={draft.intervalMinutes}
          />
        </label>

        <div className={settingRowClass}>
          <div>
            <strong className="block text-sm font-medium">应用启动时立即获取</strong>
            <span className="mt-0.5 block text-xs leading-5 text-rp-tertiary">
              启动后尝试更新一次
            </span>
          </div>
          <Switch
            aria-label="应用启动时立即获取"
            checked={draft.fetchOnAppStart}
            disabled={dependentDisabled}
            onChange={(fetchOnAppStart) => setDraft((current) => ({ ...current, fetchOnAppStart }))}
          />
        </div>

        <div className={settingRowClass}>
          <div>
            <strong className="block text-sm font-medium">网络恢复后立即获取</strong>
            <span className="mt-0.5 block text-xs leading-5 text-rp-tertiary">
              从离线恢复后尝试更新
            </span>
          </div>
          <Switch
            aria-label="网络恢复后立即获取"
            checked={draft.fetchOnNetworkRestore}
            disabled={dependentDisabled}
            onChange={(fetchOnNetworkRestore) =>
              setDraft((current) => ({ ...current, fetchOnNetworkRestore }))
            }
          />
        </div>

        <section aria-label="自动获取运行状态" className="pt-5">
          <Alert
            description={runtime.description}
            showIcon
            title={runtime.title}
            type={runtime.type}
          />
          <dl className="mt-4 grid grid-cols-[108px_minmax(0,1fr)] gap-y-2 text-xs leading-5 [&_dd]:m-0 [&_dd]:text-rp [&_dt]:text-rp-tertiary">
            <dt>上次成功</dt>
            <dd>
              {snapshot.runtime.lastSuccessfulAt
                ? formatLocalTimestamp(snapshot.runtime.lastSuccessfulAt)
                : '从未成功'}
            </dd>
            <dt>下次计划</dt>
            <dd>
              {snapshot.runtime.nextScheduledAt
                ? formatLocalTimestamp(snapshot.runtime.nextScheduledAt)
                : '暂无计划'}
            </dd>
          </dl>
        </section>

        <p className="mt-5 mb-0 text-xs leading-5 text-rp-tertiary">
          自动获取仅在 RelayPulse 运行时执行。
        </p>
      </form>
    </Drawer>
  );
}
