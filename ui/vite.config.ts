import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  server: {
    // hc-spin / hc-launcher inject the conductor port; this is only used for a
    // manually pointed dev server.
    port: 1420,
    strictPort: false,
  },
});
