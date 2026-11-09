import { describe, expect, it } from 'vitest';

import {
  buildRequestParametersPreview,
  createRequestParameters,
  mergeRequestParametersFromJson,
  newRequestParameter,
} from '@/utils/request-parameters';

describe('粘贴 JSON 请求参数', () => {
  it('旧 JSON 对象包含冲突路径时按被覆盖值处理，不生成无效父子参数', () => {
    const parameter = {
      ...newRequestParameter('options', 'json'),
      value: '{"a":1,"a.b":2}',
    };
    const result = mergeRequestParametersFromJson([parameter], '{"options":{"new":2}}');
    expect(buildRequestParametersPreview(result, '')).toEqual({ options: { new: 2 } });
    expect(parameter.value).toBe('{"a":1,"a.b":2}');
  });

  it('按 JSON 值推断参数类型，数组和 null 保留为 JSON', () => {
    const body = {
      text: '0',
      temperature: 0.2,
      stream: false,
      options: { count: 0, enabled: true, empty: {} },
      messages: [{ role: 'user', content: 'hello' }],
      stop: null,
    };
    const result = mergeRequestParametersFromJson([], JSON.stringify(body));
    expect(buildRequestParametersPreview(result, '')).toEqual(body);
    expect(result.map(({ key, type }) => ({ key, type }))).toEqual(
      expect.arrayContaining([
        { key: 'text', type: 'string' },
        { key: 'temperature', type: 'number' },
        { key: 'stream', type: 'boolean' },
        { key: 'options', type: 'object' },
        { key: 'messages', type: 'json' },
        { key: 'stop', type: 'json' },
      ]),
    );
  });

  it('覆盖同名参数的所有重复草稿并启用，保留其它参数及其状态和 ID', () => {
    const parameters = createRequestParameters({ max_tokens: 8, temperature: 0.3 });
    parameters[0]!.enabled = false;
    parameters[1]!.enabled = false;
    const duplicate = { ...newRequestParameter('max_tokens'), value: 'manual duplicate' };
    parameters.push(duplicate);
    const before = JSON.stringify(parameters);
    const result = mergeRequestParametersFromJson(parameters, '{"max_tokens":256,"new_key":"v"}');
    expect(result.filter(({ key }) => key === 'max_tokens')).toEqual([
      { ...parameters[0], enabled: true, type: 'number', value: '256' },
    ]);
    expect(result.find(({ key }) => key === 'temperature')).toEqual(parameters[1]);
    expect(buildRequestParametersPreview(result, '')).toEqual({ max_tokens: 256, new_key: 'v' });
    expect(JSON.stringify(parameters)).toBe(before);
    expect(result).not.toBe(parameters);
    expect(result.find(({ key }) => key === 'temperature')).not.toBe(parameters[1]);
  });

  it('嵌套对象按叶路径覆盖并恢复父级，保留未提及的兄弟字段', () => {
    const parameters = createRequestParameters({
      reasoning: { effort: 'high', summary: 'auto' },
      max_tokens: 8,
    });
    parameters.find(({ key }) => key === 'reasoning')!.enabled = false;
    const result = mergeRequestParametersFromJson(parameters, '{"reasoning":{"effort":"low"}}');
    expect(buildRequestParametersPreview(result, '')).toEqual({
      reasoning: { effort: 'low', summary: 'auto' },
      max_tokens: 8,
    });
    expect(result.find(({ key }) => key === 'reasoning.summary')).toEqual(
      parameters.find(({ key }) => key === 'reasoning.summary'),
    );
  });

  it('点路径自动补充或转换祖先为 object，并启用导入路径', () => {
    const parameters = createRequestParameters({ options: 'old', unrelated: 1 });
    parameters[0]!.enabled = false;
    const result = mergeRequestParametersFromJson(parameters, '{"options.reasoning.effort":"low"}');
    expect(buildRequestParametersPreview(result, '')).toEqual({
      options: { reasoning: { effort: 'low' } },
      unrelated: 1,
    });
    expect(result.find(({ key }) => key === 'options')).toMatchObject({
      id: parameters[0]!.id,
      type: 'object',
      enabled: true,
    });
  });

  it.each(['{"options":{"new":2,"same":"pasted"}}', '{"options.new":2,"options.same":"pasted"}'])(
    'JSON 对象参数参与叶字段合并：%s',
    (json) => {
      const parent = {
        ...newRequestParameter('options', 'json'),
        value: '{"old":1,"same":"manual","nested":{"keep":true}}',
        enabled: false,
      };
      const parameters = [parent];
      const before = JSON.stringify(parameters);
      const result = mergeRequestParametersFromJson(parameters, json);
      expect(buildRequestParametersPreview(result, '')).toEqual({
        options: { old: 1, same: 'pasted', new: 2, nested: { keep: true } },
      });
      expect(result.find(({ key }) => key === 'options')).toEqual({
        ...parent,
        type: 'object',
        value: '{}',
        enabled: true,
      });
      expect(JSON.stringify(parameters)).toBe(before);
    },
  );

  it.each(['{', '[1,2]', 'null', '1'])(
    '无效或非对象 JSON 值 %s 不展开，未涉及时保留原参数',
    (value) => {
      const parameters = [{ ...newRequestParameter('options', 'json'), value }];
      const unrelated = mergeRequestParametersFromJson(parameters, '{"temperature":0.2}');
      expect(unrelated[0]).toEqual(parameters[0]);
      const replaced = mergeRequestParametersFromJson(parameters, '{"options":{"new":2}}');
      expect(buildRequestParametersPreview(replaced, '')).toEqual({ options: { new: 2 } });
    },
  );

  it.each(['"off"', '[1,2]', 'null', '{}'])('值 %s 替换对象并删除旧子树', (value) => {
    const parameters = createRequestParameters({ options: { old: true, nested: { keep: 1 } } });
    const result = mergeRequestParametersFromJson(parameters, `{"options":${value}}`);
    expect(result.filter(({ key }) => key.startsWith('options.'))).toHaveLength(0);
    expect(buildRequestParametersPreview(result, '')).toEqual({ options: JSON.parse(value) });
    expect(parameters).toHaveLength(4);
  });

  it('保留已有动态模型绑定和值，导入后启用，并为新增 model 使用模型绑定', () => {
    const parameters = createRequestParameters({ model: 'original' });
    parameters[0]!.enabled = false;
    const result = mergeRequestParametersFromJson(parameters, '{"model":false}');
    expect(result[0]).toEqual({ ...parameters[0], enabled: true });
    expect(buildRequestParametersPreview(result, 'current-model')).toEqual({
      model: 'current-model',
    });
    const added = mergeRequestParametersFromJson([], '{"model":"pasted"}');
    expect(added[0]).toMatchObject({ key: 'model', type: 'string', binding: 'model' });
    expect(buildRequestParametersPreview(added, 'current-model')).toEqual({
      model: 'current-model',
    });
  });

  it.each(['', '{', '[]', 'null', '42', '"text"', 'true'])(
    '拒绝无效根输入 %s，原草稿不变',
    (json) => {
      const parameters = createRequestParameters({ max_tokens: 8 });
      const before = JSON.stringify(parameters);
      expect(() => mergeRequestParametersFromJson(parameters, json)).toThrow(/JSON/u);
      expect(JSON.stringify(parameters)).toBe(before);
    },
  );

  it.each([
    '{"a":1,"a.b":2}',
    '{"a":{"b":1},"a.b":2}',
    '{" ":1}',
    '{"a..b":1}',
    '{"a. b":1}',
    '{"value":1e400}',
  ])('拒绝冲突路径、无效路径及不可表示的数字：%s', (json) => {
    const parameters = createRequestParameters({ safe: true });
    const before = JSON.stringify(parameters);
    expect(() => mergeRequestParametersFromJson(parameters, json)).toThrow();
    expect(JSON.stringify(parameters)).toBe(before);
  });

  it.each([
    '{"__proto__":{"polluted":true}}',
    '{"config.constructor":1}',
    '{"config":{"Prototype":{}}}',
    '{"messages":[{"constructor":{"prototype":{"polluted":true}}}]}',
    '{"messages":[{"safe.__proto__":true}]}',
  ])('拒绝包含不安全字段的路径或数组内容：%s', (json) => {
    expect(() => mergeRequestParametersFromJson([], json)).toThrow('JSON 中包含不安全的对象字段');
    expect(Object.prototype).not.toHaveProperty('polluted');
  });

  it('绑定模型不能作为对象祖先，拒绝后保留原草稿', () => {
    const parameters = createRequestParameters({ model: 'original' });
    const before = JSON.stringify(parameters);
    expect(() => mergeRequestParametersFromJson(parameters, '{"model.name":"override"}')).toThrow(
      '动态模型参数 model 不能包含子参数',
    );
    expect(() =>
      mergeRequestParametersFromJson(parameters, '{"model":{"name":"override"}}'),
    ).toThrow('动态模型参数 model 不能包含子参数');
    expect(JSON.stringify(parameters)).toBe(before);
  });

  it('允许保留未涉及的无效草稿，空对象导入不清空原参数', () => {
    const parameters = [newRequestParameter(), ...createRequestParameters({ temperature: 0.2 })];
    const result = mergeRequestParametersFromJson(parameters, '{"max_tokens":128}');
    expect(result[0]).toEqual(parameters[0]);
    expect(result.find(({ key }) => key === 'temperature')).toEqual(parameters[1]);
    expect(result.find(({ key }) => key === 'max_tokens')).toMatchObject({ value: '128' });
    expect(mergeRequestParametersFromJson(parameters, '{}')).toEqual(parameters);
  });
});
