export const operationCategories = [
  'app_lifecycle',
  'configuration_change',
  'data_collection',
  'diagnostics_maintenance',
] as const;

export type OperationCategory = (typeof operationCategories)[number];

export type OperationTrigger =
  'user_action' | 'automatic' | 'system_startup' | 'system_recovery' | 'system_shutdown';

export interface OperationOccurredAt {
  value: string;
  precision: 'exact' | 'approximate';
  qualifier?: 'about' | 'after';
}

export interface OperationHistoryQuery {
  cursor?: string;
  limit: number;
  startedAfter?: string;
  startedBefore?: string;
  category?: OperationCategory;
  operationQuery?: string;
}

export interface OperationRecordSummary {
  recordRef: string;
  occurredAt: OperationOccurredAt;
  category: OperationCategory;
  actionLabel: string;
}

export interface OperationHistoryPageResult {
  items: OperationRecordSummary[];
  nextCursor?: string;
  datasetVersion: string;
}

export interface ConfigurationChange {
  fieldLabel: string;
  before?: string;
  after?: string;
}

export interface ConfigurationChangeDetails {
  kind: 'configuration_change';
  changes: ConfigurationChange[];
}

export interface LifecycleDetails {
  kind: 'app_lifecycle';
  event: 'started' | 'exited' | 'abnormal_exit';
  launchMethod?: string;
  appVersion?: string;
  restoredSiteCount?: number;
  restoredModelCount?: number;
  backgroundTasksStarted?: boolean;
  settingsSaved?: boolean;
  windowStateSaved?: boolean;
  backgroundTasksStopped?: boolean;
  previousSessionStartedAt?: string;
  lastCompletedAction?: string;
  failureSource?: string;
  crashReportFound?: boolean;
}

export interface DataCollectionDetails {
  kind: 'data_collection';
  siteCount: number;
  modelCount: number;
  completedCount: number;
  failedCount: number;
  skippedCount: number;
  canOpenProbeRecords: boolean;
}

export type DiagnosticsDetails =
  | {
      kind: 'diagnostics_maintenance';
      action: 'export';
      includedCategories: string[];
      fileCount: number;
      bundleSizeLabel: string;
      locationLabel: string;
      requestBodiesExcluded: boolean;
    }
  | {
      kind: 'diagnostics_maintenance';
      action: 'cleanup';
      beforeSizeLabel: string;
      fileCount: number;
      afterSizeLabel: string;
      settingsPreserved: boolean;
    };

export type OperationRecordSpecificDetails =
  ConfigurationChangeDetails | LifecycleDetails | DataCollectionDetails | DiagnosticsDetails;

export interface OperationException {
  reason: string;
  safeCode?: string;
  lastStep?: string;
  recommendedAction?: string;
}

export interface OperationRecordDetails {
  recordRef: string;
  occurredAt: OperationOccurredAt;
  category: OperationCategory;
  trigger: OperationTrigger;
  actionLabel: string;
  description: string;
  subject?: string;
  detail: OperationRecordSpecificDetails;
  exception?: OperationException;
}

export type OperationHistoryErrorCode =
  | 'cursor_expired'
  | 'query_busy'
  | 'invalid_query'
  | 'not_found'
  | 'backend_unavailable'
  | 'unknown';

export class OperationHistoryError extends Error {
  constructor(
    readonly code: OperationHistoryErrorCode,
    message: string,
  ) {
    super(message);
    this.name = 'OperationHistoryError';
  }
}

export interface OperationHistoryDataSource {
  listRecords(query: OperationHistoryQuery): Promise<OperationHistoryPageResult>;
  getRecordDetails(recordRef: string): Promise<OperationRecordDetails>;
}
