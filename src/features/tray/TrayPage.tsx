import { ReloadOutlined, SettingOutlined } from '@ant-design/icons';
import { Button, Tooltip } from 'antd';
import { BrandMark } from '../../components/BrandMark';
import { StatusDot } from '../../components/StatusDot';
import type { TraySnapshot, ViewName } from '../../domain/types';
import { navigateToView } from '../../app/view';
import { useRelayPulse } from '../../app/relay-pulse-context';
import styles from '../../styles/App.module.css';

function TrayHeader({ onRefresh }: { onRefresh(): void }) {
  return (
    <header className={styles.trayHeader}>
      <div className={styles.trayBrand}>
        <BrandMark compact />
        <strong>RelayPulse</strong>
      </div>
      <div className={styles.trayActions}>
        <Tooltip title="立即探测">
          <Button icon={<ReloadOutlined />} aria-label="立即探测" onClick={onRefresh} />
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
    <footer className={styles.trayFooter}>
      <span>{snapshot.completedLabel}</span>
      <span>{snapshot.nextProbeLabel}</span>
    </footer>
  );
}

export function TrayPage({ view }: { view: Extract<ViewName, 'tray-healthy' | 'tray-alerts'> }) {
  const { snapshot, reset } = useRelayPulse();
  if (!snapshot) return null;
  const healthy = view === 'tray-healthy';
  const tray = healthy ? snapshot.tray.healthy : snapshot.tray.alerts;

  return (
    <div
      className={`${styles.trayStage} ${healthy ? styles.trayStageHealthy : styles.trayStageAlerts}`}
    >
      <main className={`${styles.trayPanel} ${healthy ? styles.trayHealthy : styles.trayAlerts}`}>
        <TrayHeader onRefresh={() => void reset()} />
        {healthy ? (
          <section className={styles.trayHealthyBody}>
            <StatusDot status="healthy" halo />
            <div>
              <h1>全部正常</h1>
              <p>
                {tray.activeSites} 个站点 · {tray.activeModels} 个模型正在探测
              </p>
            </div>
          </section>
        ) : (
          <>
            <section className={styles.trayAlertBody}>
              <div className={styles.traySummary}>
                <div>
                  <StatusDot status="error" halo />
                  <h1>3 个异常</h1>
                </div>
                <span>按站点优先级排序</span>
              </div>
              <div className={styles.trayAlertRows}>
                {tray.alerts.slice(0, 3).map((alert) => (
                  <div className={styles.trayAlertRow} key={alert.id}>
                    <div className={styles.trayAlertIdentity}>
                      <StatusDot status={alert.status} />
                      <div>
                        <strong title={alert.modelName}>{alert.modelName}</strong>
                        <span title={alert.siteName}>{alert.siteName}</span>
                      </div>
                    </div>
                    <div className={styles.trayMetric}>
                      <span>首 Token</span>
                      <strong className={styles[`metric-${alert.status}`]}>
                        {alert.firstToken}
                      </strong>
                    </div>
                    <div className={styles.trayMetric}>
                      <span>总耗时</span>
                      <strong className={styles[`metric-${alert.status}`]}>
                        {alert.totalTime}
                      </strong>
                    </div>
                  </div>
                ))}
              </div>
            </section>
            <button className={styles.trayMore} type="button">
              查看全部 {tray.alertCount} 个异常 ›
            </button>
          </>
        )}
        <TrayFooter snapshot={tray} />
      </main>
    </div>
  );
}
