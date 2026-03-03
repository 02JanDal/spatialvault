import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [react()],
  build: {
    lib: {
      entry: resolve(__dirname, "src/CatalogPlugin.tsx"),
      name: "CatalogPlugin",
      fileName: "catalog-plugin",
    },
    rollupOptions: {
      external: ["Origo"],
    },
    sourcemap: true,
  },
});
