import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { publish, resetEventBusForTests, subscribe } from '@/api/events';

const tauriState = vi.hoisted(() => ({
  tauri: false,
  listen: vi.fn(),
  emit: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => tauriState.tauri }));
vi.mock('@tauri-apps/api/event', () => ({ listen: tauriState.listen, emit: tauriState.emit }));

beforeEach(() => {
  resetEventBusForTests();
  tauriState.tauri = false;
  tauriState.listen.mockReset();
  tauriState.emit.mockReset();
});

afterEach(() => {
  resetEventBusForTests();
});

describe('事件机制', () => {
  it('浏览器总线：订阅后收到发送的 payload', async () => {
    const received: number[] = [];
    const off = await subscribe<number>('demo', (p) => received.push(p));

    await publish('demo', 1);
    await publish('demo', 2);

    expect(received).toEqual([1, 2]);
    off();
  });

  it('浏览器总线：取消订阅后不再收到', async () => {
    const received: number[] = [];
    const off = await subscribe<number>('demo', (p) => received.push(p));

    await publish('demo', 1);
    off();
    await publish('demo', 2);

    expect(received).toEqual([1]);
  });

  it('浏览器总线：多个订阅者都收到；无订阅者发送不报错', async () => {
    const a: number[] = [];
    const b: number[] = [];
    const offA = await subscribe<number>('demo', (p) => a.push(p));
    const offB = await subscribe<number>('demo', (p) => b.push(p));

    await publish('demo', 7);
    expect(a).toEqual([7]);
    expect(b).toEqual([7]);

    offA();
    offB();
    await expect(publish('demo', 9)).resolves.toBeUndefined();
  });

  it('Tauri 模式：subscribe 走 listen、publish 走 emit，payload 透传', async () => {
    tauriState.tauri = true;
    const handler: { fn: ((event: { payload: unknown }) => void) | null } = { fn: null };
    const unlisten = vi.fn();
    tauriState.listen.mockImplementation(
      (_event: string, cb: (e: { payload: unknown }) => void) => {
        handler.fn = cb;
        return Promise.resolve(unlisten);
      },
    );
    tauriState.emit.mockResolvedValue(undefined);

    const received: number[] = [];
    const off = await subscribe<number>('evt', (p) => received.push(p));
    expect(tauriState.listen).toHaveBeenCalledWith('evt', expect.any(Function));

    // 模拟后端派发事件
    handler.fn?.({ payload: 42 });
    expect(received).toEqual([42]);

    await publish('evt', 99);
    expect(tauriState.emit).toHaveBeenCalledWith('evt', 99);

    off();
    expect(unlisten).toHaveBeenCalled();
  });
});
