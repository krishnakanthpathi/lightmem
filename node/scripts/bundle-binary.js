#!/usr/bin/env node
/**
 * Bundles the native precompiled binary into node/bin/ for standalone npm distribution.
 * Usage: node scripts/bundle-binary.js [path/to/lmem]
 */
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const rootDir = path.resolve(__dirname, "..");
const repoRoot = path.resolve(rootDir, "..");
const binDir = path.join(rootDir, "bin");

const defaultBinary =
  process.platform === "win32"
    ? path.join(repoRoot, "target", "release", "lmem.exe")
    : path.join(repoRoot, "target", "release", "lmem");

const srcBinary = process.argv[2] ? path.resolve(process.argv[2]) : defaultBinary;

if (!fs.existsSync(srcBinary)) {
  console.error(`Binary not found at: ${srcBinary}`);
  console.error("Run 'cargo build --release --bin lmem' first.");
  process.exit(1);
}

const exeName = process.platform === "win32" ? "lmem.exe" : "lmem";
const destBinary = path.join(binDir, exeName);

fs.mkdirSync(binDir, { recursive: true });
fs.copyFileSync(srcBinary, destBinary);

if (process.platform !== "win32") {
  fs.chmodSync(destBinary, 0o755);
  // Strip debug symbols to reduce package size
  spawnSync("strip", ["-x", destBinary]);
}

const stat = fs.statSync(destBinary);
console.log(`✔ Bundled native binary: ${destBinary} (${(stat.size / 1024 / 1024).toFixed(2)} MB)`);
