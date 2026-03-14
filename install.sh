#!/usr/bin/env bash
set -euo pipefail

echo "🔧 Lumina Installer"
echo ""

# 1. Check for WSL (Windows only)
if [[ "$(uname -s)" == *"MINGW"* ]] || [[ "$(uname -s)" == *"MSYS"* ]]; then
    echo "❌ Please run this from WSL Ubuntu, not Git Bash"
    echo "   Open WSL and run: bash /mnt/c/path/to/lumina/install.sh"
    exit 1
fi

# 2. Check for Rust
if ! command -v cargo &> /dev/null; then
    echo "📦 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# 3. Check for build tools
if ! command -v gcc &> /dev/null; then
    echo "📦 Installing build tools..."
    sudo apt-get update
    sudo apt-get install -y build-essential
fi

# 4. Determine install location
INSTALL_DIR="$HOME/.local/share/lumina"
BIN_DIR="$HOME/.local/bin"

echo "📂 Installing to: $INSTALL_DIR"

# 5. Copy project to Linux filesystem
echo "📋 Copying project to WSL..."
rm -rf "$INSTALL_DIR"
mkdir -p "$INSTALL_DIR"

# If run from Windows path, copy from there
if [[ "$PWD" == /mnt/* ]]; then
    cp -r "$PWD"/* "$INSTALL_DIR/"
else
    # Already on Linux filesystem
    cp -r . "$INSTALL_DIR/"
fi

cd "$INSTALL_DIR"

# Fix line endings
echo "🔧 Fixing line endings..."
find . -name "*.rs" -exec sed -i 's/\r$//' {} +
find . -name "*.toml" -exec sed -i 's/\r$//' {} +

# 6. Build release binary
echo "🔨 Building lumina (this takes ~5 minutes)..."
source "$HOME/.cargo/env"
cargo build --release

# 7. Create wrapper script
echo "📝 Creating wrapper script..."
mkdir -p "$BIN_DIR"

cat > "$BIN_DIR/lumina" << 'EOF'
#!/usr/bin/env bash
LUMINA_BIN="$HOME/.local/share/lumina/target/release/lumina"
exec "$LUMINA_BIN" "$@"
EOF

chmod +x "$BIN_DIR/lumina"

# 8. Add to PATH if not already there
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
    echo "📌 Adding to PATH..."

    # Detect shell
    if [[ -f "$HOME/.bashrc" ]]; then
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
        echo "   Added to ~/.bashrc"
    fi

    if [[ -f "$HOME/.zshrc" ]]; then
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.zshrc"
        echo "   Added to ~/.zshrc"
    fi

    export PATH="$BIN_DIR:$PATH"
fi

# 9. Verify installation
echo ""
echo "✅ Installation complete!"
echo ""
echo "📍 Binary location: $BIN_DIR/lumina"
echo "📍 Source location: $INSTALL_DIR"
echo ""
echo "🚀 Quick start:"
echo "   1. Get a Voyage API key: https://www.voyageai.com/"
echo "   2. Set it: export VOYAGE_API_KEY='pa-your-key'"
echo "   3. Index a repo: lumina index --repo /path/to/repo"
echo "   4. Query: lumina query 'authentication' --repo /path/to/repo"
echo ""
echo "📘 MCP setup for Claude Code:"
echo "   Add to your .claude.json:"
echo '   {'
echo '     "mcpServers": {'
echo '       "lumina": {'
echo '         "command": "wsl",'
echo '         "args": ["-d", "Ubuntu", "-e", "lumina", "mcp", "--repo", "/mnt/c/path/to/repo"],'
echo '         "env": { "VOYAGE_API_KEY": "pa-your-key" }'
echo '       }'
echo '     }'
echo '   }'
echo ""
echo "⚠️  Restart your shell to use 'lumina' command"
