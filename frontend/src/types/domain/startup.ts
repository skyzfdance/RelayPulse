export interface StartupStatus {
  status: 'ready' | 'migration_required' | 'migrating' | 'migration_completed' | 'blocked';
  message: string;
}

export interface StartupMigrationResult {
  backupDirectory: string;
}
