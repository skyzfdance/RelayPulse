import type { HealthStatus } from '../domain/types';

const statusClass: Record<HealthStatus, string> = {
  healthy: 'bg-rp-success',
  warning: 'bg-rp-warning',
  error: 'bg-rp-error',
  muted: 'bg-rp-muted',
};

const haloClass: Record<HealthStatus, string> = {
  healthy: 'shadow-[0_0_0_4px_rgb(82_196_26/10%)]',
  warning: 'shadow-[0_0_0_4px_rgb(250_173_20/10%)]',
  error: 'shadow-[0_0_0_4px_rgb(255_77_79/10%)]',
  muted: '',
};

export function StatusDot({ status, halo = false }: { status: HealthStatus; halo?: boolean }) {
  return (
    <span
      className={`inline-block flex-none rounded-full ${halo ? 'size-2.75' : 'size-1.75'} ${statusClass[status]} ${halo ? haloClass[status] : ''}`}
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
