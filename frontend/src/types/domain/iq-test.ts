import type { RequestParameter, SiteProtocol } from './sites';

export interface IqQuestionInput {
  content: string;
  referenceAnswer: string | null;
  enabled: boolean;
}
export interface IqQuestion extends IqQuestionInput {
  id: string;
  createdAtMs: number;
  updatedAtMs: number;
}
export interface IqCandidate {
  modelKey: string;
  modelName: string;
  siteId: string;
  siteName: string;
  protocol: SiteProtocol;
  health: string;
}
/** 历史站点与模型筛选目录；value 使用历史快照中的 siteId / modelKey。 */
export interface IqHistoryFilterOption {
  value: string;
  label: string;
  children?: IqHistoryFilterOption[];
}
export interface IqParticipantInput {
  modelKey: string;
  preset: string;
  headers: RequestParameter[];
  /** 仅包含独立配置的 IQ 参数；不携带探针策略或凭据。 */
  parameters: Record<string, unknown>;
}
export interface IqPrecheck {
  status: 'checking' | 'available' | 'unavailable';
  message: string;
  checkedAtMs: number | null;
}
export interface IqParticipant extends IqCandidate {
  id: string;
  snapshotId: string;
  endpoint: string;
  createdAtMs: number;
  preset: string;
  headers: RequestParameter[];
  parameters: Record<string, unknown>;
  precheck: IqPrecheck;
}
export type IqEntryStatus = 'pending' | 'running' | 'completed' | 'error' | 'timeout' | 'aborted';
export interface IqEntry {
  id: string;
  runId: string;
  questionIndex: number;
  participantIndex: number;
  status: IqEntryStatus;
  attempt: number;
  startedAtMs: number | null;
  finishedAtMs: number | null;
  firstTokenMs: number | null;
  totalTimeMs: number | null;
  inputTokens: number | null;
  outputTokens: number | null;
  errorCode: string | null;
  errorMessage: string | null;
  hasRawStream: boolean;
}
/** 后端按整场调用汇总；仅表示调用结果，不评价答案正确性。 */
export interface IqCallResult {
  value: 'all-success' | 'partial-failure' | 'all-failure' | 'aborted';
  success: number;
  failed: number;
  timeout: number;
  aborted: number;
  notStarted: number;
}
export interface IqRunSummary {
  callResult: IqCallResult;
  totalEntries: number;
  terminalEntries: number;
  successfulEntries: number;
  progressPercent: number;
  currentQuestion: number;
  totalQuestions: number;
  totalParticipants: number;
  durationMs: number;
  questionProgress: {
    questionIndex: number;
    terminalEntries: number;
    totalEntries: number;
    status: 'pending' | 'running' | 'completed' | 'aborted';
  }[];
}
export interface IqRun {
  id: string;
  status: 'running' | 'completed' | 'aborted';
  questions: IqQuestion[];
  participants: IqParticipant[];
  entries: IqEntry[];
  startedAtMs: number;
  finishedAtMs: number | null;
  summary: IqRunSummary;
}
export interface IqStartInput {
  questionIds: string[];
  retryMax: number;
}
/** IQ 独立状态通知，只携带递增版本。监听方补取快照，不传输输出正文。 */
export interface IqStateChanged {
  revision: number;
}
export interface IqSnapshot extends IqStateChanged {
  questions: IqQuestion[];
  questionSummary: { total: number; enabled: number };
  candidates: IqCandidate[];
  participants: IqParticipant[];
  activeRun: IqRun | null;
  runs: IqRun[];
}

export interface IqAttempt {
  attempt: number;
  status: IqEntryStatus;
  startedAtMs: number | null;
  finishedAtMs: number | null;
  firstTokenMs: number | null;
  totalTimeMs: number | null;
  statusCode: number | null;
  errorCode: string | null;
  bodyComplete: boolean;
  streamBytes: number;
  usage: { inputTokens?: number | null; outputTokens?: number | null; totalTokens?: number | null };
}
export interface IqOutputPart {
  kind: 'text' | 'reasoning' | 'tool';
  text: string;
}
export interface IqEntryOutput {
  entryId: string;
  attempt: number | null;
  attempts: IqAttempt[];
  parts: IqOutputPart[];
  encryptedReasoning: boolean;
  parsingComplete: boolean;
  rawResponse: string | null;
}
