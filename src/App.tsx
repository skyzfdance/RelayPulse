import { App as AntdApp, Button, ConfigProvider, Result, Spin, theme } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { CSSProperties, PropsWithChildren } from 'react';
import { DesktopMenuBridge } from './app/DesktopMenuBridge';
import { MainShell } from './app/MainShell';
import { RelayPulseProvider } from './app/RelayPulseProvider';
import { recordFrontendAction } from './app/frontend-diagnostics';
import { useRelayPulse } from './app/relay-pulse-context';
import {
  defaultViewForWindow,
  isViewAllowedForWindow,
  navigateToView,
  parseView,
} from './app/view';
import type { ThemeMode, ViewName } from './domain/types';
import { GeneralPage } from './features/general/GeneralPage';
import { ModelSettingsDrawer } from './features/models/ModelSettingsDrawer';
import { NotificationsPage } from './features/notifications/NotificationsPage';
import { OperationHistoryPage } from './features/operation-history/OperationHistoryPage';
import { RecordsPage } from './features/records/RecordsPage';
import { AddSiteModal } from './features/sites/AddSiteModal';
import { DeleteSiteModal } from './features/sites/DeleteSiteModal';
import { EmptyPage } from './features/sites/EmptyPage';
import { SitesPage } from './features/sites/SitesPage';
import { TrayPage } from './features/tray/TrayPage';

function useActiveView() {
  const windowLabel = isTauri() ? getCurrentWebviewWindow().label : null;
  const [view, setView] = useState<ViewName>(() => {
    const requested = parseView(window.location.search);
    return isViewAllowedForWindow(windowLabel, requested)
      ? requested
      : defaultViewForWindow(windowLabel);
  });
  useEffect(() => {
    const sync = () => {
      const requested = parseView(window.location.search);
      if (isViewAllowedForWindow(windowLabel, requested)) setView(requested);
    };
    window.addEventListener('popstate', sync);
    return () => window.removeEventListener('popstate', sync);
  }, [windowLabel]);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let stop: (() => void) | undefined;
    void listen<{ view: ViewName }>('relaypulse://navigate', (event) => {
      if (!disposed && isViewAllowedForWindow(windowLabel, event.payload.view)) {
        navigateToView(event.payload.view);
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [windowLabel]);
  useEffect(() => {
    if (isViewAllowedForWindow(windowLabel, view)) return;
    navigateToView(defaultViewForWindow(windowLabel));
  }, [view, windowLabel]);
  useEffect(() => {
    const compact = view === 'tray-healthy' || view === 'tray-alerts';
    document.documentElement.classList.toggle('tray-window', compact);
    document.body.classList.toggle('tray-window', compact);
    return () => {
      document.documentElement.classList.remove('tray-window');
      document.body.classList.remove('tray-window');
    };
  }, [view]);
  return view;
}

function useDesktopContextMenuGuard() {
  useEffect(() => {
    if (!isTauri()) return;

    const preventContextMenu = (event: MouseEvent) => event.preventDefault();
    document.addEventListener('contextmenu', preventContextMenu, true);
    return () => document.removeEventListener('contextmenu', preventContextMenu, true);
  }, []);
}

function useResolvedTheme(mode: ThemeMode) {
  const [systemDark, setSystemDark] = useState(
    () => window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false,
  );
  useEffect(() => {
    const media = window.matchMedia?.('(prefers-color-scheme: dark)');
    if (!media) return;
    const listener = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    media.addEventListener('change', listener);
    return () => media.removeEventListener('change', listener);
  }, []);
  return mode === 'dark' || (mode === 'system' && systemDark);
}

type RelayPulseThemeVariables = CSSProperties & Record<`--rp-${string}`, string>;

function ThemeSurface({
  children,
  dark,
  transparent = false,
}: PropsWithChildren<{ dark: boolean; transparent?: boolean }>) {
  const { token } = theme.useToken();
  const variables: RelayPulseThemeVariables = {
    '--rp-bg-stage': dark ? token.colorBgLayout : '#edf0f4',
    '--rp-bg-container': token.colorBgContainer,
    '--rp-bg-elevated': token.colorBgElevated,
    '--rp-bg-subtle': token.colorFillQuaternary,
    '--rp-bg-hover': token.colorFillTertiary,
    '--rp-bg-active': token.colorPrimaryBg,
    '--rp-bg-active-hover': token.colorPrimaryBgHover,
    '--rp-bg-error': token.colorErrorBg,
    '--rp-fill-secondary': token.colorFillSecondary,
    '--rp-primary': token.colorPrimary,
    '--rp-success': token.colorSuccess,
    '--rp-warning': token.colorWarning,
    '--rp-error': token.colorError,
    '--rp-text': token.colorText,
    '--rp-text-secondary': token.colorTextSecondary,
    '--rp-text-tertiary': token.colorTextTertiary,
    '--rp-text-quaternary': token.colorTextQuaternary,
    '--rp-border': token.colorBorderSecondary,
    '--rp-border-strong': token.colorBorder,
    '--rp-border-active': token.colorPrimaryBorder,
    '--rp-status-muted': token.colorTextQuaternary,
    '--rp-warning-text': token.colorWarningText,
  };

  return (
    <div
      className={`h-full w-full text-rp ${transparent ? 'bg-transparent' : 'bg-rp-canvas'} ${dark ? 'scheme-dark' : 'scheme-light'}`}
      data-theme={dark ? 'dark' : 'light'}
      style={variables}
    >
      <AntdApp className="h-full w-full">{children}</AntdApp>
    </div>
  );
}

function MainView({ view }: { view: ViewName }) {
  const { notificationDataSource, operationHistoryDataSource, snapshot } = useRelayPulse();
  const baseView =
    view === 'site-advanced'
      ? 'site-advanced'
      : view === 'general'
        ? 'general'
        : view === 'notifications'
          ? 'notifications'
          : view === 'operation-history'
            ? 'operation-history'
            : view === 'records'
              ? 'records'
              : view === 'empty' || snapshot?.sites.length === 0
                ? 'empty'
                : 'sites';
  return (
    <MainShell view={view}>
      {baseView === 'general' ? (
        <GeneralPage />
      ) : baseView === 'notifications' ? (
        <NotificationsPage dataSource={notificationDataSource} />
      ) : baseView === 'operation-history' ? (
        <OperationHistoryPage dataSource={operationHistoryDataSource} />
      ) : baseView === 'records' ? (
        <RecordsPage />
      ) : baseView === 'empty' ? (
        <EmptyPage />
      ) : (
        <SitesPage view={baseView} />
      )}
      {view === 'add-site' ? <AddSiteModal /> : null}
      {view === 'delete-confirm' ? <DeleteSiteModal /> : null}
      {view === 'model-settings' ? <ModelSettingsDrawer /> : null}
    </MainShell>
  );
}

function RelayPulseApplication() {
  useDesktopContextMenuGuard();
  const view = useActiveView();
  const { snapshot, loading, error } = useRelayPulse();
  const isDark = useResolvedTheme(snapshot?.generalSettings.theme ?? 'light');
  const isTrayView = view === 'tray-healthy' || view === 'tray-alerts';

  function retryMockLoad() {
    recordFrontendAction('retry-load');
    const url = new URL(window.location.href);
    url.searchParams.delete('scenario');
    window.location.assign(url);
  }

  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        algorithm: isDark ? theme.darkAlgorithm : theme.defaultAlgorithm,
        token: {
          colorPrimary: '#1677ff',
          colorSuccess: '#52c41a',
          colorWarning: '#faad14',
          colorError: '#ff4d4f',
          borderRadius: 6,
          controlHeight: 32,
          fontSize: 14,
          fontFamily:
            "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Microsoft YaHei', sans-serif",
        },
        components: {
          Table: { cellPaddingBlockSM: 9, cellPaddingInlineSM: 12 },
          Modal: { borderRadiusLG: 8 },
          Drawer: { footerPaddingBlock: 12, footerPaddingInline: 20 },
        },
      }}
    >
      <ThemeSurface dark={isDark} transparent={isTrayView}>
        <DesktopMenuBridge />
        {loading ? (
          <div className="grid h-full w-full place-items-center bg-rp-canvas">
            <Spin size="large" description="正在加载应用数据" />
          </div>
        ) : error || !snapshot ? (
          <div className="grid h-full w-full place-items-center bg-rp-canvas">
            <Result
              status="warning"
              title="应用数据加载失败"
              subTitle={error ?? '没有可用数据'}
              extra={
                <Button type="primary" onClick={retryMockLoad}>
                  重试
                </Button>
              }
            />
          </div>
        ) : isTrayView ? (
          <TrayPage view={view} />
        ) : (
          <MainView view={view} />
        )}
      </ThemeSurface>
    </ConfigProvider>
  );
}

export default function App() {
  return (
    <RelayPulseProvider>
      <RelayPulseApplication />
    </RelayPulseProvider>
  );
}
