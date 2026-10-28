import { defineStore } from 'pinia';
import { ref } from 'vue';

export type SiteTask =
  | {
      type:
        | 'sync'
        | 'probe'
        | 'save-basic'
        | 'save-connection'
        | 'save-policy'
        | 'save-model-policy'
        | 'add-model'
        | 'delete-model'
        | 'toggle-model'
        | 'toggle-site'
        | 'delete-site';
    }
  | { type: 'probe-model' | 'test-model-probe'; modelId: string };

/** 只登记进行中的任务；请求、结果和异常处理留在站点模块。 */
export const useSiteTasksStore = defineStore('site-tasks', () => {
  const tasks = ref<Partial<Record<string, SiteTask>>>({});

  function register(siteId: string, task: SiteTask): boolean {
    if (tasks.value[siteId]) return false;
    tasks.value[siteId] = task;
    return true;
  }

  function remove(siteId: string) {
    delete tasks.value[siteId];
  }

  return { tasks, register, remove };
});
