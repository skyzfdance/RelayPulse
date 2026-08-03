import { HolderOutlined, InfoCircleOutlined, PlusOutlined } from '@ant-design/icons';
import { move } from '@dnd-kit/helpers';
import { DragDropProvider } from '@dnd-kit/react';
import { useSortable } from '@dnd-kit/react/sortable';
import { Button, Switch, Tooltip } from 'antd';
import { StatusDot } from '../../components/StatusDot';
import type { Site } from '../../domain/types';
import { navigateToView } from '../../app/view';

function SortableSiteItem({
  site,
  index,
  selected,
  loading,
  disabled,
  onSelect,
  onEnabledChange,
}: {
  site: Site;
  index: number;
  selected: boolean;
  loading: boolean;
  disabled: boolean;
  onSelect(): void;
  onEnabledChange(enabled: boolean): void;
}) {
  const { ref, handleRef, isDragging } = useSortable({ id: site.id, index });
  return (
    <div
      ref={ref}
      className={`grid min-h-16 cursor-pointer grid-cols-[20px_minmax(0,1fr)_34px] items-center gap-1.5 rounded-md border px-2.5 py-2 pl-1 transition-colors ${
        selected
          ? 'border-rp-line-active bg-rp-surface hover:bg-rp-surface'
          : 'border-transparent bg-transparent hover:bg-rp-hover'
      } ${isDragging ? 'z-2 opacity-75 shadow-[0_4px_12px_rgb(0_0_0/12%)]' : ''}`}
      onClick={onSelect}
      data-site-id={site.id}
    >
      <Tooltip title="拖动调整优先级">
        <button
          ref={handleRef}
          className="h-8 w-5 cursor-grab border-0 bg-transparent p-0 text-rp-quaternary active:cursor-grabbing"
          type="button"
          aria-label={`拖动 ${site.name}，当前优先级 ${site.priority}`}
        >
          <HolderOutlined />
        </button>
      </Tooltip>
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-1.75 leading-5.5 font-semibold">
          <StatusDot status={site.status} />
          <span className="truncate" title={site.name}>
            {site.name}
          </span>
          <span className="inline-flex h-4.25 min-w-4.25 items-center justify-center rounded-sm bg-rp-fill-secondary px-1 text-[11px] font-normal text-rp-tertiary">
            {site.priority}
          </span>
        </div>
        <div className="mt-px truncate text-xs leading-5 text-rp-tertiary">{site.summary}</div>
      </div>
      <Switch
        className="justify-self-end"
        size="small"
        checked={site.enabled}
        loading={loading}
        disabled={disabled}
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
  loadingSiteId,
  onSelect,
  onEnabledChange,
  onReorder,
}: {
  sites: Site[];
  selectedSiteId: string | null;
  loadingSiteId?: string | null;
  onSelect(id: string): void;
  onEnabledChange(id: string, enabled: boolean): void;
  onReorder(ids: string[]): void;
}) {
  return (
    <aside className="w-75 flex-none overflow-y-auto border-r border-rp-line bg-rp-subtle px-3 py-4 max-[1180px]:w-69">
      <div className="mb-2 flex h-8 items-center justify-between">
        <div>
          <strong>站点</strong>
          <span className="ml-1.5 text-xs font-normal text-rp-tertiary">{sites.length}</span>
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
        <div className="flex flex-col gap-1.5">
          {sites.map((site, index) => (
            <SortableSiteItem
              key={site.id}
              site={site}
              index={index}
              selected={site.id === selectedSiteId}
              loading={site.id === loadingSiteId}
              disabled={Boolean(loadingSiteId && site.id !== loadingSiteId)}
              onSelect={() => onSelect(site.id)}
              onEnabledChange={(enabled) => onEnabledChange(site.id, enabled)}
            />
          ))}
        </div>
      </DragDropProvider>
      <div className="mt-3.5 flex items-start gap-2 text-xs leading-4.5 text-rp-tertiary">
        <InfoCircleOutlined />
        <span>拖动调整优先级；停用或没有启用模型的站点不会参与探测。</span>
      </div>
    </aside>
  );
}
