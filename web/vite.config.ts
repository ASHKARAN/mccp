import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  // Allow serving from any static file host/path without server-side routing.
  base: './',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
