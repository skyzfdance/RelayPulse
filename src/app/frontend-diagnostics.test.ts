import { describe, expect, it } from 'vitest';
import {
  beginBackendOperation,
  controlDiagnosticTarget,
  sanitizeStackFrames,
} from './frontend-diagnostics';

describe('frontend diagnostics', () => {
  it('creates backend operation identifiers accepted by the native validator', () => {
    expect(beginBackendOperation()).toMatch(/^op_[a-zA-Z0-9_-]+$/);
  });

  it('keeps useful stack locations without exporting URLs, query strings or error messages', () => {
    const frames = sanitizeStackFrames(`Error: credential sk-secret leaked
      at saveSite (https://user:password@example.test/src/features/sites/SitesPage.tsx?token=secret#x:512:18)
      at async submit (/Users/skyoc/private/project/src/features/sites/AddSiteModal.tsx:44:9)
      at https://example.test/assets/index-abcd.js:1:120
      at local (/Users/skyoc/private/generated.js:7:2)`);

    expect(frames).toEqual([
      'saveSite@src/features/sites/SitesPage.tsx:512:18',
      'async_submit@src/features/sites/AddSiteModal.tsx:44:9',
      'anonymous@assets/index-abcd.js:1:120',
      'local@generated.js:7:2',
    ]);
    expect(JSON.stringify(frames)).not.toContain('secret');
    expect(JSON.stringify(frames)).not.toContain('example.test');
    expect(JSON.stringify(frames)).not.toContain('/Users/skyoc');
  });

  it('identifies controls without using labels, names, ids or entered values', () => {
    document.body.innerHTML = `
      <main><form id="account-secret"><label>Secret user</label>
        <input name="api-key" aria-label="private-label" value="sk-secret" />
      </form></main>`;
    const input = document.querySelector('input');
    if (!input) throw new Error('fixture input missing');

    const target = controlDiagnosticTarget(input);

    expect(target).toMatch(/^input_[a-f0-9]{16}$/);
    expect(target).not.toContain('secret');
    expect(target).not.toContain('api-key');
    expect(target).not.toContain('private-label');
  });
});
