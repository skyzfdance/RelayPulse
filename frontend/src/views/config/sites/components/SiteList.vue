<script setup lang="ts">
import { storeToRefs } from 'pinia';
import { ref, watch } from 'vue';

import { reorderSites, toggleSite } from '@/api/config/sites';
import { useSortable } from '@/composables/useSortable';
import { useSiteDetailsStore, useSiteStore, useSiteTasksStore } from '@/stores/site';

import { healthColors, healthLabels } from '../site-policy';

import AppIcon from '@/components/AppIcon.vue';

const emit = defineEmits<{ add: [] }>();
const store = useSiteStore();
const detailsStore = useSiteDetailsStore();
const { sites, selectedSiteId: selectedId } = storeToRefs(store);
const taskStore = useSiteTasksStore();

function selectSite(id: string) {
  if (
    id === store.selectedSiteId ||
    (selectedId.value && taskStore.tasks[selectedId.value]?.type === 'test-model-probe')
  )
    return;
  store.selectedSiteId = id;
  void detailsStore.loadSiteDetails();
}
function isSiteProcessing(id: string) {
  return Boolean(taskStore.tasks[id]);
}
const reordering = ref(false);
async function reorder(ids: string[]) {
  if (reordering.value) return;
  reordering.value = true;
  try {
    await reorderSites(ids);
    await store.loadSite({ autoLoadDetails: false });
  } catch {
    // API 已提示；保存失败不刷新，保留原顺序。
  } finally {
    reordering.value = false;
  }
}
async function setSiteEnabled(id: string, enabled: boolean) {
  if (!taskStore.register(id, { type: 'toggle-site' })) return;
  try {
    await toggleSite(id, enabled);
    await store.loadSite({ autoLoadDetails: false });
    if (id === store.selectedSiteId) await detailsStore.loadSiteDetails();
  } catch {
    // API 已提示；保存失败不刷新，开关保持原状态。
  } finally {
    taskStore.remove(id);
  }
}
// 其他页面启动的探测/同步结束后，重新读取列表摘要。
watch(
  () => Object.entries(taskStore.tasks),
  (_current, previous) => {
    if (
      previous.some(
        ([id, task]) =>
          task && ['sync', 'probe', 'probe-model'].includes(task.type) && !taskStore.tasks[id],
      )
    ) {
      void store.loadSite({ autoLoadDetails: false });
    }
  },
);

const listElement = ref<HTMLElement | null>(null);
useSortable(listElement, {
  disabled: reordering,
  onReorder: (ids) => {
    void reorder(ids);
  },
});
function move(index: number, delta: number) {
  const ids = sites.value.map((site) => site.id);
  const next = index + delta;
  if (next < 0 || next >= ids.length) return;
  [ids[index], ids[next]] = [ids[next], ids[index]];
  void reorder(ids);
}
</script>
<template>
  <aside
    class="w-75 flex-none overflow-y-auto border-r border-rp-line bg-rp-subtle px-3 py-4 max-[1180px]:w-69"
  >
    <div class="mb-2 flex h-8 items-center justify-between">
      <div>
        <strong>站点</strong
        ><span class="ml-1.5 text-xs font-normal text-rp-tertiary">{{ sites.length }}</span>
      </div>
      <a-tooltip title="添加站点">
        <a-button aria-label="添加站点" @click="emit('add')">
          <template #icon><AppIcon name="PlusOutlined" /></template>
        </a-button>
      </a-tooltip>
    </div>
    <div ref="listElement" class="flex flex-col gap-1.5">
      <div
        v-for="(site, index) in sites"
        :key="site.id"
        :data-sort-id="site.id"
        class="grid min-h-16 cursor-pointer grid-cols-[20px_minmax(0,1fr)_34px] items-center gap-1.5 rounded-md border px-2.5 py-2 pl-1 transition-colors"
        :class="
          site.id === selectedId
            ? 'border-(--ant-color-primary-border) bg-rp-surface'
            : 'border-transparent hover:bg-rp-hover'
        "
        @click="selectSite(site.id)"
      >
        <a-tooltip title="拖动调整优先级，也可用上下方向键排序">
          <button
            :disabled="reordering"
            data-sort-handle
            class="h-8 w-5 cursor-grab border-0 bg-transparent p-0 text-rp-quaternary active:cursor-grabbing"
            :aria-label="`拖动 ${site.name}，当前优先级 ${site.priority}`"
            @click.stop
            @keydown.up.prevent="move(index, -1)"
            @keydown.down.prevent="move(index, 1)"
          >
            <AppIcon name="HolderOutlined" />
          </button>
        </a-tooltip>
        <button
          class="min-w-0 cursor-pointer border-0 bg-transparent p-0 text-left text-inherit"
          :aria-current="site.id === selectedId ? 'true' : undefined"
          @click.stop="selectSite(site.id)"
        >
          <span class="flex min-w-0 items-center gap-1.75 leading-5.5 font-semibold">
            <span
              class="size-1.75 flex-none rounded-full"
              :style="{ background: healthColors[site.health] }"
              role="img"
              :aria-label="healthLabels[site.health]"
            />
            <span class="truncate" :title="site.name">{{ site.name }}</span>
            <span
              class="inline-flex h-4.25 min-w-4.25 items-center justify-center rounded-sm bg-(--ant-color-fill-secondary) px-1 text-[11px] font-normal text-rp-tertiary"
              >{{ site.priority }}</span
            >
          </span>
          <span class="mt-px block truncate text-xs leading-5 font-normal text-rp-tertiary">{{
            `${site.modelCount} 个模型 · ${site.statusText}`
          }}</span>
        </button>
        <a-switch
          class="justify-self-end"
          :checked="site.enabled"
          size="small"
          :aria-label="`在小面板中${site.enabled ? '隐藏' : '显示'} ${site.name}`"
          :disabled="isSiteProcessing(site.id)"
          @click="(_checked: unknown, event: Event) => event.stopPropagation()"
          @change="setSiteEnabled(site.id, $event as boolean)"
        />
      </div>
    </div>
    <div class="mt-3.5 flex items-start gap-2 text-xs leading-4.5 text-rp-tertiary">
      <AppIcon name="InfoCircleOutlined" class="mt-0.5" />
      <span>拖动调整优先级；开关控制站点是否在小面板中显示。</span>
    </div>
  </aside>
</template>
