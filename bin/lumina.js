#!/usr/bin/env node

const { spawn } = require("child_process");
const path = require("path");
const fs = require("fs");
const os = require("os");

// Platform → npm package name mapping
const PLATFORM_PACKAGES = {
  "win32-x64": { pkg: "@lumina-search/win32-x64", bin: "bin/lumina.exe" },
  "darwin-arm64": { pkg: "@lumina-search/darwin-arm64", bin: "bin/lumina" },
  "darwin-x64": { pkg: "@lumina-search/darwin-x64", bin: "bin/lumina" },
  "linux-x64": { pkg: "@lumina-search/linux-x64", bin: "bin/lumina" },
  "linux-arm64": { pkg: "@lumina-search/linux-arm64", bin: "bin/lumina" },
};

function getPlatformKey() {
  return `${os.platform()}-${os.arch()}`;
}

function findBinary() {
  const platformKey = getPlatformKey();
  const pkg = path.resolve(__dirname, "..");
  const isWindows = os.platform() === "win32";
  const ext = isWindows ? ".exe" : "";

  // 1. Try platform-specific npm package (Phase 2 — best path)
  const platformInfo = PLATFORM_PACKAGES[platformKey];
  if (platformInfo) {
    try {
      const pkgDir = path.dirname(require.resolve(`${platformInfo.pkg}/package.json`));
      const binPath = path.join(pkgDir, platformInfo.bin);
      if (fs.existsSync(binPath)) {
        return binPath;
      }
    } catch {
      // Package not installed — fall through
    }
  }

  // 2. Try pre-downloaded binary (from postinstall)
  const localBin = path.join(pkg, "bin", `lumina-bin${ext}`);
  if (fs.existsSync(localBin)) {
    return localBin;
  }

  // 3. Try cargo-built binary (dev / build-from-source)
  const cargoBin = path.join(pkg, "target", "release", `lumina${ext}`);
  if (fs.existsSync(cargoBin)) {
    return cargoBin;
  }

  return null;
}

function main() {
  const args = process.argv.slice(2);
  const binaryPath = findBinary();

  if (!binaryPath) {
    const platformKey = getPlatformKey();
    const supported = Object.keys(PLATFORM_PACKAGES);

    if (!supported.includes(platformKey)) {
      console.error(`Error: Unsupported platform: ${platformKey}`);
      console.error(`Lumina supports: ${supported.join(", ")}`);
      process.exit(1);
    }

    console.error("Error: Lumina binary not found.");
    console.error("");
    console.error("Try reinstalling:");
    console.error("  npm install -g lumina-search");
    console.error("");
    console.error("Or build from source:");
    console.error("  cargo build --release");
    process.exit(1);
  }

  const child = spawn(binaryPath, args, {
    stdio: "inherit",
    env: process.env,
  });

  child.on("exit", (code) => process.exit(code || 0));
  child.on("error", (err) => {
    console.error(`Error executing lumina: ${err.message}`);
    process.exit(1);
  });
}

main();
