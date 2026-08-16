import {
  DeleteOutlined,
  ExportOutlined,
  FolderOpenOutlined,
  SaveOutlined,
} from '@ant-design/icons';
import {
  App as AntdApp,
  Button,
  InputNumber,
  Popconfirm,
  Segmented,
  Select,
  Switch,
  Tooltip,
} from 'antd';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { recordFrontendAction } from '../../app/frontend-diagnostics';
import {
  backendActionErrorDescription,
  useBackendActionFeedback,
} from '../../app/use-backend-action-feedback';
import type { LogStatistics } from '../../data/relay-pulse-data-source';
import type { GeneralSettings, ThemeMode } from '../../domain/types';
import { formatBytes } from '../../domain/time';
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
  const {
    snapshot,
    updateGeneralSettings,
    openLogDirectory,
    getLogStatistics,
    setDiagnosticLogLimit,
    clearDiagnosticLogs,
    exportDiagnostics,
  } = useRelayPulse();
  const { message, notification } = AntdApp.useApp();
  const runBackendAction = useBackendActionFeedback();
  const [pendingSetting, setPendingSetting] = useState<keyof GeneralSettings | null>(null);
  const [pendingDesktopAction, setPendingDesktopAction] = useState<
    'logs' | 'limit' | 'clear' | 'export' | null
  >(null);
  const [logStatistics, setLogStatistics] = useState<LogStatistics | null>(null);
  const [logStatisticsError, setLogStatisticsError] = useState(false);
  const [draftLogLimitMegabytes, setDraftLogLimitMegabytes] = useState<number | null>(null);
  const logLimitDirty = useRef(false);
  const settings = snapshot?.generalSettings;

  const refreshLogStatistics = useCallback(async () => {
    setLogStatisticsError(false);
    try {
      const next = await getLogStatistics();
      setLogStatistics(next);
      if (!logLimitDirty.current) {
        setDraftLogLimitMegabytes(bytesToMegabytes(next.configuredMaxTotalBytes));
      }
    } catch {
      setLogStatisticsError(true);
    }
  }, [getLogStatistics]);

  useEffect(() => {
    const initial = window.setTimeout(() => void refreshLogStatistics(), 0);
    const interval = window.setInterval(() => void refreshLogStatistics(), 10_000);
    return () => {
      window.clearTimeout(initial);
      window.clearInterval(interval);
    };
  }, [refreshLogStatistics]);

  if (!settings) return null;

  async function applySetting<K extends keyof GeneralSettings>(
    key: K,
    value: GeneralSettings[K],
    label: string,
  ) {
    if (pendingSetting) return;
    recordFrontendAction('setting-changed', {
      targetType: 'form',
      targetId: `setting-${key}`,
    });
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
      await refreshLogStatistics();
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
      await refreshLogStatistics();
      setPendingDesktopAction(null);
    }
  }

  async function clearLogs() {
    if (pendingDesktopAction) return;
    setPendingDesktopAction('clear');
    try {
      const result = await clearDiagnosticLogs();
      setLogStatistics(result.statistics);
      message.success({
        key: 'clear-diagnostic-logs',
        content: `已清理 ${formatBytes(result.reclaimedBytes)} 日志`,
        duration: 3,
      });
    } catch (error) {
      notification.error({
        key: 'clear-diagnostic-logs',
        title: '日志清理失败',
        description: backendActionErrorDescription(error, '没有删除日志文件，请稍后重试。'),
        placement: 'topRight',
        duration: 6,
        pauseOnHover: true,
        showProgress: true,
        role: 'alert',
      });
    } finally {
      await refreshLogStatistics();
      setPendingDesktopAction(null);
    }
  }

  async function saveLogLimit() {
    if (
      pendingDesktopAction ||
      draftLogLimitMegabytes === null ||
      !Number.isInteger(draftLogLimitMegabytes) ||
      draftLogLimitMegabytes < 10 ||
      draftLogLimitMegabytes > 100
    ) {
      return;
    }
    recordFrontendAction('setting-changed', {
      targetType: 'form',
      targetId: 'setting-diagnostic-log-limit',
    });
    setPendingDesktopAction('limit');
    const result: { statistics?: LogStatistics } = {};
    try {
      const succeeded = await runBackendAction(
        async () => {
          result.statistics = await setDiagnosticLogLimit(draftLogLimitMegabytes);
        },
        {
          key: 'set-diagnostic-log-limit',
          presentation: 'message',
          successTitle: '日志容量上限已保存，重启后生效',
          successDescription: '新的日志容量上限将在下次启动时生效。',
          errorTitle: '日志容量上限保存失败',
          errorDescription: '没有更改日志容量上限，请稍后重试。',
        },
      );
      if (succeeded && result.statistics) {
        logLimitDirty.current = false;
        setLogStatistics(result.statistics);
        setDraftLogLimitMegabytes(bytesToMegabytes(result.statistics.configuredMaxTotalBytes));
      }
    } finally {
      setPendingDesktopAction(null);
    }
  }

  const activeLogLimitMegabytes = logStatistics
    ? bytesToMegabytes(logStatistics.maxTotalBytes)
    : null;
  const configuredLogLimitMegabytes = logStatistics
    ? bytesToMegabytes(logStatistics.configuredMaxTotalBytes)
    : null;
  const logLimitRestartRequired =
    activeLogLimitMegabytes !== null &&
    configuredLogLimitMegabytes !== null &&
    activeLogLimitMegabytes !== configuredLogLimitMegabytes;
  const logLimitInputValid =
    draftLogLimitMegabytes !== null &&
    Number.isInteger(draftLogLimitMegabytes) &&
    draftLogLimitMegabytes >= 10 &&
    draftLogLimitMegabytes <= 100;

  const logDescription = logStatisticsError
    ? '当前无法读取日志大小'
    : logStatistics
      ? `${logStatistics.fileCount} 个文件 · ${formatBytes(logStatistics.totalBytes)} / ${formatBytes(logStatistics.maxTotalBytes)}`
      : '正在读取日志大小';

  const logLimitDescription = logLimitRestartRequired
    ? `当前进程使用 ${activeLogLimitMegabytes} MB，重启后使用 ${configuredLogLimitMegabytes} MB`
    : '可设置 10–100 MB；保存后在下次启动时生效';

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
            <SettingsRow title="日志容量上限" description={logLimitDescription}>
              <div className="flex items-center gap-2">
                <InputNumber<number>
                  className="w-32"
                  min={10}
                  max={100}
                  step={1}
                  precision={0}
                  suffix="MB"
                  value={draftLogLimitMegabytes}
                  disabled={pendingDesktopAction !== null}
                  onChange={(value) => {
                    const nextValue = typeof value === 'number' ? value : null;
                    setDraftLogLimitMegabytes(nextValue);
                    logLimitDirty.current = nextValue !== configuredLogLimitMegabytes;
                  }}
                />
                <Tooltip title="保存日志容量上限">
                  <Button
                    aria-label="保存日志容量上限"
                    icon={<SaveOutlined />}
                    loading={pendingDesktopAction === 'limit'}
                    disabled={
                      pendingDesktopAction !== null ||
                      !logLimitInputValid ||
                      draftLogLimitMegabytes === configuredLogLimitMegabytes
                    }
                    onClick={() => void saveLogLimit()}
                  />
                </Tooltip>
              </div>
            </SettingsRow>
            <SettingsRow title="诊断日志" description={logDescription}>
              <div className="flex gap-2">
                <Button
                  icon={<FolderOpenOutlined />}
                  loading={pendingDesktopAction === 'logs'}
                  disabled={pendingDesktopAction !== null && pendingDesktopAction !== 'logs'}
                  onClick={() => void openLogs()}
                >
                  打开日志目录
                </Button>
                <Popconfirm
                  title="清理本地诊断日志？"
                  description="只会删除日志文件，不会删除探测记录或站点配置。"
                  okText="清理"
                  cancelText="取消"
                  okType="danger"
                  onConfirm={() => void clearLogs()}
                  disabled={pendingDesktopAction !== null && pendingDesktopAction !== 'clear'}
                >
                  <Button
                    danger
                    aria-label="清理日志"
                    icon={<DeleteOutlined />}
                    loading={pendingDesktopAction === 'clear'}
                    disabled={pendingDesktopAction !== null && pendingDesktopAction !== 'clear'}
                  />
                </Popconfirm>
              </div>
            </SettingsRow>
            <SettingsRow
              title="诊断包"
              description="包含错误链路、失败探测摘要和日志容量信息，不包含请求响应正文"
            >
              <Button
                icon={<ExportOutlined />}
                loading={pendingDesktopAction === 'export'}
                disabled={pendingDesktopAction !== null && pendingDesktopAction !== 'export'}
                onClick={() => void exportPackage()}
              >
                导出诊断包
              </Button>
            </SettingsRow>
          </section>
        </div>
      </div>
    </>
  );
}

function bytesToMegabytes(bytes: number) {
  return Math.round(bytes / (1024 * 1024));
}
