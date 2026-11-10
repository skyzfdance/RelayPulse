import type { StartupMigrationResult, StartupStatus } from '@/types/domain/startup';

import { callBackend } from './call-backend';

export function fetchStartupStatus(): Promise<StartupStatus> {
  return callBackend<StartupStatus>('get_startup_status', undefined, {
    successFeedback: 'none',
    failFeedback: 'none',
  });
}

export function migrateStartupData(): Promise<StartupMigrationResult> {
  return callBackend<StartupMigrationResult>('migrate_startup_data', undefined, {
    successFeedback: 'none',
    failFeedback: 'none',
  });
}

export function restartAfterDataMigration(): Promise<void> {
  return callBackend<void>('restart_after_data_migration', undefined, {
    successFeedback: 'none',
    failFeedback: 'none',
  });
}
