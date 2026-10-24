import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL(".", import.meta.url));

// The Vite project lives in ui/; the single package.json stays at the repo root.
export default defineConfig({
  root,
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: "localhost" },
  build: { outDir: "dist", emptyOutDir: true, target: "es2022" },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
