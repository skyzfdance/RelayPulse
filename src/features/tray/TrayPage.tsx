import { ReloadOutlined, SettingOutlined } from '@ant-design/icons';
import { Button, Tooltip } from 'antd';
import { useState } from 'react';
import { useBackendActionFeedback } from '../../app/use-backend-action-feedback';
import { BrandMark } from '../../components/BrandMark';
import { StatusDot } from '../../components/StatusDot';
import type { TraySnapshot, ViewName } from '../../domain/types';
import { navigateToView } from '../../app/view';
import { useRelayPulse } from '../../app/relay-pulse-context';

const trayMetricStatusClass = {
  healthy: 'text-rp-success',
  warning: 'text-rp-warning-text',
  error: 'text-rp-error',
  muted: 'text-rp-secondary',
} as const;

function TrayHeader({ loading, onRefresh }: { loading: boolean; onRefresh(): void }) {
  return (
    <header className="flex h-13 items-center justify-between border-b border-rp-line pr-3.5 pl-4">
      <div className="flex items-center gap-2">
        <BrandMark compact />
        <strong>RelayPulse</strong>
      </div>
      <div className="flex gap-1 [&_.ant-btn]:w-8 [&_.ant-btn]:px-0">
        <Tooltip title="立即探测">
          <Button
            icon={<ReloadOutlined />}
            loading={loading}
            aria-label="立即探测"
            onClick={onRefresh}
          />
        </Tooltip>
        <Tooltip title="打开设置">
          <Button
            icon={<SettingOutlined />}
            aria-label="打开设置"
            onClick={() => navigateToView('sites')}
          />
        </Tooltip>
      </div>
    </header>
  );
}

function TrayFooter({ snapshot }: { snapshot: TraySnapshot }) {
  return (
    <footer className="flex h-10.5 items-center justify-between border-t border-rp-line px-4 text-xs text-rp-tertiary">
      <span>{snapshot.completedLabel}</span>
      <span>{snapshot.nextProbeLabel}</span>
    </footer>
  );
}

export function TrayPage({ view }: { view: Extract<ViewName, 'tray-healthy' | 'tray-alerts'> }) {
  const { snapshot, reset } = useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
  const [refreshing, setRefreshing] = useState(false);
  if (!snapshot) return null;
  const healthy = view === 'tray-healthy';
  const tray = healthy ? snapshot.tray.healthy : snapshot.tray.alerts;

  async function refreshTray() {
    setRefreshing(true);
    try {
      await runBackendAction(() => reset(), {
        key: 'refresh-tray-probe',
        presentation: 'message',
        successTitle: '探测状态已刷新',
        successDescription: '状态面板已更新为最新探测结果。',
        errorTitle: '立即探测失败',
        errorDescription: '仍显示上一次探测状态，请稍后重试。',
      });
    } finally {
      setRefreshing(false);
    }
  }

  return (
    <div
      className={`grid h-screen w-screen place-items-center bg-rp-canvas ${healthy ? 'p-4' : 'p-4.5'}`}
    >
      <main
        className={`overflow-hidden rounded-[10px] border border-rp-line-strong bg-rp-surface shadow-[0_12px_40px_rgb(0_0_0/16%),0_2px_8px_rgb(0_0_0/8%)] ${healthy ? 'w-92' : 'w-101'}`}
        data-testid="tray-panel"
      >
        <TrayHeader loading={refreshing} onRefresh={() => void refreshTray()} />
        {healthy ? (
          <section className="flex min-h-21.5 items-center gap-3 px-4 py-4.5">
            <StatusDot status="healthy" halo />
            <div className="min-w-0 [&_h1]:m-0 [&_h1]:text-base [&_h1]:leading-6 [&_h1]:font-semibold [&_p]:mt-0.5 [&_p]:mb-0 [&_p]:text-xs [&_p]:leading-5 [&_p]:text-rp-tertiary">
              <h1>全部正常</h1>
              <p>
                {tray.activeSites} 个站点 · {tray.activeModels} 个模型正在探测
              </p>
            </div>
          </section>
        ) : (
          <>
            <section className="px-4 pt-3">
              <div className="flex h-10 items-start justify-between">
                <div className="flex items-center gap-2.5 [&_h1]:m-0 [&_h1]:text-base [&_h1]:leading-6 [&_h1]:font-semibold">
                  <StatusDot status="error" halo />
                  <h1>3 个异常</h1>
                </div>
                <span className="pt-1.25 text-xs text-rp-tertiary">按站点优先级排序</span>
              </div>
              <div className="flex flex-col">
                {tray.alerts.slice(0, 3).map((alert) => (
                  <div
                    className="grid min-h-16 grid-cols-[minmax(0,1fr)_72px_68px] items-center gap-2 border-t border-rp-line py-2 first:border-t-0"
                    data-testid="tray-alert-row"
                    key={alert.id}
                  >
                    <div className="flex min-w-0 items-center gap-2.5">
                      <StatusDot status={alert.status} />
                      <div className="flex min-w-0 flex-col [&_span]:overflow-hidden [&_span]:text-ellipsis [&_span]:whitespace-nowrap [&_span]:text-xs [&_span]:text-rp-tertiary [&_strong]:overflow-hidden [&_strong]:text-ellipsis [&_strong]:whitespace-nowrap [&_strong]:text-[13px] [&_strong]:leading-5">
                        <strong title={alert.modelName}>{alert.modelName}</strong>
                        <span title={alert.siteName}>{alert.siteName}</span>
                      </div>
                    </div>
                    <div className="flex flex-col items-end gap-0.5 whitespace-nowrap [font-variant-numeric:tabular-nums] [&>span]:text-[10px] [&>span]:text-rp-tertiary [&>strong]:text-xs [&>strong]:font-semibold">
                      <span>首 Token</span>
                      <strong className={trayMetricStatusClass[alert.status]}>
                        {alert.firstToken}
                      </strong>
                    </div>
                    <div className="flex flex-col items-end gap-0.5 whitespace-nowrap [font-variant-numeric:tabular-nums] [&>span]:text-[10px] [&>span]:text-rp-tertiary [&>strong]:text-xs [&>strong]:font-semibold">
                      <span>总耗时</span>
                      <strong className={trayMetricStatusClass[alert.status]}>
                        {alert.totalTime}
                      </strong>
                    </div>
                  </div>
                ))}
              </div>
            </section>
            <button
              className="flex h-9 w-full cursor-pointer items-center justify-center border-0 border-t border-rp-line bg-transparent text-xs text-rp-primary hover:bg-rp-hover"
              type="button"
            >
              查看全部 {tray.alertCount} 个异常 ›
            </button>
          </>
        )}
        <TrayFooter snapshot={tray} />
      </main>
    </div>
  );
}
