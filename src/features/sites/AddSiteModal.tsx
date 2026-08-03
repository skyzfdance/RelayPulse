import { Checkbox, Form, Input, Modal, Select } from 'antd';
import { useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { useBackendActionFeedback } from '../../app/use-backend-action-feedback';
import { navigateToView } from '../../app/view';
import type { CreateSiteInput } from '../../domain/types';
import { monoInputClass } from '../../styles/tailwind';
import { siteProtocolOptions } from './protocol-options';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;

export function AddSiteModal() {
  const { createSite } = useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
  const [form] = Form.useForm<Omit<CreateSiteInput, 'credential'> & { apiKey: string }>();
  const [submitting, setSubmitting] = useState(false);

  async function submit() {
    let values: Omit<CreateSiteInput, 'credential'> & { apiKey: string };
    try {
      values = await form.validateFields();
    } catch {
      return;
    }
    setSubmitting(true);
    try {
      const created = await runBackendAction(
        () =>
          createSite({
            name: values.name,
            protocol: values.protocol,
            baseUrl: values.baseUrl,
            enabled: values.enabled,
            credential: { action: 'replace', secret: values.apiKey },
          }),
        {
          key: 'create-site',
          successTitle: '站点已添加',
          successDescription: `“${values.name}”已保存，可以继续添加或同步模型。`,
          errorTitle: '站点添加失败',
          errorDescription: '站点没有创建，当前填写内容已保留，请检查后重试。',
        },
      );
      if (created) navigateToView('sites');
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Modal
      open
      title="添加站点"
      width={560}
      centered
      getContainer={overlayContainer}
      rootStyle={{ position: 'absolute' }}
      mask={{ closable: false }}
      confirmLoading={submitting}
      okText="连接并获取模型"
      cancelText="取消"
      onOk={() => void submit()}
      onCancel={() => navigateToView('sites')}
      destroyOnHidden
    >
      <Form<Omit<CreateSiteInput, 'credential'> & { apiKey: string }>
        name="add-site"
        form={form}
        layout="vertical"
        requiredMark={false}
        initialValues={{ protocol: 'openai-compatible', enabled: true }}
        className="pt-2"
      >
        <Form.Item
          label="站点名称"
          name="name"
          rules={[{ required: true, whitespace: true, message: '请输入站点名称' }]}
        >
          <Input placeholder="例如：OpenAI 主线路" autoFocus />
        </Form.Item>
        <Form.Item label="接口协议" name="protocol" rules={[{ required: true }]}>
          <Select options={siteProtocolOptions} />
        </Form.Item>
        <Form.Item
          label="Base URL"
          name="baseUrl"
          rules={[{ required: true, type: 'url', message: '请输入有效的 Base URL' }]}
        >
          <Input className={monoInputClass} placeholder="https://api.example.com/v1" />
        </Form.Item>
        <Form.Item
          label="API Key"
          name="apiKey"
          rules={[{ required: true, message: '请输入访问凭据' }]}
        >
          <Input.Password className={monoInputClass} placeholder="输入访问凭据" />
        </Form.Item>
        <Form.Item name="enabled" valuePropName="checked" className="mb-0!">
          <Checkbox>创建后立即启用站点</Checkbox>
        </Form.Item>
      </Form>
    </Modal>
  );
}
