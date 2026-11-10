<script setup lang="ts">
import type { StartupStatus } from '@/types/domain/startup';

import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

import { fetchStartupStatus, migrateStartupData, restartAfterDataMigration } from '@/api/startup';
import { backendError } from '@/utils/backend-error';

const startup = ref<StartupStatus>();
const checking = ref(true);
const migrating = ref(false);
const restarting = ref(false);
const error = ref('');
const backupDirectory = ref('');
let disposed = false;
let pollTimer: ReturnType<typeof setTimeout> | undefined;
const busy = computed(
  () => migrating.value || restarting.value || startup.value?.status === 'migrating',
);
const migrationAvailable = computed(() => startup.value?.status === 'migration_required');
const migrated = computed(() => startup.value?.status === 'migration_completed');
const title = computed(() => {
  if (checking.value) return '正在检查启动状态';
  if (migrating.value || startup.value?.status === 'migrating') return '正在备份并迁移数据';
  if (restarting.value) return '迁移完成，正在重启应用';
  if (migrated.value) return '数据迁移已完成';
  if (migrationAvailable.value) return '需要迁移旧版本数据';
  return '应用暂时无法启动';
});

async function check() {
  clearTimeout(pollTimer);
  checking.value = true;
  error.value = '';
  try {
    const result = await fetchStartupStatus();
    if (disposed) return;
    startup.value = result;
    if (result.status === 'migrating') {
      pollTimer = setTimeout(() => void check(), 1000);
    } else if (result.status === 'migration_completed') {
      await restart();
    }
  } catch (cause) {
    if (!disposed) error.value = backendError(cause).message;
  } finally {
    checking.value = false;
  }
}
async function restart() {
  if (disposed || restarting.value) return;
  restarting.value = true;
  error.value = '';
  try {
    await restartAfterDataMigration();
  } catch (cause) {
    restarting.value = false;
    error.value = backendError(cause).message;
  }
}
async function migrate() {
  if (busy.value || !migrationAvailable.value) return;
  migrating.value = true;
  error.value = '';
  try {
    const result = await migrateStartupData();
    if (disposed) return;
    backupDirectory.value = result.backupDirectory;
    startup.value = { status: 'migration_completed', message: '旧数据已完整保留为备份。' };
  } catch (cause) {
    error.value = backendError(cause).message;
    return;
  } finally {
    migrating.value = false;
  }
  await restart();
}
onMounted(check);
onBeforeUnmount(() => {
  disposed = true;
  clearTimeout(pollTimer);
});
</script>

<template>
  <slot v-if="startup?.status === 'ready'" />
  <main
    v-else
    class="flex min-h-screen items-center justify-center bg-rp-canvas p-6"
    aria-label="应用启动"
  >
    <div class="w-full max-w-2xl">
      <a-result
        :status="checking || busy ? 'info' : migrationAvailable || migrated ? 'warning' : 'error'"
        :title="title"
        :sub-title="startup?.message"
      >
        <template #extra>
          <div class="flex flex-col items-center gap-4">
            <p v-if="busy" role="status" class="m-0 text-rp-secondary">
              请保持应用运行，完成后会自动重启。
            </p>
            <a-alert v-if="error" type="error" show-icon :title="error" class="w-full text-left" />
            <p
              v-if="backupDirectory"
              class="m-0 w-full break-all text-left text-sm text-rp-secondary"
            >
              旧数据备份：{{ backupDirectory }}
            </p>
            <a-button
              v-if="migrationAvailable || migrating"
              type="primary"
              :loading="busy"
              @click="migrate"
              >备份并迁移</a-button
            >
            <a-button v-else-if="migrated" type="primary" :loading="restarting" @click="restart"
              >重启应用</a-button
            >
            <a-button v-else :loading="checking || busy" @click="check">重新检查</a-button>
          </div>
        </template>
      </a-result>
    </div>
  </main>
</template>
