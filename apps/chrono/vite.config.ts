import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5175, strictPort: true, host: host || false, hmr: host ? { protocol: "ws", host, port: 5176 } : undefined },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: { target: "es2022", sourcemap: false, chunkSizeWarningLimit: 600 },
});
