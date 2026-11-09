<script setup lang="ts">
import type { FormInstance } from 'antdv-next';

import { computed, onScopeDispose, ref } from 'vue';

const props = withDefaults(
  defineProps<{
    question?: { id: string; content: string; referenceAnswer: string | null };
    mode?: 'view' | 'edit';
    saving?: boolean;
    error?: string;
  }>(),
  { question: undefined, mode: 'edit', saving: false, error: '' },
);
const emit = defineEmits<{
  close: [];
  save: [input: { content: string; referenceAnswer: string | null }];
}>();
let disposed = false;
onScopeDispose(() => {
  disposed = true;
});
const form = ref<FormInstance>();
const validating = ref(false);
const readOnly = computed(() => props.mode === 'view');
const busy = computed(() => !readOnly.value && (props.saving || validating.value));
const title = computed(() => {
  if (readOnly.value) return '查看题目';
  return props.question ? '编辑题目' : '新建题目';
});
const draft = ref({
  content: props.question?.content ?? '',
  referenceAnswer: props.question?.referenceAnswer ?? '',
});

function update(field: keyof typeof draft.value, value: string) {
  if (!disposed && !busy.value && !readOnly.value) draft.value[field] = value;
}

async function save() {
  if (disposed || busy.value || readOnly.value || !form.value) return;
  validating.value = true;
  try {
    await form.value.validateFields();
    if (disposed) return;
    emit('save', {
      content: draft.value.content.trim(),
      referenceAnswer: draft.value.referenceAnswer.trim() || null,
    });
  } catch {
    // 表单原位显示校验结果；保存失败由父页面返回，保留草稿。
  } finally {
    validating.value = false;
  }
}
</script>

<template>
  <a-drawer
    open
    :size="620"
    :mask-closable="false"
    :closable="{ placement: 'start', disabled: busy, 'aria-label': '关闭题目抽屉' }"
    :keyboard="!busy"
    :styles="{
      wrapper: { maxWidth: '100vw' },
      header: { padding: '12px 20px' },
      body: { padding: '16px 20px' },
    }"
    @close="!busy && emit('close')"
  >
    <template #title>
      <div>
        <span class="text-base font-semibold">{{ title }}</span>
        <div class="mt-0.5 text-xs font-normal text-rp-tertiary">
          {{ readOnly ? '题目全文与参考答案' : '维护题目与参考答案' }}
        </div>
      </div>
    </template>
    <div v-if="readOnly">
      <section aria-label="题目全文">
        <h2 class="mb-2! mt-0! text-sm font-medium text-rp-secondary">题目</h2>
        <div class="text-sm leading-6 whitespace-pre-wrap wrap-anywhere">
          {{ question?.content }}
        </div>
      </section>
      <section aria-label="题目参考答案" class="mt-6">
        <h2 class="mb-2! mt-0! text-sm font-medium text-rp-secondary">参考答案</h2>
        <div
          v-if="question?.referenceAnswer"
          class="text-sm leading-6 whitespace-pre-wrap wrap-anywhere"
        >
          {{ question.referenceAnswer }}
        </div>
        <p v-else class="m-0! text-sm text-rp-tertiary">暂无参考答案</p>
      </section>
    </div>
    <a-alert
      v-if="!readOnly && error"
      class="mb-4"
      type="error"
      title="保存失败"
      :description="error"
      show-icon
      role="alert"
    />
    <a-form
      v-if="!readOnly"
      ref="form"
      :model="draft"
      :disabled="busy"
      layout="vertical"
      :validate-trigger="['change', 'blur']"
    >
      <a-form-item
        name="content"
        label="题目"
        :rules="[
          { required: true, whitespace: true, message: '请输入题目内容' },
          { max: 4096, message: '题目最多 4096 个字符' },
        ]"
      >
        <a-textarea
          :value="draft.content"
          :auto-size="{ minRows: 6, maxRows: 14 }"
          :maxlength="4096"
          show-count
          placeholder="输入要向模型提出的问题"
          aria-label="题目内容"
          @update:value="update('content', $event)"
        />
      </a-form-item>
      <a-form-item
        name="referenceAnswer"
        label="参考答案（选填）"
        extra="仅作人工比对参考，系统不会自动判分"
        :rules="[{ max: 4096, message: '参考答案最多 4096 个字符' }]"
        class="mt-8"
      >
        <a-textarea
          :value="draft.referenceAnswer"
          :auto-size="{ minRows: 4, maxRows: 12 }"
          :maxlength="4096"
          show-count
          placeholder="可填写参考答案或解题思路"
          aria-label="参考答案"
          @update:value="update('referenceAnswer', $event)"
        />
      </a-form-item>
    </a-form>
    <template v-if="!readOnly" #footer>
      <div class="flex justify-end gap-2 py-1">
        <a-button :disabled="busy" @click="!busy && emit('close')">取消</a-button>
        <a-button type="primary" :loading="busy" @click="save">保存</a-button>
      </div>
    </template>
  </a-drawer>
</template>
