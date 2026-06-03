import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const appPort = parseInt(process.env.APP_PORT || '9999', 10)
const apiPort = parseInt(process.env.API_PORT || '8999', 10)

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: appPort,
    proxy: {
      '/api': `http://localhost:${apiPort}`,
    },
  },
  build: {
    outDir: 'dist',
  },
})
