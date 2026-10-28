/**
 * 消息通知页编排 hook：读设置、改设置（保存成功后才落到 UI）。
 * loading 一律交 callBackend / api 层统一控制，本层不另设页面级 loading；失败由 api 层弹提示。
 */
import type { NotificationSettings, NotificationTestTemplate } from '@/types/domain/notifications';

import { computed, onMounted, ref } from 'vue';

import {
  fetchNotificationSettings,
  openNotificationSettings,
  requestNotificationPermission,
  sendTestNotification,
  updateNotificationSettings,
} from '@/api/config/notifications';

/** 系统通知行「发送测试通知」用的固定样例（模板抽屉里改为传当前表单的预览内容，走同一接口）。 */
const TEST_SAMPLE: NotificationTestTemplate = {
  title: 'RelayPulse 测试通知',
  body: '这是一条测试通知，用来确认系统通知是否正常工作。',
};

export function useNotifications() {
  const settings = ref<NotificationSettings>();
  const templateOpen = ref(false);
  const templateRuleId = ref('');

  const testAvailable = computed(
    () => settings.value?.authorization === 'granted' && settings.value.notificationsEnabled,
  );

  async function load(): Promise<void> {
    settings.value = await fetchNotificationSettings();
  }

  async function setNotificationsEnabled(value: boolean): Promise<void> {
    settings.value = await updateNotificationSettings({
      ...settings.value!,
      notificationsEnabled: value,
    });
  }

  async function setRuleStatus(id: string, status: boolean): Promise<void> {
    const current = settings.value!;
    const rules = current.rules.map((rule) => (rule.id === id ? { ...rule, status } : rule));
    settings.value = await updateNotificationSettings({ ...current, rules });
  }

  async function requestPermission(): Promise<void> {
    const result = await requestNotificationPermission();
    if (settings.value) settings.value.authorization = result;
  }

  async function sendTest(): Promise<void> {
    await sendTestNotification(TEST_SAMPLE);
  }

  async function openSettings(): Promise<void> {
    await openNotificationSettings();
  }

  function openTemplate(id: string): void {
    templateRuleId.value = id;
    templateOpen.value = true;
  }

  function closeTemplate(): void {
    templateOpen.value = false;
  }

  onMounted(load);

  return {
    settings,
    testAvailable,
    templateOpen,
    templateRuleId,
    setNotificationsEnabled,
    setRuleStatus,
    requestPermission,
    sendTest,
    openSettings,
    openTemplate,
    closeTemplate,
    load,
  };
}
