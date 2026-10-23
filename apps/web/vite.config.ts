import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

// L'API Rust tourne sur 127.0.0.1:8080 (`callistseo serve`) ; le proxy évite tout besoin de CORS en dev.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': path.resolve(import.meta.dirname, './src') } },
  // CALLISTSEO_API change la cible si le port 8080 est déjà pris (ex. : `make serve ADDR=127.0.0.1:18080`).
  server: { proxy: { '/api': process.env.CALLISTSEO_API ?? 'http://127.0.0.1:8080' } },
  test: { environment: 'node' },
})
