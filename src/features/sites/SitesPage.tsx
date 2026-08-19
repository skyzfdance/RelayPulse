import {
  ApiOutlined,
  DeleteOutlined,
  ExperimentOutlined,
  PlusOutlined,
  SyncOutlined,
  UndoOutlined,
} from '@ant-design/icons';
import {
  Alert,
  App as AntdApp,
  Button,
  Drawer,
  Form,
  Input,
  InputNumber,
  Modal,
  Select,
  Tag,
  Tooltip,
} from 'antd';
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import {
  backendActionErrorDescription,
  useBackendActionFeedback,
} from '../../app/use-backend-action-feedback';
import { navigateToView } from '../../app/view';
import { recordFrontendAction } from '../../app/frontend-diagnostics';
import { StatusDot } from '../../components/StatusDot';
import type {
  RequestHeader,
  Site,
  SiteAdvancedInput,
  ModelSyncRemoval,
  UpdateSiteInput,
  ViewName,
} from '../../domain/types';
import { AddModelModal } from '../models/AddModelModal';
import { ModelTable } from '../models/ModelTable';
import { SiteList } from './SiteList';
import { siteProtocolOptions } from './protocol-options';
import { monoInputClass, pageHeaderClass } from '../../styles/tailwind';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;
const formGridClass = 'grid grid-cols-2 gap-x-5';
const detailSectionClass =
  'border-b border-rp-line pb-3 [&+section]:pt-3 [&_h3]:m-0 [&_h3]:text-sm [&_h3]:leading-5.5 [&_h3]:font-semibold';
const sectionHeaderClass =
  'mb-2.5 flex items-center justify-between gap-4 [&_p]:m-0 [&_p]:text-xs [&_p]:leading-5 [&_p]:text-rp-tertiary';

type SiteFormValues = Omit<UpdateSiteInput, 'credential'> &
  SiteAdvancedInput & {
    credentialAction: UpdateSiteInput['credential']['action'];
    credentialSecret: string;
  };

interface SiteDetailHandle {
  discard(): void;
  save(): Promise<boolean>;
}

function siteFormValues(site: Site): SiteFormValues {
  return {
    name: site.name,
    protocol: site.protocol,
    baseUrl: site.baseUrl,
    credentialAction: 'keep',
    credentialSecret: '',
    modelsEndpoint: site.modelsEndpoint,
    probeEndpoint: site.probeEndpoint,
    probeIntervalMinutes: site.probeIntervalMinutes,
    slowFirstTokenSeconds: site.slowFirstTokenSeconds,
    slowTotalSeconds: site.slowTotalSeconds,
    failureThreshold: site.failureThreshold,
    recoveryThreshold: site.recoveryThreshold,
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
    valueOrBaseline(values.credentialAction, baseline.credentialAction) !==
      baseline.credentialAction ||
    valueOrBaseline(values.credentialSecret, baseline.credentialSecret) !==
      baseline.credentialSecret ||
    valueOrBaseline(values.modelsEndpoint, baseline.modelsEndpoint) !== baseline.modelsEndpoint ||
    valueOrBaseline(values.probeEndpoint, baseline.probeEndpoint) !== baseline.probeEndpoint ||
    valueOrBaseline(values.probeIntervalMinutes, baseline.probeIntervalMinutes) !==
      baseline.probeIntervalMinutes ||
    valueOrBaseline(values.slowFirstTokenSeconds, baseline.slowFirstTokenSeconds) !==
      baseline.slowFirstTokenSeconds ||
    valueOrBaseline(values.slowTotalSeconds, baseline.slowTotalSeconds) !==
      baseline.slowTotalSeconds ||
    valueOrBaseline(values.failureThreshold, baseline.failureThreshold) !==
      baseline.failureThreshold ||
    valueOrBaseline(values.recoveryThreshold, baseline.recoveryThreshold) !==
      baseline.recoveryThreshold ||
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

function BasicFields({
  credential,
  onCredentialIntentChange,
}: {
  credential: Site['credential'];
  onCredentialIntentChange(): void;
}) {
  const form = Form.useFormInstance<SiteFormValues>();
  const credentialAction = Form.useWatch('credentialAction', form) ?? 'keep';

  function setCredentialIntent(action: SiteFormValues['credentialAction']) {
    form.setFieldValue('credentialAction', action);
    onCredentialIntentChange();
  }

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
        <Select options={siteProtocolOptions} />
      </Form.Item>
      <Form.Item
        label="Base URL"
        name="baseUrl"
        rules={[{ required: true, type: 'url', message: '请输入有效的 Base URL' }]}
      >
        <Input className={monoInputClass} />
      </Form.Item>
      <Form.Item
        label="访问凭据"
        extra={
          credentialAction === 'clear'
            ? '保存后将清除当前凭据'
            : credential.configured
              ? '留空则保留当前凭据'
              : '尚未配置凭据'
        }
      >
        <div className="flex gap-2">
          <Form.Item
            noStyle
            name="credentialSecret"
            rules={[
              {
                validator: async (_, value: string) => {
                  if (credentialAction === 'replace' && !value.trim()) {
                    throw new Error('请输入新的访问凭据');
                  }
                },
              },
            ]}
          >
            <Input.Password
              className={monoInputClass}
              autoComplete="new-password"
              disabled={credentialAction === 'clear'}
              placeholder={
                credentialAction === 'clear' ? '保存后清除' : credential.masked || '输入访问凭据'
              }
              onChange={(event) => {
                setCredentialIntent(event.target.value ? 'replace' : 'keep');
              }}
            />
          </Form.Item>
          {credential.configured ? (
            <Tooltip title={credentialAction === 'clear' ? '撤销清除凭据' : '清除凭据'}>
              <Button
                icon={credentialAction === 'clear' ? <UndoOutlined /> : <DeleteOutlined />}
                aria-label={credentialAction === 'clear' ? '撤销清除凭据' : '清除凭据'}
                danger={credentialAction !== 'clear'}
                onClick={() => {
                  form.setFieldValue('credentialSecret', '');
                  setCredentialIntent(credentialAction === 'clear' ? 'keep' : 'clear');
                }}
              />
            </Tooltip>
          ) : null}
        </div>
      </Form.Item>
      <Form.Item name="credentialAction" hidden>
        <Input />
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
  const [headers, setHeaders] = useState<RequestHeader[]>(initialHeaders ?? []);

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
            <p>端点、探测频率和自定义请求头</p>
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
        <div className={formGridClass}>
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
        </div>
        <Form.Item name="headers" hidden>
          <Input />
        </Form.Item>
        <div className="overflow-hidden rounded-md border border-rp-line">
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
            className="h-10!"
            onClick={() => updateHeaders([...headers, { key: '', value: '' }])}
          >
            添加请求头
          </Button>
        </div>
      </section>
      <section className={detailSectionClass}>
        <div className={sectionHeaderClass}>
          <div>
            <h3>状态判定</h3>
            <p>按站点设置黄灯、红灯及恢复条件</p>
          </div>
        </div>
        <div className={formGridClass}>
          <Form.Item
            label="首 Token 慢阈值"
            name="slowFirstTokenSeconds"
            tooltip="仅在流式响应提供首 Token 数据时参与黄灯判定；达到阈值不会中断请求或转为红灯"
            rules={[
              { required: true, message: '请输入首 Token 慢阈值' },
              { type: 'number', min: 1, max: 86400, message: '请输入 1 至 86400 秒' },
            ]}
          >
            <InputNumber min={1} max={86400} precision={0} suffix="秒" className="w-full" />
          </Form.Item>
          <Form.Item
            label="总耗时慢阈值"
            name="slowTotalSeconds"
            tooltip="请求成功但总耗时达到该值时显示黄灯；不会中断请求或转为红灯"
            rules={[
              { required: true, message: '请输入总耗时慢阈值' },
              { type: 'number', min: 1, max: 86400, message: '请输入 1 至 86400 秒' },
            ]}
          >
            <InputNumber min={1} max={86400} precision={0} suffix="秒" className="w-full" />
          </Form.Item>
          <Form.Item
            label="连续失败转红"
            name="failureThreshold"
            tooltip="连续失败达到该次数后显示红灯；此前保持上一次稳定状态，没有历史时保持灰色"
            rules={[
              { required: true, message: '请输入连续失败次数' },
              { type: 'number', min: 1, max: 10, message: '请输入 1 至 10 次' },
            ]}
          >
            <InputNumber min={1} max={10} precision={0} suffix="次" className="w-full" />
          </Form.Item>
          <Form.Item
            label="连续成功恢复"
            name="recoveryThreshold"
            tooltip="红灯后连续成功达到该次数才恢复；黄灯结果也计为成功"
            rules={[
              { required: true, message: '请输入连续成功次数' },
              { type: 'number', min: 1, max: 10, message: '请输入 1 至 10 次' },
            ]}
          >
            <InputNumber min={1} max={10} precision={0} suffix="次" className="w-full" />
          </Form.Item>
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

function SiteAdvancedDrawer({ site }: { site: Site }) {
  const { updateSiteAdvanced } = useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
  const [form] = Form.useForm<SiteAdvancedInput>();
  const [open, setOpen] = useState(true);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const initialValues = useMemo<SiteAdvancedInput>(
    () => ({
      modelsEndpoint: site.modelsEndpoint,
      probeEndpoint: site.probeEndpoint,
      probeIntervalMinutes: site.probeIntervalMinutes,
      slowFirstTokenSeconds: site.slowFirstTokenSeconds,
      slowTotalSeconds: site.slowTotalSeconds,
      failureThreshold: site.failureThreshold,
      recoveryThreshold: site.recoveryThreshold,
      headers: site.headers,
    }),
    [site],
  );

  async function saveAdvancedSettings() {
    let values: SiteAdvancedInput;
    try {
      values = await form.validateFields();
    } catch (error) {
      if (typeof error === 'object' && error !== null && 'errorFields' in error) return;
      throw error;
    }

    setSaving(true);
    try {
      const saved = await runBackendAction(() => updateSiteAdvanced(site.id, values), {
        key: `save-site-advanced-${site.id}`,
        successTitle: '站点高级设置已保存',
        successDescription: `“${site.name}”的端点、探测频率、状态判定和请求头已更新。`,
        errorTitle: '站点高级设置保存失败',
        errorDescription: '更改没有保存，当前编辑内容已保留，请检查后重试。',
      });
      if (saved) setOpen(false);
    } finally {
      setSaving(false);
    }
  }

  return (
    <Drawer
      open={open}
      size={620}
      closable={{ placement: 'end', disabled: saving }}
      title={
        <div>
          <strong>站点高级设置</strong>
          <div className="mt-0.5 text-xs font-normal text-rp-tertiary">{site.name}</div>
        </div>
      }
      getContainer={overlayContainer}
      rootStyle={{ position: 'absolute' }}
      mask={{ closable: false }}
      onClose={() => setOpen(false)}
      afterOpenChange={(nextOpen) => {
        if (!nextOpen) navigateToView('sites');
      }}
      footer={
        <div className="flex justify-end gap-2">
          <Button disabled={saving} onClick={() => setOpen(false)}>
            取消
          </Button>
          <Button
            type="primary"
            disabled={!dirty}
            loading={saving}
            onClick={() => void saveAdvancedSettings()}
          >
            保存更改
          </Button>
        </div>
      }
    >
      <Alert
        className="mb-4 py-1.75!"
        type="warning"
        showIcon
        title="高级设置仅在代理协议或网关字段不兼容时修改；常规 OpenAI 兼容站点保持默认即可。"
      />
      <Form
        form={form}
        layout="vertical"
        initialValues={initialValues}
        requiredMark={false}
        onValuesChange={() => setDirty(true)}
      >
        <AdvancedFields
          initialHeaders={initialValues.headers}
          onHeadersChange={() => setDirty(true)}
        />
      </Form>
    </Drawer>
  );
}

const SiteDetail = forwardRef<
  SiteDetailHandle,
  { site: Site; onDirtyChange(dirty: boolean): void }
>(function SiteDetail({ site, onDirtyChange }, ref) {
  const { updateSite, setModelEnabled, deleteModel, refreshModels, runSiteProbe, runModelProbe } =
    useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
  const { message, notification } = AntdApp.useApp();
  const [form] = Form.useForm<SiteFormValues>();
  const [addModelOpen, setAddModelOpen] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [syncingModels, setSyncingModels] = useState(false);
  const [pendingModelRemovals, setPendingModelRemovals] = useState<ModelSyncRemoval[] | null>(null);
  const [loadingModelId, setLoadingModelId] = useState<string | null>(null);
  const [deletingModelId, setDeletingModelId] = useState<string | null>(null);
  const [probingModelId, setProbingModelId] = useState<string | null>(null);
  const [siteProbeRunning, setSiteProbeRunning] = useState(false);

  const initialValues = useMemo(() => siteFormValues(site), [site]);
  const [baseline, setBaseline] = useState(initialValues);
  const defaultStrategyIsEconomical =
    site.defaultProbeSettings.maxOutputTokens === 8 &&
    site.defaultProbeSettings.prompt === '只回复 OK' &&
    site.defaultProbeSettings.parameters.length === 0;
  const defaultStrategySummary = `默认 low · 最多输出 ${site.defaultProbeSettings.maxOutputTokens} tokens · ${site.defaultProbeSettings.prompt}${
    site.defaultProbeSettings.parameters.length > 0
      ? ` · ${site.defaultProbeSettings.parameters.length} 项透传覆盖`
      : ''
  }`;

  function updateDirty(next: boolean) {
    if (dirty !== next) {
      recordFrontendAction(next ? 'form-became-dirty' : 'form-reset', {
        targetType: 'form',
        targetId: 'site-basic',
      });
    }
    setDirty(next);
    onDirtyChange(next);
  }

  function discard() {
    form.setFieldsValue(baseline);
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
          const credential: UpdateSiteInput['credential'] =
            values.credentialAction === 'replace'
              ? { action: 'replace', secret: values.credentialSecret }
              : { action: values.credentialAction };
          await updateSite(site.id, {
            name: values.name,
            protocol: values.protocol,
            baseUrl: values.baseUrl,
            credential,
          });
        },
        {
          key: `save-site-${site.id}`,
          successTitle: '站点配置已保存',
          successDescription: `“${values.name}”的基本信息已更新。`,
          errorTitle: '站点配置保存失败',
          errorDescription: '更改没有保存，当前编辑内容已保留，请检查后重试。',
        },
      );
      if (!saved) return false;
      const nextBaseline = {
        ...baseline,
        ...values,
        credentialAction: 'keep' as const,
        credentialSecret: '',
      };
      form.setFieldsValue(nextBaseline);
      setBaseline(nextBaseline);
      updateDirty(false);
      return true;
    } finally {
      setSaving(false);
    }
  }

  async function syncModels(confirmedRemovedModelIds: string[] = []) {
    setSyncingModels(true);
    try {
      const result = await refreshModels(site.id, confirmedRemovedModelIds);
      if (result.status === 'confirmationRequired') {
        setPendingModelRemovals(result.removedModels);
        if (confirmedRemovedModelIds.length > 0) {
          message.warning({
            key: `sync-models-changed-${site.id}`,
            content: '远端模型列表已变化，请重新确认待删除模型。',
            duration: 4,
          });
        }
        return;
      }
      setPendingModelRemovals(null);
      message.success({
        key: `sync-models-${site.id}`,
        content: '模型列表已同步',
        duration: 3,
      });
    } catch (error) {
      notification.error({
        key: `sync-models-${site.id}`,
        title: '模型列表同步失败',
        description: backendActionErrorDescription(error, '仍显示上一次模型列表，请稍后重试。'),
        placement: 'topRight',
        duration: 6,
        pauseOnHover: true,
        showProgress: true,
        role: 'alert',
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

  async function removeModel(modelId: string) {
    const modelName = site.models.find((model) => model.id === modelId)?.name ?? '当前模型';
    setDeletingModelId(modelId);
    try {
      await runBackendAction(() => deleteModel(site.id, modelId), {
        key: `delete-model-${modelId}`,
        presentation: 'message',
        successTitle: `已删除 ${modelName}`,
        successDescription: '模型及其探测记录已删除。',
        errorTitle: `${modelName} 删除失败`,
        errorDescription: '模型仍然保留，请重试。',
      });
    } finally {
      setDeletingModelId(null);
    }
  }

  async function probeEnabledModels() {
    const enabledCount = site.models.filter((model) => model.enabled).length;
    if (enabledCount === 0) return;
    setSiteProbeRunning(true);
    try {
      await runBackendAction(() => runSiteProbe(site.id), {
        key: `probe-site-${site.id}`,
        presentation: 'notification',
        successTitle: '站点探针已完成',
        successDescription: `已探测 ${enabledCount} 个启用模型，结果、耗时和 Token 用量已写入探测记录。`,
        errorTitle: '站点探针执行失败',
        errorDescription: '未能完成本次批量探测，请检查站点高级设置和本地凭据。',
      });
    } finally {
      setSiteProbeRunning(false);
    }
  }

  async function probeModel(modelId: string) {
    const modelName = site.models.find((model) => model.id === modelId)?.name ?? '当前模型';
    setProbingModelId(modelId);
    try {
      await runBackendAction(() => runModelProbe(site.id, modelId), {
        key: `probe-model-${modelId}`,
        presentation: 'message',
        successTitle: `${modelName} 探针已完成`,
        successDescription: '本次结果、耗时和 Token 用量已写入探测记录。',
        errorTitle: `${modelName} 探针执行失败`,
        errorDescription: '未能执行本次探测，请检查模型探针设置和本地凭据。',
      });
    } finally {
      setProbingModelId(null);
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
          updateDirty(hasFormChanges(form.getFieldsValue(), baseline));
        }}
      >
        <div className="min-h-0 flex-1 overflow-y-auto px-6 pt-4 pb-20 max-[1180px]:px-4.5">
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

          <section className={detailSectionClass}>
            <div className={sectionHeaderClass}>
              <div>
                <h3>基本信息</h3>
                <p>站点名称、协议和访问凭据</p>
              </div>
              <Button onClick={() => navigateToView('site-advanced')}>高级设置</Button>
            </div>
            <BasicFields
              credential={site.credential}
              onCredentialIntentChange={() => {
                updateDirty(hasFormChanges(form.getFieldsValue(), baseline));
              }}
            />
          </section>

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
                  icon={<ExperimentOutlined />}
                  loading={siteProbeRunning}
                  disabled={
                    site.models.every((model) => !model.enabled) ||
                    syncingModels ||
                    Boolean(probingModelId)
                  }
                  onClick={() => void probeEnabledModels()}
                >
                  探测已启用模型
                </Button>
                <Button
                  icon={<SyncOutlined />}
                  loading={syncingModels}
                  disabled={siteProbeRunning || Boolean(probingModelId)}
                  onClick={() => void syncModels()}
                >
                  同步模型
                </Button>
                <Button
                  type="primary"
                  icon={<PlusOutlined />}
                  disabled={siteProbeRunning || syncingModels || Boolean(probingModelId)}
                  onClick={() => {
                    recordFrontendAction('open-add-model', {
                      targetType: 'site',
                      targetId: site.id,
                    });
                    setAddModelOpen(true);
                  }}
                >
                  添加模型
                </Button>
              </div>
            </div>
            <ModelTable
              site={site}
              loadingModelId={loadingModelId}
              deletingModelId={deletingModelId}
              probingModelId={probingModelId}
              siteProbeRunning={siteProbeRunning}
              onEnabledChange={(modelId, enabled) => void changeModelEnabled(modelId, enabled)}
              onProbe={probeModel}
              onDelete={removeModel}
            />
          </section>
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
      <AddModelModal
        open={addModelOpen}
        site={site}
        onClose={() => {
          recordFrontendAction('close-add-model', {
            targetType: 'site',
            targetId: site.id,
          });
          setAddModelOpen(false);
        }}
      />
      {pendingModelRemovals ? (
        <Modal
          open
          title="确认删除已下架模型？"
          width={480}
          centered
          getContainer={overlayContainer}
          rootStyle={{ position: 'absolute' }}
          mask={{ closable: false }}
          closable={!syncingModels}
          okText="删除并同步"
          okType="danger"
          cancelText="取消同步"
          confirmLoading={syncingModels}
          cancelButtonProps={{ disabled: syncingModels }}
          onCancel={() => {
            recordFrontendAction('cancel-model-sync-confirmation', {
              targetType: 'site',
              targetId: site.id,
            });
            setPendingModelRemovals(null);
          }}
          onOk={() => void syncModels(pendingModelRemovals.map((model) => model.id))}
        >
          <Alert
            type="warning"
            showIcon
            title={`远端目录中已不存在以下 ${pendingModelRemovals.length} 个模型`}
            description="继续后将删除这些模型及其单独配置和探测记录；取消则本次不同步。"
          />
          <ul className="mt-3 mb-0 max-h-52 overflow-y-auto pl-5 font-mono text-[13px] leading-6">
            {pendingModelRemovals.map((model) => (
              <li key={model.id} className="wrap-anywhere">
                {model.name}
              </li>
            ))}
          </ul>
        </Modal>
      ) : null}
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
            onDirtyChange={setSiteDirty}
          />
        ) : null}
      </div>
      {selectedSite && view === 'site-advanced' ? (
        <SiteAdvancedDrawer key={selectedSite.id} site={selectedSite} />
      ) : null}
      {pendingSiteId !== null ? (
        <Modal
          open
          title="保存当前更改？"
          width={440}
          centered
          getContainer={overlayContainer}
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
