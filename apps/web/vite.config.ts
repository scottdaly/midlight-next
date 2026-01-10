import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin } from 'vite';
import { readFileSync, writeFileSync } from 'fs';
import { resolve } from 'path';

/**
 * Vite plugin to inject build version into service worker
 */
function serviceWorkerVersion(): Plugin {
  const buildVersion = Date.now().toString(36);

  return {
    name: 'sw-version',
    apply: 'build',
    closeBundle() {
      // Update sw.js in build output with version
      const swPath = resolve('build', 'sw.js');
      try {
        let content = readFileSync(swPath, 'utf-8');
        content = content.replace(
          /const CACHE_VERSION = ['"]v1['"];/,
          `const CACHE_VERSION = 'v${buildVersion}';`
        );
        writeFileSync(swPath, content);
        console.log(`[sw-version] Updated service worker version to v${buildVersion}`);
      } catch {
        // sw.js might not exist yet or path is different
        console.log('[sw-version] Could not update service worker version');
      }
    },
  };
}

export default defineConfig({
  plugins: [sveltekit(), serviceWorkerVersion()],
  server: {
    port: 5173,
    strictPort: false
  },
  build: {
    target: 'esnext',
    // Enable source maps for debugging (disabled in production via adapter)
    sourcemap: true,
    // Optimize chunk size
    chunkSizeWarningLimit: 500,
    rollupOptions: {
      output: {
        // Manual chunks for better caching
        manualChunks: (id) => {
          // Vendor chunks
          if (id.includes('node_modules')) {
            // Tiptap and ProseMirror - editor core
            if (id.includes('@tiptap') || id.includes('prosemirror')) {
              return 'vendor-editor';
            }
            // Svelte runtime
            if (id.includes('svelte')) {
              return 'vendor-svelte';
            }
            // IDB for storage
            if (id.includes('idb')) {
              return 'vendor-storage';
            }
            // Other vendor code
            return 'vendor';
          }
          // Sync module - separate chunk for cloud sync
          if (id.includes('/sync/')) {
            return 'sync';
          }
        },
      },
    },
  },
  optimizeDeps: {
    include: ['@tiptap/core', '@tiptap/starter-kit', 'idb']
  }
});
