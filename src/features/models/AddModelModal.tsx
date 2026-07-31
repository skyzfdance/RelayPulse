import { Checkbox, Form, Input, Modal } from 'antd';
import { useState } from 'react';
import { useRelayPulse } from '../../app/relay-pulse-context';
import { useBackendActionFeedback } from '../../app/use-backend-action-feedback';
import type { CreateModelInput, Site } from '../../domain/types';
import { scopedOverlayClass } from '../../styles/tailwind';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;

interface AddModelModalProps {
  open: boolean;
  site: Site;
  onClose(): void;
}

export function AddModelModal({ open, site, onClose }: AddModelModalProps) {
  const { createModel } = useRelayPulse();
  const runBackendAction = useBackendActionFeedback();
  const [form] = Form.useForm<CreateModelInput>();
  const [submitting, setSubmitting] = useState(false);

  function close() {
    form.resetFields();
    onClose();
  }

  async function submit() {
    let values: CreateModelInput;
    try {
      values = await form.validateFields();
    } catch {
      return;
    }
    setSubmitting(true);
    try {
      const created = await runBackendAction(() => createModel(site.id, values), {
        key: `create-model-${site.id}`,
        successTitle: '模型已添加',
        successDescription: `“${values.name}”已添加到“${site.name}”。`,
        errorTitle: '模型添加失败',
        errorDescription: '模型没有添加，当前填写内容已保留，请重试。',
      });
      if (created) close();
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Modal
      open={open}
      title="添加模型"
      width={480}
      centered
      getContainer={overlayContainer}
      rootClassName={scopedOverlayClass}
      rootStyle={{ position: 'absolute' }}
      mask={{ closable: false }}
      confirmLoading={submitting}
      okText="添加模型"
      cancelText="取消"
      onOk={() => void submit()}
      onCancel={close}
      destroyOnHidden
    >
      <p className="m-0 leading-5.5 text-rp-secondary">
        添加到 {site.name}；新模型默认继承站点探针策略。
      </p>
      <Form<CreateModelInput>
        name={`add-model-${site.id}`}
        form={form}
        layout="vertical"
        requiredMark={false}
        preserve={false}
        initialValues={{ enabled: true }}
        className="pt-2 [&_.ant-form-item]:mb-3"
      >
        <Form.Item
          label="模型名称"
          name="name"
          rules={[
            { required: true, whitespace: true, message: '请输入模型名称' },
            {
              validator: (_, value: string | undefined) =>
                value &&
                site.models.some((model) => model.name.toLowerCase() === value.trim().toLowerCase())
                  ? Promise.reject(new Error('该模型已存在'))
                  : Promise.resolve(),
            },
          ]}
        >
          <Input placeholder="例如：gpt-4.1-mini" autoFocus />
        </Form.Item>
        <Form.Item name="enabled" valuePropName="checked" className="mb-0!">
          <Checkbox>添加后立即启用模型</Checkbox>
        </Form.Item>
      </Form>
    </Modal>
  );
}
