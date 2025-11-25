import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react-swc";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { normalizePath } from "vite";
import { viteStaticCopy } from "vite-plugin-static-copy";

const PYODIDE_EXCLUDE = [
  "!**/*.{md,html}",
  "!**/*.d.ts",
  "!**/*.whl",
  "!**/node_modules",
];

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss(), viteStaticCopyPyodide()],
  optimizeDeps: { exclude: ["pyodide", "ty_wasm"] },
});

export function viteStaticCopyPyodide() {
  const pyodideDir = normalizePath(
    join(dirname(fileURLToPath(import.meta.resolve("pyodide"))), "*"),
  );
  return viteStaticCopy({
    targets: [
      {
        src: [pyodideDir, ...PYODIDE_EXCLUDE],
        dest: "assets",
      },
    ],
  });
}
