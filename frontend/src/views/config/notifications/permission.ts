/**
 * 系统通知授权状态的展示配置：每个状态对应 badge 文案/颜色与右侧动作。
 * 查表驱动（避免视图里堆条件判断），仅本特性使用。
 */
import type { NotificationPermission } from '@/types/domain/notifications';

/** 授权行右侧动作：请求授权 / 发送测试 / 去系统设置 / 无。 */
export type PermissionAction = 'request' | 'test' | 'settings' | 'none';

export const PERMISSION: Record<
  NotificationPermission,
  { label: string; status: 'success' | 'warning' | 'error' | 'default'; action: PermissionAction }
> = {
  unknown: { label: '状态未知', status: 'default', action: 'request' },
  not_requested: { label: '尚未授权', status: 'warning', action: 'request' },
  denied: { label: '已拒绝', status: 'error', action: 'settings' },
  granted: { label: '已授权', status: 'success', action: 'test' },
  unsupported: { label: '当前平台不支持', status: 'default', action: 'none' },
};
