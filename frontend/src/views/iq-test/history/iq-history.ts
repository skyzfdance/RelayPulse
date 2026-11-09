import type { IqCallResult, IqRun } from '@/types/domain/iq-test';

export type IqHistoryResult = IqCallResult['value'];

export interface IqHistoryResultSummary extends IqCallResult {
  label: string;
  color: 'success' | 'warning' | 'error';
}

const resultLabels: Record<IqHistoryResult, string> = {
  'all-success': '全部成功',
  'partial-failure': '部分失败',
  'all-failure': '全部失败',
  aborted: '已中止',
};
const resultColors: Record<IqHistoryResult, IqHistoryResultSummary['color']> = {
  'all-success': 'success',
  'partial-failure': 'warning',
  'all-failure': 'error',
  aborted: 'warning',
};

/** 展示后端返回的整场调用汇总；前端仅补充标签和颜色。 */
export function getIqHistoryResult(run: Pick<IqRun, 'summary'>): IqHistoryResultSummary {
  const result = run.summary.callResult;
  return { ...result, label: resultLabels[result.value], color: resultColors[result.value] };
}

export interface IqHistoryFilters {
  startAtMs: number | null;
  endAtMs: number | null;
  participants: string[][];
  result: IqHistoryResult | '';
}

/** 级联路径在同一参与者上匹配；筛选保留整场快照和调用结果。 */
export function filterIqHistory(runs: IqRun[], filters: IqHistoryFilters): IqRun[] {
  return runs.filter((run) => {
    if (run.status === 'running') return false;
    if (filters.startAtMs !== null && run.startedAtMs < filters.startAtMs) return false;
    if (filters.endAtMs !== null && run.startedAtMs > filters.endAtMs) return false;
    if (filters.result && run.summary.callResult.value !== filters.result) return false;
    return (
      !filters.participants.length ||
      filters.participants.some(([siteId, modelKey]) =>
        run.participants.some(
          (participant) =>
            participant.siteId === siteId &&
            (modelKey === undefined || participant.modelKey === modelKey),
        ),
      )
    );
  });
}
