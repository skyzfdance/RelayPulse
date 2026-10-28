import { AntdvNextResolver } from '@antdv-next/auto-import-resolver';
import vue from '@vitejs/plugin-vue';
import { fileURLToPath } from 'node:url';
import Components from 'unplugin-vue-components/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [vue(), Components({ dirs: [], dts: false, resolvers: [AntdvNextResolver()] })],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  test: {
    environment: 'jsdom',
    include: ['tests/unit/**/*.test.ts', 'tests/integration/**/*.test.ts'],
    setupFiles: ['./tests/support/setup.ts'],
    restoreMocks: true,
    maxWorkers: 4,
  },
});
