import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port in dev and does not need Vite's HMR overlay noise.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "chrome120",
    cssCodeSplit: false,
    sourcemap: false,
    chunkSizeWarningLimit: 400,
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
