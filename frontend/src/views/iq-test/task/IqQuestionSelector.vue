<script setup lang="ts">
import type { IqQuestion } from '@/types/domain/iq-test';
import type { TableProps } from 'antdv-next';

import { Empty } from 'antdv-next';
import { computed, ref, watch } from 'vue';

import IqQuestionCell from '@/views/iq-test/components/IqQuestionCell.vue';
import IqQuestionDrawer from '@/views/iq-test/components/IqQuestionDrawer.vue';

const props = defineProps<{ questions: IqQuestion[]; loading?: boolean }>();
const selectedIds = defineModel<string[]>('selectedIds', { required: true });
const keyword = ref('');
const normalizedKeyword = computed(() => keyword.value.trim().toLowerCase());
const page = ref(1);
const pageSize = ref(10);
const viewingQuestionId = ref('');
const viewingQuestion = computed(() =>
  props.questions.find((question) => question.id === viewingQuestionId.value),
);
const selectableIds = computed(
  () =>
    new Set(props.questions.filter((question) => question.enabled).map((question) => question.id)),
);
const filteredQuestions = computed(() =>
  props.questions.filter((question) =>
    question.content.toLowerCase().includes(normalizedKeyword.value),
  ),
);
const visibleQuestions = computed(() =>
  filteredQuestions.value.slice((page.value - 1) * pageSize.value, page.value * pageSize.value),
);
const columns: TableProps<IqQuestion>['columns'] = [{ title: '题目', key: 'content' }];
const rowSelection = computed<TableProps<IqQuestion>['rowSelection']>(() => ({
  columnWidth: 40,
  selectedRowKeys: selectedIds.value,
  preserveSelectedRowKeys: true,
  getCheckboxProps: (question) => ({
    disabled: props.loading || !question.enabled,
    'aria-label': `选择题目 ${question.content}`,
  }),
  getTitleCheckboxProps: () => ({
    disabled: props.loading || !visibleQuestions.value.some((question) => question.enabled),
    'aria-label': '全选当前页题目',
  }),
  onChange: (keys) => {
    selectedIds.value = keys.filter(
      (key): key is string => typeof key === 'string' && selectableIds.value.has(key),
    );
  },
}));
watch(
  () => filteredQuestions.value.length,
  (total) => {
    page.value = Math.min(page.value, Math.max(1, Math.ceil(total / pageSize.value)));
  },
);
watch(keyword, () => {
  page.value = 1;
});
watch(viewingQuestion, (question) => {
  if (!question) viewingQuestionId.value = '';
});
function changePage(nextPage: number, nextPageSize: number) {
  page.value = nextPageSize === pageSize.value ? nextPage : 1;
  pageSize.value = nextPageSize;
}
const showTotal = (total: number) => `共 ${total} 道`;
</script>

<template>
  <a-card title="题目选择" size="small" class="min-w-0" aria-label="测试题目选择">
    <template #extra>
      <span class="text-xs text-rp-tertiary" aria-label="已选题目数量">
        已选 {{ selectedIds.length }} 道 · 可选 {{ selectableIds.size }} 道
      </span>
    </template>
    <a-input
      v-model:value="keyword"
      type="search"
      allow-clear
      aria-label="搜索测试题目"
      placeholder="搜索题目内容"
      class="mb-3"
    />
    <a-table
      class="iq-question-table"
      :columns="columns"
      :data-source="visibleQuestions"
      :row-selection="rowSelection"
      :pagination="false"
      :loading="loading"
      :scroll="{ y: 360 }"
      size="small"
      row-key="id"
      table-layout="fixed"
    >
      <template #bodyCell="{ column, record }">
        <IqQuestionCell
          v-if="column.key === 'content'"
          :question="record"
          @view="viewingQuestionId = record.id"
        />
      </template>
      <template #emptyText>
        <a-empty
          :image="Empty.PRESENTED_IMAGE_SIMPLE"
          :description="questions.length ? '暂无数据' : '还没有测试题目'"
        >
          <a-button v-if="questions.length && normalizedKeyword" @click="keyword = ''">
            清空搜索
          </a-button>
          <RouterLink v-else-if="!questions.length" to="/iq-test/bank">前往题库管理</RouterLink>
        </a-empty>
      </template>
    </a-table>
    <div class="mt-3 min-w-0 overflow-x-auto pb-1" aria-label="题目选择分页">
      <a-pagination
        :current="page"
        :page-size="pageSize"
        :total="filteredQuestions.length"
        :page-size-options="['10', '20', '50', '100']"
        show-size-changer
        :show-quick-jumper="false"
        :show-total="showTotal"
        :disabled="loading"
        size="small"
        @change="changePage"
      />
    </div>
    <IqQuestionDrawer
      v-if="viewingQuestion"
      :question="viewingQuestion"
      mode="view"
      @close="viewingQuestionId = ''"
    />
  </a-card>
</template>

<style scoped>
.iq-question-table :deep(.ant-table-body) {
  height: 360px;
}

.iq-question-table :deep(.ant-table-placeholder > td) {
  height: 360px;
  padding-block: 0;
  vertical-align: middle;
}
</style>
