import type { ManagedSite, SiteDraft } from '@/types/domain/sites';

export const healthLabels = { healthy: '正常', warning: '偏慢', error: '异常', muted: '未探测' };
export const healthColors = {
  healthy: 'var(--ant-color-success)',
  warning: 'var(--ant-color-warning)',
  error: 'var(--ant-color-error)',
  muted: 'var(--ant-color-text-quaternary)',
};
/** 从已加载的详情复制基本信息草稿；高级配置由抽屉独立编辑。 */
export function siteDraft(site: ManagedSite): SiteDraft {
  return {
    name: site.name,
    protocol: site.protocol,
    baseUrl: site.baseUrl,
    credentialAction: 'keep',
    secret: '',
  };
}

export function validateSite(draft: SiteDraft): string | null {
  if (!draft.name.trim()) return '请输入站点名称';
  try {
    const url = new URL(draft.baseUrl);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password)
      return '请输入不含凭据的 HTTP 或 HTTPS 地址';
  } catch {
    return '请输入有效的 Base URL';
  }
  if (draft.credentialAction === 'replace' && !draft.secret.trim()) return '请输入新的访问凭据';
  return null;
}
