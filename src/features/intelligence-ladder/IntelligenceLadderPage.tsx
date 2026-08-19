import { LoadingOutlined, ReloadOutlined, SettingOutlined } from '@ant-design/icons';
import { Alert, App as AntdApp, Button, Empty, Result, Spin, Tooltip } from 'antd';
import { useState } from 'react';
import { recordFrontendAction } from '../../app/frontend-diagnostics';
import { formatLocalTimestamp } from '../../domain/time';
import { pageHeaderClass } from '../../styles/tailwind';
import { IntelligencePodium } from './IntelligencePodium';
import { IntelligenceRankingList } from './IntelligenceRankingList';
import { IntelligenceSettingsDrawer } from './IntelligenceSettingsDrawer';
import type {
  IntelligenceAutoFetchSettings,
  IntelligenceLadderDataSource,
  IntelligenceLadderSnapshot,
} from './intelligence-ladder-types';
import { useIntelligenceLadder } from './use-intelligence-ladder';

function IntelligenceLadderContent({
  loadError,
  loading,
  snapshot,
}: {
  loadError: string | null;
  loading: boolean;
  snapshot: IntelligenceLadderSnapshot | null;
}) {
  if (loading && !snapshot) {
    return (
      <div className="grid min-h-80 place-items-center">
        <Spin size="large" description="正在加载智力天梯" />
      </div>
    );
  }

  if (loadError && !snapshot) {
    return <Result status="warning" title="智力天梯加载失败" subTitle={loadError} />;
  }

  if (!snapshot) return null;

  if (snapshot.runtime.state === 'not_connected') {
    return (
      <Result
        status="info"
        title="智力天梯数据源尚未接入"
        subTitle="生产环境不显示开发样例数据。"
      />
    );
  }

  if (snapshot.entries.length === 0) {
    if (snapshot.lastError || snapshot.runtime.state === 'failed') {
      return (
        <Result
          status="warning"
          title="暂时无法获取排名"
          subTitle={snapshot.lastError?.message ?? '请稍后重试。'}
        />
      );
    }
    return (
      <div className="grid min-h-80 place-items-center">
        <Empty description="暂无排名数据" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      </div>
    );
  }

  return (
    <div className="mx-auto flex h-full min-h-0 w-full max-w-260 flex-col">
      {snapshot.runtime.state === 'running' ? (
        <Alert className="mb-4" showIcon title="数据刷新中..." type="info" />
      ) : snapshot.lastError ? (
        <Alert
          className="mb-4"
          description={snapshot.lastError.message}
          showIcon
          title={
            snapshot.runtime.state === 'scraper_unavailable' ? '采集服务不可用' : '上次刷新失败'
          }
          type={snapshot.runtime.state === 'scraper_unavailable' ? 'error' : 'warning'}
        />
      ) : null}

      <IntelligencePodium entries={snapshot.entries} />
      <IntelligenceRankingList entries={snapshot.entries} />

      <footer className="flex min-h-12 flex-none flex-wrap items-center justify-center gap-x-5 gap-y-1 py-3 text-xs text-rp-tertiary">
        <span>数据来源：Codex Radar</span>
        <span>
          上次成功：
          {snapshot.fetchedAt ? formatLocalTimestamp(snapshot.fetchedAt) : '从未成功'}
        </span>
      </footer>
    </div>
  );
}

export function IntelligenceLadderPage({
  dataSource,
}: {
  dataSource: IntelligenceLadderDataSource;
}) {
  const { message } = AntdApp.useApp();
  const { snapshot, loading, loadError, refresh, updateSettings } =
    useIntelligenceLadder(dataSource);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const running = snapshot?.runtime.state === 'running';
  const notConnected = snapshot?.runtime.state === 'not_connected';

  async function handleRefresh() {
    recordFrontendAction('refresh-intelligence-ladder', {
      targetType: 'intelligence-ladder',
      targetId: 'ranking',
    });
    try {
      const result = await refresh();
      if (result.status === 'already_running') {
        message.info({ key: 'intelligence-refresh', content: '数据刷新中...', duration: 3 });
      } else if (result.status === 'scraper_unavailable') {
        message.error({
          key: 'intelligence-refresh',
          content: '采集服务暂不可用',
          duration: 4,
        });
      }
    } catch {
      message.error({
        key: 'intelligence-refresh',
        content: '智力天梯刷新失败',
        duration: 4,
      });
    }
  }

  async function handleSave(settings: IntelligenceAutoFetchSettings) {
    recordFrontendAction('save-intelligence-settings', {
      targetType: 'intelligence-settings',
      targetId: 'auto-fetch',
    });
    setSaving(true);
    try {
      await updateSettings(settings);
      message.success({
        key: 'intelligence-settings',
        content: '自动获取设置已保存',
        duration: 3,
      });
      setSettingsOpen(false);
    } catch {
      message.error({
        key: 'intelligence-settings',
        content: '自动获取设置保存失败',
        duration: 4,
      });
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <header className={pageHeaderClass}>
        <div>
          <h1>智力天梯</h1>
          <p>查看模型智力效率排名</p>
        </div>
        <div className="flex gap-2">
          <Tooltip title={running ? '数据刷新中' : '刷新智力天梯'}>
            <Button
              aria-busy={running}
              aria-label="刷新智力天梯"
              disabled={loading || notConnected}
              icon={running ? <LoadingOutlined spin /> : <ReloadOutlined />}
              onClick={() => void handleRefresh()}
              shape="circle"
            />
          </Tooltip>
          <Tooltip title="自动获取设置">
            <Button
              aria-label="打开自动获取设置"
              disabled={loading || !snapshot}
              icon={<SettingOutlined />}
              onClick={() => {
                recordFrontendAction('open-intelligence-settings', {
                  targetType: 'intelligence-settings',
                  targetId: 'auto-fetch',
                });
                setSettingsOpen(true);
              }}
              shape="circle"
            />
          </Tooltip>
        </div>
      </header>

      <main
        className="min-h-0 flex-1 overflow-hidden px-6 py-5"
        data-testid="intelligence-ladder-content"
      >
        <IntelligenceLadderContent loadError={loadError} loading={loading} snapshot={snapshot} />
      </main>

      {snapshot ? (
        <IntelligenceSettingsDrawer
          disabled={notConnected}
          onClose={() => {
            if (!saving) {
              recordFrontendAction('close-intelligence-settings', {
                targetType: 'intelligence-settings',
                targetId: 'auto-fetch',
              });
              setSettingsOpen(false);
            }
          }}
          onSave={handleSave}
          open={settingsOpen}
          saving={saving}
          snapshot={snapshot}
        />
      ) : null}
    </>
  );
}
