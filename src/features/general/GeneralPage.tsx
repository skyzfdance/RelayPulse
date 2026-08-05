import { ExportOutlined, FolderOpenOutlined } from '@ant-design/icons';
import { App as AntdApp, Button, Segmented, Select, Switch } from 'antd';
import { useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { useBackendActionFeedback } from '../../app/use-backend-action-feedback';
import type { GeneralSettings, ThemeMode } from '../../domain/types';
import { pageHeaderClass } from '../../styles/tailwind';

function SettingsRow({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid min-h-14.5 grid-cols-[minmax(0,1fr)_280px] items-center gap-6">
      <div className="flex flex-col gap-0.5">
        <strong>{title}</strong>
        {description ? <span className="text-xs text-rp-tertiary">{description}</span> : null}
      </div>
      <div className="flex items-center justify-end">{children}</div>
    </div>
  );
}

export function GeneralPage() {
  const { snapshot, updateGeneralSettings, openLogDirectory, exportDiagnostics } = useRelayPulse();
  const { message, notification } = AntdApp.useApp();
  const runBackendAction = useBackendActionFeedback();
  const [pendingSetting, setPendingSetting] = useState<keyof GeneralSettings | null>(null);
  const [pendingDesktopAction, setPendingDesktopAction] = useState<'logs' | 'export' | null>(null);
  const settings = snapshot?.generalSettings;
  if (!settings) return null;

  async function applySetting<K extends keyof GeneralSettings>(
    key: K,
    value: GeneralSettings[K],
    label: string,
  ) {
    if (pendingSetting) return;
    setPendingSetting(key);
    try {
      const nextSettings = { ...settings, [key]: value } as GeneralSettings;
      await runBackendAction(() => updateGeneralSettings(nextSettings), {
        key: `update-general-${key}`,
        presentation: 'message',
        successTitle: `${label}已更新`,
        successDescription: `${label}已应用。`,
        errorTitle: `${label}更新失败`,
        errorDescription: `未能更新${label}，已恢复原值。`,
      });
    } finally {
      setPendingSetting(null);
    }
  }

  const controlsDisabled = pendingSetting !== null;

  async function openLogs() {
    if (pendingDesktopAction) return;
    setPendingDesktopAction('logs');
    try {
      await runBackendAction(() => openLogDirectory(), {
        key: 'open-log-directory',
        presentation: 'message',
        successTitle: '日志目录已打开',
        successDescription: '已在系统文件管理器中打开本地日志目录。',
        errorTitle: '日志目录打开失败',
        errorDescription: '无法调用系统文件管理器，请稍后重试。',
      });
    } finally {
      setPendingDesktopAction(null);
    }
  }

  async function exportPackage() {
    if (pendingDesktopAction) return;
    setPendingDesktopAction('export');
    try {
      const result = await exportDiagnostics();
      if (result.status === 'exported') {
        message.success({
          key: 'export-diagnostics',
          content: '诊断包已导出',
          duration: 3,
        });
      }
    } catch {
      notification.error({
        key: 'export-diagnostics',
        title: '诊断包导出失败',
        description: '没有写入诊断包，请检查目标目录权限后重试。',
        placement: 'topRight',
        duration: 6,
        pauseOnHover: true,
        showProgress: true,
        role: 'alert',
      });
    } finally {
      setPendingDesktopAction(null);
    }
  }

  return (
    <>
      <header className={pageHeaderClass}>
        <div>
          <h1>通用设置</h1>
          <p>控制 RelayPulse 的系统行为、外观与本地数据</p>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto" data-testid="general-content">
        <div className="w-[min(890px,calc(100%-64px))] pt-5 pb-5 pl-7.5">
          <section className="border-b border-rp-line pb-3 [&>h2]:mb-1 [&>h2]:text-base">
            <h2>启动与后台</h2>
            <SettingsRow title="开机自动启动" description="登录系统后自动在状态栏运行 RelayPulse">
              <Switch
                size="small"
                checked={settings.launchAtStartup}
                loading={pendingSetting === 'launchAtStartup'}
                disabled={controlsDisabled && pendingSetting !== 'launchAtStartup'}
                onChange={(launchAtStartup) =>
                  void applySetting('launchAtStartup', launchAtStartup, '开机自动启动')
                }
              />
            </SettingsRow>
            <SettingsRow title="关闭主窗口后继续运行" description="关闭配置窗口后继续执行后台探测">
              <Switch
                size="small"
                checked={settings.continueInBackground}
                loading={pendingSetting === 'continueInBackground'}
                disabled={controlsDisabled && pendingSetting !== 'continueInBackground'}
                onChange={(continueInBackground) =>
                  void applySetting('continueInBackground', continueInBackground, '后台运行设置')
                }
              />
            </SettingsRow>
            <SettingsRow
              title="显示状态栏图标"
              description="点击图标查看状态面板；关闭后移除图标，后台探测不受影响"
            >
              <Switch
                size="small"
                checked={settings.trayAction === 'panel'}
                loading={pendingSetting === 'trayAction'}
                disabled={controlsDisabled && pendingSetting !== 'trayAction'}
                onChange={(enabled) =>
                  void applySetting('trayAction', enabled ? 'panel' : 'disabled', '状态栏图标')
                }
              />
            </SettingsRow>
          </section>

          <section className="border-b border-rp-line pt-3.5 pb-3 [&>h2]:mb-1 [&>h2]:text-base">
            <h2>外观与语言</h2>
            <SettingsRow title="主题" description="跟随系统时会自动切换浅色和深色模式">
              <Segmented<ThemeMode>
                value={settings.theme}
                disabled={controlsDisabled}
                options={[
                  { value: 'system', label: '跟随系统' },
                  { value: 'light', label: '浅色' },
                  { value: 'dark', label: '深色' },
                ]}
                onChange={(theme) => void applySetting('theme', theme, '主题')}
              />
            </SettingsRow>
            <SettingsRow title="界面语言">
              <Select
                className="w-42.5"
                value={settings.language}
                disabled={controlsDisabled}
                loading={pendingSetting === 'language'}
                options={[{ value: 'zh-CN', label: '简体中文' }]}
                onChange={(language) => void applySetting('language', language, '界面语言')}
              />
            </SettingsRow>
          </section>

          <section className="border-b border-rp-line pt-3.5 pb-3 [&>h2]:mb-1 [&>h2]:text-base">
            <h2>本地数据</h2>
            <SettingsRow
              title="探测记录保留时间"
              description="过期记录会自动清理，不影响站点和模型配置"
            >
              <Select
                className="w-42.5"
                value={settings.historyRetentionDays}
                disabled={controlsDisabled}
                loading={pendingSetting === 'historyRetentionDays'}
                options={[{ value: 30, label: '保留 30 天' }]}
                onChange={(days) =>
                  void applySetting('historyRetentionDays', days, '探测记录保留时间')
                }
              />
            </SettingsRow>
            <SettingsRow title="诊断日志" description="日志保存在本机，不包含 API Key 明文">
              <div className="flex gap-2">
                <Button
                  icon={<FolderOpenOutlined />}
                  loading={pendingDesktopAction === 'logs'}
                  disabled={pendingDesktopAction !== null && pendingDesktopAction !== 'logs'}
                  onClick={() => void openLogs()}
                >
                  打开日志目录
                </Button>
                <Button
                  icon={<ExportOutlined />}
                  loading={pendingDesktopAction === 'export'}
                  disabled={pendingDesktopAction !== null && pendingDesktopAction !== 'export'}
                  onClick={() => void exportPackage()}
                >
                  导出诊断包
                </Button>
              </div>
            </SettingsRow>
          </section>
        </div>
      </div>
    </>
  );
}
