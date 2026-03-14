#!/usr/bin/env node

const { execSync, spawnSync } = require("child_process");
const os = require("os");
const path = require("path");
const fs = require("fs");

const PKG = path.resolve(__dirname, "..");
const platform = os.platform();

// ── Helpers ──

function log(msg) {
  console.log(`[lumina] ${msg}`);
}

function err(msg) {
  console.error(`[lumina] ERROR: ${msg}`);
}

function hasCommand(cmd) {
  try {
    if (platform === "win32") {
      execSync(`where ${cmd}`, { stdio: "ignore" });
    } else {
      execSync(`which ${cmd}`, { stdio: "ignore" });
    }
    return true;
  } catch {
    return false;
  }
}

function run(cmd, opts = {}) {
  log(`> ${cmd}`);
  return spawnSync(cmd, {
    shell: true,
    stdio: "inherit",
    cwd: opts.cwd || PKG,
    env: { ...process.env, ...opts.env },
    timeout: opts.timeout || 600000, // 10 min default
  });
}

// ── Platform builders ──

function buildLinuxMac() {
  // Check for Rust
  if (!hasCommand("cargo")) {
    log("Rust not found. Installing via rustup...");
    const res = run('curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y');
    if (res.status !== 0) {
      err("Failed to install Rust. Install manually: https://rustup.rs");
      process.exit(1);
    }
    // Source cargo env for this process
    const cargoEnv = path.join(os.homedir(), ".cargo", "env");
    if (fs.existsSync(cargoEnv)) {
      process.env.PATH = `${path.join(os.homedir(), ".cargo", "bin")}:${process.env.PATH}`;
    }
  }

  log("Building lumina (release)...");
  const res = run("cargo build --release", { timeout: 600000 });
  if (res.status !== 0) {
    err("Build failed. Check that you have build-essential installed:");
    err("  sudo apt-get install -y build-essential");
    process.exit(1);
  }

  // Copy binary to bin/
  const src = path.join(PKG, "target", "release", "lumina");
  const dst = path.join(PKG, "bin", "lumina-bin");
  if (fs.existsSync(src)) {
    fs.copyFileSync(src, dst);
    fs.chmodSync(dst, 0o755);
    log(`Binary installed to bin/lumina-bin`);
  }

  log("Done!");
}

function buildWindows() {
  // Strategy: Build in WSL Ubuntu
  log("Windows detected — building in WSL Ubuntu...");

  // Check WSL
  if (!hasCommand("wsl")) {
    err("WSL not found. Install WSL first:");
    err("  wsl --install");
    err("");
    err("Then run: npm run postinstall");
    process.exit(1);
  }

  // Check Ubuntu in WSL
  const wslList = execSync("wsl --list --quiet", { encoding: "utf-8" });
  if (!wslList.includes("Ubuntu")) {
    err("Ubuntu not found in WSL. Install it:");
    err("  wsl --install -d Ubuntu");
    process.exit(1);
  }

  // Convert package path to WSL path
  const resolved = path.resolve(PKG);
  const drive = resolved.charAt(0).toLowerCase();
  const rest = resolved.slice(2).replace(/\\/g, "/");
  const wslPkg = `/mnt/${drive}${rest}`;

  // Install dir on Linux filesystem (avoids cross-fs build issues)
  const installDir = "$HOME/.local/share/lumina";

  const script = [
    "set -e",
    'source "$HOME/.cargo/env" 2>/dev/null || true',
    "",
    "# Install Rust if needed",
    "if ! command -v cargo &>/dev/null; then",
    '  echo "[lumina] Installing Rust..."',
    "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y",
    '  source "$HOME/.cargo/env"',
    "fi",
    "",
    "# Install build tools if needed",
    "if ! command -v gcc &>/dev/null; then",
    '  echo "[lumina] Installing build tools..."',
    "  sudo apt-get update && sudo apt-get install -y build-essential",
    "fi",
    "",
    `# Copy to Linux filesystem`,
    `INSTALL_DIR="${installDir}"`,
    `rm -rf "$INSTALL_DIR"`,
    `mkdir -p "$INSTALL_DIR"`,
    `cp -r "${wslPkg}"/* "$INSTALL_DIR/"`,
    "",
    "# Fix CRLF line endings",
    `find "$INSTALL_DIR" -name '*.rs' -exec sed -i 's/\\r$//' {} +`,
    `find "$INSTALL_DIR" -name '*.toml' -exec sed -i 's/\\r$//' {} +`,
    "",
    "# Build",
    'echo "[lumina] Building (this may take a few minutes)..."',
    `cd "$INSTALL_DIR"`,
    "cargo build --release",
    "",
    'echo "[lumina] Build complete!"',
  ].join("\n");

  const res = spawnSync("wsl", ["-d", "Ubuntu", "-e", "bash", "-c", script], {
    stdio: "inherit",
    timeout: 600000,
  });

  if (res.status !== 0) {
    err("WSL build failed.");
    err("Try manually: wsl -d Ubuntu bash -c 'cd ~/.local/share/lumina && cargo build --release'");
    process.exit(1);
  }

  log("Done! Binary built in WSL.");
  log("The lumina command will run via WSL automatically.");
}

// ── Main ──

function main() {
  log("Installing lumina...");
  log(`Platform: ${platform}, Arch: ${os.arch()}`);

  // Skip build in CI environments
  if (process.env.CI) {
    log("CI detected — skipping build. Download pre-built binary instead.");
    return;
  }

  switch (platform) {
    case "linux":
    case "darwin":
      buildLinuxMac();
      break;
    case "win32":
      buildWindows();
      break;
    default:
      err(`Unsupported platform: ${platform}`);
      err("Lumina supports: Linux, macOS, Windows (via WSL)");
      process.exit(1);
  }
}

main();
