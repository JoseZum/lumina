#!/usr/bin/env node

/**
 * Lumina postinstall script.
 *
 * Resolution order:
 * 1. Platform npm package (@lumina-search/<platform>) already has the binary → done.
 * 2. Download pre-built binary from GitHub Releases → done.
 * 3. Fail with a clear message (never build from source, never use WSL).
 */

const https = require("https");
const fs = require("fs");
const path = require("path");
const os = require("os");

const PKG = path.resolve(__dirname, "..");
const BIN_DIR = path.join(PKG, "bin");

// ── Platform mapping ──

const PLATFORMS = {
  "win32-x64": {
    pkg: "@lumina-search/win32-x64",
    bin: "bin/lumina.exe",
    artifact: "lumina-windows.exe",
    localBin: "lumina-bin.exe",
  },
  "darwin-arm64": {
    pkg: "@lumina-search/darwin-arm64",
    bin: "bin/lumina",
    artifact: "lumina-macos-arm64",
    localBin: "lumina-bin",
  },
  "darwin-x64": {
    pkg: "@lumina-search/darwin-x64",
    bin: "bin/lumina",
    artifact: "lumina-macos-x64",
    localBin: "lumina-bin",
  },
  "linux-x64": {
    pkg: "@lumina-search/linux-x64",
    bin: "bin/lumina",
    artifact: "lumina-linux-x64",
    localBin: "lumina-bin",
  },
  "linux-arm64": {
    pkg: "@lumina-search/linux-arm64",
    bin: "bin/lumina",
    artifact: "lumina-linux-arm64",
    localBin: "lumina-bin",
  },
};

function log(msg) {
  console.log(`[lumina] ${msg}`);
}

function err(msg) {
  console.error(`[lumina] ${msg}`);
}

// ── Check if platform package already has the binary ──

function hasPlatformBinary(info) {
  try {
    const pkgDir = path.dirname(require.resolve(`${info.pkg}/package.json`));
    const binPath = path.join(pkgDir, info.bin);
    return fs.existsSync(binPath);
  } catch {
    return false;
  }
}

// ── Download with redirect following ──

function downloadFile(url, dest, maxRedirects = 5) {
  return new Promise((resolve, reject) => {
    if (maxRedirects <= 0) {
      return reject(new Error("Too many redirects"));
    }

    const proto = url.startsWith("https") ? https : require("http");
    proto.get(url, (response) => {
      if (response.statusCode === 302 || response.statusCode === 301) {
        return downloadFile(response.headers.location, dest, maxRedirects - 1)
          .then(resolve)
          .catch(reject);
      }

      if (response.statusCode !== 200) {
        return reject(new Error(`HTTP ${response.statusCode}`));
      }

      const file = fs.createWriteStream(dest);
      const totalBytes = parseInt(response.headers["content-length"], 10);
      let downloadedBytes = 0;

      response.on("data", (chunk) => {
        downloadedBytes += chunk.length;
        if (totalBytes) {
          const pct = ((downloadedBytes / totalBytes) * 100).toFixed(0);
          process.stdout.write(`\r[lumina] Downloading... ${pct}%`);
        }
      });

      response.pipe(file);

      file.on("finish", () => {
        file.close();
        if (totalBytes) console.log("");
        resolve();
      });

      file.on("error", (e) => {
        fs.unlink(dest, () => {});
        reject(e);
      });
    }).on("error", reject);
  });
}

// ── Main ──

async function main() {
  const platformKey = `${os.platform()}-${os.arch()}`;

  // Skip in CI
  if (process.env.CI) {
    log("CI detected — skipping binary installation");
    return;
  }

  const info = PLATFORMS[platformKey];

  if (!info) {
    err(`Unsupported platform: ${platformKey}`);
    err(`Lumina supports: ${Object.keys(PLATFORMS).join(", ")}`);
    err("See https://github.com/JoseZum/lumina for details.");
    process.exit(1);
  }

  log(`Installing for ${platformKey}...`);

  // 1. Check if platform npm package already has the binary
  if (hasPlatformBinary(info)) {
    log("Binary found via platform package. Done!");
    return;
  }

  // 2. Download from GitHub Releases
  const packageJson = require(path.join(PKG, "package.json"));
  const version = packageJson.version;
  const url = `https://github.com/JoseZum/lumina/releases/download/v${version}/${info.artifact}`;
  const dest = path.join(BIN_DIR, info.localBin);

  if (!fs.existsSync(BIN_DIR)) {
    fs.mkdirSync(BIN_DIR, { recursive: true });
  }

  log(`Downloading binary from GitHub Releases (v${version})...`);

  try {
    await downloadFile(url, dest);

    // Make executable on Unix
    if (os.platform() !== "win32") {
      fs.chmodSync(dest, 0o755);
    }

    log("Installation complete!");
    log("Run 'lumina --help' to get started.");
  } catch (error) {
    err(`Download failed: ${error.message}`);
    err("");
    err("This usually means:");
    err(`  - Release v${version} doesn't have a binary for ${platformKey}`);
    err("  - GitHub is temporarily unreachable");
    err("");
    err("You can build from source instead:");
    err("  1. Install Rust: https://rustup.rs");
    err("  2. Run: cargo build --release");
    err(`  3. Copy target/release/lumina to ${BIN_DIR}/`);
    process.exit(1);
  }
}

main().catch((error) => {
  err(`Installation failed: ${error.message}`);
  process.exit(1);
});
