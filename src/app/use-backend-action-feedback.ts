import { App as AntdApp } from 'antd';
import { useCallback } from 'react';
import { RelayPulseBackendError } from '../data/tauri-relay-pulse-data-source';

interface BackendActionFeedback {
  key: string;
  presentation?: 'notification' | 'message';
  successTitle: string;
  successDescription: string;
  errorTitle: string;
  errorDescription: string;
}

export function useBackendActionFeedback() {
  const { message, notification } = AntdApp.useApp();

  return useCallback(
    async (action: () => Promise<unknown>, feedback: BackendActionFeedback) => {
      try {
        await action();
        if (feedback.presentation === 'message') {
          message.success({
            key: feedback.key,
            content: feedback.successTitle,
            duration: 3,
          });
          return true;
        }
        notification.success({
          key: feedback.key,
          title: feedback.successTitle,
          description: feedback.successDescription,
          placement: 'topRight',
          duration: 4.5,
          pauseOnHover: true,
          showProgress: true,
          role: 'status',
        });
        return true;
      } catch (error) {
        if (feedback.presentation === 'message') {
          message.error({
            key: feedback.key,
            content: feedback.errorTitle,
            duration: 4,
          });
          return false;
        }
        notification.error({
          key: feedback.key,
          title: feedback.errorTitle,
          description: backendActionErrorDescription(error, feedback.errorDescription),
          placement: 'topRight',
          duration: 6,
          pauseOnHover: true,
          showProgress: true,
          role: 'alert',
        });
        return false;
      }
    },
    [message, notification],
  );
}

export function backendActionErrorDescription(error: unknown, fallback: string) {
  if (!(error instanceof RelayPulseBackendError)) return fallback;
  const operation = error.operationId ? ` 错误编号：${error.operationId}。` : '';
  return `${fallback} 错误代码：${error.code}。${operation}`;
}
