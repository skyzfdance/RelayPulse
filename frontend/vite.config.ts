import { AntdvNextResolver } from '@antdv-next/auto-import-resolver';
import tailwindcss from '@tailwindcss/vite';
import vue from '@vitejs/plugin-vue';
import { fileURLToPath } from 'node:url';
import Components from 'unplugin-vue-components/vite';
import { defineConfig } from 'vite';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  plugins: [
    vue(),
    Components({ dirs: [], dts: false, resolvers: [AntdvNextResolver()] }),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    port: 1421,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1422 } : undefined,
    watch: {
      ignored: ['**/backend/**'],
    },
  },
  build: {
    outDir: 'dist/development',
  },
});
