import packageMetadata from '../../package.json';

/** 应用元信息的唯一来源；所有需要应用名等常量的地方统一经此 hook 读取。 */
const APP_NAME = 'RelayPulse';

export function useAppInfo() {
  return { appName: APP_NAME, appVersion: packageMetadata.version };
}
