---
description: How to build and test lumina using WSL Ubuntu
---

# Build & Test Lumina (WSL)

All compilation and testing must happen through WSL Ubuntu because lumina depends on native C/C++ libraries (tree-sitter, openssl, protobuf, lancedb).

## Prerequisites (one-time)
// turbo-all

1. Install build dependencies in WSL:
```bash
wsl -d Ubuntu -e bash -c "echo ramone | sudo -S apt install -y build-essential pkg-config libssl-dev cmake protobuf-compiler clang git"
```

2. Install Rust in WSL (if not already):
```bash
wsl -d Ubuntu -e bash -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
```

## Build Commands

### cargo check
```powershell
wsl -d Ubuntu -e bash -c "source ~/.cargo/env && export CARGO_TARGET_DIR=~/.cargo-target/lumina && cd /mnt/c/Users/jfzum/OneDrive/Documentos/Proyectos/lumina && cargo check"
```

### cargo test
```powershell
wsl -d Ubuntu -e bash -c "source ~/.cargo/env && export CARGO_TARGET_DIR=~/.cargo-target/lumina && cd /mnt/c/Users/jfzum/OneDrive/Documentos/Proyectos/lumina && cargo test"
```

### cargo build (release)
```powershell
wsl -d Ubuntu -e bash -c "source ~/.cargo/env && export CARGO_TARGET_DIR=~/.cargo-target/lumina && cd /mnt/c/Users/jfzum/OneDrive/Documentos/Proyectos/lumina && cargo build --release"
```

## Notes
- `CARGO_TARGET_DIR` is set to a Linux-native path to avoid cross-filesystem permission issues with `/mnt/c/` (OneDrive).
- The source code lives on Windows (`/mnt/c/...`) but the build artifacts go to `~/.cargo-target/lumina` inside WSL.
