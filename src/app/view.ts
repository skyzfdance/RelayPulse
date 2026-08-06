import { viewNames, type ViewName } from '../domain/types';
import { recordFrontendAction } from './frontend-diagnostics';

const trayViews = new Set<ViewName>(['tray-healthy', 'tray-alerts']);

export function parseView(search: string): ViewName {
  const value = new URLSearchParams(search).get('view');
  return viewNames.find((view) => view === value) ?? 'sites';
}

export function navigateToView(view: ViewName) {
  const currentView = parseView(window.location.search);
  if (currentView !== view) {
    recordFrontendAction('navigate', {
      destinationView: view,
      targetType: 'view',
      targetId: view,
    });
  }
  const url = new URL(window.location.href);
  url.searchParams.set('view', view);
  window.history.pushState({ view }, '', url);
  window.dispatchEvent(new PopStateEvent('popstate', { state: { view } }));
}

export function isViewAllowedForWindow(windowLabel: string | null, view: ViewName) {
  if (windowLabel === 'status-panel') return trayViews.has(view);
  if (windowLabel === 'main') return !trayViews.has(view);
  return true;
}

export function defaultViewForWindow(windowLabel: string | null): ViewName {
  return windowLabel === 'status-panel' ? 'tray-healthy' : 'sites';
}
