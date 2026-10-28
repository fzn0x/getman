import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

// During development the UI runs on Vite's server and the API on getman's.
// Getman refuses requests from other origins, so the proxy drops the
// Origin header the browser adds; open the dev server with the
// `#token=...` link getman prints, on this port instead of its own.
const api = process.env.GETMAN_URL ?? "http://127.0.0.1:7878";

export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: api,
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on("proxyReq", (request) => request.removeHeader("origin"));
        },
      },
    },
  },
  build: {
    target: "es2022",
    outDir: "dist",
    assetsInlineLimit: 0,
    chunkSizeWarningLimit: 1200,
  },
  test: {
    include: ["src/**/*.test.ts"],
    // Node loads the packages that use `graphql` with its CommonJS build;
    // ours must use the same copy, or its checks refuse our schemas.
    alias: { graphql: "graphql/index.js" },
  },
});
