#!/usr/bin/env node

const { execSync } = require("child_process");
const https = require("https");
const fs = require("fs");
const path = require("path");
const os = require("os");

const PKG = path.resolve(__dirname, "..");
const BIN_DIR = path.join(PKG, "bin");
const platform = os.platform();
const arch = os.arch();

function log(msg) {
  console.log(`[lumina] ${msg}`);
}

function err(msg) {
  console.error(`[lumina] ${msg}`);
}

// Detect platform and binary name
function getPlatformInfo() {
  let platformName, binaryName;

  if (platform === "win32") {
    platformName = "windows";
    binaryName = "lumina.exe";
  } else if (platform === "darwin") {
    platformName = arch === "arm64" ? "macos-arm64" : "macos-x64";
    binaryName = "lumina";
  } else if (platform === "linux") {
    platformName = "linux-x64";
    binaryName = "lumina";
  } else {
    return null;
  }

  return { platformName, binaryName };
}

// Download file with progress
function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);

    https.get(url, (response) => {
      if (response.statusCode === 302 || response.statusCode === 301) {
        // Follow redirect
        return downloadFile(response.headers.location, dest).then(resolve).catch(reject);
      }

      if (response.statusCode !== 200) {
        reject(new Error(`Failed to download: ${response.statusCode}`));
        return;
      }

      const totalBytes = parseInt(response.headers['content-length'], 10);
      let downloadedBytes = 0;

      response.on('data', (chunk) => {
        downloadedBytes += chunk.length;
        const percent = ((downloadedBytes / totalBytes) * 100).toFixed(1);
        process.stdout.write(`\r[lumina] Downloading binary... ${percent}%`);
      });

      response.pipe(file);

      file.on('finish', () => {
        file.close();
        console.log(""); // New line after progress
        resolve();
      });
    }).on('error', (err) => {
      fs.unlink(dest, () => {});
      reject(err);
    });
  });
}

// Try to build from source (fallback)
function buildFromSource() {
  log("Pre-built binary not available. Building from source...");
  log("This requires Rust to be installed: https://rustup.rs");

  try {
    execSync("cargo --version", { stdio: "ignore" });
  } catch {
    err("Rust not found. Install Rust from https://rustup.rs");
    err("Then run: npm rebuild lumina-search");
    process.exit(1);
  }

  log("Building (this may take 5-10 minutes)...");
  try {
    execSync("cargo build --release", {
      cwd: PKG,
      stdio: "inherit",
      timeout: 600000,
    });

    const src = path.join(PKG, "target", "release", getPlatformInfo().binaryName);
    const dst = path.join(BIN_DIR, "lumina-bin");

    if (fs.existsSync(src)) {
      fs.copyFileSync(src, dst);
      if (platform !== "win32") {
        fs.chmodSync(dst, 0o755);
      }
      log("Build complete!");
    }
  } catch (error) {
    err("Build failed: " + error.message);
    process.exit(1);
  }
}

async function main() {
  log(`Installing for ${platform}-${arch}...`);

  // Skip in CI
  if (process.env.CI) {
    log("CI detected — skipping binary installation");
    return;
  }

  const platformInfo = getPlatformInfo();

  if (!platformInfo) {
    err(`Unsupported platform: ${platform}-${arch}`);
    err("Lumina supports: Windows x64, macOS (x64/arm64), Linux x64");
    process.exit(1);
  }

  const { platformName, binaryName } = platformInfo;

  // Get latest release version
  const packageJson = require(path.join(PKG, "package.json"));
  const version = packageJson.version;

  const binaryUrl = `https://github.com/JoseZum/lumina/releases/download/v${version}/lumina-${platformName}${binaryName === "lumina.exe" ? ".exe" : ""}`;
  const binaryPath = path.join(BIN_DIR, "lumina-bin" + (binaryName === "lumina.exe" ? ".exe" : ""));

  // Ensure bin directory exists
  if (!fs.existsSync(BIN_DIR)) {
    fs.mkdirSync(BIN_DIR, { recursive: true });
  }

  log(`Downloading pre-built binary from GitHub Releases...`);
  log(`URL: ${binaryUrl}`);

  try {
    await downloadFile(binaryUrl, binaryPath);

    // Make executable on Unix
    if (platform !== "win32") {
      fs.chmodSync(binaryPath, 0o755);
    }

    log("Installation complete!");
    log(`Binary installed to: ${binaryPath}`);
    log("");
    log("Run 'lumina --help' to get started");
  } catch (error) {
    log(`Download failed: ${error.message}`);
    log("Falling back to building from source...");
    buildFromSource();
  }
}

main().catch((error) => {
  err("Installation failed: " + error.message);
  process.exit(1);
});
