import type { HealthStatus } from '../domain/types';
import styles from '../styles/App.module.css';

export function StatusDot({ status, halo = false }: { status: HealthStatus; halo?: boolean }) {
  return (
    <span
      className={`${styles.statusDot} ${styles[`status-${status}`]} ${halo ? styles.statusHalo : ''}`}
      aria-label={
        status === 'healthy'
          ? '正常'
          : status === 'warning'
            ? '注意'
            : status === 'error'
              ? '异常'
              : '未启用'
      }
      role="img"
    />
  );
}
