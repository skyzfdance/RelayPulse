/**
 * 应用事件机制（订阅 + 发送）的统一封装，与 call-backend（请求/响应）并列的另一半通信。
 *
 * - Tauri 壳内：经 @tauri-apps/api/event 的 listen/emit，事件在后端与各前端窗口间路由；
 * - 浏览器（mock / dev）：用进程内事件总线，让订阅/发送在无后端时也能闭环，便于联调与单测。
 *
 * 事件名与 payload 的具体约定留给调用方；本层只做传输，数据无关（payload 泛型透传）。
 */

import type { Options } from '@tauri-apps/api/event';

import { isTauri } from '@tauri-apps/api/core';
import { emit as tauriEmit, listen as tauriListen } from '@tauri-apps/api/event';

import { backendError } from '@/utils/backend-error';

type EventHandler<T> = (payload: T) => void;

/** 浏览器（无后端）进程内事件总线：让 publish/subscribe 在 mock 模式下自成闭环。 */
const browserBus = new Map<string, Set<EventHandler<unknown>>>();

function browserSubscribe<T>(event: string, handler: EventHandler<T>): () => void {
  const handlers = browserBus.get(event) ?? new Set<EventHandler<unknown>>();
  handlers.add(handler as EventHandler<unknown>);
  browserBus.set(event, handlers);
  return () => {
    handlers.delete(handler as EventHandler<unknown>);
    if (handlers.size === 0) browserBus.delete(event);
  };
}

function browserPublish<T>(event: string, payload: T): void {
  const handlers = browserBus.get(event);
  if (!handlers) return;
  // 先快照一份再派发，容忍 handler 内部顺带取消订阅
  for (const handler of [...handlers]) handler(payload);
}

/** 订阅事件，返回取消订阅函数；浏览器模式走进程内总线。 */
export async function subscribe<T>(
  event: string,
  handler: EventHandler<T>,
  options?: Options,
): Promise<() => void> {
  if (!isTauri()) return browserSubscribe(event, handler);
  try {
    if (!options) return await tauriListen<T>(event, (e) => handler(e.payload));
    return await tauriListen<T>(event, (e) => handler(e.payload), options);
  } catch (cause) {
    throw backendError(cause, 'background-task');
  }
}

/** 发送事件；Tauri 内广播到后端与各窗口，浏览器内派发给本进程订阅者。 */
export async function publish<T>(event: string, payload?: T): Promise<void> {
  if (!isTauri()) {
    browserPublish(event, payload as T);
    return;
  }
  try {
    await tauriEmit(event, payload);
  } catch (cause) {
    throw backendError(cause, 'background-task');
  }
}

/** 仅供单测复位进程内总线。 */
export function resetEventBusForTests(): void {
  browserBus.clear();
}
