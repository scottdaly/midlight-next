import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    port: 5173,
    strictPort: false
  },
  build: {
    target: 'esnext'
  },
  optimizeDeps: {
    include: ['@tiptap/core', '@tiptap/starter-kit']
  }
});
