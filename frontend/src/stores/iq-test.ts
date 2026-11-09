import type {
  IqCandidate,
  IqParticipant,
  IqParticipantInput,
  IqQuestion,
  IqQuestionInput,
  IqRun,
  IqSnapshot,
  IqStartInput,
  IqStateChanged,
} from '@/types/domain/iq-test';

import { defineStore } from 'pinia';
import { onScopeDispose, ref } from 'vue';

import { subscribe } from '@/api/events';
import { deleteIqQuestion, saveIqQuestion, setIqQuestionEnabled } from '@/api/iq-test/bank';
import { IQ_STATE_CHANGED } from '@/api/iq-test/events';
import { deleteIqRuns, fetchIqRun, openIqRawFile } from '@/api/iq-test/history';
import { fetchIqSnapshot } from '@/api/iq-test/snapshot';
import {
  abortIqRun,
  createIqParticipant,
  removeIqParticipant,
  replaceIqParticipant,
  runIqPrecheck,
  startIqRun,
} from '@/api/iq-test/task';

/** 共享状态由接口快照补取，跨路由保存；预检结果始终绑定 snapshotId。 */
export const useIqTestStore = defineStore('iq-test', () => {
  const questions = ref<IqQuestion[]>([]);
  const questionSummary = ref({ total: 0, enabled: 0 });
  const candidates = ref<IqCandidate[]>([]);
  const participants = ref<IqParticipant[]>([]);
  const activeRun = ref<IqRun | null>(null);
  const lastRun = ref<IqRun | null>(null);
  const runs = ref<IqRun[]>([]);
  const loading = ref(false);
  const error = ref('');
  let revision = -1;
  let queryVersion = 0;
  let requests = 0;
  let disposed = false;
  let connectPromise: Promise<void> | null = null;
  const unsubscribers: (() => void)[] = [];

  function apply(next: IqSnapshot, requestVersion: number) {
    if (disposed || next.revision < revision) return;
    // 候选源配置变化不一定推进 IQ revision。同版本查询仍按请求顺序防迟到覆盖。
    if (next.revision === revision && requestVersion !== queryVersion) return;
    revision = next.revision;
    if (activeRun.value && !next.activeRun)
      lastRun.value = next.runs.find((run) => run.id === activeRun.value!.id) ?? null;
    questions.value = next.questions;
    questionSummary.value = next.questionSummary;
    candidates.value = next.candidates;
    participants.value = next.participants.map((participant) => {
      const known = participants.value.find((item) => item.snapshotId === participant.snapshotId);
      // 单份快照的预检只完成一次；预检命令先返回时，旧查询不能将结果退回检测中。
      if (
        participant.precheck.status === 'checking' &&
        known &&
        known.precheck.status !== 'checking'
      )
        return { ...participant, precheck: known.precheck };
      return participant;
    });
    activeRun.value = next.activeRun;
    runs.value = next.runs;
  }
  async function refresh(): Promise<void> {
    const requestVersion = ++queryVersion;
    requests += 1;
    loading.value = true;
    error.value = '';
    try {
      apply(await fetchIqSnapshot(), requestVersion);
    } catch (cause) {
      if (!disposed && requestVersion === queryVersion)
        error.value = cause instanceof Error ? cause.message : 'IQ 数据读取失败';
    } finally {
      requests -= 1;
      loading.value = requests > 0;
    }
  }
  /** 订阅在 store 生命周期内常驻，切离 IQ 页面后仍同步运行看板状态。 */
  async function connect(): Promise<() => void> {
    connectPromise ??= (async () => {
      const offState = await subscribe<IqStateChanged>(IQ_STATE_CHANGED, (event) => {
        if (event.revision > revision) void refresh();
      });
      if (disposed) offState();
      else unsubscribers.push(offState);
    })();
    try {
      await connectPromise;
    } catch (cause) {
      connectPromise = null;
      error.value = cause instanceof Error ? cause.message : '无法订阅 IQ 状态';
    }
    // 页面卸载保留订阅；Pinia scope 销毁时统一释放。
    return () => undefined;
  }
  onScopeDispose(() => {
    disposed = true;
    unsubscribers.forEach((off) => off());
  });
  async function saveQuestion(input: IqQuestionInput, id?: string) {
    const result = await saveIqQuestion(input, id);
    await refresh();
    return result;
  }
  async function deleteQuestion(id: string) {
    await deleteIqQuestion(id);
    await refresh();
  }
  async function setQuestionEnabled(id: string, enabled: boolean) {
    await setIqQuestionEnabled(id, enabled);
    await refresh();
  }
  async function precheck(item: IqParticipant) {
    try {
      const result = await runIqPrecheck(item.snapshotId);
      const current = participants.value.find(
        (value) => value.id === item.id && value.snapshotId === result.snapshotId,
      );
      if (current) current.precheck = result.precheck;
    } catch (cause) {
      const current = participants.value.find(
        (value) => value.id === item.id && value.snapshotId === item.snapshotId,
      );
      if (current)
        current.precheck = {
          status: 'unavailable',
          message: cause instanceof Error ? cause.message : '预检失败',
          checkedAtMs: Date.now(),
        };
    }
  }
  async function addParticipant(input: IqParticipantInput) {
    await connect();
    const item = await createIqParticipant(input);
    await refresh();
    void precheck(item);
    return item;
  }
  async function recheckParticipant(id: string) {
    const item = await replaceIqParticipant(id);
    await refresh();
    void precheck(item);
  }
  async function removeParticipant(id: string) {
    await removeIqParticipant(id);
    await refresh();
  }
  async function start(input: IqStartInput) {
    await connect();
    const run = await startIqRun(input);
    lastRun.value = null;
    await refresh();
    return run;
  }
  async function abort() {
    await abortIqRun();
    await refresh();
  }
  async function deleteRuns(ids: string[]) {
    await deleteIqRuns(ids);
    await refresh();
  }
  const getRun = fetchIqRun;
  const openRawFile = openIqRawFile;
  function resetBoard() {
    lastRun.value = null;
  }

  return {
    questions,
    questionSummary,
    candidates,
    participants,
    activeRun,
    lastRun,
    runs,
    loading,
    error,
    refresh,
    connect,
    saveQuestion,
    deleteQuestion,
    setQuestionEnabled,
    addParticipant,
    recheckParticipant,
    removeParticipant,
    start,
    abort,
    deleteRuns,
    getRun,
    openRawFile,
    resetBoard,
  };
});
