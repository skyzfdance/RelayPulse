/**
 * 浏览器（mock）模式下的命令分发注册表。
 *
 * callBackend 在非 Tauri 环境按命令名查找 handler；handler 的入参/返回值
 * 与 Tauri command 的线格式保持一致（不做应用层组装，组装归 api 层调用方）。
 * 未注册的命令直接抛错（fail loud），让漏 mock 在开发与测试中立刻暴露。
 */

export type MockHandler = (args: unknown) => unknown;

const handlers = new Map<string, MockHandler>();

export function registerMockHandler(command: string, handler: MockHandler): void {
  handlers.set(command, handler);
}

export function getMockHandler(command: string): MockHandler | undefined {
  return handlers.get(command);
}

/** 仅供单测复位使用。 */
export function clearMockHandlers(): void {
  handlers.clear();
}
