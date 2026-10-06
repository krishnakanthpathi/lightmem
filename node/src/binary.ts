import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawnSync, spawn } from "node:child_process";

export function isNativeBinary(filePath: string): boolean {
  try {
    if (!fs.existsSync(filePath)) return false;
    const stat = fs.statSync(filePath);
    if (!stat.isFile()) return false;

    // Check executable permission on POSIX
    if (process.platform !== "win32") {
      try {
        fs.accessSync(filePath, fs.constants.X_OK);
      } catch {
        return false;
      }
    }

    const fd = fs.openSync(filePath, "r");
    const buffer = Buffer.alloc(4);
    const bytesRead = fs.readSync(fd, buffer, 0, 4, 0);
    fs.closeSync(fd);

    if (bytesRead < 2) return false;

    // Reject shell, python, or node wrapper scripts starting with '#!'
    if (buffer[0] === 0x23 && buffer[1] === 0x21) {
      return false;
    }

    // Mach-O: 0xcffaedfe, 0xfeedfacf, 0xcafebabe
    if (
      (buffer[0] === 0xcf && buffer[1] === 0xfa && buffer[2] === 0xed && buffer[3] === 0xfe) ||
      (buffer[0] === 0xfe && buffer[1] === 0xed && buffer[2] === 0xfa && buffer[3] === 0xcf) ||
      (buffer[0] === 0xca && buffer[1] === 0xfe && buffer[2] === 0xba && buffer[3] === 0xbe)
    ) {
      return true;
    }

    // ELF: 0x7f 'E' 'L' 'F'
    if (buffer[0] === 0x7f && buffer[1] === 0x45 && buffer[2] === 0x4c && buffer[3] === 0x46) {
      return true;
    }

    // Windows PE: 'M' 'Z'
    if (buffer[0] === 0x4d && buffer[1] === 0x5a) {
      return true;
    }

    return false;
  } catch {
    return false;
  }
}

export function getPlatformPackageName(): string {
  return `@lmem/${process.platform}-${process.arch}`;
}

export function getBinaryPath(): string {
  const exeName = process.platform === "win32" ? "lmem.exe" : "lmem";

  // 0. Check optional platform-specific package if installed
  const pkgName = getPlatformPackageName();
  try {
    const pkgJson = require.resolve(`${pkgName}/package.json`);
    const pkgBinary = path.join(path.dirname(pkgJson), "bin", exeName);
    if (isNativeBinary(pkgBinary)) {
      return pkgBinary;
    }
  } catch {}

  // 1. Check bundled binary packaged with the npm package
  const bundledCandidates = [
    path.join(__dirname, "bin", exeName),
    path.join(__dirname, "..", "bin", exeName),
    path.join(__dirname, "..", "..", "bin", exeName),
  ];
  for (const candidate of bundledCandidates) {
    if (isNativeBinary(candidate)) {
      return candidate;
    }
  }

  // 1. Environment variable override
  const envPath = process.env.LMEM_BINARY_PATH || process.env.LIGHTMEM_BINARY_PATH;
  if (envPath && isNativeBinary(envPath)) {
    return envPath;
  }

  // 2. Well-known user directories
  const home = os.homedir();
  const userCandidates = [
    path.join(home, ".lightmem", "bin", exeName),
    path.join(home, ".local", "bin", exeName),
    path.join(home, ".cargo", "bin", exeName),
  ];
  for (const candidate of userCandidates) {
    if (isNativeBinary(candidate)) {
      return candidate;
    }
  }

  // 3. System PATH, skipping script wrappers
  const pathEnv = process.env.PATH || "";
  const pathDirs = pathEnv.split(path.delimiter);
  for (const dir of pathDirs) {
    if (!dir) continue;
    const candidate = path.join(dir, exeName);
    if (isNativeBinary(candidate)) {
      return candidate;
    }
  }

  // 4. Auto-download pre-built binary
  try {
    const downloaded = ensureBinaryDownloaded();
    if (downloaded && isNativeBinary(downloaded)) {
      return downloaded;
    }
  } catch (err) {
    // pass through to final error
  }

  throw new Error(
    "Could not find the 'lmem' native binary on this system.\n" +
      "Install it via:\n" +
      "  curl -fsSL https://raw.githubusercontent.com/krishnakanthpathi/lightmem/neural-reranker/install.sh | sh\n" +
      "Or set the LMEM_BINARY_PATH environment variable."
  );
}

export function ensureBinaryDownloaded(version = "v0.2.4"): string {
  const exeName = process.platform === "win32" ? "lmem.exe" : "lmem";
  let target = "";

  if (process.platform === "darwin") {
    target = process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
  } else if (process.platform === "linux") {
    target = process.arch === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu";
  } else if (process.platform === "win32") {
    target = "x86_64-pc-windows-msvc";
  } else {
    throw new Error(`Unsupported platform: ${process.platform}`);
  }

  const destDir = path.join(os.homedir(), ".lightmem", "bin");
  fs.mkdirSync(destDir, { recursive: true });
  const destExe = path.join(destDir, exeName);

  if (isNativeBinary(destExe)) {
    return destExe;
  }

  const archiveName = process.platform === "win32" ? `lmem-${target}.zip` : `lmem-${target}.tar.gz`;
  const url = `https://github.com/krishnakanthpathi/lightmem/releases/download/${version}/${archiveName}`;
  const tmpArchive = path.join(destDir, archiveName);

  process.stderr.write(`Downloading LightMem native engine (${version}) from GitHub...\n`);

  try {
    // Download using curl or node curl fallback
    const curlResult = spawnSync("curl", ["-fsSL", "--retry", "3", url, "-o", tmpArchive], {
      stdio: "inherit",
    });
    if (curlResult.status !== 0) {
      throw new Error(`curl download failed with exit code ${curlResult.status}`);
    }

    if (archiveName.endsWith(".zip")) {
      const unzipResult = spawnSync("tar", ["-xf", tmpArchive, "-C", destDir]);
      if (unzipResult.status !== 0) {
        throw new Error(`Failed to extract zip archive`);
      }
    } else {
      const tarResult = spawnSync("tar", ["-xzf", tmpArchive, "-C", destDir]);
      if (tarResult.status !== 0) {
        throw new Error(`Failed to extract tar.gz archive`);
      }
    }

    try {
      fs.unlinkSync(tmpArchive);
    } catch {}

    if (process.platform !== "win32") {
      fs.chmodSync(destExe, 0o755);
    }

    process.stderr.write("LightMem native engine successfully installed.\n");
    return destExe;
  } catch (err) {
    try {
      fs.unlinkSync(tmpArchive);
    } catch {}
    if (version !== "v0.2.3") {
      try {
        return ensureBinaryDownloaded("v0.2.3");
      } catch {}
    }
    throw new Error(`Failed to auto-download lmem binary from ${url}: ${(err as Error).message}`);
  }
}

export function executeLmem(
  args: string[],
  inputText?: string,
  binaryPath?: string
): { code: number; stdout: string; stderr: string } {
  const binary = binaryPath || getBinaryPath();
  const res = spawnSync(binary, args, {
    input: inputText,
    encoding: "utf-8",
    maxBuffer: 50 * 1024 * 1024,
  });

  return {
    code: res.status ?? (res.error ? 1 : 0),
    stdout: res.stdout || "",
    stderr: res.stderr || (res.error ? res.error.message : ""),
  };
}

export function executeLmemAsync(
  args: string[],
  inputText?: string,
  binaryPath?: string
): Promise<{ code: number; stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    try {
      const binary = binaryPath || getBinaryPath();
      const child = spawn(binary, args, {
        stdio: ["pipe", "pipe", "pipe"],
      });

      let stdout = "";
      let stderr = "";

      child.stdout.setEncoding("utf-8");
      child.stdout.on("data", (chunk: string | Buffer) => {
        stdout += chunk;
      });

      child.stderr.setEncoding("utf-8");
      child.stderr.on("data", (chunk: string | Buffer) => {
        stderr += chunk;
      });

      child.on("error", (err: Error) => {
        reject(err);
      });

      child.on("close", (code: number | null) => {
        resolve({
          code: code ?? 0,
          stdout,
          stderr,
        });
      });

      if (inputText !== undefined) {
        child.stdin.write(inputText);
        child.stdin.end();
      } else {
        child.stdin.end();
      }
    } catch (err) {
      reject(err);
    }
  });
}
