import { DeleteOutlined, ExperimentOutlined, PlusOutlined } from '@ant-design/icons';
import { Alert, Button, Drawer, Input, InputNumber, Segmented, Select, Switch, Tag } from 'antd';
import { useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { navigateToView } from '../../app/view';
import type {
  CustomProbeSettings,
  ParameterType,
  PassThroughParameter,
  ProbeMode,
  ProbeSettings,
} from '../../domain/types';
import {
  buildCustomProbeRequest,
  buildProbeRequest,
  customReservedProbeFields,
  isParameterValueValid,
  reservedProbeFields,
  resolveCustomProbeEndpoint,
} from '../../domain/probe-request';
import styles from '../../styles/App.module.css';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;
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
  settings: ProbeSettings,
  customProbe: CustomProbeSettings,
) {
  if (!customProbe.enabled) {
    return JSON.stringify(buildProbeRequest(modelName, settings), null, 2);
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

  const customActive = !siteDefault && customProbe.enabled;
  const parameters = customActive ? customProbe.parameters : settings.parameters;
  const reservedFields = customActive ? customReservedProbeFields : reservedProbeFields;
  const invalidReservedParameter = parameters.find((parameter) =>
    reservedFields.has(parameter.key.trim()),
  );
  const invalidValueParameter = parameters.find(
    (parameter) => parameter.key.trim() && !isParameterValueValid(parameter),
  );
  const invalidEndpoint =
    customActive &&
    !endpointIsValid(site?.baseUrl ?? 'https://example.invalid', customProbe.endpoint);
  const hasValidationError = Boolean(
    invalidReservedParameter || invalidValueParameter || invalidEndpoint,
  );
  const preview = requestPreview(
    site?.baseUrl ?? 'https://example.invalid',
    siteDefault ? '<模型名称>' : (model?.name ?? 'model'),
    settings,
    customActive ? customProbe : customDefaults,
  );

  if (!site || (!siteDefault && !model)) return null;

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
    setSettings(siteDefault ? economicalDefaults : { ...economicalDefaults, mode: 'inherit' });
    if (!siteDefault) setCustomProbe(customDefaults);
    setMockValidated(false);
  }

  return (
    <Drawer
      open
      size={560}
      closable={{ placement: 'end' }}
      title={
        <div>
          <strong>{siteDefault ? '站点默认探针策略' : '模型探针设置'}</strong>
          <div className={styles.drawerSubtitle}>
            {siteDefault ? `${site.name} · 新启用模型默认继承` : `${model?.name} · ${site.name}`}
          </div>
        </div>
      }
      getContainer={overlayContainer}
      rootClassName={styles.scopedOverlay}
      rootStyle={{ position: 'absolute' }}
      mask={{ closable: false }}
      onClose={() => navigateToView('sites')}
      footer={
        <div className={styles.drawerFooter}>
          <Button onClick={restoreDefaults}>{siteDefault ? '恢复极省模式' : '恢复继承'}</Button>
          <span />
          <Button onClick={() => navigateToView('sites')}>取消</Button>
          <Button
            type="primary"
            disabled={hasValidationError}
            onClick={async () => {
              if (siteDefault) {
                await updateSiteProbeSettings(site.id, settings);
              } else if (model) {
                await updateModelProbeSettings(site.id, model.id, settings);
                await updateModelCustomProbeSettings(site.id, model.id, customProbe);
              }
              navigateToView('sites');
            }}
          >
            保存设置
          </Button>
        </div>
      }
    >
      {!siteDefault ? (
        <section className={styles.drawerSection}>
          <div className={styles.drawerSectionHeader}>
            <div>
              <h3>自定义探针请求</h3>
              <p>仅覆盖当前模型的探测端点和请求参数</p>
            </div>
            <Switch
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
                <span className={styles.fieldLabel}>自定义探测端点</span>
                <Input
                  className={styles.mono}
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
              <div className={styles.capabilityTags}>
                <Tag>POST</Tag>
                <Tag color="blue">继承站点凭据和请求头</Tag>
                <Tag>首 Token 显示 —</Tag>
              </div>
              {invalidEndpoint ? (
                <p className={styles.parameterError}>端点必须与站点 Base URL 同源。</p>
              ) : null}
            </>
          ) : null}
        </section>
      ) : null}

      {!siteDefault && !customActive ? (
        <section className={styles.drawerSection}>
          <h3>推理能力</h3>
          <p>用于判断可发送的推理参数</p>
          <div className={styles.capabilityTags}>
            <Tag color="blue">自动识别</Tag>
            <Tag color="green">支持关闭思考</Tag>
            <Tag>支持 low / medium / high</Tag>
          </div>
        </section>
      ) : null}

      {siteDefault || !customActive ? (
        <section className={styles.drawerSection}>
          <h3>{siteDefault ? '站点默认探针策略' : '本模型探针策略'}</h3>
          <p>{siteDefault ? '新启用模型默认继承；单个模型仍可覆盖' : '覆盖站点默认的“极省模式”'}</p>
          <label className={styles.fieldLabel}>推理模式</label>
          <Segmented<ProbeMode>
            value={settings.mode}
            options={
              siteDefault
                ? [
                    { value: 'disabled-thinking', label: '优先关闭思考' },
                    { value: 'minimum-reasoning', label: '自定义策略' },
                  ]
                : [
                    { value: 'inherit', label: '继承默认' },
                    { value: 'disabled-thinking', label: '关闭思考' },
                    { value: 'minimum-reasoning', label: '最低推理' },
                  ]
            }
            onChange={(mode) => setSettings((current) => ({ ...current, mode }))}
          />
          <div className={styles.drawerFormGrid}>
            <label>
              <span className={styles.fieldLabel}>最大输出</span>
              <InputNumber
                min={1}
                max={128}
                value={settings.maxOutputTokens}
                suffix="tokens"
                className={styles.fullWidth}
                onChange={(value) =>
                  setSettings((current) => ({ ...current, maxOutputTokens: value ?? 8 }))
                }
              />
            </label>
            <label>
              <span className={styles.fieldLabel}>探针提示词</span>
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

      <section className={styles.drawerSection}>
        <div className={styles.drawerSectionHeader}>
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
        <div className={styles.paramTable}>
          <div className={styles.paramHeader}>
            <span>字段</span>
            <span>值</span>
            <span>类型</span>
            <span />
          </div>
          {parameters.map((parameter, index) => (
            <div className={styles.paramRow} key={index}>
              <Input
                className={styles.mono}
                status={reservedFields.has(parameter.key.trim()) ? 'error' : undefined}
                value={parameter.key}
                onChange={(event) => updateParameter(index, { key: event.target.value })}
              />
              <Input
                className={styles.mono}
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
          className={
            invalidReservedParameter || invalidValueParameter
              ? styles.parameterError
              : styles.parameterNote
          }
        >
          {invalidReservedParameter
            ? `保留字段 ${invalidReservedParameter.key.trim()} 由 RelayPulse 管理，不能覆盖。`
            : invalidValueParameter
              ? `参数 ${invalidValueParameter.key.trim()} 的 ${invalidValueParameter.type} 值无效。`
              : customActive
                ? 'model 由 RelayPulse 自动注入；JSON 类型可填写数组或对象。'
                : '保留字段 model、messages、stream 由 RelayPulse 管理，不能覆盖。'}
        </p>
      </section>

      <section className={styles.drawerSection}>
        <div className={styles.drawerSectionHeader}>
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
        <pre className={styles.codePreview}>{preview}</pre>
        {mockValidated ? (
          <Alert
            className={styles.mockProbeAlert}
            type="info"
            showIcon
            title="请求配置校验通过；当前 Mock 阶段未发送网络请求。"
          />
        ) : null}
      </section>
    </Drawer>
  );
}
