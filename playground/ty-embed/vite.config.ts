import { defineConfig } from "vite";
import { viteStaticCopy } from "vite-plugin-static-copy";

export default defineConfig({
  plugins: [
    viteStaticCopy({
      targets: [
        {
          src: ["ty_wasm/*", "!ty_wasm/.gitignore"],
          dest: "ty_wasm",
        },
      ],
    }),
  ],
  build: {
    lib: {
      entry: "./src/index.ts",
      name: "TyEmbed",
      formats: ["es", "umd"],
      fileName: (format) => `ty-embed.${format}.js`,
    },
    rollupOptions: {
      external: ["ty_wasm"],
      output: {
        assetFileNames: (assetInfo) => {
          if (assetInfo.name === "style.css") return "ty-embed.css";
          return assetInfo.name ?? "asset";
        },
        paths: {
          ty_wasm: "./ty_wasm/ty_wasm.js",
        },
      },
    },
    copyPublicDir: false,
  },
  optimizeDeps: {
    exclude: ["ty_wasm"],
  },
  server: {
    port: 3001,
    open: "/example-dev.html",
  },
});
