import { HolderOutlined, InfoCircleOutlined, PlusOutlined } from '@ant-design/icons';
import { move } from '@dnd-kit/helpers';
import { DragDropProvider } from '@dnd-kit/react';
import { useSortable } from '@dnd-kit/react/sortable';
import { Button, Switch, Tooltip } from 'antd';
import { StatusDot } from '../../components/StatusDot';
import type { Site } from '../../domain/types';
import { navigateToView } from '../../app/view';
import styles from '../../styles/App.module.css';

function SortableSiteItem({
  site,
  index,
  selected,
  onSelect,
  onEnabledChange,
}: {
  site: Site;
  index: number;
  selected: boolean;
  onSelect(): void;
  onEnabledChange(enabled: boolean): void;
}) {
  const { ref, handleRef, isDragging } = useSortable({ id: site.id, index });
  return (
    <div
      ref={ref}
      className={`${styles.siteItem} ${selected ? styles.siteItemActive : ''} ${isDragging ? styles.siteItemDragging : ''}`}
      onClick={onSelect}
      data-site-id={site.id}
    >
      <Tooltip title="拖动调整优先级">
        <button
          ref={handleRef}
          className={styles.dragHandle}
          type="button"
          aria-label={`拖动 ${site.name}，当前优先级 ${site.priority}`}
        >
          <HolderOutlined />
        </button>
      </Tooltip>
      <div className={styles.siteItemCopy}>
        <div className={styles.siteItemName}>
          <StatusDot status={site.status} />
          <span title={site.name}>{site.name}</span>
          <span className={styles.priority}>{site.priority}</span>
        </div>
        <div className={styles.siteItemMeta}>{site.summary}</div>
      </div>
      <Switch
        size="small"
        checked={site.enabled}
        aria-label={`${site.enabled ? '停用' : '启用'} ${site.name}`}
        onClick={(_, event) => event.stopPropagation()}
        onChange={onEnabledChange}
      />
    </div>
  );
}

export function SiteList({
  sites,
  selectedSiteId,
  onSelect,
  onEnabledChange,
  onReorder,
}: {
  sites: Site[];
  selectedSiteId: string | null;
  onSelect(id: string): void;
  onEnabledChange(id: string, enabled: boolean): void;
  onReorder(ids: string[]): void;
}) {
  return (
    <aside className={styles.siteList}>
      <div className={styles.siteListHeader}>
        <div>
          <strong>站点</strong>
          <span className={styles.siteCount}>{sites.length}</span>
        </div>
        <Tooltip title="添加站点">
          <Button
            icon={<PlusOutlined />}
            aria-label="添加站点"
            onClick={() => navigateToView('add-site')}
          />
        </Tooltip>
      </div>
      <DragDropProvider
        onDragEnd={(event) => {
          if (event.canceled || !event.operation.target) return;
          const next = move(
            sites.map((site) => site.id),
            event,
          );
          onReorder(next);
        }}
      >
        <div className={styles.siteItems}>
          {sites.map((site, index) => (
            <SortableSiteItem
              key={site.id}
              site={site}
              index={index}
              selected={site.id === selectedSiteId}
              onSelect={() => onSelect(site.id)}
              onEnabledChange={(enabled) => onEnabledChange(site.id, enabled)}
            />
          ))}
        </div>
      </DragDropProvider>
      <div className={styles.siteTip}>
        <InfoCircleOutlined />
        <span>拖动调整优先级；停用或没有启用模型的站点不会参与探测。</span>
      </div>
    </aside>
  );
}
