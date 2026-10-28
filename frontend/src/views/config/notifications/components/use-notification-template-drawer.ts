import type { TemplateValidation } from '../template-logic';
import type {
  NotificationMarker,
  NotificationTemplateDetail,
  NotificationTemplateMode,
  NotificationTemplateUpdate,
  NotificationVariable,
} from '@/types/domain/notifications';

import { computed, reactive, ref, watch } from 'vue';

import { fetchNotificationMarkers, fetchNotificationVariables } from '@/api/catalog';
import {
  fetchNotificationTemplateDetail,
  sendTestNotification,
  updateNotificationTemplate,
} from '@/api/config/notifications';

import {
  renderTemplatePreview,
  validateNotificationPreview,
  validateTemplate,
} from '../template-logic';

export interface NotificationTemplateDrawerProps {
  open: boolean;
  ruleId: string | null;
  testAvailable: boolean;
}

interface NotificationTemplateDrawerCallbacks {
  onClose: () => void;
  onSaved: () => void;
}

export function useNotificationTemplateDrawer(
  props: Readonly<NotificationTemplateDrawerProps>,
  { onClose, onSaved }: NotificationTemplateDrawerCallbacks,
) {
  const form = ref<NotificationTemplateDetail>();
  const markers = ref<NotificationMarker[]>([]);
  const variables = ref<NotificationVariable[]>([]);
  const loading = ref(false);
  const bodyRange = reactive({ start: 0, end: 0 });

  const custom = computed(() => form.value?.templateType === 'custom');
  const selectedMarker = computed(() =>
    markers.value.find((item) => item.markerId === form.value?.markerId),
  );

  const markerOptions = computed(() =>
    markers.value.map((item) => ({
      value: item.markerId,
      label: `${item.symbol || '—'}  ${item.name}`,
    })),
  );

  const variableGroups = computed(() => [...new Set(variables.value.map((item) => item.group))]);

  const variableMenu = computed(() => ({
    items: variableGroups.value.map((group) => ({
      type: 'group' as const,
      label: group,
      children: variables.value
        .filter((item) => item.group === group)
        .map((item) => ({ key: item.key, label: `${item.label}  {{${item.key}}}` })),
    })),
    onClick: ({ key }: { key: string }) => insertVariable(key),
  }));

  const validation = computed<TemplateValidation>(() =>
    form.value
      ? validateTemplate(
          form.value.titleTemplate,
          form.value.bodyTemplate,
          variables.value,
          selectedMarker.value,
        )
      : {},
  );
  const invalid = computed(() => Object.values(validation.value).some(Boolean));
  const preview = computed(() =>
    renderTemplatePreview(
      form.value?.titleTemplate ?? '',
      form.value?.bodyTemplate ?? '',
      variables.value,
      selectedMarker.value,
    ),
  );

  // 模板保存与成品发送分别校验；示例正文展开超长不阻止保存合法模板。
  const testError = computed(() => {
    if (!form.value || invalid.value) return undefined;
    const errors = validateNotificationPreview(preview.value);
    if (errors.title) return `测试标题：${errors.title}`;
    if (errors.body) return `测试正文：${errors.body}`;
    return undefined;
  });

  watch(
    () => [props.open, props.ruleId] as const,
    async ([open, id], _, onCleanup) => {
      if (!open || !id) return;
      // 关闭或切换规则后，忽略这次加载的结果。
      let active = true;
      onCleanup(() => {
        active = false;
      });
      loading.value = true;
      form.value = undefined;
      bodyRange.start = bodyRange.end = 0;
      try {
        const [data, markerList, variableList] = await Promise.all([
          fetchNotificationTemplateDetail(id),
          fetchNotificationMarkers(),
          fetchNotificationVariables(),
        ]);
        if (!active) return;
        form.value = data;
        markers.value = markerList;
        variables.value = variableList;
        if (data.templateType === 'default') restoreDefault();
      } catch {
        if (active) onClose(); // API 已提示加载错误。
      } finally {
        if (active) loading.value = false;
      }
    },
    { immediate: true },
  );

  /** 切回默认时直接覆盖表单，不保留未保存的自定义内容。 */
  function restoreDefault(): void {
    if (!form.value) return;
    const current = form.value;
    current.templateType = 'default';
    current.markerId = current.defaultMarkerId;
    current.titleTemplate = current.defaultTitleTemplate;
    current.bodyTemplate = current.defaultBodyTemplate;
    bodyRange.start = bodyRange.end = 0;
  }

  function changeTemplateType(value: NotificationTemplateMode): void {
    if (!form.value || loading.value) return;
    form.value.templateType = value;
    if (value === 'default') restoreDefault();
  }

  function close(): void {
    if (!loading.value) onClose();
  }

  function rememberRange(payload: Event): void {
    const textarea = payload.target as HTMLTextAreaElement | null;
    if (!textarea) return;
    bodyRange.start = textarea.selectionStart ?? 0;
    bodyRange.end = textarea.selectionEnd ?? 0;
  }

  function insertVariable(key: string): void {
    if (!form.value || !custom.value || loading.value) return;
    const token = `{{${key}}}`;
    const body = form.value.bodyTemplate;
    form.value.bodyTemplate = body.slice(0, bodyRange.start) + token + body.slice(bodyRange.end);
    const caret = bodyRange.start + token.length;
    bodyRange.start = caret;
    bodyRange.end = caret;
  }

  function testTemplate(): void {
    if (loading.value || invalid.value || testError.value || !props.testAvailable || !form.value)
      return;
    void sendTestNotification({
      title: preview.value.title,
      body: preview.value.body,
      markerId: form.value.markerId,
    }).catch(() => {
      // API 负责全局反馈，测试结果不影响抽屉状态。
    });
  }

  async function save(): Promise<void> {
    if (!form.value || invalid.value || loading.value) return;
    loading.value = true;
    try {
      const { id, templateType, markerId, titleTemplate, bodyTemplate } = form.value;
      const update: NotificationTemplateUpdate = {
        templateType,
        markerId,
        titleTemplate,
        bodyTemplate,
      };
      await updateNotificationTemplate(id, update);
      onClose();
      onSaved();
    } catch {
      // API 已提示错误，保留表单供重试。
    } finally {
      loading.value = false;
    }
  }

  return {
    form,
    loading,
    custom,
    markerOptions,
    variableMenu,
    validation,
    invalid,
    preview,
    testError,
    restoreDefault,
    changeTemplateType,
    close,
    rememberRange,
    testTemplate,
    save,
  };
}
