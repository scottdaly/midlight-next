/// <reference types="node" />
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig(({ mode }) => {
  // In Node.js config context, use process.env
  const host = process.env.TAURI_DEV_HOST;

  return {
    plugins: [
      svelte(),
    ],

    // Vite options tailored for Tauri development
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      host: host || false,
      hmr: host
        ? {
            protocol: 'ws',
            host,
            port: 1421,
          }
        : undefined,
      watch: {
        ignored: ['**/src-tauri/**'],
      },
    },
    build: {
      target: 'esnext',
      minify: mode !== 'development' ? 'esbuild' : false,
      sourcemap: mode === 'development',
    },
    resolve: {
      alias: {
        $lib: '/src/lib',
      },
      // Ensure browser/client builds are used (order matters!)
      conditions: ['svelte', 'browser', 'import', 'module', 'default'],
    },
    // Explicitly set for non-SSR mode
    appType: 'spa',
  };
});
