import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  acquireGlobalLoading,
  resetGlobalLoadingForTests,
  useGlobalLoading,
} from '@/composables/useGlobalLoading';

beforeEach(() => {
  vi.useFakeTimers();
  resetGlobalLoadingForTests();
});

afterEach(() => {
  vi.useRealTimers();
  resetGlobalLoadingForTests();
});

describe('页面级全局 loading 单例', () => {
  it('短任务（防闪烁窗口内完成）全程不出现遮罩', () => {
    const guard = acquireGlobalLoading();
    vi.advanceTimersByTime(199);
    expect(useGlobalLoading().visible.value).toBe(false);

    guard.release();
    vi.advanceTimersByTime(1000);
    expect(useGlobalLoading().visible.value).toBe(false);
  });

  it('长任务：延迟到点后出现遮罩，结束后隐藏', () => {
    const guard = acquireGlobalLoading();
    vi.advanceTimersByTime(200);
    expect(useGlobalLoading().visible.value).toBe(true);

    guard.release();
    vi.advanceTimersByTime(300);
    expect(useGlobalLoading().visible.value).toBe(false);
  });

  it('已显示的遮罩满足最短停留时间，避免尾部闪现', () => {
    const guard = acquireGlobalLoading();
    vi.advanceTimersByTime(200);

    guard.release();
    vi.advanceTimersByTime(299);
    expect(useGlobalLoading().visible.value).toBe(true);

    vi.advanceTimersByTime(1);
    expect(useGlobalLoading().visible.value).toBe(false);
  });

  it('并发请求引用计数：全部结束后才隐藏', () => {
    const first = acquireGlobalLoading();
    const second = acquireGlobalLoading();
    vi.advanceTimersByTime(200);
    expect(useGlobalLoading().visible.value).toBe(true);

    first.release();
    vi.advanceTimersByTime(1000);
    expect(useGlobalLoading().visible.value).toBe(true);

    second.release();
    vi.advanceTimersByTime(300);
    expect(useGlobalLoading().visible.value).toBe(false);
  });

  it('隐藏倒计时期间的新请求会取消隐藏（连续请求不闪断）', () => {
    const first = acquireGlobalLoading();
    vi.advanceTimersByTime(200);
    first.release(); // 进入最短停留倒计时

    const second = acquireGlobalLoading(); // 新请求抵消待执行的隐藏
    vi.advanceTimersByTime(1000);
    expect(useGlobalLoading().visible.value).toBe(true);

    second.release();
    vi.advanceTimersByTime(300);
    expect(useGlobalLoading().visible.value).toBe(false);
  });
});
