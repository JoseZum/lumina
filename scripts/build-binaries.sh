#!/bin/bash
# Build pre-compiled binaries for all platforms

set -e

VERSION=$(node -p "require('./package.json').version")
echo "Building Lumina v$VERSION binaries..."

# Create release directory
mkdir -p release

# Build for current platform (WSL = Linux)
echo "Building for Linux x64..."
cargo build --release
cp target/release/lumina release/lumina-linux-x64

echo ""
echo "Binaries built successfully in release/"
echo ""
echo "To create a GitHub release:"
echo "1. git tag v$VERSION"
echo "2. git push --tags"
echo "3. gh release create v$VERSION release/* --title \"v$VERSION\" --notes \"Release v$VERSION\""
echo ""
echo "Or upload manually to: https://github.com/JoseZum/lumina/releases/new"
