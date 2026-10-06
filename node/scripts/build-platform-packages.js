#!/usr/bin/env node
/**
 * Bifurcates native binary releases into dedicated platform package folders:
 *   node/npm/darwin-arm64/
 *   node/npm/darwin-x64/
 *   node/npm/linux-x64/
 *   node/npm/linux-arm64/
 *   node/npm/win32-x64/
 *
 * Usage:
 *   node scripts/build-platform-packages.js [path/to/binary] [target-platform]
 */
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const rootDir = path.resolve(__dirname, "..");
const repoRoot = path.resolve(rootDir, "..");
const npmDir = path.join(rootDir, "npm");

const currentPlatformKey = `${process.platform}-${process.arch}`;
const targetKey = process.argv[3] || currentPlatformKey;

const defaultBinary =
  process.platform === "win32"
    ? path.join(repoRoot, "target", "release", "lmem.exe")
    : path.join(repoRoot, "target", "release", "lmem");

const srcBinary = process.argv[2] ? path.resolve(process.argv[2]) : defaultBinary;

const PLATFORM_CONFIGS = {
  "darwin-arm64": { os: ["darwin"], cpu: ["arm64"], exe: "lmem" },
  "darwin-x64": { os: ["darwin"], cpu: ["x64"], exe: "lmem" },
  "linux-x64": { os: ["linux"], cpu: ["x64"], exe: "lmem" },
  "linux-arm64": { os: ["linux"], cpu: ["arm64"], exe: "lmem" },
  "win32-x64": { os: ["win32"], cpu: ["x64"], exe: "lmem.exe" },
};

const config = PLATFORM_CONFIGS[targetKey];
if (!config) {
  console.error(`Unknown target platform: ${targetKey}`);
  console.error(`Available targets: ${Object.keys(PLATFORM_CONFIGS).join(", ")}`);
  process.exit(1);
}

if (!fs.existsSync(srcBinary)) {
  console.error(`Binary not found at: ${srcBinary}`);
  process.exit(1);
}

const targetDir = path.join(npmDir, targetKey);
const binDir = path.join(targetDir, "bin");
fs.mkdirSync(binDir, { recursive: true });

const destBinary = path.join(binDir, config.exe);
fs.copyFileSync(srcBinary, destBinary);

if (!targetKey.startsWith("win32")) {
  fs.chmodSync(destBinary, 0o755);
  spawnSync("strip", ["-x", destBinary]);
}

const pkgJsonPath = path.join(targetDir, "package.json");
const pkg = {
  name: `@lmem/${targetKey}`,
  version: "0.2.4",
  description: `Precompiled native engine for LightMem (${targetKey})`,
  os: config.os,
  cpu: config.cpu,
  files: ["bin"],
  license: "MIT",
  repository: {
    type: "git",
    url: "https://github.com/krishnakanthpathi/lightmem.git",
    directory: `node/npm/${targetKey}`,
  },
};
fs.writeFileSync(pkgJsonPath, JSON.stringify(pkg, null, 2) + "\n");

const sizeMb = (fs.statSync(destBinary).size / 1024 / 1024).toFixed(2);
console.log(`✔ Bifurcated release package ready: @lmem/${targetKey} -> ${targetDir} (${sizeMb} MB)`);
