import {
  AimOutlined,
  HistoryOutlined,
  QuestionCircleOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import type { PropsWithChildren } from 'react';
import { BrandMark } from '../components/BrandMark';
import type { ViewName } from '../domain/types';
import { navigateToView } from './view';
import styles from '../styles/App.module.css';

export function MainShell({ children, view }: PropsWithChildren<{ view: ViewName }>) {
  const active = view === 'general' ? 'general' : view === 'records' ? 'records' : 'sites';
  return (
    <div className={styles.desktopStage}>
      <main className={styles.desktopFrame}>
        <header className={styles.titlebar}>
          <div className={styles.brand}>
            <BrandMark />
            <strong>RelayPulse</strong>
          </div>
          <div className={styles.titlebarTitle}>模型可用性探针</div>
          <div className={styles.titlebarSpacer} aria-hidden="true" />
        </header>
        <div className={styles.appBody} id="relaypulse-overlay-root">
          <aside className={styles.sidebar}>
            <div className={styles.navLabel}>配置</div>
            <button
              className={`${styles.navItem} ${active === 'sites' ? styles.navItemActive : ''}`}
              onClick={() => navigateToView('sites')}
            >
              <AimOutlined />
              站点管理
            </button>
            <button
              className={`${styles.navItem} ${active === 'general' ? styles.navItemActive : ''}`}
              onClick={() => navigateToView('general')}
            >
              <SettingOutlined />
              通用设置
            </button>
            <div className={styles.navLabelSecondary}>运行</div>
            <button
              className={`${styles.navItem} ${active === 'records' ? styles.navItemActive : ''}`}
              onClick={() => navigateToView('records')}
            >
              <HistoryOutlined />
              探测记录
            </button>
            <div className={styles.navSpacer} />
            <button className={styles.navItem} disabled title="将在后续版本提供">
              <QuestionCircleOutlined />
              帮助与反馈
            </button>
            <div className={styles.version}>RelayPulse 0.1.0</div>
          </aside>
          <section className={styles.content}>{children}</section>
        </div>
      </main>
    </div>
  );
}
