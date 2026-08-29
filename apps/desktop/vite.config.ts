import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and to fail if it's taken, rather than silently
// moving to the next one and leaving tauri.conf.json's devUrl pointing at
// nothing.
export default defineConfig({
  plugins: [react()],
  // Tauri serves the built app from a custom protocol, not a real HTTP
  // origin — root-absolute asset paths ("/assets/x.js") don't reliably
  // resolve there and produce a blank window with no console error.
  // Relative paths sidestep it.
  base: "./",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "esnext",
    outDir: "dist",
    emptyOutDir: true,
  },
});
