import { describe, expect, it } from 'vitest';

import {
  buildRequestHeaders,
  mergeRequestHeadersFromJson,
  requestHeadersError,
} from '@/utils/request-headers';
import { newRequestParameter } from '@/utils/request-parameters';
import { headerValueError } from '@/views/config/sites/connection-validation';
import { siteHeadersError, siteRequestPreview } from '@/views/config/sites/site-request';

describe('请求头参数表', () => {
  it('模型获取与参数编辑使用相同的 HTTP 值校验，不修改输入', () => {
    for (const value of ['a\0b', 'a\x1fb', 'a\x7fb', ' trailing ', 'a\nb']) {
      expect(headerValueError(value)).toBeTruthy();
      expect(requestHeadersError([{ ...newRequestParameter('x-test'), value }])).toBeTruthy();
    }
    for (const value of ['', 'plain', 'a\tb', '原文']) {
      expect(headerValueError(value)).toBeNull();
      expect(buildRequestHeaders([{ ...newRequestParameter('x-test'), value }])).toEqual([
        { key: 'x-test', value },
      ]);
    }
  });

  it('站点敏感头在预览脱敏，业务凭据限制不进入通用参数表机制', () => {
    const headers = [{ ...newRequestParameter('X-Auth'), value: 'dummy-secret' }];
    const preview = siteRequestPreview('https://example.test/messages', 'anthropic', headers, {});
    expect(preview.headers.configured['x-auth']).toBe('[REDACTED]');
    expect(JSON.stringify(preview)).not.toContain('dummy-secret');
    const credential = [{ ...newRequestParameter('Authorization'), value: 'test' }];
    expect(requestHeadersError(credential)).toBeNull();
    expect(siteHeadersError(credential)).toContain('凭据');
    expect(headers[0].value).toBe('dummy-secret');
  });
  it('保留点号字段和字符串原文，大小写同名导入覆盖，未启用行保留但不发送', () => {
    const original = [
      { ...newRequestParameter('X.Route'), value: 'old' },
      { ...newRequestParameter('disabled'), value: 'saved', enabled: false },
    ];
    const result = mergeRequestHeadersFromJson(original, '{"x.route":"new","model":"literal"}');
    expect(result[0]?.id).toBe(original[0]?.id);
    expect(result[1]).toEqual(original[1]);
    expect(buildRequestHeaders(result)).toEqual([
      { key: 'x.route', value: 'new' },
      { key: 'model', value: 'literal' },
    ]);
    expect(original[0]?.value).toBe('old');
  });

  it.each([
    '{"x":true}',
    '{"x":2}',
    '{"x":{}}',
    '{"x":"one","X":"two"}',
    '{"x":" trailing "}',
    '{"x":"line\\nend"}',
  ])('导入错误时保留草稿，不自动转换：%s', (json) => {
    const original = [{ ...newRequestParameter('safe'), value: 'kept' }];
    const before = JSON.stringify(original);
    expect(() => mergeRequestHeadersFromJson(original, json)).toThrow();
    expect(JSON.stringify(original)).toBe(before);
  });

  it('启用行校验大小写重名、凭据字段及类型；禁用的无效草稿不发送', () => {
    const headers = [
      { ...newRequestParameter('X-Route'), value: 'one' },
      { ...newRequestParameter('x-route'), value: 'two' },
    ];
    expect(requestHeadersError(headers)).toContain('重复');
    headers[1]!.enabled = false;
    expect(buildRequestHeaders(headers)).toEqual([{ key: 'X-Route', value: 'one' }]);
    expect(
      siteHeadersError([{ ...newRequestParameter('Authorization'), value: 'secret' }]),
    ).toContain('凭据');
    expect(requestHeadersError([{ ...newRequestParameter('x', 'number'), value: '3' }])).toContain(
      'string',
    );
    expect(requestHeadersError([{ ...newRequestParameter('K-header'), value: 'value' }])).toContain(
      '名称',
    );
    expect(buildRequestHeaders([{ ...newRequestParameter('x-label'), value: '原文' }])).toEqual([
      { key: 'x-label', value: '原文' },
    ]);
  });

  it('完整预览保持字面地址并脱敏敏感请求头，Body 保持原值', () => {
    const headers = mergeRequestHeadersFromJson(
      [],
      '{"anthropic-beta":"feature","x-vendor-authorization":"Bearer private-value"}',
    );
    const preview = siteRequestPreview('https://example.test/usagev1/res', 'anthropic', headers, {
      speed: 'fast',
    });
    expect(preview.method).toBe('POST');
    expect(preview.address).toBe('https://example.test/usagev1/res');
    expect(preview.headers.configured).toEqual({
      'anthropic-beta': 'feature',
      'x-vendor-authorization': '[REDACTED]',
    });
    expect(preview.headers.defaults).toEqual({
      'x-api-key': '[已配置凭据]',
      'content-type': 'application/json',
      accept: '*/*',
      'user-agent': '[应用默认 User-Agent]',
    });
    expect(JSON.stringify(preview)).not.toContain('private-value');
    expect(preview.body).toEqual({ speed: 'fast' });
  });

  it('服务要求的请求头缺失或禁用时不补充，用户启用后才按原值预览', () => {
    for (const protocol of [
      'anthropic',
      'openai-compatible',
      'openai-responses',
      'custom-http',
    ] as const) {
      const configured = { ...newRequestParameter('anthropic-version'), value: 'user-version' };
      for (const headers of [[], [{ ...configured, enabled: false }], [configured]]) {
        const before = JSON.stringify(headers);
        const preview = siteRequestPreview('https://example.test/messages', protocol, headers, {});
        const combined = { ...preview.headers.defaults, ...preview.headers.configured };
        if (headers[0]?.enabled) expect(combined['anthropic-version']).toBe('user-version');
        else expect(combined).not.toHaveProperty('anthropic-version');
        expect(preview.headers.defaults).not.toHaveProperty('anthropic-version');
        expect(JSON.stringify(headers)).toBe(before);
      }
    }
  });

  it('预览将用户覆盖的请求头归入用户配置，默认项不重复展示，禁用行不展示', () => {
    const headers = mergeRequestHeadersFromJson(
      [],
      '{"Content-Type":"application/custom+json","Accept":"application/json","User-Agent":"custom-agent","Anthropic-Version":"custom-version"}',
    );
    headers.push({ ...newRequestParameter('x-disabled'), value: 'saved', enabled: false });
    const before = JSON.stringify(headers);
    const preview = siteRequestPreview('https://example.test//messages', 'anthropic', headers, {});
    expect(preview.headers.configured).toEqual({
      'content-type': 'application/custom+json',
      accept: 'application/json',
      'user-agent': 'custom-agent',
      'anthropic-version': 'custom-version',
    });
    expect(preview.headers.defaults).toEqual({ 'x-api-key': '[已配置凭据]' });
    expect(preview.address).toBe('https://example.test//messages');
    expect(JSON.stringify(headers)).toBe(before);
  });
});
