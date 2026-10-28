/**
 * Tauri command 统一调用层（前后端通信的唯一出入口）。
 *
 * - 分流：运行时 isTauri() 判定——Tauri 壳内走 invoke；浏览器（mock 模式）
 *   按 命令名 → handler 注册表分发（见 mocks/handlers，未注册直接抛错）。
 * - loading：options.loading 开启时占用页面级全局遮罩（引用计数 + 防闪烁，
 *   见 composables/useGlobalLoading）。
 * - feedback：成功/失败提示走 composables/useFeedback 单例队列；失败内容
 *   直接使用后端错误 message，不做二次包装。
 * - 错误契约：任何失败都经 backendError 归一后 reject 出
 *   RelayPulseBackendError，feedback 只是附加行为，调用方仍可自行处理。
 *
 * 调用约定：
 * - 读操作显式传 successFeedback: 'none'（默认值是给写操作定的）。
 * - 有独立错误出口的后台任务（启动引导、事件订阅 refetch）显式传
 *   failFeedback: 'none'，避免与路由级/页面级错误处理双重提示。
 */
import type { FeedbackMode } from '@/composables/useFeedback';

import { invoke, isTauri } from '@tauri-apps/api/core';

import { useFeedback } from '@/composables/useFeedback';
import { acquireGlobalLoading } from '@/composables/useGlobalLoading';
import { getMockHandler } from '@/mocks/handlers/registry';
import { backendError } from '@/utils/backend-error';

export type CallBackendFeedback = 'none' | FeedbackMode;

export interface CallBackendOptions {
  /** 页面级全局 loading，默认 false。 */
  loading?: boolean;
  /** 全局 loading 文案（仅 loading 为 true 时生效），默认「数据加载中...」。 */
  loadingTips?: string;
  /** 成功提示模式，默认 'message'。 */
  successFeedback?: CallBackendFeedback;
  /** 成功文案，默认「操作成功」。 */
  successText?: string;
  /** 失败提示模式，默认 'modal'，内容为后端错误 message。 */
  failFeedback?: CallBackendFeedback;
}

export const DEFAULT_SUCCESS_TEXT = '操作成功';

let mockHandlersReady: Promise<unknown> | null = null;

/** 浏览器模式下按需装载 Mock 命令注册表；Tauri 模式不调用。 */
async function ensureMockHandlers(): Promise<void> {
  mockHandlersReady ??= import('@/mocks/handlers');
  await mockHandlersReady;
}

async function dispatchBackend<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) return invoke<T>(command, args);
  await ensureMockHandlers();
  const handler = getMockHandler(command);
  if (!handler) throw new Error(`浏览器 mock 模式未注册命令：${command}`);
  return (await handler(args)) as T;
}

export async function callBackend<T = unknown>(
  command: string,
  args?: Record<string, unknown>,
  options: CallBackendOptions = {},
): Promise<T> {
  const {
    loading = false,
    loadingTips,
    successFeedback = 'message',
    successText = DEFAULT_SUCCESS_TEXT,
    failFeedback = 'modal',
  } = options;

  const loadingGuard = loading ? acquireGlobalLoading(loadingTips) : null;
  try {
    const result = await dispatchBackend<T>(command, args);
    if (successFeedback !== 'none') useFeedback().success(successText, successFeedback);
    return result;
  } catch (cause) {
    const error = backendError(cause);
    if (failFeedback !== 'none') useFeedback().error(error.message, failFeedback);
    throw error;
  } finally {
    loadingGuard?.release();
  }
}
