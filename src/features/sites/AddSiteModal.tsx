import { Checkbox, Form, Input, Modal, Select } from 'antd';
import { useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { navigateToView } from '../../app/view';
import type { CreateSiteInput } from '../../domain/types';
import styles from '../../styles/App.module.css';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;

export function AddSiteModal() {
  const { createSite } = useRelayPulse();
  const [form] = Form.useForm<CreateSiteInput>();
  const [submitting, setSubmitting] = useState(false);

  async function submit() {
    const values = await form.validateFields();
    setSubmitting(true);
    try {
      await createSite(values);
      navigateToView('sites');
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
      rootClassName={styles.scopedOverlay}
      rootStyle={{ position: 'absolute' }}
      mask={{ closable: false }}
      confirmLoading={submitting}
      okText="连接并获取模型"
      cancelText="取消"
      onOk={() => void submit()}
      onCancel={() => navigateToView('sites')}
      destroyOnHidden
    >
      <Form<CreateSiteInput>
        name="add-site"
        form={form}
        layout="vertical"
        requiredMark={false}
        initialValues={{ protocol: 'OpenAI 兼容', enabled: true }}
        className={styles.addSiteForm}
      >
        <Form.Item
          label="站点名称"
          name="name"
          rules={[{ required: true, whitespace: true, message: '请输入站点名称' }]}
        >
          <Input placeholder="例如：OpenAI 主线路" autoFocus />
        </Form.Item>
        <Form.Item label="接口协议" name="protocol" rules={[{ required: true }]}>
          <Select
            options={[
              { value: 'OpenAI 兼容', label: 'OpenAI 兼容' },
              { value: 'Anthropic', label: 'Anthropic' },
              { value: 'Gemini', label: 'Gemini' },
            ]}
          />
        </Form.Item>
        <Form.Item
          label="Base URL"
          name="baseUrl"
          rules={[{ required: true, type: 'url', message: '请输入有效的 Base URL' }]}
        >
          <Input className={styles.mono} placeholder="https://api.example.com/v1" />
        </Form.Item>
        <Form.Item
          label="API Key"
          name="apiKey"
          rules={[{ required: true, message: '请输入访问凭据' }]}
        >
          <Input.Password className={styles.mono} placeholder="输入访问凭据" />
        </Form.Item>
        <Form.Item name="enabled" valuePropName="checked" className={styles.inlineSwitchItem}>
          <Checkbox>创建后立即启用站点</Checkbox>
        </Form.Item>
      </Form>
    </Modal>
  );
}
