import { invoke, isTauri } from '@tauri-apps/api/core';
import { viewNames, type ViewName } from '../domain/types';

export type FrontendAction =
  | 'navigate'
  | 'select-site'
  | 'open-add-model'
  | 'close-add-model'
  | 'open-probe-record'
  | 'close-probe-record'
  | 'records-mode-changed'
  | 'records-period-changed'
  | 'records-site-filter-changed'
  | 'records-model-filter-changed'
  | 'records-status-filter-changed'
  | 'records-search-changed'
  | 'records-table-changed'
  | 'refresh-probe-records'
  | 'open-clear-probe-records'
  | 'control-activated'
  | 'field-focused'
  | 'form-submitted'
  | 'invoke-backend-operation'
  | 'setting-changed'
  | 'form-became-dirty'
  | 'form-reset'
  | 'modal-cancelled'
  | 'retry-load';

export type FrontendErrorSource = 'react-render' | 'window-error' | 'unhandled-rejection';

interface FrontendActionOptions {
  destinationView?: string;
  targetType?: 'site' | 'model' | 'probe-record' | 'view' | 'form' | 'control' | 'operation';
  targetId?: string;
}

let lastActionId: string | null = null;
let installed = false;

export function beginBackendOperation(): string {
  const operationId = diagnosticId('op');
  recordFrontendAction('invoke-backend-operation', {
    targetType: 'operation',
    targetId: operationId,
  });
  lastActionId = operationId;
  return operationId;
}

export function recordFrontendAction(
  action: FrontendAction,
  options: FrontendActionOptions = {},
): string {
  const actionId = diagnosticId('ui');
  lastActionId = actionId;
  sendDiagnostic('record_frontend_action', {
    input: {
      actionId,
      action,
      view: currentView(),
      destinationView: options.destinationView,
      targetType: options.targetType,
      targetId: options.targetId,
    },
  });
  return actionId;
}

export function reportFrontendError(
  source: FrontendErrorSource,
  reason: unknown,
  additionalStack?: string | null,
): string {
  const errorId = diagnosticId('fe');
  const error = normalizeError(reason);
  const fingerprint = errorFingerprint(`${error.name}\n${error.message}\n${error.stack ?? ''}`);
  const stackFrames = sanitizeStackFrames(
    [error.stack, additionalStack].filter(Boolean).join('\n'),
  );
  sendDiagnostic('record_frontend_error', {
    input: {
      errorId,
      source,
      view: currentView(),
      errorName: sanitizeErrorName(error.name),
      fingerprint,
      stackFrames,
      lastActionId,
    },
  });
  return errorId;
}

export function installFrontendDiagnostics() {
  if (installed || typeof window === 'undefined') return;
  installed = true;
  window.addEventListener('error', (event) => {
    reportFrontendError('window-error', event.error ?? new Error(event.message));
  });
  window.addEventListener('unhandledrejection', (event) => {
    reportFrontendError('unhandled-rejection', event.reason);
  });
  window.addEventListener(
    'click',
    (event) => {
      const control = closestElement(
        event.target,
        'button,a[href],input,select,textarea,[role="button"],[role="menuitem"],[role="tab"],[role="switch"],[role="checkbox"],[role="radio"]',
      );
      if (!control) return;
      recordFrontendAction('control-activated', {
        targetType: 'control',
        targetId: controlDiagnosticTarget(control),
      });
    },
    true,
  );
  window.addEventListener(
    'focusin',
    (event) => {
      const field = closestElement(
        event.target,
        'input:not([type="button"]):not([type="submit"]):not([type="checkbox"]):not([type="radio"]),textarea,select,[contenteditable="true"]',
      );
      if (!field) return;
      recordFrontendAction('field-focused', {
        targetType: 'control',
        targetId: controlDiagnosticTarget(field),
      });
    },
    true,
  );
  window.addEventListener(
    'submit',
    (event) => {
      const form = closestElement(event.target, 'form');
      if (!form) return;
      recordFrontendAction('form-submitted', {
        targetType: 'form',
        targetId: controlDiagnosticTarget(form),
      });
    },
    true,
  );
}

export function sanitizeStackFrames(stack: string | null | undefined): string[] {
  if (!stack) return [];
  const frames: string[] = [];
  for (const rawLine of stack.split(/\r?\n/)) {
    const line = rawLine.trim();
    const chrome = line.match(/^(?:at\s+)?(?:(.*?)\s+\()?(.+?):(\d+):(\d+)\)?$/);
    const safari = chrome ? null : line.match(/^(.*?)@(.+?):(\d+):(\d+)$/);
    const match = chrome ?? safari;
    if (!match) continue;
    const functionName = sanitizeFunctionName(match[1] || 'anonymous');
    const source = sanitizeStackSource(match[2]);
    if (!source) continue;
    frames.push(`${functionName}@${source}:${match[3]}:${match[4]}`.slice(0, 240));
    if (frames.length === 12) break;
  }
  return frames;
}

export function controlDiagnosticTarget(element: Element): string {
  const path: string[] = [];
  let cursor: Element | null = element;
  while (cursor && path.length < 6) {
    const tag = cursor.tagName.toLowerCase().replace(/[^a-z0-9-]/g, '') || 'element';
    const role = safeElementRole(cursor.getAttribute('role'));
    const type = cursor instanceof HTMLInputElement ? safeInputType(cursor.type) : null;
    const siblings = cursor.parentElement
      ? Array.from(cursor.parentElement.children).filter(
          (child) => child.tagName === cursor?.tagName,
        )
      : [];
    const siblingIndex = Math.max(0, siblings.indexOf(cursor));
    path.push([tag, role, type, siblingIndex].filter((value) => value !== null).join(':'));
    cursor = cursor.parentElement;
  }
  const kind =
    element.tagName
      .toLowerCase()
      .replace(/[^a-z0-9]/g, '')
      .slice(0, 16) || 'control';
  return `${kind}_${errorFingerprint(path.join('/'))}`;
}

function sendDiagnostic(command: string, args: Record<string, unknown>) {
  if (!isTauri()) return;
  void invoke(command, args).catch(() => undefined);
}

function closestElement(target: EventTarget | null, selector: string): Element | null {
  return target instanceof Element ? target.closest(selector) : null;
}

function safeElementRole(value: string | null): string | null {
  return value &&
    ['button', 'menuitem', 'tab', 'switch', 'checkbox', 'radio', 'dialog'].includes(value)
    ? value
    : null;
}

function safeInputType(value: string): string | null {
  return ['text', 'password', 'search', 'number', 'checkbox', 'radio', 'button', 'submit'].includes(
    value,
  )
    ? value
    : null;
}

function currentView(): ViewName {
  if (typeof window === 'undefined') return 'sites';
  const value = new URLSearchParams(window.location.search).get('view');
  return viewNames.find((view) => view === value) ?? 'sites';
}

function diagnosticId(prefix: 'op' | 'ui' | 'fe') {
  const random = globalThis.crypto?.randomUUID?.().replace(/-/g, '');
  if (random) return `${prefix}_${random}`;
  const fallback = `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`;
  return `${prefix}_${fallback.replace(/[^a-z0-9]/gi, '').slice(0, 48)}`;
}

function normalizeError(reason: unknown): Error {
  if (reason instanceof Error) return reason;
  let message = 'NonErrorRejection';
  try {
    message = String(reason);
  } catch {
    // Keep a stable non-sensitive fallback for exotic rejection values.
  }
  const error = new Error(message);
  error.name = 'NonErrorRejection';
  return error;
}

function sanitizeErrorName(value: string) {
  const normalized = value.replace(/[^A-Za-z0-9_.-]/g, '').slice(0, 64);
  return normalized || 'Error';
}

function sanitizeFunctionName(value: string) {
  const normalized = value.replace(/[^A-Za-z0-9_.$<>-]/g, '_').slice(0, 80);
  return normalized || 'anonymous';
}

function sanitizeStackSource(value: string): string | null {
  let source = value.replace(/\\/g, '/').split(/[?#]/, 1)[0];
  try {
    source = new URL(source).pathname;
  } catch {
    // Relative Vite and WebView sources do not require URL parsing.
  }
  const srcIndex = source.lastIndexOf('/src/');
  if (srcIndex >= 0) source = source.slice(srcIndex + 1);
  else if (source.includes('/assets/')) {
    source = source.slice(source.lastIndexOf('/assets/') + 1);
  } else {
    const segments = source.split('/').filter(Boolean);
    source = segments[segments.length - 1] ?? '';
  }
  const normalized = source.replace(/[^A-Za-z0-9_./-]/g, '').slice(0, 160);
  return normalized || null;
}

function errorFingerprint(value: string) {
  let first = 0x811c9dc5;
  let second = 0x9e3779b9;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    first = Math.imul(first ^ code, 0x01000193) >>> 0;
    second = Math.imul(second ^ code, 0x85ebca6b) >>> 0;
  }
  return `${first.toString(16).padStart(8, '0')}${second.toString(16).padStart(8, '0')}`;
}
