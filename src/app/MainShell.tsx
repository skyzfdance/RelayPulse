import {
  AimOutlined,
  BellOutlined,
  HistoryOutlined,
  ProfileOutlined,
  QuestionCircleOutlined,
  SettingOutlined,
  TrophyOutlined,
} from '@ant-design/icons';
import type { PropsWithChildren } from 'react';
import { BrandMark } from '../components/BrandMark';
import type { ViewName } from '../domain/types';
import { APP_VERSION } from './app-metadata';
import { navigateToView } from './view';

const navItemClass =
  'mb-1 flex h-10 w-full cursor-pointer items-center gap-3 rounded-md border-0 px-3 text-left text-sm transition-colors disabled:cursor-default disabled:text-rp-quaternary [&_.anticon]:w-4 [&_.anticon]:text-base';
const activeNavItemClass = 'bg-rp-active font-medium text-rp-primary hover:bg-rp-active-hover';
const inactiveNavItemClass = 'bg-transparent text-rp-secondary hover:bg-rp-hover';

function navItemClasses(selected: boolean) {
  return `${navItemClass} ${selected ? activeNavItemClass : inactiveNavItemClass}`;
}

export function MainShell({ children, view }: PropsWithChildren<{ view: ViewName }>) {
  const active =
    view === 'general'
      ? 'general'
      : view === 'notifications'
        ? 'notifications'
        : view === 'records'
          ? 'records'
          : view === 'operation-history'
            ? 'operation-history'
            : view === 'intelligence-ladder'
              ? 'intelligence-ladder'
              : 'sites';
  return (
    <div className="h-full w-full min-w-240 overflow-hidden bg-rp-surface">
      <main
        className="h-full w-full min-w-240 overflow-hidden bg-rp-surface"
        data-testid="desktop-frame"
      >
        <div className="relative flex h-full overflow-hidden" id="relaypulse-overlay-root">
          <aside
            className="flex w-54 flex-none flex-col border-r border-rp-line bg-rp-subtle px-3 pb-4.5"
            data-testid="sidebar"
          >
            <div className="-mx-3 mb-4 flex min-h-19 flex-none items-center gap-2.5 border-b border-rp-line px-6 text-rp">
              <BrandMark />
              <strong>RelayPulse</strong>
            </div>
            <div className="px-3 pb-2 text-xs text-rp-tertiary">配置</div>
            <button
              aria-current={active === 'sites' ? 'page' : undefined}
              className={navItemClasses(active === 'sites')}
              onClick={() => navigateToView('sites')}
            >
              <AimOutlined />
              站点管理
            </button>
            <button
              aria-current={active === 'general' ? 'page' : undefined}
              className={navItemClasses(active === 'general')}
              onClick={() => navigateToView('general')}
            >
              <SettingOutlined />
              通用设置
            </button>
            <button
              aria-current={active === 'notifications' ? 'page' : undefined}
              className={navItemClasses(active === 'notifications')}
              onClick={() => navigateToView('notifications')}
            >
              <BellOutlined />
              消息通知
            </button>
            <div className="mt-5.5 px-3 pb-2 text-xs text-rp-tertiary">运行</div>
            <button
              aria-current={active === 'records' ? 'page' : undefined}
              className={navItemClasses(active === 'records')}
              onClick={() => navigateToView('records')}
            >
              <HistoryOutlined />
              探测记录
            </button>
            <button
              aria-current={active === 'operation-history' ? 'page' : undefined}
              className={navItemClasses(active === 'operation-history')}
              onClick={() => navigateToView('operation-history')}
            >
              <ProfileOutlined />
              操作记录
            </button>
            <div className="mt-5.5 px-3 pb-2 text-xs text-rp-tertiary">情报</div>
            <button
              aria-current={active === 'intelligence-ladder' ? 'page' : undefined}
              className={navItemClasses(active === 'intelligence-ladder')}
              onClick={() => navigateToView('intelligence-ladder')}
            >
              <TrophyOutlined />
              智力天梯
            </button>
            <div className="flex-1" />
            <button
              className={`${navItemClass} ${inactiveNavItemClass}`}
              disabled
              title="将在后续版本提供"
            >
              <QuestionCircleOutlined />
              帮助与反馈
            </button>
            <div className="px-3 pt-3 text-[11px] text-rp-quaternary">RelayPulse {APP_VERSION}</div>
          </aside>
          <section
            className="relative flex min-w-0 flex-1 flex-col overflow-hidden bg-rp-surface"
            data-testid="content"
          >
            {children}
          </section>
        </div>
      </main>
    </div>
  );
}
