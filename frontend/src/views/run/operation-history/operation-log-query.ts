import type {
  OperationActionGroup,
  OperationLogQuery,
  OperationResult,
  OperationSource,
} from '@/types/domain/operation-logs';

export const DEFAULT_OPERATION_QUERY: OperationLogQuery = {
  startDate: null,
  endDate: null,
  source: [],
  result: [],
  actions: [],
  page: 1,
  pageSize: 20,
};

const sources: OperationSource[] = ['system', 'user'];
const results: OperationResult[] = ['success', 'failure', 'cancelled', 'blocked'];

function calendarDate(value: unknown): string | null {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return null;
  const time = Date.parse(`${value}T00:00:00Z`);
  if (!Number.isFinite(time) || value.startsWith('0000')) return null;
  return new Date(time).toISOString().slice(0, 10) === value ? value : null;
}

function positiveInteger(value: unknown): number | null {
  if (typeof value !== 'number' && (typeof value !== 'string' || !/^[1-9]\d*$/.test(value)))
    return null;
  const number = Number(value);
  return Number.isSafeInteger(number) && number > 0 ? number : null;
}

function selectedValues<T extends string>(value: unknown, allowed: readonly T[]): T[] {
  let values: string[] = [];
  if (Array.isArray(value))
    values = value.flatMap((entry) => (typeof entry === 'string' ? entry.split(',') : []));
  else if (typeof value === 'string') values = value.split(',');
  return [...new Set(values.filter((entry): entry is T => allowed.includes(entry as T)))];
}

function selectedActionGroups(value: unknown): OperationActionGroup[] {
  const entries = (Array.isArray(value) ? value : [value]).flatMap((entry) =>
    typeof entry === 'string' ? entry.split(',') : [],
  );
  return [
    ...new Set(
      entries.filter(
        (v): v is string => typeof v === 'string' && /^[a-zA-Z0-9_.-]{1,200}$/.test(v),
      ),
    ),
  ].slice(0, 100);
}

export function parseOperationLogQuery(query: Record<string, unknown>): OperationLogQuery {
  const page = positiveInteger(query.page);
  const pageSize = positiveInteger(query.pageSize);
  let startDate = calendarDate(query.startDate);
  let endDate = calendarDate(query.endDate);
  if (startDate && endDate && startDate > endDate) [startDate, endDate] = [endDate, startDate];
  return {
    startDate,
    endDate,
    source: selectedValues(query.source, sources),
    result: selectedValues(query.result, results),
    actions: selectedActionGroups(query.actions),
    page: page ?? 1,
    pageSize:
      pageSize !== null && [10, 20, 50, 100].includes(pageSize)
        ? pageSize
        : DEFAULT_OPERATION_QUERY.pageSize,
  };
}

export function serializeOperationLogQuery(query: OperationLogQuery): Record<string, string> {
  return {
    ...(query.startDate ? { startDate: query.startDate } : {}),
    ...(query.endDate ? { endDate: query.endDate } : {}),
    ...(query.source.length ? { source: query.source.join(',') } : {}),
    ...(query.result.length ? { result: query.result.join(',') } : {}),
    ...(query.actions.length ? { actions: query.actions.join(',') } : {}),
    page: String(query.page),
    pageSize: String(query.pageSize),
  };
}
