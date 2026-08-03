import { ExclamationCircleFilled } from '@ant-design/icons';
import { Modal } from 'antd';
import { useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { useBackendActionFeedback } from '../../app/use-backend-action-feedback';
import { navigateToView } from '../../app/view';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;

export function DeleteSiteModal() {
  const { snapshot, deleteSite } = useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
  const [deleting, setDeleting] = useState(false);
  const site = snapshot?.sites.find((item) => item.id === snapshot.selectedSiteId);
  if (!site) return null;

  return (
    <Modal
      open
      width={440}
      centered
      getContainer={overlayContainer}
      rootStyle={{ position: 'absolute' }}
      closable={false}
      mask={{ closable: false }}
      okText="删除站点"
      okButtonProps={{ danger: true }}
      confirmLoading={deleting}
      cancelText="取消"
      onCancel={() => navigateToView('sites')}
      onOk={async () => {
        setDeleting(true);
        try {
          const deleted = await runBackendAction(() => deleteSite(site.id), {
            key: `delete-site-${site.id}`,
            successTitle: '站点已删除',
            successDescription: `“${site.name}”及其本地探测记录已删除。`,
            errorTitle: '站点删除失败',
            errorDescription: '站点仍然保留，请稍后重试。',
          });
          if (deleted) navigateToView('sites');
        } finally {
          setDeleting(false);
        }
      }}
    >
      <div className="flex items-start gap-3.5 pt-2 pb-0.5">
        <span className="grid size-8.5 flex-none place-items-center rounded-full bg-rp-error-bg text-lg text-rp-error">
          <ExclamationCircleFilled />
        </span>
        <div className="[&_h3]:mt-0 [&_h3]:mb-1.5 [&_h3]:text-base [&_p]:m-0 [&_p]:leading-5.5 [&_p]:text-rp-secondary">
          <h3>删除“{site.name}”？</h3>
          <p>将同时删除该站点的 {site.models.length} 个模型配置和本地探测记录。此操作无法撤销。</p>
        </div>
      </div>
    </Modal>
  );
}
