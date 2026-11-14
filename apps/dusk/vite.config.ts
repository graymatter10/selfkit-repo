import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  clearScreen: false,
  server: { port: 5180, strictPort: true, host: host || false, hmr: host ? { protocol: "ws", host, port: 5181 } : undefined },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: { target: "es2022", sourcemap: false, chunkSizeWarningLimit: 600 },
});
