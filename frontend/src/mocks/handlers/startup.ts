import { registerMockHandler } from './registry';

registerMockHandler('get_startup_status', () => ({ status: 'ready', message: '' }));
registerMockHandler('migrate_startup_data', () => {
  throw new Error('当前没有需要迁移的桌面数据。');
});
registerMockHandler('restart_after_data_migration', () => {
  throw new Error('当前没有已完成的数据迁移。');
});
