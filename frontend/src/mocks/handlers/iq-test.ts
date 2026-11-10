/** IQ 浏览器夹具：只保留会话内脱敏数据，不执行网络请求或文件系统操作。 */
import type {
  IqCandidate,
  IqEntry,
  IqHistoryFilterOption,
  IqParticipant,
  IqParticipantInput,
  IqQuestion,
  IqQuestionInput,
  IqRun,
  IqSnapshot,
  IqStartInput,
} from '@/types/domain/iq-test';

import { publish } from '@/api/events';
import { IQ_STATE_CHANGED } from '@/api/iq-test/events';
import { RelayPulseBackendError } from '@/utils/backend-error';

import {
  requestMockIqCompletionNotification,
  resetMockIqSystemNotificationRequests,
} from './config/notifications';
import { state as sitesState } from './config/sites';
import { summarizeIqCallResult } from './iq-test-history';
import { registerMockHandler } from './registry';
import { appendMockIqOperation } from './run/operation-logs';

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value));
const id = () => crypto.randomUUID();
const scenario = () => new URLSearchParams(window.location.search).get('iqScenario');
const terminal = (entry: IqEntry) => entry.status !== 'pending' && entry.status !== 'running';
const precheckCount = new Map<string, number>();
const snapshotAvailability = new Map<string, boolean>();
const runScenarios = new Map<string, { scenario: string | null; retryMax: number }>();
let timer: ReturnType<typeof setInterval> | null = null;
let questionTicks = 0;

type FixtureOutcome =
  'completed' | 'upstream-error' | 'timeout' | 'aborted-running' | 'aborted-pending';

const historicalQuestions: IqQuestion[] = [
  {
    id: 'iq-fixture-question-logic',
    content: '有三只盒子，标签全部贴错，只允许取出一个水果，如何确定每只盒子的内容？',
    referenceAnswer: '从标着“混合”的盒子取出一个水果，再据此排除另外两盒。',
    enabled: true,
    createdAtMs: Date.UTC(2026, 9, 1),
    updatedAtMs: Date.UTC(2026, 9, 1),
  },
  {
    id: 'iq-fixture-question-design',
    content: '设计一个并发写入下仍保持幂等的支付回调流程，并说明失败恢复边界。',
    referenceAnswer: null,
    enabled: true,
    createdAtMs: Date.UTC(2026, 9, 1),
    updatedAtMs: Date.UTC(2026, 9, 1),
  },
  {
    id: 'iq-fixture-question-sequence',
    content: '数列为 2、6、12、20、30，下一项是什么？请说明规律。',
    referenceAnswer: '42；第 n 项为 n(n+1)。',
    enabled: true,
    createdAtMs: Date.UTC(2026, 9, 1),
    updatedAtMs: Date.UTC(2026, 9, 1),
  },
];

const historicalParticipants: IqParticipant[] = [
  {
    id: 'iq-fixture-participant-gpt',
    snapshotId: 'iq-fixture-snapshot-gpt',
    modelKey: 'site-1/model-0-0',
    modelName: 'gpt-demo',
    siteId: 'site-1',
    siteName: 'OpenAI 主站',
    protocol: 'openai-compatible',
    health: 'healthy',
    endpoint: 'https://api.openai.com/v1',
    createdAtMs: Date.UTC(2026, 9, 1),
    preset: 'high',
    parameters: { reasoning_effort: 'high', max_tokens: 512 },
    precheck: { status: 'available', message: '预检通过', checkedAtMs: Date.UTC(2026, 9, 1) },
  },
  {
    id: 'iq-fixture-participant-claude',
    snapshotId: 'iq-fixture-snapshot-claude',
    modelKey: 'site-2/model-1-0',
    modelName: 'claude-sonnet-demo',
    siteId: 'site-2',
    siteName: 'Anthropic',
    protocol: 'anthropic',
    health: 'healthy',
    endpoint: 'https://api.anthropic.com/v1',
    createdAtMs: Date.UTC(2026, 9, 1),
    preset: 'medium',
    parameters: { temperature: 0.2, max_tokens: 512 },
    precheck: { status: 'available', message: '预检通过', checkedAtMs: Date.UTC(2026, 9, 1) },
  },
  {
    id: 'iq-fixture-participant-retired',
    snapshotId: 'iq-fixture-snapshot-retired',
    modelKey: 'retired-site/retired-model',
    modelName: 'legacy-model',
    siteId: 'retired-site',
    siteName: '旧备用线路',
    protocol: 'openai-compatible',
    health: 'healthy',
    endpoint: 'https://archive.example.invalid/v1',
    createdAtMs: Date.UTC(2026, 9, 1),
    preset: 'custom',
    parameters: { reasoning_effort: 'low' },
    precheck: { status: 'available', message: '预检通过', checkedAtMs: Date.UTC(2026, 9, 1) },
  },
];

const historicalRunDefinitions: {
  id: string;
  status: 'completed' | 'aborted';
  startedAtMs: number;
  durationMs: number;
  questionIndexes: number[];
  participantIndexes: number[];
  outcomes: FixtureOutcome[][];
}[] = [
  {
    id: 'iq-fixture-run-all-success',
    status: 'completed',
    startedAtMs: Date.UTC(2026, 9, 29, 9, 20),
    durationMs: 48_600,
    questionIndexes: [0, 2],
    participantIndexes: [0, 1],
    outcomes: [
      ['completed', 'completed'],
      ['completed', 'completed'],
    ],
  },
  {
    id: 'iq-fixture-run-mixed',
    status: 'completed',
    startedAtMs: Date.UTC(2026, 9, 26, 14, 10),
    durationMs: 91_300,
    questionIndexes: [0, 1, 2],
    participantIndexes: [0, 1, 2],
    outcomes: [
      ['completed', 'upstream-error', 'completed'],
      ['upstream-error', 'completed', 'completed'],
      ['completed', 'completed', 'upstream-error'],
    ],
  },
  {
    id: 'iq-fixture-run-timeout',
    status: 'completed',
    startedAtMs: Date.UTC(2026, 9, 22, 11, 35),
    durationMs: 723_400,
    questionIndexes: [1],
    participantIndexes: [0],
    outcomes: [['timeout']],
  },
  {
    id: 'iq-fixture-run-aborted',
    status: 'aborted',
    startedAtMs: Date.UTC(2026, 9, 18, 16, 5),
    durationMs: 26_800,
    questionIndexes: [0, 1, 2],
    participantIndexes: [0, 1],
    outcomes: [
      ['completed', 'completed'],
      ['aborted-running', 'aborted-running'],
      ['aborted-pending', 'aborted-pending'],
    ],
  },
];

export const iqState: {
  revision: number;
  questions: IqQuestion[];
  participants: IqParticipant[];
  runs: IqRun[];
} = { revision: 0, questions: [], participants: [], runs: [] };

function validation(message: string, code = 'iq_validation'): never {
  throw new RelayPulseBackendError({
    code,
    category: code === 'AlreadyRunning' ? 'conflict' : 'validation',
    message,
    retryable: false,
  });
}
function assertAvailable(write = false) {
  const current = scenario();
  if (current === 'error' || (write && current === 'action-error'))
    throw new RelayPulseBackendError({
      code: 'iq_unavailable',
      category: 'unavailable',
      message: '暂时无法读取或保存 IQ 数据，请重试',
      retryable: true,
    });
}
function changed() {
  iqState.revision += 1;
  void publish(IQ_STATE_CHANGED, { revision: iqState.revision });
}
function activeRun() {
  return iqState.runs.find((run) => run.status === 'running') ?? null;
}
function assertIdle() {
  if (activeRun()) validation('测试进行中，请先中止当前测试', 'AlreadyRunning');
}
function candidates(): IqCandidate[] {
  return sitesState.sites.flatMap((site) =>
    site.models
      .filter((model) => model.enabled)
      .map((model) => ({
        modelKey: `${site.id}/${model.id}`,
        modelName: model.name,
        siteId: site.id,
        siteName: site.name,
        protocol: site.protocol,
        health: model.health,
      })),
  );
}
function summarize(run: IqRun) {
  const ended = run.entries.filter(terminal);
  const currentQuestion = run.entries.find((entry) => entry.status === 'running')?.questionIndex;
  run.summary = {
    callResult: summarizeIqCallResult(run),
    totalEntries: run.entries.length,
    terminalEntries: ended.length,
    successfulEntries: run.entries.filter((entry) => entry.status === 'completed').length,
    progressPercent: Math.round((ended.length / run.entries.length) * 100),
    currentQuestion: currentQuestion === undefined ? run.questions.length : currentQuestion + 1,
    totalQuestions: run.questions.length,
    totalParticipants: run.participants.length,
    durationMs: (run.finishedAtMs ?? Date.now()) - run.startedAtMs,
    questionProgress: run.questions.map((_, questionIndex) => {
      const entries = run.entries.filter((entry) => entry.questionIndex === questionIndex);
      let status: 'pending' | 'running' | 'completed' | 'aborted' = 'pending';
      if (entries.some((entry) => entry.status === 'running')) status = 'running';
      if (entries.every(terminal))
        status = entries.some((entry) => entry.status === 'aborted') ? 'aborted' : 'completed';
      return {
        questionIndex,
        terminalEntries: entries.filter(terminal).length,
        totalEntries: entries.length,
        status,
      };
    }),
  };
}

function historicalEntry(
  runId: string,
  startedAtMs: number,
  questionIndex: number,
  participantIndex: number,
  outcome: FixtureOutcome,
): IqEntry {
  const hasStarted = outcome !== 'aborted-pending';
  let status: IqEntry['status'] = 'completed';
  if (outcome === 'upstream-error') status = 'error';
  else if (outcome === 'timeout') status = 'timeout';
  else if (outcome === 'aborted-pending' || outcome === 'aborted-running') status = 'aborted';
  const entryStartedAtMs = hasStarted
    ? startedAtMs + questionIndex * 20_000 + participantIndex * 500
    : null;
  let totalTimeMs: number | null = null;
  if (outcome === 'timeout') totalTimeMs = 720_000;
  else if (outcome === 'upstream-error') totalTimeMs = 1_400 + questionIndex * 330;
  else if (outcome === 'aborted-running') totalTimeMs = 6_800 - participantIndex * 500;
  else if (outcome === 'completed')
    totalTimeMs = 6_200 + questionIndex * 1_250 + participantIndex * 430;
  const finishedAtMs =
    entryStartedAtMs !== null && totalTimeMs !== null ? entryStartedAtMs + totalTimeMs : null;
  const completed = outcome === 'completed';
  const partialAbort = outcome === 'aborted-running' && participantIndex === 0;
  let attempt = 1;
  if (outcome === 'timeout') attempt = 6;
  else if (outcome === 'aborted-pending') attempt = 0;
  else if (outcome === 'aborted-running' && participantIndex === 0) attempt = 2;
  let firstTokenMs: number | null = null;
  let inputTokens: number | null = null;
  let outputTokens: number | null = null;
  if (completed) {
    firstTokenMs = 560 + participantIndex * 130;
    inputTokens = 36 + questionIndex * 12;
    outputTokens = 128 + participantIndex * 44;
  } else if (partialAbort) {
    firstTokenMs = 810;
    inputTokens = 22;
    outputTokens = 41;
  }
  let errorCode: string | null = null;
  let errorMessage: string | null = null;
  if (outcome === 'upstream-error') {
    errorCode = 'upstream_http_502';
    errorMessage = '上游返回 HTTP 502，调用未完成';
  } else if (outcome === 'timeout') {
    errorCode = 'output_timeout';
    errorMessage = '连续无有效输出，超时重试已耗尽';
  }

  return {
    id: `${runId}-q${questionIndex + 1}-p${participantIndex + 1}`,
    runId,
    questionIndex,
    participantIndex,
    status,
    attempt,
    startedAtMs: entryStartedAtMs,
    finishedAtMs,
    firstTokenMs,
    totalTimeMs,
    inputTokens,
    outputTokens,
    errorCode,
    errorMessage,
    hasRawStream: hasStarted,
  };
}

function createHistoricalRun(definition: (typeof historicalRunDefinitions)[number]): IqRun {
  const questions = definition.questionIndexes.map((index) => clone(historicalQuestions[index]!));
  const participants = definition.participantIndexes.map((index) =>
    clone(historicalParticipants[index]!),
  );
  const run: IqRun = {
    id: definition.id,
    status: definition.status,
    questions,
    participants,
    entries: definition.outcomes.flatMap((outcomes, questionIndex) =>
      outcomes.map((outcome, participantIndex) =>
        historicalEntry(
          definition.id,
          definition.startedAtMs,
          questionIndex,
          participantIndex,
          outcome,
        ),
      ),
    ),
    startedAtMs: definition.startedAtMs,
    finishedAtMs: definition.startedAtMs + definition.durationMs,
    summary: {} as IqRun['summary'],
  };
  summarize(run);
  return run;
}

export function seedDefaultIqHistory() {
  if (timer) clearInterval(timer);
  timer = null;
  questionTicks = 0;
  runScenarios.clear();
  iqState.runs = historicalRunDefinitions.map(createHistoricalRun);
}

function snapshot(): IqSnapshot {
  assertAvailable();
  iqState.runs.forEach(summarize);
  return clone({
    revision: iqState.revision,
    questions: iqState.questions,
    questionSummary: {
      total: iqState.questions.length,
      enabled: iqState.questions.filter((item) => item.enabled).length,
    },
    participants: iqState.participants,
    candidates: candidates(),
    activeRun: activeRun(),
    runs: iqState.runs.filter((run) => run.status !== 'running'),
  });
}
function historyFilterOptions(): IqHistoryFilterOption[] {
  assertAvailable();
  const sites = new Map<string, { label: string; models: Map<string, IqHistoryFilterOption> }>();
  // 同一身份出现过改名时，使用最近一次终态历史快照的显示名。
  const runs = iqState.runs
    .filter((run) => run.status !== 'running')
    .sort((left, right) => right.startedAtMs - left.startedAtMs || left.id.localeCompare(right.id));
  for (const run of runs) {
    for (const participant of run.participants) {
      let site = sites.get(participant.siteId);
      if (!site) {
        site = { label: participant.siteName, models: new Map() };
        sites.set(participant.siteId, site);
      }
      if (!site.models.has(participant.modelKey)) {
        site.models.set(participant.modelKey, {
          value: participant.modelKey,
          label: participant.modelName,
        });
      }
    }
  }
  const compare = (left: IqHistoryFilterOption, right: IqHistoryFilterOption) =>
    left.label.localeCompare(right.label, 'zh-CN') || left.value.localeCompare(right.value);
  return Array.from(sites, ([value, site]) => ({
    value,
    label: site.label,
    children: Array.from(site.models.values()).sort(compare),
  })).sort(compare);
}
function question(id: string) {
  const found = iqState.questions.find((item) => item.id === id);
  if (!found) validation('题目已不存在，请刷新题库');
  return found;
}
function participant(id: string) {
  const found = iqState.participants.find((item) => item.id === id);
  if (!found) validation('参与者已移除');
  return found;
}
function validQuestion(input: IqQuestionInput) {
  if (
    typeof input.content !== 'string' ||
    !input.content.trim() ||
    input.content.trim().length > 4096
  )
    validation('题目内容必须为 1–4096 个字符');
  if (
    input.referenceAnswer !== null &&
    (typeof input.referenceAnswer !== 'string' || input.referenceAnswer.length > 4096)
  )
    validation('参考答案最多 4096 个字符');
  if (typeof input.enabled !== 'boolean') validation('题目启用状态无效');
  return {
    content: input.content.trim(),
    referenceAnswer: input.referenceAnswer?.trim() || null,
    enabled: input.enabled,
  };
}
function validParameters(parameters: Record<string, unknown>) {
  if (!parameters || typeof parameters !== 'object' || Array.isArray(parameters))
    validation('IQ 请求参数必须为对象');
  for (const key of Object.keys(parameters)) {
    if (
      [
        'model',
        'stream',
        'messages',
        'input',
        'prompt',
        'system',
        '__proto__',
        'constructor',
        'prototype',
      ].includes(key.split('.')[0])
    )
      validation(`IQ 请求参数不能覆盖系统保留字段：${key}`);
  }
  function checkNested(value: unknown): void {
    if (value && typeof value === 'object') {
      for (const [key, child] of Object.entries(value)) {
        if (
          key
            .split('.')
            .some((segment) =>
              ['__proto__', 'constructor', 'prototype'].includes(segment.toLowerCase()),
            )
        )
          validation('请求参数含有不安全的对象路径');
        checkNested(child);
      }
    }
  }
  checkNested(parameters);
}
function buildParticipant(input: IqParticipantInput, participantId: string = id()): IqParticipant {
  validParameters(input.parameters);
  const candidate = candidates().find((item) => item.modelKey === input.modelKey);
  if (!candidate) validation('源模型已删除或停用，请刷新候选模型');
  const site = sitesState.sites.find((item) => item.id === candidate.siteId)!;
  // 仅取连接地址的脱敏投影；不读取 policy、customRequest、请求头或真实凭据。
  let endpoint = '已脱敏地址';
  try {
    const url = new URL(site.baseUrl);
    endpoint = `${url.origin}${url.pathname}`;
  } catch {
    /* 不回显无法解析的原始地址 */
  }
  const snapshotId = id();
  snapshotAvailability.set(snapshotId, site.credentialConfigured);
  return {
    ...candidate,
    id: participantId,
    snapshotId,
    endpoint,
    createdAtMs: Date.now(),
    preset: input.preset,
    parameters: clone(input.parameters),
    precheck: { status: 'checking', message: '', checkedAtMs: null },
  };
}
function startQuestion(run: IqRun, questionIndex: number) {
  questionTicks = 0;
  for (const entry of run.entries.filter((entry) => entry.questionIndex === questionIndex)) {
    entry.status = 'running';
    entry.attempt = 1;
    entry.startedAtMs = Date.now();
  }
}
function finishEntry(entry: IqEntry, status: 'completed' | 'error' | 'timeout') {
  entry.status = status;
  entry.finishedAtMs = Date.now();
  entry.totalTimeMs = entry.finishedAtMs - entry.startedAtMs!;
  entry.hasRawStream = true;
  if (status === 'completed') {
    entry.firstTokenMs = 680 + entry.participantIndex * 120;
    entry.inputTokens = 42;
    entry.outputTokens = 384;
  }
  if (status === 'error') {
    entry.errorCode = 'upstream_error';
    entry.errorMessage = '模拟上游 HTTP 502：请求未完成';
  }
  if (status === 'timeout') {
    entry.errorCode = 'output_timeout';
    entry.errorMessage = '模拟连续无有效输出，超时重试已耗尽';
  }
}
export function advanceIqRun() {
  const run = activeRun();
  if (!run) return;
  const runScenario = runScenarios.get(run.id);
  const current = run.entries.find((entry) => entry.status === 'running')?.questionIndex;
  if (current === undefined) return;
  questionTicks += 1;
  for (const entry of run.entries.filter(
    (entry) => entry.questionIndex === current && entry.status === 'running',
  )) {
    if (questionTicks >= 3 + entry.participantIndex * 2 + (entry.attempt - 1) * 2) {
      const failed = runScenario?.scenario === 'run-error' && entry.participantIndex === 0;
      if (failed && entry.attempt <= (runScenario?.retryMax ?? 0)) {
        entry.attempt += 1;
        entry.startedAtMs = Date.now();
        entry.errorCode = 'retrying_request';
        entry.errorMessage = `请求失败，正在第 ${entry.attempt - 1} / ${runScenario!.retryMax} 次重试`;
      } else finishEntry(entry, failed ? 'error' : 'completed');
    }
  }
  if (run.entries.filter((entry) => entry.questionIndex === current).every(terminal)) {
    if (current + 1 < run.questions.length) startQuestion(run, current + 1);
    else {
      run.status = 'completed';
      iqState.participants = [];
      run.finishedAtMs = Date.now();
      if (timer) clearInterval(timer);
      timer = null;
      summarize(run);
      changed();
      requestMockIqCompletionNotification(run.id);
      return;
    }
  }
  summarize(run);
  changed();
}
export function seedIqState(empty = false) {
  resetMockIqSystemNotificationRequests();
  if (timer) clearInterval(timer);
  timer = null;
  questionTicks = 0;
  precheckCount.clear();
  snapshotAvailability.clear();
  runScenarios.clear();
  iqState.revision = 0;
  iqState.participants = [];
  iqState.runs = [];
  const now = Date.now();
  iqState.questions = empty
    ? []
    : [
        [
          '有三只盒子，分别标着“苹果”“橙子”“混合”，但标签全部贴错。只允许取出一个水果，你如何确定每只盒子的内容？',
          '从“混合”盒子取一个水果，据此排除并确定剩余两盒。',
          true,
        ],
        [
          '请设计一个在并发写入下仍能保证幂等的支付回调处理流程，说明失败恢复与边界条件。',
          null,
          true,
        ],
        [
          '一个数列依次为 2、6、12、20、30，下一项是什么？解释你的推理过程。',
          '42；第 n 项为 n(n+1)。',
          true,
        ],
        ['证明：任意六个人中，必有三个人互相认识或三个人互相不认识。', null, false],
        ['17 × 19 等于多少？请给出一种便于心算的方法。', '323；可计算 17 × (20 − 1)。', true],
        [
          '你面前有两扇门，一扇通向出口、一扇通向陷阱。两名守卫中一人总说真话、一人总说假话，且你不知道谁是谁。只能向其中一人问一个是非问题，你会怎样找到出口？请写出问题并解释两种情况下为什么都有效。',
          '询问一名守卫：“如果我问另一名守卫左门是否通向出口，他会回答是吗？”若回答是，选右门；若回答否，选左门。',
          true,
        ],
        [
          '阅读以下 TypeScript 代码，指出边界条件并给出修正版本：\n\nfunction average(values: number[]) {\n  return values.reduce((sum, value) => sum + value, 0) / values.length;\n}\n\n请分别讨论空数组、非有限数字及调用方可预期的返回类型。',
          null,
          true,
        ],
        ['将“春眠不觉晓，处处闻啼鸟”译为自然英文，并简述你对意象和语气的取舍。', null, true],
        [
          '甲、乙、丙三人中只有一人获奖。甲说“不是我”，乙说“是丙”，丙说“乙在说谎”。若只有一人说真话，谁获奖？',
          '甲获奖。乙与丙的陈述互为否定，恰有一真；因此甲说假话。',
          false,
        ],
        [
          '一个袋子中有 3 个红球、2 个蓝球。随机取出两个且不放回，两个球同色的概率是多少？',
          '2/5；同色组合共 C(3,2)+C(2,2)=4 种，总组合 C(5,2)=10 种。',
          true,
        ],
        [
          '请为离线笔记应用设计增量同步协议。约束：\n1. 同一条笔记可能在两台设备离线编辑；\n2. 删除必须能够同步；\n3. 网络可重复、乱序或丢失请求；\n4. 用户应能恢复冲突版本。\n\n给出数据结构、同步消息示例和一次并发冲突的处理过程。不要假设设备时钟完全一致。',
          null,
          true,
        ],
        ['用一句话解释“相关不等于因果”，并给出一个日常反例。', null, true],
        [
          '下面的 SQL 为什么可能统计错误？请给出不会漏掉零订单用户的查询。\n\nSELECT u.id, COUNT(*)\nFROM users u\nLEFT JOIN orders o ON o.user_id = u.id\nGROUP BY u.id;',
          'LEFT JOIN 为无订单用户保留一行，COUNT(*) 会计为 1。应使用 COUNT(o.id)，并保证订单主键非空。',
          true,
        ],
        [
          '在一个只能回答“是”或“否”的系统里，要从 1000 个编号中确定唯一目标，最少需要多少次最坏情况提问？说明下界和可行策略。',
          '10 次。2^9 < 1000 ≤ 2^10，每次尽量二分候选集合即可达到下界。',
          false,
        ],
        [
          '判断下面的推理是否成立：所有会飞的动物都有翅膀；企鹅有翅膀；所以企鹅会飞。请指出逻辑形式中的问题。',
          '不成立；由 A⇒B 和 B 不能推出 A，属于肯定后件。',
          true,
        ],
        [
          '为一个面向家庭的共享日历设计“重复事件”的修改交互。需支持仅修改本次、修改本次及以后、修改整个系列，并考虑时区变化、例外事件与误操作撤销。请用具体用户旅程说明各选项的行为。',
          null,
          true,
        ],
        [
          '找出这段 Python 代码在连续调用时的行为差异，并修正：\n\ndef append_item(value, items=[]):\n    items.append(value)\n    return items\n\nprint(append_item("A"))\nprint(append_item("B"))',
          '默认列表在函数定义时创建，后续调用共享它。改用 items=None，并在函数内部创建新列表。',
          true,
        ],
        ['如果今天是星期三，100 天后是星期几？', '星期五；100 mod 7 = 2。', false],
        [
          '你负责一个每分钟接收 10 万条事件的分析系统。事件可能迟到 24 小时、重复投递或被生产者更正。请设计按天聚合的方案，讨论去重标识、时间窗口、迟到数据修正、可重放性和成本之间的取舍。',
          null,
          true,
        ],
        [
          '请审阅以下 JSON 配置，并说明如何向用户报告三个相互独立的校验错误：\n\n{\n  "timeoutSeconds": 59,\n  "retryCount": 5.5,\n  "endpoint": ""\n}\n\n要求错误可定位到字段，且不会因第一个错误而丢失其他反馈。',
          null,
          true,
        ],
        [
          '一根绳子燃烧完需要 60 分钟，但各处燃烧速度不均匀。给你两根这样的绳子和火柴，如何准确计时 45 分钟？',
          '同时点燃第一根两端和第二根一端。30 分钟后第一根烧完，点燃第二根另一端，剩余部分再烧 15 分钟。',
          true,
        ],
        [
          '有 9 枚外观相同的硬币，其中 1 枚更重。使用天平且不使用砝码，最少称几次可以保证找到它？写出分组过程。',
          '2 次。先分成三组各 3 枚，称两组确定重币所在组，再从该组取两枚比较。',
          true,
        ],
        [
          '设计一次用户访谈，以判断“搜索结果太少”究竟来自索引缺失、筛选条件过严还是用户不理解搜索范围。给出不诱导的提问、观察任务与能够区分三种解释的证据。',
          null,
          false,
        ],
        [
          '请评估一个缓存一致性方案：写数据库成功后删除缓存；读取缓存未命中时从数据库回填。\n\n请构造“旧读取覆盖新写入”的并发时间线，并分别讨论短 TTL、版本号、事务消息及重试的作用和限制。最后说明你会如何在测试中稳定复现该竞争。',
          null,
          true,
        ],
      ].map(([content, referenceAnswer, enabled], index) => ({
        id: `iq-question-${index + 1}`,
        content: content as string,
        referenceAnswer: referenceAnswer as string | null,
        enabled: enabled as boolean,
        createdAtMs: now - 86400000,
        updatedAtMs: now - index * 60000,
      }));
}
const emptyScenario = scenario() === 'empty';
seedIqState(emptyScenario);
if (!emptyScenario) seedDefaultIqHistory();

registerMockHandler('get_iq_test_snapshot', snapshot);
registerMockHandler('get_iq_history_filter_options', historyFilterOptions);
registerMockHandler('save_iq_question', (args) => {
  assertAvailable(true);
  const { input, id: questionId } = args as { input: IqQuestionInput; id?: string };
  const values = validQuestion(input);
  const now = Date.now();
  const saved = questionId
    ? Object.assign(question(questionId), values, { updatedAtMs: now })
    : { ...values, id: id(), createdAtMs: now, updatedAtMs: now };
  if (!questionId) iqState.questions.unshift(saved);
  appendMockIqOperation(
    questionId ? 'iq_question_updated' : 'iq_question_created',
    questionId ? '修改 IQ 题目' : '新建 IQ 题目',
    saved.content.slice(0, 60),
  );
  changed();
  return clone(saved);
});
registerMockHandler('delete_iq_question', (args) => {
  assertAvailable(true);
  const { id } = args as { id: string };
  const item = question(id);
  iqState.questions = iqState.questions.filter((value) => value.id !== id);
  appendMockIqOperation('iq_question_deleted', '删除 IQ 题目', item.content.slice(0, 60));
  changed();
});
registerMockHandler('set_iq_question_enabled', (args) => {
  assertAvailable(true);
  const { id, enabled } = args as { id: string; enabled: boolean };
  const item = question(id);
  item.enabled = enabled;
  item.updatedAtMs = Date.now();
  appendMockIqOperation(
    'iq_question_updated',
    enabled ? '启用 IQ 题目' : '停用 IQ 题目',
    item.content.slice(0, 60),
  );
  changed();
});
registerMockHandler('create_iq_participant_snapshot', (args) => {
  assertAvailable(true);
  assertIdle();
  const { input } = args as { input: IqParticipantInput };
  const item = buildParticipant(input);
  iqState.participants.push(item);
  changed();
  return clone(item);
});
registerMockHandler('refresh_iq_participant_snapshot', (args) => {
  assertAvailable(true);
  assertIdle();
  const { id } = args as { id: string };
  const old = participant(id);
  if (old.precheck.status !== 'unavailable') validation('仅预检失败后可以重新探测');
  const item = buildParticipant(old, old.id);
  iqState.participants.splice(iqState.participants.indexOf(old), 1, item);
  changed();
  return clone(item);
});
registerMockHandler('run_iq_precheck', async (args) => {
  const { snapshotId } = args as { snapshotId: string };
  const captured = iqState.participants.find((item) => item.snapshotId === snapshotId);
  if (!captured) validation('参与者快照已替换');
  const count = precheckCount.get(captured.modelKey) ?? 0;
  const activeScenario = scenario();
  precheckCount.set(captured.modelKey, count + 1);
  const configured = Number(
    new URLSearchParams(window.location.search).get('iqPrecheckDelay') ??
      (import.meta.env.MODE === 'test' ? 0 : 900),
  );
  if (configured > 0)
    await new Promise((resolve) => setTimeout(resolve, Math.min(configured, 10000)));
  const failed =
    !snapshotAvailability.get(snapshotId) || (activeScenario === 'precheck-failure' && count === 0);
  const precheck = {
    status: failed ? ('unavailable' as const) : ('available' as const),
    message: failed ? '模拟连接失败，请重新探测以读取最新源配置' : '当前独立快照预检通过',
    checkedAtMs: Date.now(),
  };
  const current = iqState.participants.find((item) => item.snapshotId === snapshotId);
  if (current) {
    current.precheck = precheck;
    changed();
  }
  return { snapshotId, precheck };
});
registerMockHandler('remove_iq_participant', (args) => {
  assertAvailable(true);
  assertIdle();
  const { id } = args as { id: string };
  iqState.participants = iqState.participants.filter((item) => item.id !== id);
  changed();
});
registerMockHandler('start_iq_run', (args) => {
  assertAvailable(true);
  assertIdle();
  const { input } = args as { input: IqStartInput };
  if (!Number.isInteger(input.retryMax) || input.retryMax < 0 || input.retryMax > 20)
    validation('重试次数须为 0–20 的整数');
  if (!input.questionIds.length || new Set(input.questionIds).size !== input.questionIds.length)
    validation('请至少选择一道有效题目');
  const selected = input.questionIds.map(question);
  if (selected.some((item) => !item.enabled)) validation('停用的题目不能参与测试');
  if (
    !iqState.participants.length ||
    iqState.participants.some((item) => item.precheck.status !== 'available')
  )
    validation('每位参与者的当前快照均须预检可用');
  const runId = id();
  const run: IqRun = {
    id: runId,
    status: 'running',
    questions: clone(selected),
    participants: clone(iqState.participants),
    entries: [],
    startedAtMs: Date.now(),
    finishedAtMs: null,
    summary: {} as IqRun['summary'],
  };
  run.entries = run.questions.flatMap((_, questionIndex) =>
    run.participants.map((_, participantIndex): IqEntry => ({
      id: id(),
      runId,
      questionIndex,
      participantIndex,
      status: 'pending',
      attempt: 0,
      startedAtMs: null,
      finishedAtMs: null,
      firstTokenMs: null,
      totalTimeMs: null,
      inputTokens: null,
      outputTokens: null,
      errorCode: null,
      errorMessage: null,
      hasRawStream: false,
    })),
  );
  startQuestion(run, 0);
  runScenarios.set(run.id, { scenario: scenario(), retryMax: input.retryMax });
  summarize(run);
  iqState.runs.unshift(run);
  timer = setInterval(advanceIqRun, 1000);
  appendMockIqOperation(
    'iq_run_started',
    '开始 IQ 测试',
    `${run.questions.length} 题 × ${run.participants.length} 位参与者`,
  );
  changed();
  return clone(run);
});
registerMockHandler('abort_iq_run', () => {
  assertAvailable(true);
  const run = activeRun();
  if (!run) return;
  if (timer) clearInterval(timer);
  timer = null;
  for (const entry of run.entries.filter((item) => !terminal(item))) {
    entry.status = 'aborted';
    if (entry.startedAtMs !== null) {
      entry.finishedAtMs = Date.now();
      entry.totalTimeMs = entry.finishedAtMs - entry.startedAtMs;
      entry.hasRawStream = true;
    }
  }
  run.status = 'aborted';
  iqState.participants = [];
  run.finishedAtMs = Date.now();
  summarize(run);
  appendMockIqOperation('iq_run_aborted', '中止 IQ 测试', run.id);
  changed();
});
registerMockHandler('get_iq_run_details', (args) => {
  assertAvailable();
  const { id } = args as { id: string };
  const run = iqState.runs.find((item) => item.id === id);
  if (!run) validation('运行记录已不存在或已被清理', 'iq_not_found');
  summarize(run);
  return clone(run);
});
registerMockHandler('delete_iq_runs', (args) => {
  assertAvailable(true);
  const { ids } = args as { ids: string[] };
  if (iqState.runs.some((run) => ids.includes(run.id) && run.status === 'running'))
    validation('不能删除进行中的测试');
  iqState.runs = iqState.runs.filter((run) => !ids.includes(run.id));
  appendMockIqOperation('iq_run_deleted', '删除 IQ 记录及产物', `${ids.length} 场测试`);
  changed();
});
registerMockHandler('open_iq_raw_stream', (args) => {
  assertAvailable();
  const { entryId, attempt } = args as { entryId: string; attempt?: number };
  const entry = iqState.runs.flatMap((run) => run.entries).find((item) => item.id === entryId);
  if (!entry || !entry.hasRawStream || !terminal(entry)) validation('未发起调用，暂无原始流文件');
  if (
    attempt !== undefined &&
    (!Number.isInteger(attempt) || attempt < 1 || attempt > entry.attempt)
  )
    validation('尝试次数不存在');
});

registerMockHandler('get_iq_entry_output', (args) => {
  assertAvailable();
  const { entryId, attempt } = args as { entryId: string; attempt?: number };
  const entry = iqState.runs.flatMap((run) => run.entries).find((item) => item.id === entryId);
  if (!entry || !terminal(entry)) validation('该调用不存在或尚未结束');
  if (
    attempt !== undefined &&
    (!Number.isInteger(attempt) || attempt < 1 || attempt > entry.attempt)
  )
    validation('尝试次数不存在');
  const selected = entry.attempt ? (attempt ?? entry.attempt) : null;
  const previousStatus =
    runScenarios.get(entry.runId)?.scenario === 'run-error' ? 'error' : 'timeout';
  return {
    entryId,
    attempt: selected,
    attempts: Array.from({ length: entry.attempt }, (_, index) => ({
      attempt: index + 1,
      status: index + 1 === entry.attempt ? entry.status : previousStatus,
      startedAtMs: entry.startedAtMs,
      finishedAtMs: entry.finishedAtMs,
      firstTokenMs: entry.firstTokenMs,
      totalTimeMs: entry.totalTimeMs,
      statusCode: 200,
      errorCode: entry.errorCode,
      bodyComplete: entry.status === 'completed',
      streamBytes: 200,
      usage: { inputTokens: entry.inputTokens, outputTokens: entry.outputTokens },
    })),
    parts:
      entry.status === 'completed'
        ? [
            {
              kind: 'text',
              text: '## 模拟回答\n\n这是浏览器演示数据。实际桌面回放读取本地保存的原始输出。',
            },
          ]
        : [],
    encryptedReasoning: false,
    parsingComplete: true,
    rawResponse: null,
  };
});
