import { describe, expect, it } from 'vitest';

import { requestUrl } from '@/utils/request-url';

describe('请求地址原文拼接', () => {
  it.each([
    ['https://aitogether.site/usage', 'v1/res', 'https://aitogether.site/usagev1/res'],
    ['https://example.test', '/v1/models', 'https://example.test/v1/models'],
    ['https://example.test/v1', 'responses', 'https://example.test/v1responses'],
    ['https://example.test/v1', '/responses', 'https://example.test/v1/responses'],
    ['https://example.test/v1/', 'responses', 'https://example.test/v1/responses'],
    ['https://example.test/v1/', '/responses', 'https://example.test/v1//responses'],
    ['https://example.test/v1', '/v1/models', 'https://example.test/v1/v1/models'],
    ['https://example.test/base/', '//models', 'https://example.test/base///models'],
    [
      'https://example.test/base',
      'https://other.test/models',
      'https://example.test/basehttps://other.test/models',
    ],
    [
      'https://example.test/base',
      '/res?region=x#part',
      'https://example.test/base/res?region=x#part',
    ],
  ])('%s + %s = %s', (base, endpoint, address) => {
    for (const sameOrigin of [false, true])
      expect(requestUrl(base, endpoint, sameOrigin)).toBe(address);
  });

  it.each(['/../models', '/./models', '/%2e%2e/models', '/a\\models', '/a\tmodels', '/a b'])(
    '拒绝需要改写的端点 %s',
    (endpoint) => {
      expect(() => requestUrl('https://example.test/base', endpoint)).toThrow();
    },
  );

  it.each([
    '',
    ' https://example.test',
    'https://example.test ',
    'https://exam\tple.test',
    'ftp://example.test',
    'https://user:secret@example.test',
  ])('拒绝无效基址 %s', (base) => {
    expect(() => requestUrl(base, '/models')).toThrow();
  });
});
