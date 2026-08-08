import {
  BellOutlined,
  CheckCircleOutlined,
  DesktopOutlined,
  ExclamationCircleOutlined,
  SaveOutlined,
  SendOutlined,
} from '@ant-design/icons';
import { App as AntdApp, Badge, Button, Empty, Result, Spin, Switch, Table } from 'antd';
import type { TableProps } from 'antd';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { recordFrontendAction } from '../../app/frontend-diagnostics';
import { pageHeaderClass } from '../../styles/tailwind';
import type {
  NotificationChannelSnapshot,
  NotificationDataSource,
  NotificationEventCatalogItem,
  NotificationPermission,
  NotificationSettingsSnapshot,
  NotificationSettingsUpdate,
} from './notification-types';

interface NotificationsPageProps {
  dataSource: NotificationDataSource;
}

interface RuleRow extends NotificationEventCatalogItem {
  enabled: boolean;
}

const permissionPresentation: Record<
  NotificationPermission,
  { label: string; status: 'success' | 'warning' | 'error' | 'default' }
> = {
  unknown: { label: '状态未知', status: 'default' },
  not_requested: { label: '尚未授权', status: 'warning' },
  denied: { label: '授权已拒绝', status: 'error' },
  granted: { label: '已授权', status: 'success' },
  unsupported: { label: '当前平台不支持', status: 'default' },
};

function toDraft(snapshot: NotificationSettingsSnapshot): NotificationSettingsUpdate {
  const rules = new Map(snapshot.rules.map((rule) => [rule.eventType, rule]));
  return {
    revision: snapshot.revision,
    notificationsEnabled: snapshot.notificationsEnabled,
    rules: snapshot.eventCatalog.map((event) => {
      const rule = rules.get(event.eventType);
      return {
        eventType: event.eventType,
        enabled: rule?.enabled ?? event.defaultEnabled,
        channelIds: [...(rule?.channelIds ?? event.supportedChannels)],
      };
    }),
  };
}

function draftsMatch(left: NotificationSettingsUpdate, right: NotificationSettingsUpdate) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function eventIcon(event: NotificationEventCatalogItem) {
  if (event.tone === 'critical') return <ExclamationCircleOutlined />;
  if (event.tone === 'positive') return <CheckCircleOutlined />;
  return <BellOutlined />;
}

function eventIconClass(event: NotificationEventCatalogItem) {
  if (event.tone === 'critical') return 'text-rp-error bg-rp-error-bg';
  if (event.tone === 'positive') return 'text-rp-success bg-rp-active';
  return 'text-rp-primary bg-rp-active';
}

function groupedEvents(events: NotificationEventCatalogItem[]) {
  const groups = new Map<string, { label: string; events: NotificationEventCatalogItem[] }>();
  for (const event of events) {
    const group = groups.get(event.category);
    if (group) group.events.push(event);
    else {
      groups.set(event.category, {
        label: event.categoryLabel?.trim() || event.category,
        events: [event],
      });
    }
  }
  return [...groups.entries()].map(([key, value]) => ({ key, ...value }));
}

export function NotificationsPage({ dataSource }: NotificationsPageProps) {
  const { message, notification } = AntdApp.useApp();
  const source = dataSource;
  const [snapshot, setSnapshot] = useState<NotificationSettingsSnapshot | null>(null);
  const [draft, setDraft] = useState<NotificationSettingsUpdate | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [saving, setSaving] = useState(false);
  const [authorizingChannelId, setAuthorizingChannelId] = useState<string | null>(null);
  const [testingChannelId, setTestingChannelId] = useState<string | null>(null);

  const loadSettings = useCallback(async () => {
    setLoading(true);
    setLoadError(false);
    try {
      const next = await source.getSettings();
      setSnapshot(next);
      setDraft(toDraft(next));
    } catch {
      setLoadError(true);
    } finally {
      setLoading(false);
    }
  }, [source]);

  useEffect(() => {
    let active = true;
    void source
      .getSettings()
      .then((next) => {
        if (!active) return;
        setSnapshot(next);
        setDraft(toDraft(next));
      })
      .catch(() => {
        if (active) setLoadError(true);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [source]);

  const savedDraft = useMemo(() => (snapshot ? toDraft(snapshot) : null), [snapshot]);
  const dirty = Boolean(draft && savedDraft && !draftsMatch(draft, savedDraft));
  const channelsById = useMemo(
    () => new Map(snapshot?.channels.map((channel) => [channel.id, channel]) ?? []),
    [snapshot],
  );
  const draftRulesByType = useMemo(
    () => new Map(draft?.rules.map((rule) => [rule.eventType, rule]) ?? []),
    [draft],
  );

  function updateRule(eventType: string, enabled: boolean) {
    recordFrontendAction('setting-changed', {
      targetType: 'form',
      targetId: `notification-rule-${eventType.replace(/\./g, '-')}`,
    });
    setDraft((current) =>
      current
        ? {
            ...current,
            rules: current.rules.map((rule) =>
              rule.eventType === eventType ? { ...rule, enabled } : rule,
            ),
          }
        : current,
    );
  }

  async function saveSettings() {
    if (!draft || !dirty || saving) return;
    setSaving(true);
    try {
      const next = await source.updateSettings(draft);
      setSnapshot(next);
      setDraft(toDraft(next));
      message.success({ content: '通知设置已保存', duration: 3 });
    } catch {
      notification.error({
        title: '通知设置保存失败',
        description: '当前修改尚未保存，草稿已保留。请稍后重试。',
        placement: 'topRight',
        duration: 6,
        pauseOnHover: true,
        role: 'alert',
      });
    } finally {
      setSaving(false);
    }
  }

  async function sendTestNotification(channel: NotificationChannelSnapshot) {
    if (testingChannelId) return;
    setTestingChannelId(channel.id);
    try {
      await source.sendTestNotification(channel.id);
      message.success({ content: '测试通知已提交到系统通知中心', duration: 3 });
    } catch {
      notification.error({
        title: '测试通知发送失败',
        description: '系统通知当前不可用，请检查权限和通知设置后重试。',
        placement: 'topRight',
        duration: 6,
        pauseOnHover: true,
        role: 'alert',
      });
    } finally {
      setTestingChannelId(null);
    }
  }

  async function requestPermission(channel: NotificationChannelSnapshot) {
    if (authorizingChannelId) return;
    setAuthorizingChannelId(channel.id);
    try {
      const updated = await source.requestPermission(channel.id);
      setSnapshot((current) =>
        current
          ? {
              ...current,
              channels: current.channels.map((candidate) =>
                candidate.id === updated.id ? updated : candidate,
              ),
            }
          : current,
      );
      if (updated.permission === 'granted') {
        message.success({ content: '系统通知权限已授权', duration: 3 });
      } else {
        notification.warning({
          title: '系统通知权限未授权',
          description: 'RelayPulse 无法发送系统通知。',
          placement: 'topRight',
          duration: 6,
          pauseOnHover: true,
          role: 'alert',
        });
      }
    } catch {
      notification.error({
        title: '系统通知授权失败',
        description: '当前运行环境无法请求系统通知权限。',
        placement: 'topRight',
        duration: 6,
        pauseOnHover: true,
        role: 'alert',
      });
    } finally {
      setAuthorizingChannelId(null);
    }
  }

  function ruleColumns(): TableProps<RuleRow>['columns'] {
    return [
      {
        title: '事件',
        dataIndex: 'displayName',
        render: (_, event) => (
          <div className="flex min-w-0 items-start gap-2.5">
            <span
              aria-hidden="true"
              className={`grid size-7 flex-none place-items-center rounded-md text-[15px] ${eventIconClass(event)}`}
            >
              {eventIcon(event)}
            </span>
            <div className="min-w-0">
              <div className="font-medium break-words text-rp">{event.displayName}</div>
              <div className="mt-0.5 text-xs leading-5 break-words text-rp-tertiary">
                {event.description}
              </div>
            </div>
          </div>
        ),
      },
      {
        title: '通知渠道',
        dataIndex: 'supportedChannels',
        width: 170,
        render: (channelIds: string[]) => (
          <div className="flex flex-wrap gap-x-3 gap-y-1 text-rp-secondary">
            {channelIds.map((channelId) => (
              <span className="inline-flex items-center gap-1.5 whitespace-nowrap" key={channelId}>
                <DesktopOutlined aria-hidden="true" />
                {channelsById.get(channelId)?.displayName ?? channelId}
              </span>
            ))}
          </div>
        ),
      },
      {
        title: '启用',
        dataIndex: 'enabled',
        align: 'center',
        width: 88,
        render: (_, event) => (
          <Switch
            aria-label={`启用${event.displayName}通知`}
            checked={draftRulesByType.get(event.eventType)?.enabled ?? event.defaultEnabled}
            disabled={saving}
            onChange={(enabled) => updateRule(event.eventType, enabled)}
            size="small"
          />
        ),
      },
    ];
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-rp-surface">
      <header className={pageHeaderClass}>
        <div>
          <h1>消息通知</h1>
          <p>管理通知渠道和需要接收的事件</p>
        </div>
      </header>

      {loading ? (
        <div className="grid min-h-0 flex-1 place-items-center" data-testid="notifications-loading">
          <Spin size="large" description="正在加载通知设置" />
        </div>
      ) : loadError || !snapshot || !draft ? (
        <div className="grid min-h-0 flex-1 place-items-center px-6">
          <Result
            status="warning"
            title="通知设置加载失败"
            subTitle="暂时无法读取通知渠道和规则，请稍后重试。"
            extra={
              <Button type="primary" onClick={() => void loadSettings()}>
                重试
              </Button>
            }
          />
        </div>
      ) : (
        <>
          <div
            className="min-h-0 flex-1 overflow-y-auto px-7 pt-5 pb-8 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
            data-testid="notifications-content"
          >
            <div className="max-w-245">
              <section className="pb-5" aria-labelledby="notification-service-heading">
                <h2
                  className="mb-2.5 text-base leading-6 font-semibold"
                  id="notification-service-heading"
                >
                  通知服务
                </h2>
                <div className="grid min-h-14.5 grid-cols-[minmax(0,1fr)_auto] items-center gap-6 py-2">
                  <div>
                    <div className="font-medium">启用消息通知</div>
                    <div className="mt-0.5 text-xs leading-5 text-rp-tertiary">
                      关闭后保留当前规则，但不再发送任何通知
                    </div>
                  </div>
                  <Switch
                    aria-label="启用消息通知"
                    checked={draft.notificationsEnabled}
                    disabled={saving}
                    onChange={(notificationsEnabled) => {
                      recordFrontendAction('setting-changed', {
                        targetType: 'form',
                        targetId: 'notification-master',
                      });
                      setDraft((current) =>
                        current ? { ...current, notificationsEnabled } : current,
                      );
                    }}
                    size="small"
                  />
                </div>
              </section>

              <section
                className="border-t border-rp-line py-5"
                aria-labelledby="notification-channels-heading"
              >
                <h2
                  className="mb-2.5 text-base leading-6 font-semibold"
                  id="notification-channels-heading"
                >
                  通知渠道
                </h2>
                {snapshot.channels.length === 0 ? (
                  <Empty
                    className="py-5"
                    description="暂无可用通知渠道"
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                  />
                ) : (
                  snapshot.channels.map((channel) => {
                    const permission = permissionPresentation[channel.permission];
                    const testDisabled =
                      !snapshot.notificationsEnabled ||
                      !draft.notificationsEnabled ||
                      !channel.supported ||
                      channel.permission !== 'granted' ||
                      saving ||
                      authorizingChannelId === channel.id;
                    return (
                      <div
                        className="grid min-h-14.5 grid-cols-[minmax(0,1fr)_auto] items-center gap-6 py-2"
                        key={channel.id}
                      >
                        <div>
                          <div className="font-medium">{channel.displayName}</div>
                          <div className="mt-0.5 text-xs leading-5 text-rp-tertiary">
                            {channel.description}
                          </div>
                        </div>
                        <div className="flex flex-wrap items-center justify-end gap-3">
                          <Badge
                            status={permission.status}
                            text={
                              <span className="text-xs text-rp-secondary">{permission.label}</span>
                            }
                          />
                          {channel.permission === 'not_requested' ? (
                            <Button
                              disabled={saving || testingChannelId === channel.id}
                              icon={<BellOutlined aria-hidden="true" />}
                              loading={authorizingChannelId === channel.id}
                              onClick={() => void requestPermission(channel)}
                            >
                              请求授权
                            </Button>
                          ) : null}
                          <Button
                            disabled={testDisabled}
                            icon={<SendOutlined aria-hidden="true" />}
                            loading={testingChannelId === channel.id}
                            onClick={() => void sendTestNotification(channel)}
                          >
                            发送测试通知
                          </Button>
                        </div>
                      </div>
                    );
                  })
                )}
              </section>

              <section
                className="border-t border-rp-line pt-5"
                aria-labelledby="notification-rules-heading"
              >
                <h2 className="text-base leading-6 font-semibold" id="notification-rules-heading">
                  通知规则
                </h2>
                {snapshot.eventCatalog.length === 0 ? (
                  <Empty
                    className="py-12"
                    description="暂无可配置的通知事件"
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                  />
                ) : (
                  <div className="flex flex-col gap-5">
                    {groupedEvents(snapshot.eventCatalog).map((group) => {
                      const rows: RuleRow[] = group.events.map((event) => ({
                        ...event,
                        enabled:
                          draftRulesByType.get(event.eventType)?.enabled ?? event.defaultEnabled,
                      }));
                      return (
                        <div key={group.key}>
                          <h3 className="mb-2 text-xs leading-5 font-normal text-rp-tertiary">
                            {group.label}
                          </h3>
                          <Table<RuleRow>
                            className="notification-rule-table"
                            columns={ruleColumns()}
                            dataSource={rows}
                            pagination={false}
                            rowHoverable={false}
                            rowKey="eventType"
                            tableLayout="fixed"
                          />
                        </div>
                      );
                    })}
                  </div>
                )}
              </section>
            </div>
          </div>

          <footer className="flex min-h-18 flex-none items-center justify-end gap-2.5 px-6">
            <Button disabled={!dirty || saving} onClick={() => savedDraft && setDraft(savedDraft)}>
              放弃更改
            </Button>
            <Button
              disabled={!dirty}
              icon={<SaveOutlined aria-hidden="true" />}
              loading={saving}
              onClick={() => void saveSettings()}
              type="primary"
            >
              保存设置
            </Button>
          </footer>
        </>
      )}
    </div>
  );
}
