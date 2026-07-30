import { Button, Segmented, Select, Switch } from 'antd';
import { useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import type { GeneralSettings, ThemeMode } from '../../domain/types';
import styles from '../../styles/App.module.css';

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
    <div className={styles.settingsRow}>
      <div className={styles.settingsCopy}>
        <strong>{title}</strong>
        {description ? <span>{description}</span> : null}
      </div>
      <div className={styles.settingsControl}>{children}</div>
    </div>
  );
}

export function GeneralPage() {
  const { snapshot, updateGeneralSettings } = useRelayPulse();
  const [settings, setSettings] = useState<GeneralSettings | null>(
    snapshot?.generalSettings ?? null,
  );
  if (!settings) return null;

  return (
    <>
      <header className={styles.pageHeader}>
        <div>
          <h1>通用设置</h1>
          <p>控制 RelayPulse 的系统行为、外观与本地数据</p>
        </div>
      </header>
      <div className={styles.generalContent}>
        <section className={styles.settingsSection}>
          <h2>启动与后台</h2>
          <SettingsRow title="开机自动启动" description="登录系统后自动在状态栏运行 RelayPulse">
            <Switch
              size="small"
              checked={settings.launchAtStartup}
              onChange={(launchAtStartup) => setSettings({ ...settings, launchAtStartup })}
            />
          </SettingsRow>
          <SettingsRow
            title="关闭主窗口后继续运行"
            description="关闭配置窗口时保留状态栏图标和后台探测"
          >
            <Switch
              size="small"
              checked={settings.continueInBackground}
              onChange={(continueInBackground) =>
                setSettings({ ...settings, continueInBackground })
              }
            />
          </SettingsRow>
          <SettingsRow title="状态栏图标操作" description="选择单击状态栏图标时显示的内容">
            <Select
              className={styles.settingsSelect}
              value={settings.trayAction}
              options={[{ value: 'panel', label: '打开状态小面板' }]}
              onChange={(trayAction) => setSettings({ ...settings, trayAction })}
            />
          </SettingsRow>
        </section>

        <section className={styles.settingsSection}>
          <h2>外观与语言</h2>
          <SettingsRow title="主题" description="跟随系统时会自动切换浅色和深色模式">
            <Segmented<ThemeMode>
              value={settings.theme}
              options={[
                { value: 'system', label: '跟随系统' },
                { value: 'light', label: '浅色' },
                { value: 'dark', label: '深色' },
              ]}
              onChange={(theme) => setSettings({ ...settings, theme })}
            />
          </SettingsRow>
          <SettingsRow title="界面语言">
            <Select
              className={styles.settingsSelect}
              value={settings.language}
              options={[{ value: 'zh-CN', label: '简体中文' }]}
            />
          </SettingsRow>
        </section>

        <section className={styles.settingsSection}>
          <h2>本地数据</h2>
          <SettingsRow
            title="探测记录保留时间"
            description="过期记录会自动清理，不影响站点和模型配置"
          >
            <Select
              className={styles.settingsSelect}
              value={settings.historyRetentionDays}
              options={[{ value: 30, label: '保留 30 天' }]}
            />
          </SettingsRow>
          <SettingsRow title="诊断日志" description="日志保存在本机，不包含 API Key 明文">
            <div className={styles.settingsButtons}>
              <Button>打开日志目录</Button>
              <Button>导出诊断包</Button>
            </div>
          </SettingsRow>
        </section>
      </div>
      <footer className={styles.generalFooter}>
        <Button type="primary" onClick={() => void updateGeneralSettings(settings)}>
          保存设置
        </Button>
      </footer>
    </>
  );
}
