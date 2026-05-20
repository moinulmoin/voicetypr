import { copyFile, mkdir, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { platform } from "node:os";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = resolve(root, "dist");
const sidecarDist = resolve(root, "..", "dist");
const isWindows = platform() === "win32";
const hostTriple = defaultTargetTriple();
const targetTriple = process.env.TARGET || hostTriple;
const targetIsWindows = targetTriple.includes("windows");
const exeName = isWindows ? "formatting-sidecar.exe" : "formatting-sidecar";
const targetExeName = targetIsWindows ? `formatting-sidecar-${targetTriple}.exe` : `formatting-sidecar-${targetTriple}`;
const exePath = resolve(sidecarDist, exeName);
const targetExePath = resolve(sidecarDist, targetExeName);
const blobPath = resolve(dist, "sea-prep.blob");
const seaConfigPath = resolve(dist, "sea-config.json");

if (targetTriple !== hostTriple) {
  throw new Error(
    [
      "Node SEA sidecar must be built with a Node runtime matching TARGET.",
      `Host=${hostTriple}`,
      `TARGET=${targetTriple}`,
      "Use a matching build runner or provide a target-specific Node executable.",
    ].join(" "),
  );
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", shell: isWindows, ...options });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with status ${result.status}`);
  }
}

function defaultTargetTriple() {
  if (platform() === "darwin") {
    return process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
  }
  if (platform() === "win32") {
    return process.arch === "arm64" ? "aarch64-pc-windows-msvc" : "x86_64-pc-windows-msvc";
  }
  if (platform() === "linux") {
    return process.arch === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu";
  }
  return `${process.arch}-${platform()}`;
}

await mkdir(dist, { recursive: true });
await mkdir(sidecarDist, { recursive: true });
await rm(blobPath, { force: true });

run(process.execPath, [resolve(root, "scripts/build.mjs")]);

await writeFile(
  seaConfigPath,
  JSON.stringify(
    {
      main: resolve(dist, "index.cjs"),
      output: blobPath,
      disableExperimentalSEAWarning: true,
      useSnapshot: false,
      useCodeCache: false,
      execArgv: ["--no-warnings"],
      execArgvExtension: "none",
    },
    null,
    2,
  ),
);

run(process.execPath, ["--experimental-sea-config", seaConfigPath]);
await copyFile(process.execPath, exePath);

if (platform() === "darwin") {
  run("codesign", ["--remove-signature", exePath]);
}

const postjectArgs = [
  exePath,
  "NODE_SEA_BLOB",
  blobPath,
  "--sentinel-fuse",
  "NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2",
];
if (platform() === "darwin") {
  postjectArgs.push("--macho-segment-name", "NODE_SEA");
}
run(resolve(root, "node_modules/.bin/postject") + (isWindows ? ".cmd" : ""), postjectArgs);

if (platform() === "darwin") {
  run("codesign", ["--sign", "-", exePath]);
}

if (!existsSync(exePath)) {
  throw new Error(`Expected sidecar executable missing: ${exePath}`);
}

await copyFile(exePath, targetExePath);

console.log(`Built formatting sidecar: ${exePath}`);
console.log(`Built Tauri target sidecar: ${targetExePath}`);
