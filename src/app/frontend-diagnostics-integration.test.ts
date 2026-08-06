import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn().mockResolvedValue(undefined) }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  isTauri: () => true,
}));

import { beginBackendOperation, installFrontendDiagnostics } from './frontend-diagnostics';

describe('frontend diagnostic interaction capture', () => {
  beforeEach(() => {
    invoke.mockClear();
    document.body.innerHTML = '';
  });

  it('captures generic field, control and submit intent without user-facing values', () => {
    document.body.innerHTML = `
      <form id="secret-form">
        <input name="api-key" aria-label="private account" value="sk-sensitive" />
        <button type="button">Private site name</button>
      </form>`;
    installFrontendDiagnostics();
    const input = document.querySelector('input');
    const button = document.querySelector('button');
    const form = document.querySelector('form');
    if (!input || !button || !form) throw new Error('fixture controls missing');

    input.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
    button.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));

    expect(invoke.mock.calls.map(([, args]) => args.input.action)).toEqual([
      'field-focused',
      'control-activated',
      'form-submitted',
    ]);
    const payload = JSON.stringify(invoke.mock.calls);
    expect(payload).not.toContain('sk-sensitive');
    expect(payload).not.toContain('private account');
    expect(payload).not.toContain('Private site name');
    expect(payload).not.toContain('api-key');
  });

  it('emits an explicit frontend-to-backend operation link', () => {
    const operationId = beginBackendOperation();

    expect(invoke).toHaveBeenCalledWith('record_frontend_action', {
      input: expect.objectContaining({
        action: 'invoke-backend-operation',
        targetType: 'operation',
        targetId: operationId,
      }),
    });
  });
});
