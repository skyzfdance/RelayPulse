import { ExclamationCircleFilled } from '@ant-design/icons';
import { Modal } from 'antd';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { navigateToView } from '../../app/view';
import styles from '../../styles/App.module.css';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;

export function DeleteSiteModal() {
  const { snapshot, deleteSite } = useRelayPulse();
  const site = snapshot?.sites.find((item) => item.id === snapshot.selectedSiteId);
  if (!site) return null;

  return (
    <Modal
      open
      width={440}
      centered
      getContainer={overlayContainer}
      rootClassName={styles.scopedOverlay}
      rootStyle={{ position: 'absolute' }}
      closable={false}
      mask={{ closable: false }}
      okText="删除站点"
      okButtonProps={{ danger: true }}
      cancelText="取消"
      onCancel={() => navigateToView('sites')}
      onOk={async () => {
        await deleteSite(site.id);
        navigateToView('sites');
      }}
    >
      <div className={styles.deleteConfirm}>
        <span className={styles.deleteIcon}>
          <ExclamationCircleFilled />
        </span>
        <div>
          <h3>删除“{site.name}”？</h3>
          <p>将同时删除该站点的 {site.models.length} 个模型配置和本地探测记录。此操作无法撤销。</p>
        </div>
      </div>
    </Modal>
  );
}
