import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import dts from "vite-plugin-dts";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [react(), dts({ tsconfigPath: "./tsconfig.app.json" })],
  build: {
    lib: {
      entry: resolve(__dirname, "src/index.tsx"),
      name: "CatalogPlugin",
      fileName: "catalog-plugin",
    },
    rollupOptions: {
      external: ["Origo"],
    },
    sourcemap: true,
  },
});
