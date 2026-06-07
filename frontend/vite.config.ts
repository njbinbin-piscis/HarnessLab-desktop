import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Dedicated, fixed port for the Tauri desktop dev server (Tauri convention is
// 1420). `strictPort` makes Vite fail loudly instead of silently falling back
// to another port, which would desync from `tauri.conf.json`'s `devUrl`.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
  },
});
