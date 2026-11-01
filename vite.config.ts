import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || '127.0.0.1',
    hmr: host ? { protocol: 'ws', host, port: 5174 } : undefined
  },
  build: {
    target: 'es2022',
    outDir: 'dist',
    sourcemap: false
  },
  test: {
    environment: 'jsdom',
    include: ['tests/unit/**/*.test.ts']
  }
});
