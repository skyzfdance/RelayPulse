import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { reactive } from 'vue';

import {
  addModel,
  applyModelSync,
  createSite,
  deleteModel,
  deleteSite,
  fetchSite,
  fetchSites,
  reorderSites,
  runProbe,
  saveSite,
  saveSiteConnection,
  syncModels,
  toggleModel,
  toggleSite,
} from '@/api/config/sites';
import { resetFeedbackForTests } from '@/composables/useFeedback';
import { seedSites, state } from '@/mocks/handlers/config/sites';
import { siteDraft } from '@/views/config/sites/site-policy';

import { chatPolicy } from '../../../support/parameter-fields';

beforeEach(() => {
  window.history.replaceState({}, '', '/');
  seedSites();
  resetFeedbackForTests();
});
afterEach(() => {
  window.history.replaceState({}, '', '/');
});

describe('站点管理契约与 Mock', () => {
  it('编辑响应式草稿可以保存，并且凭据不保留在返回数据或模拟状态中', async () => {
    const site = await fetchSite((await fetchSites())[0].id);
    const draft = reactive(siteDraft(site));
    draft.name = '新名称';
    draft.secret = 'test-only-secret';
    draft.credentialAction = 'replace';
    const saved = await saveSite(site.id, draft);
    expect(saved).toMatchObject({
      name: '新名称',
      credentialConfigured: true,
      connection: site.connection,
    });
    expect(JSON.stringify(state)).not.toContain('test-only-secret');
    draft.name = '未保存的名称';
    expect((await fetchSite(site.id)).name).toBe('新名称');
  });
  it('修改查询结果不会修改数据源；保存失败保持原始配置', async () => {
    const sites = await fetchSites();
    sites[0].name = '尚未保存';
    expect((await fetchSites())[0].name).toBe('OpenAI 主站');
    window.history.replaceState({}, '', '/?scenario=action-error');
    await expect(saveSite(sites[0].id, siteDraft(await fetchSite(sites[0].id)))).rejects.toThrow();
    expect((await fetchSites())[0].name).toBe('OpenAI 主站');
  });
  it('新增、修改小面板显示设置与删除站点的数据关系独立于旧快照', async () => {
    const siteId = await createSite({
      name: '测试站点',
      protocol: 'anthropic',
      baseUrl: 'https://example.com',
      secret: 'test-only-secret',
      enabled: true,
    });
    const created = await fetchSite(siteId);
    expect(created.connection.probePath).toBe('/v1/messages');
    await addModel(siteId, 'new-model');
    expect((await fetchSites()).find((s) => s.id === siteId)?.modelCount).toBe(
      created.models.length + 1,
    );
    await toggleSite(siteId, false);
    expect((await fetchSite(siteId)).enabled).toBe(false);
    await deleteSite(siteId);
    expect((await fetchSites()).find((s) => s.id === siteId)).toBeUndefined();
  });
  it('高级配置独立保存，后续保存旧的基本信息草稿不会覆盖高级配置', async () => {
    const site = await fetchSite('site-1');
    const draft = siteDraft(site);
    const connection = reactive(structuredClone(site.connection));
    connection.intervalSeconds = 15;
    connection.headers = [{ key: 'X-Route', value: 'saved' }];
    const saved = await saveSiteConnection(site.id, connection);
    expect(saved).toEqual({ ...site, connection });
    connection.intervalSeconds = 99;
    connection.headers[0].value = 'unsaved';
    draft.name = '新名称';
    const updated = await saveSite(site.id, draft);
    expect(updated.connection).toEqual(saved.connection);
    expect(updated.name).toBe('新名称');
  });
  it('高级配置保存失败时不修改已保存数据', async () => {
    const site = await fetchSite('site-1');
    window.history.replaceState({}, '', '/?scenario=action-error');
    await expect(
      saveSiteConnection(site.id, { ...site.connection, intervalSeconds: 15 }),
    ).rejects.toThrow();
    expect(await fetchSite(site.id)).toEqual(site);
  });
  it.each([
    ['intervalSeconds', 86400],
    ['slowFirstTokenSeconds', 100],
    ['slowTotalSeconds', 500],
    ['failureThreshold', 10],
    ['recoveryThreshold', 10],
  ] as const)('保存接口拒绝 %s 的空值、非整数和越界值，保留已保存配置', async (field, max) => {
    const site = await fetchSite('site-1');
    const connection = structuredClone(site.connection);
    for (const invalid of [null, 0, max + 1, 1.5]) {
      Object.assign(connection, { [field]: invalid });
      await expect(saveSiteConnection(site.id, connection)).rejects.toThrow();
      expect((await fetchSite(site.id)).connection).toEqual(site.connection);
    }
  });
  it('保存接口拒绝端点和请求头首尾空格，允许请求头值为空', async () => {
    const site = await fetchSite('site-1');
    for (const field of ['modelsPath', 'probePath'] as const) {
      for (const value of ['', ' /v1/test', '/v1/test ']) {
        const connection = structuredClone(site.connection);
        connection[field] = value;
        await expect(saveSiteConnection(site.id, connection)).rejects.toThrow();
      }
    }
    const connection = structuredClone(site.connection);
    for (const header of [
      { key: '', value: '' },
      { key: ' X-Route', value: '' },
      { key: 'X-Route ', value: '' },
      { key: 'X-Route', value: ' value' },
      { key: 'X-Route', value: 'value ' },
    ]) {
      connection.headers = [header];
      await expect(saveSiteConnection(site.id, connection)).rejects.toThrow();
    }
    connection.headers = [{ key: 'X-Route', value: '' }];
    const saved = await saveSiteConnection(site.id, connection);
    expect(saved.connection.headers).toEqual([{ key: 'X-Route', value: '' }]);
    expect(saved.connection.intervalSeconds).toBe(300);
  });
  it('排序严格验证集合且失败不改变优先级', async () => {
    const ids = (await fetchSites()).map((s) => s.id);
    await expect(reorderSites([ids[0], ids[0], ids[1]])).rejects.toThrow();
    expect((await fetchSites()).map((s) => s.id)).toEqual(ids);
    await reorderSites([...ids].reverse());
    expect((await fetchSites()).map((s) => s.id)).toEqual([...ids].reverse());
    window.history.replaceState({}, '', '/?scenario=action-error');
    await expect(reorderSites(ids)).rejects.toThrow();
    expect((await fetchSites()).map((s) => s.id)).toEqual([...ids].reverse());
  });
  it('同步检查传模型名称，只返回新增和删除差异，不修改已保存模型', async () => {
    await deleteModel('site-1', 'model-0-1');
    const before = await fetchSite('site-1');
    const changes = await syncModels(
      before.id,
      before.models.map((model) => model.name),
    );
    expect(changes).toEqual({
      added: ['gpt-mini-demo'],
      removed: [{ id: 'model-0-2', name: 'legacy-demo' }],
    });
    expect(await fetchSite(before.id)).toEqual(before);
    changes.added.push('not-saved');
    changes.removed[0].name = 'not-saved';
    expect(await fetchSite(before.id)).toEqual(before);
  });
  it('确认接口只写入已确认差异，无返回数据，并保留已有模型配置', async () => {
    await deleteModel('site-1', 'model-0-1');
    await toggleModel('site-1', 'model-0-0', false);
    state.sites[0].models[0].policy = chatPolicy('保留策略', 16);
    const before = await fetchSite('site-1');
    const models = before.models.map((model) => model.name);
    const changes = await syncModels(before.id, models);
    expect(
      await applyModelSync(before.id, {
        models,
        added: changes.added,
        removed: changes.removed.map((model) => model.id),
      }),
    ).toBeUndefined();
    const saved = await fetchSite(before.id);
    expect(saved.models.map((model) => model.name)).toEqual(['gpt-demo', 'gpt-mini-demo']);
    expect(saved.models[0]).toEqual(before.models[0]);
    expect(saved.models[1]).toMatchObject({
      enabled: true,
      health: 'muted',
      firstTokenMs: null,
      durationMs: null,
      policy: null,
      customRequest: null,
    });
    expect(saved.models[1].id).toBeTruthy();
    // 同步后的模型 ID 与远端名称不同，再次检查也不得重复新增。
    expect(
      await syncModels(
        saved.id,
        saved.models.map((model) => model.name),
      ),
    ).toEqual({
      added: [],
      removed: [],
    });
    expect((await fetchSite(saved.id)).models).toEqual(saved.models);
  });
  it('没有差异时也只返回两个空数组，检查不改变数据', async () => {
    const before = await fetchSite('site-2');
    expect(
      await syncModels(
        before.id,
        before.models.map((model) => model.name),
      ),
    ).toEqual({
      added: [],
      removed: [],
    });
    expect(await fetchSite(before.id)).toEqual(before);
  });
  it('检查拒绝缺失、重复、未知名称或内部 ID 组成的模型名称集合', async () => {
    const site = await fetchSite('site-1');
    for (const models of [
      [],
      ['gpt-demo', 'gpt-demo', 'legacy-demo'],
      ['gpt-demo', 'claude-sonnet-demo', 'legacy-demo'],
      site.models.map((model) => model.id),
    ]) {
      await expect(syncModels(site.id, models)).rejects.toThrow('模型列表已变化');
    }
    expect(await fetchSite(site.id)).toEqual(site);
  });
  it('确认前目录变动时拒绝旧差异；重新检查后才能写入', async () => {
    const site = await fetchSite('site-1');
    const models = site.models.map((model) => model.name);
    const first = await syncModels(site.id, models);
    await addModel(site.id, 'temporary-model');
    const changed = await fetchSite(site.id);
    await expect(
      applyModelSync(site.id, {
        models,
        added: first.added,
        removed: first.removed.map((model) => model.id),
      }),
    ).rejects.toThrow('模型列表已变化');
    expect(await fetchSite(site.id)).toEqual(changed);
    const currentNames = changed.models.map((model) => model.name);
    const latest = await syncModels(site.id, currentNames);
    await applyModelSync(site.id, {
      models: currentNames,
      added: latest.added,
      removed: latest.removed.map((model) => model.id),
    });
    expect((await fetchSite(site.id)).models.map((model) => model.name)).toEqual([
      'gpt-demo',
      'gpt-mini-demo',
    ]);
  });
  it('确认接口先校验全部差异，错误删除 ID 或重复新增不会造成部分写入', async () => {
    const site = await fetchSite('site-1');
    const models = site.models.map((model) => model.name);
    for (const changes of [
      { added: ['new-model'], removed: ['model-1-0'] },
      { added: ['new-model', 'new-model'], removed: ['model-0-2'] },
      { added: ['gpt-demo'], removed: ['model-0-2'] },
      { added: [], removed: ['model-0-2', 'model-0-2'] },
    ]) {
      await expect(applyModelSync(site.id, { models, ...changes })).rejects.toThrow('同步差异无效');
      expect(await fetchSite(site.id)).toEqual(site);
    }
  });
  it('确认写入失败保留已保存模型，检查本身不受写入失败场景影响', async () => {
    const site = await fetchSite('site-1');
    const models = site.models.map((model) => model.name);
    window.history.replaceState({}, '', '/?scenario=action-error');
    const changes = await syncModels(site.id, models);
    await expect(
      applyModelSync(site.id, {
        models,
        added: changes.added,
        removed: changes.removed.map((model) => model.id),
      }),
    ).rejects.toThrow();
    expect(await fetchSite(site.id)).toEqual(site);
  });
  it('内部 ID 与模型名完全不同也按名称匹配，不误删或重复新增', async () => {
    state.sites[0].models = state.sites[0].models.slice(0, 2);
    state.sites[0].models[0].id = 'sql-model-42';
    state.sites[0].models[1].id = 'sql-model-99';
    const before = await fetchSite('site-1');
    expect(await syncModels(before.id, ['gpt-demo', 'gpt-mini-demo'])).toEqual({
      added: [],
      removed: [],
    });
    expect(await fetchSite(before.id)).toEqual(before);
  });
  it('手动探测更新启用模型的状态和耗时，保留未参与模型', async () => {
    const site = await fetchSite((await fetchSites())[0].id);
    const disabled = site.models.filter((model) => !model.enabled);
    await runProbe(site.id);
    const updated = await fetchSite(site.id);
    for (const model of updated.models.filter((model) => model.enabled)) {
      expect(model).toMatchObject({ health: 'healthy', firstTokenMs: 420, durationMs: 1250 });
    }
    expect(updated.models.filter((model) => !model.enabled)).toEqual(disabled);
  });
});
