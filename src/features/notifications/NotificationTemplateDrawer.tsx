import {
  CodeOutlined,
  DownOutlined,
  ExclamationCircleOutlined,
  ReloadOutlined,
  SendOutlined,
} from '@ant-design/icons';
import { App as AntdApp, Button, Drawer, Dropdown, Form, Input, Segmented, Select } from 'antd';
import type { MenuProps } from 'antd';
import type { TextAreaRef } from 'antd/es/input/TextArea';
import { useEffect, useMemo, useRef, useState } from 'react';
import type {
  NotificationChannelSnapshot,
  NotificationEventCatalogItem,
} from './notification-types';
import {
  defaultTemplateSelection,
  effectiveTemplateSelection,
  graphemeCount,
  notificationTemplateMarkers,
  renderTemplatePreview,
  type NotificationTemplateDescriptor,
  type NotificationTemplateMode,
  type NotificationTemplateSelection,
  validateTemplate,
} from './notification-templates';

const overlayContainer = () => document.getElementById('relaypulse-overlay-root') ?? document.body;
const PREVIEW_DELAY_MS = 160;

interface NotificationTemplateDrawerProps {
  open: boolean;
  event: NotificationEventCatalogItem;
  channel: NotificationChannelSnapshot;
  descriptor: NotificationTemplateDescriptor;
  value: NotificationTemplateSelection;
  testAvailable: boolean;
  onApply(value: NotificationTemplateSelection): void;
  onClose(): void;
  onDirtyChange(dirty: boolean): void;
  onTest(value: NotificationTemplateSelection): Promise<void>;
}

export function NotificationTemplateDrawer({
  open,
  event,
  channel,
  descriptor,
  value,
  testAvailable,
  onApply,
  onClose,
  onDirtyChange,
  onTest,
}: NotificationTemplateDrawerProps) {
  const { modal } = AntdApp.useApp();
  const [selection, setSelection] = useState(value);
  const [initialSelection] = useState(value);
  const [previewSelection, setPreviewSelection] = useState(value);
  const [restoreUndo, setRestoreUndo] = useState<NotificationTemplateSelection | null>(null);
  const [composing, setComposing] = useState(false);
  const [testing, setTesting] = useState(false);
  const textAreaRef = useRef<TextAreaRef>(null);
  const textSelectionRef = useRef({ start: 0, end: 0 });

  useEffect(() => {
    if (!open || composing) return;
    const timeout = window.setTimeout(() => setPreviewSelection(selection), PREVIEW_DELAY_MS);
    return () => window.clearTimeout(timeout);
  }, [composing, open, selection]);

  const validation = useMemo(
    () => validateTemplate(descriptor, previewSelection),
    [descriptor, previewSelection],
  );
  const actionValidation = useMemo(
    () => (composing ? validation : validateTemplate(descriptor, selection)),
    [composing, descriptor, selection, validation],
  );
  const effective = useMemo(
    () => effectiveTemplateSelection(descriptor, previewSelection),
    [descriptor, previewSelection],
  );
  const preview = useMemo(
    () => renderTemplatePreview(descriptor, previewSelection),
    [descriptor, previewSelection],
  );
  const dirty = JSON.stringify(selection) !== JSON.stringify(initialSelection);
  const custom = selection.mode === 'custom';
  const invalid = Boolean(actionValidation.title || actionValidation.body);
  const variableMenuItems = useMemo<MenuProps['items']>(
    () =>
      ['通用变量', '事件变量'].map((group) => ({
        type: 'group' as const,
        label: group,
        children: descriptor.variables
          .filter((variable) => variable.group === group)
          .map((variable) => ({
            key: variable.key,
            label: (
              <div className="py-0.5">
                <div className="font-medium">{variable.label}</div>
                <div className="text-xs text-rp-tertiary">{`{{${variable.key}}}`}</div>
              </div>
            ),
          })),
      })),
    [descriptor.variables],
  );

  useEffect(() => {
    onDirtyChange(dirty);
    return () => onDirtyChange(false);
  }, [dirty, onDirtyChange]);

  function updateMode(mode: NotificationTemplateMode) {
    setSelection((current) => ({ ...current, mode }));
  }

  function captureTextSelection() {
    const textArea = textAreaRef.current?.resizableTextArea?.textArea;
    if (!textArea) return;
    textSelectionRef.current = {
      start: textArea.selectionStart,
      end: textArea.selectionEnd,
    };
  }

  function insertVariable(key: string) {
    const textArea = textAreaRef.current?.resizableTextArea?.textArea;
    if (!textArea) return;
    const token = `{{${key}}}`;
    const { start, end } = textSelectionRef.current;
    textArea.focus();
    textArea.setSelectionRange(start, end);
    // insertText keeps the browser's native textarea undo stack intact.
    const inserted = document.execCommand('insertText', false, token);
    if (!inserted) {
      textArea.setRangeText(token, start, end, 'end');
      setSelection((current) => ({ ...current, bodyTemplate: textArea.value }));
    }
    const caret = start + token.length;
    textSelectionRef.current = { start: caret, end: caret };
    window.queueMicrotask(() => {
      textArea.focus();
      textArea.setSelectionRange(caret, caret);
    });
  }

  function closeDrawer() {
    if (!dirty) {
      onClose();
      return;
    }
    modal.confirm({
      title: '放弃未应用的模板更改？',
      icon: <ExclamationCircleOutlined />,
      content: '关闭后，本次在抽屉内的修改不会进入页面草稿。',
      okText: '放弃更改',
      cancelText: '继续编辑',
      okButtonProps: { danger: true },
      onOk: onClose,
    });
  }

  function restoreDefault() {
    modal.confirm({
      title: '恢复默认模板？',
      icon: <ExclamationCircleOutlined />,
      content: '当前自定义内容将被默认值替换，应用前仍可撤销。',
      okText: '恢复默认',
      cancelText: '取消',
      onOk: () => {
        setRestoreUndo(selection);
        setSelection(defaultTemplateSelection(descriptor));
      },
    });
  }

  async function testTemplate() {
    if (testing || invalid || composing || !testAvailable) return;
    setTesting(true);
    try {
      await onTest(selection);
    } finally {
      setTesting(false);
    }
  }

  return (
    <Drawer
      afterOpenChange={(nextOpen) => {
        if (nextOpen) textAreaRef.current?.focus({ cursor: 'end' });
      }}
      closable={{ placement: 'end', disabled: testing }}
      destroyOnHidden
      footer={
        <div className="flex items-center justify-between gap-3">
          <Button
            disabled={!testAvailable || invalid || composing}
            icon={<SendOutlined aria-hidden="true" />}
            loading={testing}
            onClick={() => void testTemplate()}
          >
            测试当前模板
          </Button>
          <div className="flex items-center gap-2">
            <Button disabled={testing} onClick={closeDrawer}>
              取消
            </Button>
            <Button
              disabled={invalid || composing || testing}
              onClick={() => onApply(selection)}
              type="primary"
            >
              应用
            </Button>
          </div>
        </div>
      }
      getContainer={overlayContainer}
      mask={{ closable: false }}
      onClose={closeDrawer}
      open={open}
      rootStyle={{ position: 'absolute' }}
      size={580}
      title={
        <div>
          <strong>编辑通知模板</strong>
          <div className="mt-0.5 text-xs font-normal text-rp-tertiary">
            {event.displayName} · {channel.displayName}
          </div>
        </div>
      }
    >
      <div className="flex flex-col gap-5" data-testid="notification-template-editor">
        <div className="flex items-center gap-2">
          <Segmented<NotificationTemplateMode>
            aria-label="模板模式"
            block
            className="min-w-60 flex-1"
            onChange={updateMode}
            options={[
              { label: '默认模板', value: 'default' },
              { label: '自定义模板', value: 'custom' },
            ]}
            value={selection.mode}
          />
          <Button
            aria-label="恢复默认模板"
            icon={<ReloadOutlined aria-hidden="true" />}
            onClick={restoreDefault}
            title="恢复默认"
          />
          {restoreUndo ? (
            <Button
              onClick={() => {
                setSelection(restoreUndo);
                setRestoreUndo(null);
              }}
              type="link"
            >
              撤销恢复
            </Button>
          ) : null}
        </div>

        <Form layout="vertical" requiredMark={false}>
          <Form.Item label="标题标记">
            <Select
              aria-label="标题标记"
              disabled={!custom}
              onChange={(markerId) => setSelection((current) => ({ ...current, markerId }))}
              options={notificationTemplateMarkers
                .filter((marker) => marker.enabled)
                .map((marker) => ({
                  value: marker.markerId,
                  label: (
                    <span className="inline-flex items-center gap-2">
                      <span className="inline-block w-5 text-center" aria-hidden="true">
                        {marker.symbol || '—'}
                      </span>
                      {marker.accessibleName}
                    </span>
                  ),
                }))}
              value={effective.markerId}
            />
          </Form.Item>

          <Form.Item
            help={validation.title}
            label={
              <span className="flex w-full items-center justify-between gap-4">
                <span>标题</span>
                <span className="text-xs font-normal text-rp-tertiary">
                  {graphemeCount(effective.titleTemplate)} / 120
                </span>
              </span>
            }
            validateStatus={validation.title ? 'error' : undefined}
          >
            <Input
              aria-label="通知标题"
              disabled={!custom}
              maxLength={240}
              onChange={(change) =>
                setSelection((current) => ({ ...current, titleTemplate: change.target.value }))
              }
              onCompositionEnd={() => setComposing(false)}
              onCompositionStart={() => setComposing(true)}
              value={effective.titleTemplate}
            />
          </Form.Item>

          <Form.Item
            help={validation.body}
            label={
              <span className="flex w-full items-center justify-between gap-4">
                <span>通知详情</span>
                <span className="text-xs font-normal text-rp-tertiary">
                  {graphemeCount(effective.bodyTemplate)} / 500
                </span>
              </span>
            }
            validateStatus={validation.body ? 'error' : undefined}
          >
            <div
              className={`overflow-hidden rounded-md border bg-rp-surface transition-[border-color,box-shadow] ${
                validation.body
                  ? 'border-rp-error focus-within:border-rp-error'
                  : 'border-rp-line focus-within:border-rp-line-active focus-within:shadow-[0_0_0_2px_rgb(22_119_255/14%)]'
              }`}
              data-testid="notification-template-body-editor"
            >
              <Input.TextArea
                aria-label="通知详情"
                autoSize={{ minRows: 6, maxRows: 10 }}
                className="!rounded-none !bg-transparent !px-3 !py-2.5"
                defaultValue={custom ? selection.bodyTemplate : undefined}
                disabled={!custom}
                key={custom ? 'custom-body' : 'default-body'}
                maxLength={1_000}
                onChange={(change) => {
                  setSelection((current) => ({ ...current, bodyTemplate: change.target.value }));
                  textSelectionRef.current = {
                    start: change.target.selectionStart,
                    end: change.target.selectionEnd,
                  };
                }}
                onCompositionEnd={(compositionEvent) => {
                  setComposing(false);
                  textSelectionRef.current = {
                    start: compositionEvent.currentTarget.selectionStart,
                    end: compositionEvent.currentTarget.selectionEnd,
                  };
                }}
                onCompositionStart={() => setComposing(true)}
                onClick={captureTextSelection}
                onKeyUp={captureTextSelection}
                onSelect={captureTextSelection}
                ref={textAreaRef}
                value={custom ? undefined : effective.bodyTemplate}
                variant="borderless"
              />
              <div className="flex min-h-10 items-center justify-between gap-3 border-t border-rp-line bg-rp-subtle px-2 py-1">
                <span className="pl-1 text-xs text-rp-tertiary">在光标位置插入</span>
                <Dropdown
                  disabled={!custom}
                  menu={{
                    items: variableMenuItems,
                    onClick: ({ key }) => insertVariable(key),
                  }}
                  trigger={['click']}
                >
                  <Button
                    aria-label="插入变量"
                    disabled={!custom}
                    icon={<CodeOutlined aria-hidden="true" />}
                    onMouseDownCapture={captureTextSelection}
                    onPointerDownCapture={captureTextSelection}
                    size="small"
                    type="text"
                  >
                    插入变量
                    <DownOutlined aria-hidden="true" />
                  </Button>
                </Dropdown>
              </div>
            </div>
          </Form.Item>
        </Form>

        <section aria-labelledby="notification-template-preview-heading">
          <div className="mb-2 flex items-center justify-between gap-4">
            <h3 className="text-sm font-medium" id="notification-template-preview-heading">
              通知预览
            </h3>
            <span className="text-xs text-rp-tertiary">固定示例数据</span>
          </div>
          <div className="rounded-md border border-rp-line bg-rp-subtle p-4">
            <div className="font-semibold break-words text-rp">{preview.title || '—'}</div>
            <div className="mt-1.5 whitespace-pre-wrap break-words text-sm leading-6 text-rp-secondary">
              {preview.body || '—'}
            </div>
          </div>
        </section>
      </div>
    </Drawer>
  );
}
