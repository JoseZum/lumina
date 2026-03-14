#!/usr/bin/env node

const { spawn } = require("child_process");
const path = require("path");
const fs = require("fs");
const os = require("os");

// ── Find the binary ──

function findBinary() {
  const platform = os.platform();
  const pkg = path.resolve(__dirname, "..");

  // 1. Check for pre-built binary next to this script
  const localBin =
    platform === "win32"
      ? path.join(pkg, "bin", "lumina.exe")
      : path.join(pkg, "bin", "lumina-bin");

  if (fs.existsSync(localBin)) {
    return { type: "local", path: localBin };
  }

  // 2. Check for cargo-built binary in target/release
  const cargoBin = path.join(pkg, "target", "release", "lumina");
  if (fs.existsSync(cargoBin)) {
    return { type: "local", path: cargoBin };
  }

  const cargoBinExe = path.join(pkg, "target", "release", "lumina.exe");
  if (fs.existsSync(cargoBinExe)) {
    return { type: "local", path: cargoBinExe };
  }

  // 3. Check if installed in WSL (Windows only)
  if (platform === "win32") {
    return { type: "wsl", path: "$HOME/.local/share/lumina/target/release/lumina" };
  }

  // 4. Check global install in ~/.local/share/lumina
  const globalBin = path.join(
    os.homedir(),
    ".local",
    "share",
    "lumina",
    "target",
    "release",
    "lumina"
  );
  if (fs.existsSync(globalBin)) {
    return { type: "local", path: globalBin };
  }

  return null;
}

// ── Resolve repo path for WSL ──

function toWslPath(winPath) {
  // Convert C:\Users\foo → /mnt/c/Users/foo
  const resolved = path.resolve(winPath);
  const drive = resolved.charAt(0).toLowerCase();
  const rest = resolved.slice(2).replace(/\\/g, "/");
  return `/mnt/${drive}${rest}`;
}

// ── Main ──

function main() {
  const args = process.argv.slice(2);
  const binary = findBinary();

  if (!binary) {
    console.error("Error: Lumina binary not found.");
    console.error("");
    console.error("Run the postinstall script to build it:");
    console.error("  node scripts/postinstall.js");
    console.error("");
    console.error("Or build manually:");
    console.error("  cargo build --release");
    process.exit(1);
  }

  // Convert --repo paths to WSL paths if on Windows + WSL mode
  let finalArgs = [...args];

  if (binary.type === "wsl") {
    // Rewrite --repo argument to WSL path
    const repoIdx = finalArgs.indexOf("--repo");
    if (repoIdx !== -1 && repoIdx + 1 < finalArgs.length) {
      const repoPath = finalArgs[repoIdx + 1];
      // Only convert if it looks like a Windows path
      if (/^[A-Za-z]:/.test(repoPath) || repoPath.includes("\\")) {
        finalArgs[repoIdx + 1] = toWslPath(repoPath);
      }
    }

    // If --repo is not specified, default to current directory
    if (!finalArgs.includes("--repo") && !["--help", "--version", "-h", "-V"].some(f => finalArgs.includes(f))) {
      const subcommands = ["index", "query", "mcp", "status"];
      if (subcommands.some(s => finalArgs.includes(s))) {
        finalArgs.push("--repo", toWslPath(process.cwd()));
      }
    }

    // Build env string for WSL
    const envParts = [];
    if (process.env.VOYAGE_API_KEY) {
      envParts.push(`VOYAGE_API_KEY=${process.env.VOYAGE_API_KEY}`);
    }

    const envPrefix = envParts.length > 0 ? envParts.join(" ") + " " : "";
    const cmd = `source $HOME/.cargo/env 2>/dev/null; ${envPrefix}${binary.path} ${finalArgs.join(" ")}`;

    const child = spawn("wsl", ["-e", "bash", "-c", cmd], {
      stdio: "inherit",
      env: process.env,
    });

    child.on("exit", (code) => process.exit(code || 0));
    child.on("error", (err) => {
      if (err.code === "ENOENT") {
        console.error("Error: WSL not found. Install WSL first:");
        console.error("  wsl --install");
      } else {
        console.error("Error:", err.message);
      }
      process.exit(1);
    });
  } else {
    // Direct binary execution (Linux/Mac or Windows with local binary)
    const child = spawn(binary.path, finalArgs, {
      stdio: "inherit",
      env: process.env,
    });

    child.on("exit", (code) => process.exit(code || 0));
    child.on("error", (err) => {
      console.error("Error:", err.message);
      process.exit(1);
    });
  }
}

main();
