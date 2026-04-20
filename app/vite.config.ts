import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @see https://v2.tauri.app/reference/config/#buildconfig
export default defineConfig({
  plugins: [react()],

  // Tauri expects a fixed port, fail if that port is not available.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: false,
    watch: {
      // Tell vite to ignore watching `src-tauri` and crates.
      ignored: ["**/crates/**", "**/target/**"],
    },
  },

  // Env vars starting with TAURI_ are exposed to the Rust side only.
  envPrefix: ["VITE_", "TAURI_ENV_*"],

  build: {
    // Tauri supports es2021 and above; modern browsers are fine.
    target: "es2022",
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
