import { build } from "esbuild";
import { mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
await mkdir(resolve(root, "dist"), { recursive: true });

await build({
  entryPoints: [resolve(root, "src/index.ts")],
  outfile: resolve(root, "dist/index.cjs"),
  bundle: true,
  platform: "node",
  target: "node22",
  format: "cjs",
  sourcemap: false,
  minify: false,
  banner: {
    js: "const __require = require;",
  },
  logLevel: "info",
});
