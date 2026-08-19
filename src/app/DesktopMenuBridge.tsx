import { App as AntdApp } from 'antd';
import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useRef } from 'react';
import { reportFrontendError } from './frontend-diagnostics';
import { useRelayPulse } from './relay-pulse-context';

type DesktopMenuAction = 'open-log-directory' | 'export-diagnostics';

interface DesktopMenuActionEvent {
  action: DesktopMenuAction;
}

export function DesktopMenuBridge() {
  const { message, notification } = AntdApp.useApp();
  const { openLogDirectory, exportDiagnostics } = useRelayPulse();
  const pending = useRef(false);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let stop: (() => void) | undefined;

    async function runAction(action: DesktopMenuAction) {
      if (pending.current) return;
      pending.current = true;
      try {
        if (action === 'open-log-directory') {
          await openLogDirectory();
          message.success('日志目录已打开');
          return;
        }
        const result = await exportDiagnostics();
        if (result.status === 'exported') message.success('诊断包已导出');
      } catch {
        notification.error({
          title: action === 'open-log-directory' ? '日志目录打开失败' : '诊断包导出失败',
          description:
            action === 'open-log-directory'
              ? '无法调用系统文件管理器，请稍后重试。'
              : '没有写入诊断包，请检查目标目录权限后重试。',
          placement: 'topRight',
          duration: 6,
          pauseOnHover: true,
          showProgress: true,
          role: 'alert',
        });
      } finally {
        pending.current = false;
      }
    }

    void listen<DesktopMenuActionEvent>('relaypulse://desktop-menu-action', (event) => {
      if (!disposed) void runAction(event.payload.action);
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch((error) => {
        reportFrontendError('background-task', error);
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [exportDiagnostics, message, notification, openLogDirectory]);

  return null;
}
