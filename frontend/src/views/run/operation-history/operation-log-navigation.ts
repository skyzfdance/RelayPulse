import type { OperationLogPage } from '@/types/domain/operation-logs';

interface SavedView {
  page: OperationLogPage;
  scrollTop: number;
}

// Only retain a view while following its probe-record link, then consume it on return.
const savedViews = new Map<string, SavedView>();

export function saveOperationLogView(key: string, view: SavedView) {
  if (savedViews.size >= 8) savedViews.delete(savedViews.keys().next().value!);
  savedViews.set(key, view);
}

export function takeOperationLogView(key: string) {
  const view = savedViews.get(key);
  savedViews.delete(key);
  return view;
}
