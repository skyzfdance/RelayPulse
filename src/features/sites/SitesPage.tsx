import { ApiOutlined, DeleteOutlined, PlusOutlined, SyncOutlined } from '@ant-design/icons';
import { Alert, Button, Form, Input, InputNumber, Modal, Select, Tag } from 'antd';
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { useBackendActionFeedback } from '../../app/use-backend-action-feedback';
import { navigateToView } from '../../app/view';
import { StatusDot } from '../../components/StatusDot';
import type {
  RequestHeader,
  Site,
  SiteAdvancedInput,
  UpdateSiteInput,
  ViewName,
} from '../../domain/types';
import { AddModelModal } from '../models/AddModelModal';
import { ModelTable } from '../models/ModelTable';
import { SiteList } from './SiteList';
import {
  monoInputClass,
  pageHeaderClass,
  scopedOverlayClass,
  scrollbarNoneClass,
} from '../../styles/tailwind';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;
const formGridClass =
  'grid grid-cols-2 gap-x-5 [&_.ant-form-item]:mb-2.5 [&_.ant-form-item-label]:pb-1';
const detailSectionClass =
  'border-b border-rp-line pb-3 [&+section]:pt-3 [&_h3]:m-0 [&_h3]:text-sm [&_h3]:leading-5.5 [&_h3]:font-semibold';
const sectionHeaderClass =
  'mb-2.5 flex items-center justify-between gap-4 [&_p]:m-0 [&_p]:text-xs [&_p]:leading-5 [&_p]:text-rp-tertiary';

type SiteFormValues = UpdateSiteInput & SiteAdvancedInput;

interface SiteDetailHandle {
  discard(): void;
  save(): Promise<boolean>;
}

function siteFormValues(site: Site): SiteFormValues {
  return {
    name: site.name,
    protocol: site.protocol,
    baseUrl: site.baseUrl,
    apiKeyMasked: site.apiKeyMasked,
    modelsEndpoint: site.modelsEndpoint,
    probeEndpoint: site.probeEndpoint,
    probeIntervalMinutes: site.probeIntervalMinutes,
    firstTokenTimeoutSeconds: site.firstTokenTimeoutSeconds,
    totalTimeoutSeconds: site.totalTimeoutSeconds,
    headers: site.headers,
  };
}

function valueOrBaseline<T>(value: T | undefined, baseline: T) {
  return value === undefined ? baseline : value;
}

function hasFormChanges(values: Partial<SiteFormValues>, baseline: SiteFormValues) {
  return (
    valueOrBaseline(values.name, baseline.name) !== baseline.name ||
    valueOrBaseline(values.protocol, baseline.protocol) !== baseline.protocol ||
    valueOrBaseline(values.baseUrl, baseline.baseUrl) !== baseline.baseUrl ||
    valueOrBaseline(values.apiKeyMasked, baseline.apiKeyMasked) !== baseline.apiKeyMasked ||
    valueOrBaseline(values.modelsEndpoint, baseline.modelsEndpoint) !== baseline.modelsEndpoint ||
    valueOrBaseline(values.probeEndpoint, baseline.probeEndpoint) !== baseline.probeEndpoint ||
    valueOrBaseline(values.probeIntervalMinutes, baseline.probeIntervalMinutes) !==
      baseline.probeIntervalMinutes ||
    valueOrBaseline(values.firstTokenTimeoutSeconds, baseline.firstTokenTimeoutSeconds) !==
      baseline.firstTokenTimeoutSeconds ||
    valueOrBaseline(values.totalTimeoutSeconds, baseline.totalTimeoutSeconds) !==
      baseline.totalTimeoutSeconds ||
    JSON.stringify(valueOrBaseline(values.headers, baseline.headers)) !==
      JSON.stringify(baseline.headers)
  );
}

function PageHeader() {
  return (
    <header className={pageHeaderClass}>
      <div>
        <h1>站点管理</h1>
        <p>配置多个模型站点，并通过拖动顺序决定故障时的优先级</p>
      </div>
    </header>
  );
}

function BasicFields() {
  return (
    <div className={formGridClass}>
      <Form.Item
        label="站点名称"
        name="name"
        rules={[{ required: true, whitespace: true, message: '请输入站点名称' }]}
      >
        <Input />
      </Form.Item>
      <Form.Item label="接口协议" name="protocol" rules={[{ required: true }]}>
        <Select options={[{ value: 'OpenAI 兼容', label: 'OpenAI 兼容' }]} />
      </Form.Item>
      <Form.Item
        label="Base URL"
        name="baseUrl"
        rules={[{ required: true, type: 'url', message: '请输入有效的 Base URL' }]}
      >
        <Input className={monoInputClass} />
      </Form.Item>
      <Form.Item label="API Key" name="apiKeyMasked" rules={[{ required: true }]}>
        <Input.Password className={monoInputClass} visibilityToggle />
      </Form.Item>
    </div>
  );
}

function AdvancedFields({
  initialHeaders,
  onHeadersChange,
}: {
  initialHeaders: RequestHeader[];
  onHeadersChange(headers: RequestHeader[]): void;
}) {
  const [headers, setHeaders] = useState<RequestHeader[]>(initialHeaders);

  function updateHeaders(next: RequestHeader[]) {
    setHeaders(next);
    onHeadersChange(next);
  }

  return (
    <>
      <section className={detailSectionClass}>
        <div className={sectionHeaderClass}>
          <div>
            <h3>高级连接</h3>
            <p>端点、超时和自定义请求头</p>
          </div>
        </div>
        <div className={formGridClass}>
          <Form.Item label="模型列表端点" name="modelsEndpoint" rules={[{ required: true }]}>
            <Input className={monoInputClass} />
          </Form.Item>
          <Form.Item label="探测端点" name="probeEndpoint" rules={[{ required: true }]}>
            <Input className={monoInputClass} />
          </Form.Item>
        </div>
        <div className="grid grid-cols-3 gap-x-5 [&_.ant-form-item]:mb-2.5 [&_.ant-form-item-label]:pb-1">
          <Form.Item
            label="探测频率"
            name="probeIntervalMinutes"
            tooltip="每个已启用模型自动探测的间隔"
            rules={[
              { required: true, message: '请输入探测频率' },
              { type: 'number', min: 1, max: 1440, message: '请输入 1 至 1440 分钟' },
            ]}
          >
            <InputNumber min={1} max={1440} precision={0} suffix="分钟" className="w-full" />
          </Form.Item>
          <Form.Item label="首 Token 超时" name="firstTokenTimeoutSeconds">
            <InputNumber min={1} max={120} suffix="秒" className="w-full" />
          </Form.Item>
          <Form.Item label="请求总超时" name="totalTimeoutSeconds">
            <InputNumber min={1} max={300} suffix="秒" className="w-full" />
          </Form.Item>
        </div>
        <Form.Item name="headers" hidden>
          <Input />
        </Form.Item>
        <div className="overflow-hidden rounded-md border border-rp-line [&>.ant-btn-link]:h-10">
          <div className="grid min-h-10 grid-cols-[1fr_1.5fr_40px] items-center border-b border-rp-line bg-rp-subtle px-3 text-xs text-rp-secondary">
            <span>请求头</span>
            <span>值</span>
            <span />
          </div>
          {headers.map((header, index) => (
            <div
              className="grid min-h-10 grid-cols-[1fr_1.5fr_40px] items-center border-b border-rp-line last-of-type:border-b"
              key={`${header.key}-${index}`}
            >
              <Input
                variant="borderless"
                className={monoInputClass}
                value={header.key}
                onChange={(event) => {
                  const next = [...headers];
                  next[index] = { ...header, key: event.target.value };
                  updateHeaders(next);
                }}
              />
              <Input
                variant="borderless"
                className={monoInputClass}
                value={header.value}
                onChange={(event) => {
                  const next = [...headers];
                  next[index] = { ...header, value: event.target.value };
                  updateHeaders(next);
                }}
              />
              <Button
                type="text"
                icon={<DeleteOutlined />}
                aria-label={`删除请求头 ${header.key}`}
                onClick={() => updateHeaders(headers.filter((_, current) => current !== index))}
              />
            </div>
          ))}
          <Button
            type="link"
            icon={<PlusOutlined />}
            onClick={() => updateHeaders([...headers, { key: '', value: '' }])}
          >
            添加请求头
          </Button>
        </div>
      </section>
      <HeaderSync headers={headers} />
    </>
  );
}

function HeaderSync({ headers }: { headers: RequestHeader[] }) {
  const form = Form.useFormInstance();
  useEffect(() => form.setFieldValue('headers', headers), [form, headers]);
  return null;
}

const SiteDetail = forwardRef<
  SiteDetailHandle,
  { site: Site; advanced: boolean; onDirtyChange(dirty: boolean): void }
>(function SiteDetail({ site, advanced, onDirtyChange }, ref) {
  const { updateSite, updateSiteAdvanced, setModelEnabled, refreshModels } = useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
  const [form] = Form.useForm<SiteFormValues>();
  const [advancedDraftVersion, setAdvancedDraftVersion] = useState(0);
  const [addModelOpen, setAddModelOpen] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [syncingModels, setSyncingModels] = useState(false);
  const [loadingModelId, setLoadingModelId] = useState<string | null>(null);

  const initialValues = useMemo(() => siteFormValues(site), [site]);
  const [baseline, setBaseline] = useState(initialValues);
  const defaultStrategyIsEconomical =
    site.defaultProbeSettings.mode === 'disabled-thinking' &&
    site.defaultProbeSettings.maxOutputTokens === 8 &&
    site.defaultProbeSettings.prompt === '只回复 OK' &&
    site.defaultProbeSettings.parameters.length === 0;
  const defaultStrategySummary = `${
    site.defaultProbeSettings.mode === 'disabled-thinking' ? '优先关闭思考' : '使用最低推理'
  } · 最多输出 ${site.defaultProbeSettings.maxOutputTokens} tokens · ${site.defaultProbeSettings.prompt}`;

  function updateDirty(next: boolean) {
    setDirty(next);
    onDirtyChange(next);
  }

  function discard() {
    form.setFieldsValue(baseline);
    setAdvancedDraftVersion((current) => current + 1);
    updateDirty(false);
  }

  async function save() {
    let values: SiteFormValues;
    try {
      values = await form.validateFields();
    } catch (error) {
      if (typeof error === 'object' && error !== null && 'errorFields' in error) return false;
      throw error;
    }

    setSaving(true);
    try {
      const saved = await runBackendAction(
        async () => {
          await updateSite(site.id, values);
          if (advanced) {
            await updateSiteAdvanced(site.id, {
              modelsEndpoint: values.modelsEndpoint,
              probeEndpoint: values.probeEndpoint,
              probeIntervalMinutes: values.probeIntervalMinutes,
              firstTokenTimeoutSeconds: values.firstTokenTimeoutSeconds,
              totalTimeoutSeconds: values.totalTimeoutSeconds,
              headers: values.headers,
            });
          }
        },
        {
          key: `save-site-${site.id}`,
          successTitle: advanced ? '站点高级设置已保存' : '站点配置已保存',
          successDescription: advanced
            ? `“${values.name}”的端点、探测频率、超时和请求头已更新。`
            : `“${values.name}”的基本信息已更新。`,
          errorTitle: advanced ? '站点高级设置保存失败' : '站点配置保存失败',
          errorDescription: '更改没有保存，当前编辑内容已保留，请检查后重试。',
        },
      );
      if (!saved) return false;
      setBaseline(values);
      updateDirty(false);
      return true;
    } finally {
      setSaving(false);
    }
  }

  async function syncModels() {
    setSyncingModels(true);
    try {
      await runBackendAction(() => refreshModels(site.id), {
        key: `sync-models-${site.id}`,
        presentation: 'message',
        successTitle: '模型列表已同步',
        successDescription: `已从“${site.name}”获取最新模型列表。`,
        errorTitle: '模型列表同步失败',
        errorDescription: '仍显示上一次模型列表，请稍后重试。',
      });
    } finally {
      setSyncingModels(false);
    }
  }

  async function changeModelEnabled(modelId: string, enabled: boolean) {
    const modelName = site.models.find((model) => model.id === modelId)?.name ?? '当前模型';
    setLoadingModelId(modelId);
    try {
      await runBackendAction(() => setModelEnabled(site.id, modelId, enabled), {
        key: `toggle-model-${modelId}`,
        presentation: 'message',
        successTitle: enabled ? `已启用 ${modelName}` : `已停用 ${modelName}`,
        successDescription: enabled ? '模型将参与后续探测。' : '模型将不再参与探测。',
        errorTitle: enabled ? `${modelName} 启用失败` : `${modelName} 停用失败`,
        errorDescription: '模型状态没有更改，请重试。',
      });
    } finally {
      setLoadingModelId(null);
    }
  }

  useImperativeHandle(ref, () => ({ discard, save }));

  return (
    <section
      className="relative min-w-0 flex-1 overflow-hidden bg-rp-surface"
      data-testid="site-detail"
    >
      <Form
        form={form}
        layout="vertical"
        initialValues={initialValues}
        requiredMark={false}
        className="flex h-full flex-col"
        onValuesChange={() => {
          updateDirty(hasFormChanges(form.getFieldsValue(true), baseline));
        }}
      >
        <div
          className={`min-h-0 flex-1 overflow-y-auto px-6 pt-4 pb-20 max-[1180px]:px-4.5 ${scrollbarNoneClass}`}
        >
          <div className="mb-4 flex min-h-8 items-center justify-between">
            <div className="flex min-w-0 items-center gap-2 [&_h2]:m-0 [&_h2]:truncate [&_h2]:text-lg [&_h2]:leading-6.5">
              <StatusDot status={site.status} />
              <h2>{site.name}</h2>
              <Tag color="blue">优先级 {site.priority}</Tag>
              {site.enabled ? <Tag color="green">运行中</Tag> : <Tag>已停用</Tag>}
            </div>
            <Button type="link" danger onClick={() => navigateToView('delete-confirm')}>
              删除站点
            </Button>
          </div>

          {advanced ? (
            <Alert
              className="mb-4 py-1.75!"
              type="warning"
              showIcon
              title="高级设置仅在代理协议或网关字段不兼容时修改；常规 OpenAI 兼容站点保持默认即可。"
            />
          ) : null}

          <section className={detailSectionClass}>
            <div className={sectionHeaderClass}>
              <div>
                <h3>基本信息</h3>
                <p>站点名称、协议和访问凭据</p>
              </div>
              <Button onClick={() => navigateToView(advanced ? 'sites' : 'site-advanced')}>
                {advanced ? '收起高级设置' : '高级设置'}
              </Button>
            </div>
            <BasicFields />
          </section>

          {advanced ? (
            <AdvancedFields
              key={`${site.id}-${advancedDraftVersion}`}
              initialHeaders={baseline.headers}
              onHeadersChange={(headers) => {
                const values = { ...form.getFieldsValue(true), headers };
                updateDirty(hasFormChanges(values, baseline));
              }}
            />
          ) : (
            <>
              <section className={detailSectionClass}>
                <div className={sectionHeaderClass}>
                  <div>
                    <h3>默认探针策略</h3>
                    <p>新启用模型默认继承；单个模型可覆盖</p>
                  </div>
                  <Button
                    onClick={() => {
                      const url = new URL(window.location.href);
                      url.searchParams.set('scope', 'site-default');
                      url.searchParams.delete('model');
                      window.history.replaceState({}, '', url);
                      navigateToView('model-settings');
                    }}
                  >
                    编辑默认值
                  </Button>
                </div>
                <div className="flex min-h-16.5 items-center gap-3 rounded-md border border-rp-line bg-rp-subtle px-3.5 py-3">
                  <span className="grid size-8.5 flex-none place-items-center rounded-md bg-rp-active text-rp-primary">
                    <ApiOutlined />
                  </span>
                  <div className="flex min-w-0 flex-1 flex-col gap-0.5 [&>span]:truncate [&>span]:text-xs [&>span]:text-rp-tertiary">
                    <strong>{defaultStrategyIsEconomical ? '极省模式' : '自定义策略'}</strong>
                    <span>{defaultStrategySummary}</span>
                  </div>
                  {defaultStrategyIsEconomical ? <Tag color="green">推荐</Tag> : null}
                </div>
              </section>

              <section className={detailSectionClass}>
                <div className={sectionHeaderClass}>
                  <div>
                    <h3>模型探针</h3>
                    <p>
                      {site.models.filter((model) => model.enabled).length} / {site.models.length}{' '}
                      个模型已启用
                    </p>
                  </div>
                  <div className="flex gap-2">
                    <Button
                      icon={<SyncOutlined />}
                      loading={syncingModels}
                      onClick={() => void syncModels()}
                    >
                      同步模型
                    </Button>
                    <Button
                      type="primary"
                      icon={<PlusOutlined />}
                      onClick={() => setAddModelOpen(true)}
                    >
                      添加模型
                    </Button>
                  </div>
                </div>
                <ModelTable
                  site={site}
                  loadingModelId={loadingModelId}
                  onEnabledChange={(modelId, enabled) => void changeModelEnabled(modelId, enabled)}
                />
              </section>
            </>
          )}
        </div>
        {dirty ? (
          <footer className="absolute inset-x-0 bottom-0 z-3 flex min-h-14.5 flex-none items-center justify-end gap-2 border-t border-rp-line bg-rp-surface px-6 py-3 shadow-[0_-4px_12px_rgb(0_0_0/6%)]">
            <span
              className="mr-auto inline-flex items-center gap-2 text-rp-secondary"
              role="status"
            >
              <span className="size-1.75 flex-none rounded-full bg-rp-warning" aria-hidden="true" />
              有未保存的更改
            </span>
            <Button onClick={discard}>取消</Button>
            <Button type="primary" loading={saving} onClick={() => void save()}>
              保存更改
            </Button>
          </footer>
        ) : null}
      </Form>
      <AddModelModal open={addModelOpen} site={site} onClose={() => setAddModelOpen(false)} />
    </section>
  );
});

export function SitesPage({ view }: { view: ViewName }) {
  const { snapshot, selectSite, reorderSites, setSiteEnabled } = useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
  const detailRef = useRef<SiteDetailHandle>(null);
  const [siteDirty, setSiteDirty] = useState(false);
  const [pendingSiteId, setPendingSiteId] = useState<string | null>(null);
  const [switchSaving, setSwitchSaving] = useState(false);
  const [loadingSiteId, setLoadingSiteId] = useState<string | null>(null);
  if (!snapshot) return null;
  const currentSnapshot = snapshot;
  const selectedSite =
    currentSnapshot.sites.find((site) => site.id === currentSnapshot.selectedSiteId) ??
    currentSnapshot.sites[0];

  function requestSiteSelection(id: string) {
    if (id === selectedSite?.id) return;
    if (siteDirty) {
      setPendingSiteId(id);
      return;
    }
    selectSite(id);
  }

  function finishSiteSelection(id: string) {
    setPendingSiteId(null);
    setSiteDirty(false);
    selectSite(id);
  }

  function discardAndSwitch() {
    if (!pendingSiteId) return;
    detailRef.current?.discard();
    finishSiteSelection(pendingSiteId);
  }

  async function saveAndSwitch() {
    if (!pendingSiteId || !detailRef.current) return;
    setSwitchSaving(true);
    try {
      if (await detailRef.current.save()) finishSiteSelection(pendingSiteId);
    } finally {
      setSwitchSaving(false);
    }
  }

  async function changeSiteEnabled(id: string, enabled: boolean) {
    const siteName = currentSnapshot.sites.find((site) => site.id === id)?.name ?? '当前站点';
    setLoadingSiteId(id);
    try {
      await runBackendAction(() => setSiteEnabled(id, enabled), {
        key: `toggle-site-${id}`,
        presentation: 'message',
        successTitle: enabled ? `已启用 ${siteName}` : `已停用 ${siteName}`,
        successDescription: enabled ? '站点将在存在启用模型时参与探测。' : '站点将不再参与探测。',
        errorTitle: enabled ? `${siteName} 启用失败` : `${siteName} 停用失败`,
        errorDescription: '站点状态没有更改，请重试。',
      });
    } finally {
      setLoadingSiteId(null);
    }
  }

  async function saveSiteOrder(ids: string[]) {
    await runBackendAction(() => reorderSites(ids), {
      key: 'reorder-sites',
      presentation: 'message',
      successTitle: '站点优先级已更新',
      successDescription: '新的探测优先级顺序已保存。',
      errorTitle: '站点排序保存失败',
      errorDescription: '优先级顺序没有更改，请重试。',
    });
  }

  return (
    <>
      <PageHeader />
      <div className="flex min-h-0 flex-1">
        <SiteList
          sites={currentSnapshot.sites}
          selectedSiteId={selectedSite?.id ?? null}
          loadingSiteId={loadingSiteId}
          onSelect={requestSiteSelection}
          onEnabledChange={(id, enabled) => void changeSiteEnabled(id, enabled)}
          onReorder={(ids) => void saveSiteOrder(ids)}
        />
        {selectedSite ? (
          <SiteDetail
            key={selectedSite.id}
            ref={detailRef}
            site={selectedSite}
            advanced={view === 'site-advanced'}
            onDirtyChange={setSiteDirty}
          />
        ) : null}
      </div>
      {pendingSiteId !== null ? (
        <Modal
          open
          title="保存当前更改？"
          width={440}
          centered
          getContainer={overlayContainer}
          rootClassName={scopedOverlayClass}
          rootStyle={{ position: 'absolute' }}
          mask={{ closable: false }}
          closable={false}
          footer={[
            <Button key="continue" onClick={() => setPendingSiteId(null)}>
              继续编辑
            </Button>,
            <Button key="discard" danger onClick={discardAndSwitch}>
              放弃并切换
            </Button>,
            <Button
              key="save"
              type="primary"
              loading={switchSaving}
              onClick={() => void saveAndSwitch()}
            >
              保存并切换
            </Button>,
          ]}
        >
          <p className="m-0 leading-5.5 text-rp-secondary">
            “{selectedSite?.name}”有未保存的更改。切换站点前，请选择保存或放弃这些更改。
          </p>
        </Modal>
      ) : null}
    </>
  );
}
