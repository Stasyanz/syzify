/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { readFileSync } from "fs";
import { configDefaults } from "vitest/config";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

const pkg = JSON.parse(readFileSync("package.json", "utf-8"));

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
    // 4. serve nothing of `src-tauri` (#305): a build given the ANT+ key
    // keeps it in its target/ (the binary, the build script's output), and
    // the dev server serves any file of the root it does not deny. A deny
    // list replaces Vite's own, so those come first.
    fs: {
      deny: [".env", ".env.*", "*.{crt,pem}", "**/.git/**", "**/src-tauri/**"],
    },
  },

  test: {
    // Without this, vitest run from the main checkout also picks up
    // worktree copies of the test suite under .claude/worktrees/**.
    exclude: [...configDefaults.exclude, "**/.claude/worktrees/**"],
    coverage: {
      provider: "v8" as const,
      // Every source file counts, not only the ones imported by tests
      // (vitest 4 does so for whatever `include` names).
      include: ["src/**"],
      reporter: ["text-summary", "lcov"],
    },
  },
}));
