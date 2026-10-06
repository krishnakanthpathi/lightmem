#!/usr/bin/env node
const { spawnSync } = require("child_process");
const { getBinaryPath } = require("../dist/binary.js");

try {
  const binary = getBinaryPath();
  const result = spawnSync(binary, process.argv.slice(2), {
    stdio: "inherit",
    env: process.env,
  });
  process.exit(result.status ?? 0);
} catch (err) {
  console.error(`Error: ${err.message}`);
  process.exit(1);
}
