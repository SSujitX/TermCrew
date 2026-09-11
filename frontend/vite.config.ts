import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import http from 'node:http'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = path.dirname(fileURLToPath(import.meta.url))

// Keep-alive agent for the dev proxy. Node's default for `agent: false` sends
// `Connection: close`, and hyper's per-request close races the client on Windows
// loopback, landing as ECONNRESET/502 on ~5% of requests. Reusing warm sockets
// avoids that close entirely. The Rust backend has no idle timeout, so sockets
// never go stale on its side.
const keepAliveAgent = new http.Agent({ keepAlive: true, maxSockets: 100 })

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    tailwindcss(),
    svelte(),
  ],
  resolve: {
    // monaco-editor package "exports" only expose /esm; aggregated CSS is under /min.
    alias: {
      'monaco-editor-css': path.resolve(
        rootDir,
        'node_modules/monaco-editor/min/vs/editor/editor.main.css'
      ),
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3001',
        changeOrigin: true,
        agent: keepAliveAgent,
      },
      // PTY sockets do not use this proxy. The browser connects to
      // ws://127.0.0.1:3001 in dev (see ptyWsUrl) because http-proxy WS
      // upgrades reset on Windows loopback.
    },
  },
})