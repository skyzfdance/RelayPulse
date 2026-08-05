import { DeleteOutlined, ExperimentOutlined, PlusOutlined } from '@ant-design/icons';
import { Alert, Button, Drawer, Input, InputNumber, Select, Switch, Tag } from 'antd';
import { useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { useBackendActionFeedback } from '../../app/use-backend-action-feedback';
import { navigateToView } from '../../app/view';
import type {
  CustomProbeSettings,
  ParameterType,
  PassThroughParameter,
  ProbeSettings,
  SiteProtocol,
} from '../../domain/types';
import {
  buildCustomProbeRequest,
  buildProbeRequest,
  customReservedProbeFields,
  isParameterPathValid,
  isReservedParameter,
  isParameterValueValid,
  reservedProbeFields,
  resolveCustomProbeEndpoint,
} from '../../domain/probe-request';
import { monoInputClass } from '../../styles/tailwind';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;
const drawerSectionClass =
  'border-b border-rp-line pb-3.5 [&+section]:pt-3.5 [&_h3]:m-0 [&_h3]:text-sm [&_h3]:leading-5.5 [&_h3]:font-semibold [&>p]:m-0 [&>p]:text-xs [&>p]:leading-5 [&>p]:text-rp-tertiary';
const drawerSectionHeaderClass =
  'mb-2 flex items-center justify-between gap-4 [&_p]:m-0 [&_p]:text-xs [&_p]:leading-5 [&_p]:text-rp-tertiary';
const fieldLabelClass = 'mt-2.25 mb-1.25 block text-xs text-rp-secondary';
const economicalDefaults: ProbeSettings = {
  mode: 'disabled-thinking',
  maxOutputTokens: 8,
  prompt: '只回复 OK',
  parameters: [],
};
const customDefaults: CustomProbeSettings = {
  enabled: false,
  endpoint: '',
  parameters: [],
};

function endpointIsValid(baseUrl: string, endpoint: string) {
  if (!endpoint.trim()) return false;
  try {
    return /^https?:\/\//i.test(resolveCustomProbeEndpoint(baseUrl, endpoint));
  } catch {
    return false;
  }
}

function requestPreview(
  baseUrl: string,
  modelName: string,
  protocol: SiteProtocol,
  settings: ProbeSettings,
  customProbe: CustomProbeSettings,
) {
  if (!customProbe.enabled) {
    return JSON.stringify(buildProbeRequest(modelName, protocol, settings), null, 2);
  }

  try {
    const endpoint = resolveCustomProbeEndpoint(baseUrl, customProbe.endpoint);
    const body = buildCustomProbeRequest(modelName, customProbe);
    return `POST ${endpoint}\n\n${JSON.stringify(body, null, 2)}`;
  } catch {
    return '请先完成自定义端点和参数配置。';
  }
}

export function ModelSettingsDrawer() {
  const {
    snapshot,
    updateModelCustomProbeSettings,
    updateModelProbeSettings,
    updateSiteProbeSettings,
  } = useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
  const site = snapshot?.sites.find((item) => item.id === snapshot.selectedSiteId);
  const search = new URLSearchParams(window.location.search);
  const siteDefault = search.get('scope') === 'site-default';
  const requestedModel = search.get('model');
  const model = site?.models.find((item) => item.id === requestedModel) ?? site?.models[1];
  const [settings, setSettings] = useState<ProbeSettings>(
    (siteDefault ? site?.defaultProbeSettings : model?.probeSettings) ?? economicalDefaults,
  );
  const [customProbe, setCustomProbe] = useState<CustomProbeSettings>(
    model?.customProbe ?? customDefaults,
  );
  const [mockValidated, setMockValidated] = useState(false);
  const [open, setOpen] = useState(true);
  const [saving, setSaving] = useState(false);

  const customActive = !siteDefault && customProbe.enabled;
  const parameters = customActive ? customProbe.parameters : settings.parameters;
  const reservedFields = customActive
    ? customReservedProbeFields
    : reservedProbeFields(site?.protocol ?? 'openai-compatible');
  const invalidEmptyParameter = parameters.find((parameter) => !parameter.key.trim());
  const invalidPathParameter = parameters.find(
    (parameter) => parameter.key.trim() && !isParameterPathValid(parameter.key),
  );
  const invalidReservedParameter = parameters.find((parameter) =>
    isReservedParameter(parameter.key, reservedFields),
  );
  const invalidValueParameter = parameters.find(
    (parameter) => parameter.key.trim() && !isParameterValueValid(parameter),
  );
  const invalidEndpoint =
    customActive &&
    !endpointIsValid(site?.baseUrl ?? 'https://example.invalid', customProbe.endpoint);
  const hasValidationError = Boolean(
    invalidEmptyParameter ||
    invalidPathParameter ||
    invalidReservedParameter ||
    invalidValueParameter ||
    invalidEndpoint,
  );
  const preview = requestPreview(
    site?.baseUrl ?? 'https://example.invalid',
    siteDefault ? '<模型名称>' : (model?.name ?? 'model'),
    site?.protocol ?? 'openai-compatible',
    settings,
    customActive ? customProbe : customDefaults,
  );

  if (!site || (!siteDefault && !model)) return null;
  const currentSite = site;
  const currentModel = model;

  function updateParameter(index: number, patch: Partial<PassThroughParameter>) {
    if (customActive) {
      setCustomProbe((current) => ({
        ...current,
        parameters: current.parameters.map((parameter, currentIndex) =>
          currentIndex === index ? { ...parameter, ...patch } : parameter,
        ),
      }));
    } else {
      setSettings((current) => ({
        ...current,
        parameters: current.parameters.map((parameter, currentIndex) =>
          currentIndex === index ? { ...parameter, ...patch } : parameter,
        ),
      }));
    }
    setMockValidated(false);
  }

  function addParameter() {
    const parameter: PassThroughParameter = { key: '', value: '', type: 'string' };
    if (customActive) {
      setCustomProbe((current) => ({
        ...current,
        parameters: [...current.parameters, parameter],
      }));
    } else {
      setSettings((current) => ({
        ...current,
        parameters: [...current.parameters, parameter],
      }));
    }
    setMockValidated(false);
  }

  function removeParameter(index: number) {
    if (customActive) {
      setCustomProbe((current) => ({
        ...current,
        parameters: current.parameters.filter((_, currentIndex) => currentIndex !== index),
      }));
    } else {
      setSettings((current) => ({
        ...current,
        parameters: current.parameters.filter((_, currentIndex) => currentIndex !== index),
      }));
    }
    setMockValidated(false);
  }

  function restoreDefaults() {
    setSettings(
      siteDefault
        ? economicalDefaults
        : { ...currentSite.defaultProbeSettings, mode: 'disabled-thinking' },
    );
    if (!siteDefault) setCustomProbe(customDefaults);
    setMockValidated(false);
  }

  function closeDrawer() {
    setOpen(false);
  }

  async function saveSettings() {
    setSaving(true);
    try {
      const normalizedSettings: ProbeSettings = { ...settings, mode: 'disabled-thinking' };
      const saved = await runBackendAction(
        async () => {
          if (siteDefault) {
            await updateSiteProbeSettings(currentSite.id, normalizedSettings);
          } else if (currentModel) {
            await updateModelProbeSettings(currentSite.id, currentModel.id, normalizedSettings);
            await updateModelCustomProbeSettings(currentSite.id, currentModel.id, {
              ...customProbe,
              parameters: customProbe.enabled
                ? customProbe.parameters
                : normalizedSettings.parameters,
            });
          }
        },
        {
          key: siteDefault
            ? `save-site-probe-${currentSite.id}`
            : `save-model-probe-${currentModel?.id}`,
          successTitle: siteDefault ? '站点默认策略已保存' : '模型探针设置已保存',
          successDescription: siteDefault
            ? `“${currentSite.name}”的新启用模型将使用当前默认策略。`
            : `“${currentModel?.name}”的探针端点和请求参数已更新。`,
          errorTitle: siteDefault ? '站点默认策略保存失败' : '模型探针设置保存失败',
          errorDescription: '设置没有保存，当前编辑内容已保留，请检查后重试。',
        },
      );
      if (saved) closeDrawer();
    } finally {
      setSaving(false);
    }
  }

  return (
    <Drawer
      open={open}
      size={560}
      closable={{ placement: 'end' }}
      title={
        <div>
          <strong>{siteDefault ? '站点默认探针策略' : '模型探针设置'}</strong>
          <div className="mt-0.5 text-xs font-normal text-rp-tertiary">
            {siteDefault ? `${site.name} · 新启用模型默认继承` : `${model?.name} · ${site.name}`}
          </div>
        </div>
      }
      getContainer={overlayContainer}
      rootStyle={{ position: 'absolute' }}
      mask={{ closable: false }}
      onClose={closeDrawer}
      afterOpenChange={(nextOpen) => {
        if (!nextOpen) navigateToView('sites');
      }}
      footer={
        <div className="grid grid-cols-[auto_1fr_auto_auto] gap-2">
          <Button onClick={restoreDefaults}>{siteDefault ? '恢复默认值' : '恢复站点默认'}</Button>
          <span />
          <Button onClick={closeDrawer}>取消</Button>
          <Button
            type="primary"
            disabled={hasValidationError}
            loading={saving}
            onClick={() => void saveSettings()}
          >
            保存设置
          </Button>
        </div>
      }
    >
      {!siteDefault ? (
        <section className={drawerSectionClass}>
          <div className={drawerSectionHeaderClass}>
            <div>
              <h3>自定义探针请求</h3>
              <p>仅覆盖当前模型的探测端点和请求参数</p>
            </div>
            <Switch
              size="small"
              checked={customProbe.enabled}
              aria-label="使用自定义探针请求"
              onChange={(enabled) => {
                setCustomProbe((current) => ({ ...current, enabled }));
                setMockValidated(false);
              }}
            />
          </div>
          {customActive ? (
            <>
              <label>
                <span className={fieldLabelClass}>自定义探测端点</span>
                <Input
                  className={monoInputClass}
                  status={invalidEndpoint ? 'error' : undefined}
                  aria-label="自定义探测端点"
                  placeholder="例如 /v1/images/generations"
                  value={customProbe.endpoint}
                  onChange={(event) => {
                    setCustomProbe((current) => ({
                      ...current,
                      endpoint: event.target.value,
                    }));
                    setMockValidated(false);
                  }}
                />
              </label>
              <div className="mt-2 flex flex-wrap gap-1">
                <Tag>POST</Tag>
                <Tag color="blue">继承站点凭据和请求头</Tag>
                <Tag>首 Token 显示 —</Tag>
              </div>
              {invalidEndpoint ? (
                <p className="mt-1.75! text-xs! text-rp-error!">端点必须与站点 Base URL 同源。</p>
              ) : null}
            </>
          ) : null}
        </section>
      ) : null}

      {siteDefault || !customActive ? (
        <section className={drawerSectionClass}>
          <h3>{siteDefault ? '站点默认探针请求' : '本模型探针请求'}</h3>
          <p>默认推理强度为 low；透传参数可覆盖默认业务字段</p>
          <div className="mt-2.5 grid grid-cols-2 gap-x-5">
            <label>
              <span className={fieldLabelClass}>最大输出</span>
              <InputNumber
                min={1}
                max={128}
                value={settings.maxOutputTokens}
                suffix="tokens"
                className="w-full"
                onChange={(value) =>
                  setSettings((current) => ({ ...current, maxOutputTokens: value ?? 8 }))
                }
              />
            </label>
            <label>
              <span className={fieldLabelClass}>探针提示词</span>
              <Input
                value={settings.prompt}
                onChange={(event) =>
                  setSettings((current) => ({ ...current, prompt: event.target.value }))
                }
              />
            </label>
          </div>
        </section>
      ) : null}

      <section className={drawerSectionClass}>
        <div className={drawerSectionHeaderClass}>
          <div>
            <h3>{customActive ? '请求参数' : '透传参数'}</h3>
            <p>
              {customActive
                ? 'RelayPulse 自动注入 model，其余字段由使用者配置'
                : '仅添加服务商需要的额外字段'}
            </p>
          </div>
          <Button icon={<PlusOutlined />} onClick={addParameter}>
            添加参数
          </Button>
        </div>
        <div className="overflow-hidden rounded-md border border-rp-line">
          <div className="grid min-h-9.5 grid-cols-[minmax(0,1.25fr)_minmax(0,1.15fr)_92px_36px] items-center gap-2.5 border-b border-rp-line bg-rp-subtle px-2.5 text-xs text-rp-secondary">
            <span>字段</span>
            <span>值</span>
            <span>类型</span>
            <span />
          </div>
          {parameters.map((parameter, index) => (
            <div
              className="grid min-h-11 grid-cols-[minmax(0,1.25fr)_minmax(0,1.15fr)_92px_36px] items-center gap-2.5 border-b border-rp-line px-2 py-1.25 last:border-b-0"
              data-testid="parameter-row"
              key={index}
            >
              <Input
                className={monoInputClass}
                status={
                  !parameter.key.trim() ||
                  !isParameterPathValid(parameter.key) ||
                  isReservedParameter(parameter.key, reservedFields)
                    ? 'error'
                    : undefined
                }
                value={parameter.key}
                onChange={(event) => updateParameter(index, { key: event.target.value })}
              />
              <Input
                className={monoInputClass}
                status={isParameterValueValid(parameter) ? undefined : 'error'}
                value={parameter.value}
                onChange={(event) => updateParameter(index, { value: event.target.value })}
              />
              <Select<ParameterType>
                value={parameter.type}
                options={[
                  { value: 'string', label: 'string' },
                  { value: 'boolean', label: 'boolean' },
                  { value: 'number', label: 'number' },
                  { value: 'json', label: 'JSON' },
                ]}
                onChange={(type) => updateParameter(index, { type })}
              />
              <Button
                type="text"
                icon={<DeleteOutlined />}
                aria-label={`删除参数 ${parameter.key || index + 1}`}
                onClick={() => removeParameter(index)}
              />
            </div>
          ))}
        </div>
        <p
          className={`mt-1.75! text-xs! ${
            invalidEmptyParameter ||
            invalidPathParameter ||
            invalidReservedParameter ||
            invalidValueParameter
              ? 'text-rp-error!'
              : 'text-rp-tertiary!'
          }`}
        >
          {invalidEmptyParameter
            ? '请输入参数字段。'
            : invalidPathParameter
              ? `参数路径 ${invalidPathParameter.key.trim()} 无效，请使用 reasoning.effort 这类对象路径。`
              : invalidReservedParameter
                ? `保留字段 ${invalidReservedParameter.key.trim()} 由 RelayPulse 管理，不能覆盖。`
                : invalidValueParameter
                  ? `参数 ${invalidValueParameter.key.trim()} 的 ${invalidValueParameter.type} 值无效。`
                  : customActive
                    ? '支持对象路径；model 由 RelayPulse 自动注入。'
                    : `支持 reasoning.effort 这类对象路径；${[...reservedFields].join('、')} 由 RelayPulse 管理。`}
        </p>
      </section>

      <section className={drawerSectionClass}>
        <div className={drawerSectionHeaderClass}>
          <div>
            <h3>有效请求预览</h3>
            <p>{customActive ? '自定义请求仅记录总耗时' : '保存后实际发送的最小请求'}</p>
          </div>
          {customActive ? (
            <Button
              icon={<ExperimentOutlined />}
              disabled={hasValidationError}
              onClick={() => setMockValidated(true)}
            >
              试探一次
            </Button>
          ) : null}
        </div>
        <pre
          className="mt-2 mb-0 overflow-visible rounded-md bg-[#141414] p-3 font-mono text-[11px] leading-4 whitespace-pre-wrap text-[#f5f5f5] wrap-anywhere"
          data-testid="request-preview"
        >
          {preview}
        </pre>
        {mockValidated ? (
          <Alert
            className="mt-2.5"
            type="info"
            showIcon
            title="请求配置校验通过；当前 Mock 阶段未发送网络请求。"
          />
        ) : null}
      </section>
    </Drawer>
  );
}
