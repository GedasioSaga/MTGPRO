import { fileURLToPath, URL } from 'node:url'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

/** Porta padrao do `mtg-server`; sobrescreva com MTG_SERVER ao rodar em outra. */
const engineOrigin = process.env.MTG_SERVER ?? 'http://127.0.0.1:8080'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    proxy: {
      '/api': { target: engineOrigin, changeOrigin: true },
      '/ws': { target: engineOrigin, changeOrigin: true, ws: true },
    },
  },
  build: {
    target: 'es2022',
  },
})
