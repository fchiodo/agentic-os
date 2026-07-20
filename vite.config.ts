import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  clearScreen: false,
  plugins: [react()],
  preview: {
    host: '0.0.0.0',
    port: 4173,
    strictPort: true,
  },
  resolve: {
    tsconfigPaths: true,
  },
  server: {
    host: '0.0.0.0',
    port: 1420,
    strictPort: true,
  },
})
