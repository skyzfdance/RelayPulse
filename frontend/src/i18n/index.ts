import dayjs from 'dayjs';

import 'dayjs/locale/zh-cn';

/**
 * 应用级国际化初始化。组件文案由 ConfigProvider 的 locale 提供，
 * 这里只负责 dayjs 的中文语言包（日期格式化、星期几等跟随 zh-cn）。
 */
export function setupI18n() {
  dayjs.locale('zh-cn');
}
