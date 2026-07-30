import { viewNames, type ViewName } from '../domain/types';

export function parseView(search: string): ViewName {
  const value = new URLSearchParams(search).get('view');
  return viewNames.find((view) => view === value) ?? 'sites';
}

export function navigateToView(view: ViewName) {
  const url = new URL(window.location.href);
  url.searchParams.set('view', view);
  window.history.pushState({ view }, '', url);
  window.dispatchEvent(new PopStateEvent('popstate', { state: { view } }));
}
