import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri serves the dev frontend from a fixed port and embeds the production
// build output (`dist/`) at compile time. See crates/agentic-hub/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2021",
    outDir: "dist",
    emptyOutDir: true,
  },
});
