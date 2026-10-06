const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const rootDir = path.resolve(__dirname, "..");
const distDir = path.join(rootDir, "dist");

console.log("==> Building LightMem Node.js / TypeScript SDK...");

// 1. Clean dist
if (fs.existsSync(distDir)) {
  fs.rmSync(distDir, { recursive: true, force: true });
}

// 2. Run TypeScript compiler to emit CJS + declaration files
const tscBin = path.join(rootDir, "node_modules", ".bin", "tsc");
const tscResult = spawnSync(tscBin, ["-p", "tsconfig.json"], {
  cwd: rootDir,
  stdio: "inherit",
});

if (tscResult.status !== 0) {
  console.error("TypeScript compilation failed.");
  process.exit(1);
}

// 3. Emit ESM wrappers (.mjs) for seamless dual package support
const files = fs.readdirSync(distDir);
for (const file of files) {
  if (file.endsWith(".js") && !file.endsWith(".d.ts")) {
    const base = path.basename(file, ".js");
    const mjsPath = path.join(distDir, `${base}.mjs`);
    const cjsPath = `./${base}.js`;

    if (base === "index") {
      fs.writeFileSync(
        mjsPath,
        `import pkg from '${cjsPath}';
export const {
  LightMem,
  getBinaryPath,
  isNativeBinary,
  ensureBinaryDownloaded,
  executeLmem,
  executeLmemAsync,
  VERSION
} = pkg;
export default pkg.LightMem || pkg;
`
      );
    } else {
      fs.writeFileSync(
        mjsPath,
        `import pkg from '${cjsPath}';
export default pkg;
export * from '${cjsPath}';
`
      );
    }
  }
}

console.log("✔ LightMem Node.js SDK build complete: dist/ (CJS + ESM + d.ts)");
