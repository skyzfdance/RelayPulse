/** 小面板命令的浏览器 mock，模拟数据和刷新延时仅由本层维护。 */
import { createPanelMock } from '../panel';
import { getMockGeneralSettings } from './config/general';
import { registerMockHandler } from './registry';

let snapshot = createPanelMock('waiting');

function currentSnapshot() {
  return { ...structuredClone(snapshot), theme: getMockGeneralSettings().theme };
}

registerMockHandler('get_panel_snapshot', currentSnapshot);

registerMockHandler('refresh_panel', async () => {
  await new Promise<void>((resolve) => setTimeout(resolve, 1800));
  const completed = createPanelMock('ready');
  completed.revision = snapshot.revision + 1;
  if (completed.lastProbe) completed.lastProbe.completedAt = new Date().toISOString();
  snapshot = completed;
  return currentSnapshot();
});
