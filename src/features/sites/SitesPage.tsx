import { ApiOutlined, DeleteOutlined, PlusOutlined, SyncOutlined } from '@ant-design/icons';
import { Alert, Button, Form, Input, InputNumber, Modal, Select, Tag } from 'antd';
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
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
import styles from '../../styles/App.module.css';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;

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
    <header className={styles.pageHeader}>
      <div>
        <h1>站点管理</h1>
        <p>配置多个模型站点，并通过拖动顺序决定故障时的优先级</p>
      </div>
    </header>
  );
}

function BasicFields() {
  return (
    <div className={styles.formGrid}>
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
        <Input className={styles.mono} />
      </Form.Item>
      <Form.Item label="API Key" name="apiKeyMasked" rules={[{ required: true }]}>
        <Input.Password className={styles.mono} visibilityToggle />
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
      <section className={styles.detailSection}>
        <div className={styles.sectionHeader}>
          <div>
            <h3>高级连接</h3>
            <p>端点、超时和自定义请求头</p>
          </div>
        </div>
        <div className={styles.formGrid}>
          <Form.Item label="模型列表端点" name="modelsEndpoint" rules={[{ required: true }]}>
            <Input className={styles.mono} />
          </Form.Item>
          <Form.Item label="探测端点" name="probeEndpoint" rules={[{ required: true }]}>
            <Input className={styles.mono} />
          </Form.Item>
          <Form.Item label="首 Token 超时" name="firstTokenTimeoutSeconds">
            <InputNumber min={1} max={120} suffix="秒" className={styles.fullWidth} />
          </Form.Item>
          <Form.Item label="请求总超时" name="totalTimeoutSeconds">
            <InputNumber min={1} max={300} suffix="秒" className={styles.fullWidth} />
          </Form.Item>
        </div>
        <Form.Item name="headers" hidden>
          <Input />
        </Form.Item>
        <div className={styles.keyValueTable}>
          <div className={styles.keyValueHeader}>
            <span>请求头</span>
            <span>值</span>
            <span />
          </div>
          {headers.map((header, index) => (
            <div className={styles.keyValueRow} key={`${header.key}-${index}`}>
              <Input
                variant="borderless"
                className={styles.mono}
                value={header.key}
                onChange={(event) => {
                  const next = [...headers];
                  next[index] = { ...header, key: event.target.value };
                  updateHeaders(next);
                }}
              />
              <Input
                variant="borderless"
                className={styles.mono}
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
  const [form] = Form.useForm<SiteFormValues>();
  const [advancedDraftVersion, setAdvancedDraftVersion] = useState(0);
  const [addModelOpen, setAddModelOpen] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);

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
      await updateSite(site.id, values);
      if (advanced) {
        await updateSiteAdvanced(site.id, {
          modelsEndpoint: values.modelsEndpoint,
          probeEndpoint: values.probeEndpoint,
          firstTokenTimeoutSeconds: values.firstTokenTimeoutSeconds,
          totalTimeoutSeconds: values.totalTimeoutSeconds,
          headers: values.headers,
        });
      }
      setBaseline(values);
      updateDirty(false);
      return true;
    } finally {
      setSaving(false);
    }
  }

  useImperativeHandle(ref, () => ({ discard, save }));

  return (
    <section className={styles.siteDetail}>
      <Form
        form={form}
        layout="vertical"
        initialValues={initialValues}
        requiredMark={false}
        className={styles.detailForm}
        onValuesChange={() => {
          updateDirty(hasFormChanges(form.getFieldsValue(true), baseline));
        }}
      >
        <div className={styles.detailScroll}>
          <div className={styles.detailTitle}>
            <div className={styles.detailTitleGroup}>
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
              className={styles.advancedAlert}
              type="warning"
              showIcon
              title="高级设置仅在代理协议或网关字段不兼容时修改；常规 OpenAI 兼容站点保持默认即可。"
            />
          ) : null}

          <section className={styles.detailSection}>
            <div className={styles.sectionHeader}>
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
              <section className={styles.detailSection}>
                <div className={styles.sectionHeader}>
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
                <div className={styles.strategyCard}>
                  <span className={styles.strategyIcon}>
                    <ApiOutlined />
                  </span>
                  <div className={styles.strategyCopy}>
                    <strong>{defaultStrategyIsEconomical ? '极省模式' : '自定义策略'}</strong>
                    <span>{defaultStrategySummary}</span>
                  </div>
                  {defaultStrategyIsEconomical ? <Tag color="green">推荐</Tag> : null}
                </div>
              </section>

              <section className={styles.detailSection}>
                <div className={styles.sectionHeader}>
                  <div>
                    <h3>模型探针</h3>
                    <p>
                      {site.models.filter((model) => model.enabled).length} / {site.models.length}{' '}
                      个模型已启用
                    </p>
                  </div>
                  <div className={styles.sectionActions}>
                    <Button icon={<SyncOutlined />} onClick={() => void refreshModels(site.id)}>
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
                  onEnabledChange={(modelId, enabled) =>
                    void setModelEnabled(site.id, modelId, enabled)
                  }
                />
              </section>
            </>
          )}
        </div>
        {dirty ? (
          <footer className={`${styles.pageFooter} ${styles.dirtyActionBar}`}>
            <span className={styles.dirtyMessage} role="status">
              <span className={styles.dirtyDot} aria-hidden="true" />
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
  const detailRef = useRef<SiteDetailHandle>(null);
  const [siteDirty, setSiteDirty] = useState(false);
  const [pendingSiteId, setPendingSiteId] = useState<string | null>(null);
  const [switchSaving, setSwitchSaving] = useState(false);
  if (!snapshot) return null;
  const selectedSite =
    snapshot.sites.find((site) => site.id === snapshot.selectedSiteId) ?? snapshot.sites[0];

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

  return (
    <>
      <PageHeader />
      <div className={styles.siteWorkspace}>
        <SiteList
          sites={snapshot.sites}
          selectedSiteId={selectedSite?.id ?? null}
          onSelect={requestSiteSelection}
          onEnabledChange={(id, enabled) => void setSiteEnabled(id, enabled)}
          onReorder={(ids) => void reorderSites(ids)}
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
          rootClassName={styles.scopedOverlay}
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
          <p className={styles.modalLead}>
            “{selectedSite?.name}”有未保存的更改。切换站点前，请选择保存或放弃这些更改。
          </p>
        </Modal>
      ) : null}
    </>
  );
}
