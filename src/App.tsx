import { App as AntdApp, Button, ConfigProvider, Result, Spin, theme } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { useEffect, useState } from 'react';
import type { CSSProperties, PropsWithChildren } from 'react';
import { MainShell } from './app/MainShell';
import { RelayPulseProvider } from './app/RelayPulseProvider';
import { useRelayPulse } from './app/relay-pulse-context';
import { parseView } from './app/view';
import type { ThemeMode, ViewName } from './domain/types';
import { GeneralPage } from './features/general/GeneralPage';
import { ModelSettingsDrawer } from './features/models/ModelSettingsDrawer';
import { RecordsPage } from './features/records/RecordsPage';
import { AddSiteModal } from './features/sites/AddSiteModal';
import { DeleteSiteModal } from './features/sites/DeleteSiteModal';
import { EmptyPage } from './features/sites/EmptyPage';
import { SitesPage } from './features/sites/SitesPage';
import { TrayPage } from './features/tray/TrayPage';
import styles from './styles/App.module.css';

function useActiveView() {
  const [view, setView] = useState<ViewName>(() => parseView(window.location.search));
  useEffect(() => {
    const sync = () => setView(parseView(window.location.search));
    window.addEventListener('popstate', sync);
    return () => window.removeEventListener('popstate', sync);
  }, []);
  return view;
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

function ThemeSurface({ children, dark }: PropsWithChildren<{ dark: boolean }>) {
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
    <div className={styles.themeSurface} data-theme={dark ? 'dark' : 'light'} style={variables}>
      <AntdApp className={styles.antdRoot}>{children}</AntdApp>
    </div>
  );
}

function MainView({ view }: { view: ViewName }) {
  const { snapshot } = useRelayPulse();
  const baseView =
    view === 'site-advanced'
      ? 'site-advanced'
      : view === 'general'
        ? 'general'
        : view === 'records'
          ? 'records'
          : view === 'empty' || snapshot?.sites.length === 0
            ? 'empty'
            : 'sites';
  return (
    <MainShell view={view}>
      {baseView === 'general' ? (
        <GeneralPage />
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
  const view = useActiveView();
  const { snapshot, loading, error } = useRelayPulse();
  const isDark = useResolvedTheme(snapshot?.generalSettings.theme ?? 'light');

  function retryMockLoad() {
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
      <ThemeSurface dark={isDark}>
        {loading ? (
          <div className={styles.fullState}>
            <Spin size="large" description="正在加载 Mock 数据" />
          </div>
        ) : error || !snapshot ? (
          <div className={styles.fullState}>
            <Result
              status="warning"
              title="Mock 数据加载失败"
              subTitle={error ?? '没有可用数据'}
              extra={
                <Button type="primary" onClick={retryMockLoad}>
                  重试
                </Button>
              }
            />
          </div>
        ) : view === 'tray-healthy' || view === 'tray-alerts' ? (
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
