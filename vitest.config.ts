import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

// Node-environment unit tests for UI logic (the Zustand stores under src/state).
// The Tauri IPC boundary and Sonner toasts are mocked per-suite, so no DOM or
// Tauri runtime is required.
export default defineConfig({
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
