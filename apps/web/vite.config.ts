import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
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
