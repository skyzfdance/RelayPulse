<script setup lang="ts">
import type {
  CustomProbeRequest,
  CustomProbeTestResult,
  ProbePolicy,
  SiteModel,
} from '@/types/domain/sites';

import { storeToRefs } from 'pinia';
import { computed, inject, onScopeDispose, ref, watch } from 'vue';
import { routerKey } from 'vue-router';

import { savePolicy, testCustomProbe } from '@/api/config/sites';
import { useSiteDetailsStore, useSiteTasksStore } from '@/stores/site';

import { customEndpointError, policyError } from '../probe-policy';

import ProbePolicyFields from './ProbePolicyFields.vue';
import AppIcon from '@/components/AppIcon.vue';
const props = defineProps<{ model: SiteModel }>();
const emit = defineEmits<{ close: [] }>();
const detailsStore = useSiteDetailsStore();
const taskStore = useSiteTasksStore();
const { selectedSiteDetail: selected } = storeToRefs(detailsStore);
const site = selected.value!;
const model = props.model;
const saving = ref(false);
const testing = ref(false);
const submitting = computed(() => saving.value || testing.value);
const disabled = computed(() => submitting.value || Boolean(taskStore.tasks[site.id]));
const router = inject(routerKey, undefined);
const removeRouteGuard = router?.beforeEach(() => !testing.value);
let disposed = false;
onScopeDispose(() => {
  disposed = true;
  removeRouteGuard?.();
});
// 每次打开都从当前已保存的模型配置新建草稿，关闭时随抽屉销毁。
const custom = ref(Boolean(model.customRequest));
const { version, headers, parameters } = model.customRequest ?? model.policy ?? site.policy;
const policy = ref<ProbePolicy>(JSON.parse(JSON.stringify({ version, headers, parameters })));
// 端点开关与参数草稿分开，切换端点时不替换 Headers / Body。
const endpoint = ref(model.customRequest?.endpoint ?? site.connection.probePath);
const activeCustomEndpoint = computed({
  get: () => (custom.value ? endpoint.value : undefined),
  set: (value) => {
    if (!disabled.value && value !== undefined) endpoint.value = value;
  },
});
const policyFields = ref<InstanceType<typeof ProbePolicyFields>>();
const error = computed(
  () =>
    (custom.value ? customEndpointError(site.baseUrl, endpoint.value) : null) ??
    policyError(policy.value),
);
const testResult = ref<CustomProbeTestResult>();
const testError = ref('');
const testTitle = computed(() => {
  const result = testResult.value;
  if (!result) return '';
  const status = result.statusCode === null ? '' : ` · HTTP ${result.statusCode}`;
  return `${result.simulated ? '模拟' : ''}试探${result.success ? '通过' : '失败'}${status} · 总耗时 ${(result.durationMs / 1000).toFixed(2)} s`;
});
watch(
  [custom, endpoint, policy],
  () => {
    testResult.value = undefined;
    testError.value = '';
  },
  { deep: true, flush: 'sync' },
);

async function test() {
  if (
    disposed ||
    !custom.value ||
    error.value ||
    !policyFields.value?.canSubmit ||
    disabled.value ||
    !taskStore.register(site.id, { type: 'test-model-probe', modelId: model.id })
  )
    return;
  testing.value = true;
  testResult.value = undefined;
  testError.value = '';
  const input: CustomProbeRequest = {
    ...JSON.parse(JSON.stringify(policy.value)),
    endpoint: endpoint.value,
  };
  try {
    const result = await testCustomProbe(site.id, model.id, input);
    if (!disposed) testResult.value = result;
  } catch (cause) {
    if (!disposed) testError.value = cause instanceof Error ? cause.message : '试探失败，请重试';
  } finally {
    taskStore.remove(site.id);
    testing.value = false;
  }
}

async function save() {
  if (
    disposed ||
    error.value ||
    !policyFields.value?.canSubmit ||
    disabled.value ||
    !taskStore.register(site.id, { type: 'save-model-policy' })
  )
    return;
  saving.value = true;
  try {
    await policyFields.value?.validate();
    if (disposed) return;
    const draft: ProbePolicy = JSON.parse(JSON.stringify(policy.value));
    const saved = await savePolicy(
      site.id,
      model.id,
      custom.value ? null : draft,
      custom.value ? { ...draft, endpoint: endpoint.value } : null,
    );
    detailsStore.setSiteDetail(saved);
    if (!disposed) emit('close');
  } catch {
    // 校验与 API 分别提示；失败保留本次编辑内容。
  } finally {
    taskStore.remove(site.id);
    saving.value = false;
  }
}
</script>
<template>
  <a-drawer
    open
    size="large"
    :mask-closable="false"
    :closable="{ disabled: submitting }"
    :keyboard="!submitting"
    @close="!submitting && emit('close')"
  >
    <template #title>
      <div>
        <strong>模型探针设置</strong>
        <div class="mt-0.5 text-xs font-normal text-rp-tertiary">
          {{ site.name }} · {{ model.name }}
        </div>
      </div>
    </template>
    <template #extra>
      <a-space>
        <a-button :disabled="submitting" @click="!submitting && emit('close')">取消</a-button>
        <a-button
          type="primary"
          :disabled="disabled || Boolean(error) || !policyFields?.canSubmit"
          :title="policyFields?.submissionHint"
          :loading="saving"
          @click="save"
          >保存</a-button
        >
      </a-space>
    </template>
    <section class="mb-4 flex items-center justify-between gap-4 border-b border-rp-line pb-4">
      <div>
        <h3 class="m-0 text-sm leading-5.5 font-semibold">自定义探针请求</h3>
        <p class="m-0 text-xs leading-5 text-rp-tertiary">
          关闭时使用站点探测端点；开启后可单独配置探测端点
        </p>
      </div>
      <a-switch
        :checked="custom"
        :disabled="disabled"
        size="small"
        aria-label="使用自定义探针请求"
        @update:checked="!disabled && (custom = $event as boolean)"
      />
    </section>
    <ProbePolicyFields
      ref="policyFields"
      v-model="policy"
      v-model:custom-endpoint="activeCustomEndpoint"
      :protocol="site.protocol"
      :model-name="model.name"
      :base-url="site.baseUrl"
      :probe-path="site.connection.probePath"
      :disabled="disabled"
      @change="
        testResult = undefined;
        testError = '';
      "
    >
      <template #preview-extra>
        <a-button
          v-if="custom"
          :disabled="disabled || Boolean(error) || !policyFields?.canSubmit"
          :title="policyFields?.submissionHint"
          :loading="testing"
          @click="test"
        >
          <template #icon><AppIcon name="ExperimentOutlined" /></template>试探一次
        </a-button>
      </template>
      <template #preview-result>
        <a-alert
          v-if="custom && testResult"
          class="mt-3"
          :type="testResult.success ? 'success' : 'error'"
          :title="testTitle"
          :description="testResult.message"
          show-icon
        />
        <a-alert
          v-else-if="custom && testError"
          class="mt-3"
          type="error"
          title="试探失败"
          :description="testError"
          show-icon
        />
      </template>
    </ProbePolicyFields>
  </a-drawer>
</template>
