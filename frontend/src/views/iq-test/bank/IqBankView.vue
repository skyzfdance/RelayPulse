<script setup lang="ts">
import type { IqQuestion } from '@/types/domain/iq-test';
import type { TableProps } from 'antdv-next';

import { computed, onMounted, ref, shallowRef, watch } from 'vue';

import { useIqTestStore } from '@/stores/iq-test';

import AppIcon from '@/components/AppIcon.vue';
import IqQuestionCell from '@/views/iq-test/components/IqQuestionCell.vue';
import IqQuestionDrawer from '@/views/iq-test/components/IqQuestionDrawer.vue';

const store = useIqTestStore();
const keyword = ref('');
const normalizedKeyword = computed(() => keyword.value.trim().toLowerCase());
const status = ref<'all' | 'enabled' | 'disabled'>('all');
const page = ref(1);
const pageSize = ref(10);
const tableViewport = ref<HTMLElement>();
const getScrollContainer = () => tableViewport.value ?? document.body;
const drawerOpen = ref(false);
const drawerMode = ref<'view' | 'edit'>('edit');
const editingQuestion = shallowRef<IqQuestion>();
const viewingQuestionId = ref('');
const viewingQuestion = computed(() =>
  store.questions.find((question) => question.id === viewingQuestionId.value),
);
const drawerQuestion = computed(() =>
  drawerMode.value === 'view' ? viewingQuestion.value : editingQuestion.value,
);
const saving = ref(false);
const pendingId = ref('');
const saveError = ref('');
const actionError = ref('');
const busy = computed(() => saving.value || Boolean(pendingId.value));
const statusOptions = [
  { value: 'all', label: '全部题目' },
  { value: 'enabled', label: '已启用' },
  { value: 'disabled', label: '已停用' },
];
const filteredQuestions = computed(() =>
  store.questions.filter((question) => {
    if (!question.content.toLowerCase().includes(normalizedKeyword.value)) return false;
    if (status.value === 'enabled') return question.enabled;
    if (status.value === 'disabled') return !question.enabled;
    return true;
  }),
);
const emptyDescription = computed(() => {
  if (!store.questions.length) return '还没有测试题目';
  return normalizedKeyword.value ? '没有匹配的题目' : '没有符合筛选条件的题目';
});
const visibleQuestions = computed(() => {
  const start = (page.value - 1) * pageSize.value;
  return filteredQuestions.value.slice(start, start + pageSize.value);
});
const columns: TableProps<IqQuestion>['columns'] = [
  { title: '题目', key: 'content' },
  { title: '启用', key: 'enabled', width: 88 },
  { title: '操作', key: 'actions', width: 112, align: 'center', fixed: 'end' },
];
const showTotal = (total: number, range: [number, number]) =>
  `第 ${range[0]}–${range[1]} 条，共 ${total} 条`;

function resetScroll() {
  if (tableViewport.value) tableViewport.value.scrollTop = 0;
}
function changePage(next: number, nextSize: number) {
  if (store.loading || busy.value) return;
  page.value = nextSize === pageSize.value ? next : 1;
  pageSize.value = nextSize;
}
watch([keyword, status], () => {
  page.value = 1;
  resetScroll();
});
function resetFilters() {
  keyword.value = '';
  status.value = 'all';
}
watch(filteredQuestions, (questions) => {
  page.value = Math.min(page.value, Math.max(1, Math.ceil(questions.length / pageSize.value)));
});
watch([page, pageSize], resetScroll, { flush: 'post' });
watch(viewingQuestion, (question) => {
  if (drawerOpen.value && drawerMode.value === 'view' && !question) drawerOpen.value = false;
});

function openEditor(question?: IqQuestion) {
  if (busy.value) return;
  editingQuestion.value = question;
  drawerMode.value = 'edit';
  saveError.value = '';
  drawerOpen.value = true;
}
function viewQuestion(question: IqQuestion) {
  if (busy.value) return;
  viewingQuestionId.value = question.id;
  drawerMode.value = 'view';
  saveError.value = '';
  drawerOpen.value = true;
}

async function save(input: { content: string; referenceAnswer: string | null }) {
  if (saving.value || !drawerOpen.value || drawerMode.value !== 'edit') return;
  saving.value = true;
  saveError.value = '';
  try {
    const questionId = editingQuestion.value?.id;
    const latestQuestion = store.questions.find((question) => question.id === questionId);
    if (questionId && !latestQuestion) throw new Error('题目已不存在，请刷新题库');
    await store.saveQuestion({ ...input, enabled: latestQuestion?.enabled ?? true }, questionId);
    drawerOpen.value = false;
  } catch (cause) {
    saveError.value = cause instanceof Error ? cause.message : '请重试';
  } finally {
    saving.value = false;
  }
}

async function setEnabled(question: IqQuestion, enabled: boolean) {
  if (busy.value || store.loading) return;
  pendingId.value = question.id;
  actionError.value = '';
  try {
    await store.setQuestionEnabled(question.id, enabled);
  } catch (cause) {
    actionError.value = cause instanceof Error ? cause.message : '更新题目状态失败';
  } finally {
    pendingId.value = '';
  }
}

async function remove(question: IqQuestion) {
  if (busy.value || store.loading) return;
  pendingId.value = question.id;
  actionError.value = '';
  try {
    await store.deleteQuestion(question.id);
  } catch (cause) {
    actionError.value = cause instanceof Error ? cause.message : '删除题目失败';
  } finally {
    pendingId.value = '';
  }
}

onMounted(async () => {
  await store.connect();
  await store.refresh();
});
</script>

<template>
  <div class="flex h-full min-h-0 min-w-0 flex-col overflow-hidden p-6">
    <div class="mb-4 flex-none border-b border-rp-line pb-4">
      <div class="flex items-center gap-2">
        <a-input
          v-model:value="keyword"
          type="search"
          allow-clear
          class="min-w-0 w-full! max-w-80!"
          placeholder="搜索题目内容"
          aria-label="搜索题目内容"
        >
          <template #prefix><AppIcon name="SearchOutlined" class="text-rp-tertiary" /></template>
        </a-input>
        <a-select
          v-model:value="status"
          :options="statusOptions"
          aria-label="题目状态筛选"
          class="w-32! shrink-0"
        />
        <div class="ml-auto flex shrink-0 items-center gap-2">
          <a-button type="primary" ghost :disabled="busy" @click="openEditor()">
            <template #icon><AppIcon name="PlusOutlined" /></template>
            新建题目
          </a-button>
        </div>
      </div>
    </div>
    <a-alert
      v-if="store.error || actionError"
      class="mb-4 flex-none"
      type="error"
      :title="store.error ? '题库读取失败' : '操作失败'"
      :description="store.error || actionError"
      show-icon
      role="alert"
    >
      <template v-if="store.error" #action>
        <a-button size="small" :loading="store.loading" @click="store.refresh()">重试</a-button>
      </template>
    </a-alert>
    <section
      ref="tableViewport"
      aria-label="IQ 题库列表"
      :aria-busy="store.loading"
      class="min-h-0 min-w-0 flex-1 overflow-auto"
    >
      <a-table
        v-if="!store.error || store.questions.length"
        :columns="columns"
        :data-source="visibleQuestions"
        :loading="store.loading"
        :pagination="false"
        :scroll="{ x: 600 }"
        :sticky="{ getContainer: getScrollContainer }"
        size="small"
        row-key="id"
        table-layout="fixed"
      >
        <template #bodyCell="{ column, record }">
          <IqQuestionCell
            v-if="column.key === 'content'"
            :question="record"
            @view="viewQuestion(record)"
          />
          <a-switch
            v-else-if="column.key === 'enabled'"
            :checked="record.enabled"
            size="small"
            :loading="pendingId === record.id"
            :disabled="busy || store.loading"
            :aria-label="`${record.enabled ? '停用' : '启用'}题目 ${record.content}`"
            @change="setEnabled(record, $event as boolean)"
          />
          <div v-else-if="column.key === 'actions'" class="flex items-center justify-center gap-1">
            <a-tooltip title="编辑题目">
              <a-button
                color="primary"
                variant="text"
                shape="circle"
                :disabled="busy || store.loading"
                :aria-label="`编辑题目 ${record.content}`"
                @click="openEditor(record)"
              >
                <template #icon><AppIcon name="EditOutlined" /></template>
              </a-button>
            </a-tooltip>
            <a-popconfirm
              title="删除这道题目？"
              description="历史运行保存的题目快照不受影响。"
              ok-text="删除"
              cancel-text="取消"
              :ok-button-props="{ danger: true }"
              :disabled="busy || store.loading"
              @confirm="remove(record)"
            >
              <a-button
                type="text"
                shape="circle"
                danger
                :disabled="busy || store.loading"
                :aria-label="`删除题目 ${record.content}`"
              >
                <template #icon><AppIcon name="DeleteOutlined" /></template>
              </a-button>
            </a-popconfirm>
          </div>
        </template>
        <template #emptyText>
          <a-empty class="py-8" :description="emptyDescription">
            <a-button v-if="!store.questions.length" type="primary" @click="openEditor()">
              新建第一道题目
            </a-button>
            <div v-else class="flex justify-center gap-2">
              <a-button v-if="normalizedKeyword" @click="keyword = ''">清空搜索</a-button>
              <a-button @click="resetFilters">查看全部题目</a-button>
            </div>
          </a-empty>
        </template>
      </a-table>
    </section>
    <div
      v-if="!store.error"
      class="flex min-w-0 flex-none flex-wrap items-center gap-2 pt-4 sm:justify-end"
    >
      <div class="min-w-0 max-w-full overflow-x-auto pb-1">
        <a-pagination
          :current="page"
          :page-size="pageSize"
          :page-size-options="['10', '20', '50', '100']"
          show-size-changer
          :show-quick-jumper="false"
          :show-total="showTotal"
          size="small"
          :total="filteredQuestions.length"
          :disabled="store.loading || busy"
          @change="changePage"
        />
      </div>
    </div>
    <IqQuestionDrawer
      v-if="drawerOpen"
      :mode="drawerMode"
      :question="drawerQuestion"
      :saving="saving"
      :error="saveError"
      @close="!saving && (drawerOpen = false)"
      @save="save"
    />
  </div>
</template>
