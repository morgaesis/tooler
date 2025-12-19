#!/bin/bash
set -e

# Installation script for tooler (Rust version)
# This script downloads and installs the latest release of tooler

echo "🚀 Installing tooler..."

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')

case $ARCH in
x86_64) ARCH="amd64" ;;
aarch64 | arm64) ARCH="arm64" ;;
arm*) ARCH="arm" ;;
esac

# Determine binary name
BINARY_NAME="tooler"
if [[ "$OS" == "windows" ]]; then
  BINARY_NAME="tooler.exe"
fi

# Get latest release info
echo "📡 Fetching latest release..."
RELEASE_INFO=$(curl -s "https://api.github.com/repos/morgaesis/tooler/releases/latest")
TAG=$(echo "$RELEASE_INFO" | grep -o '"tag_name": "[^"]*' | cut -d'"' -f4)

if [[ -z "$TAG" ]]; then
  echo "❌ Failed to fetch release information"
  exit 1
fi

echo "📦 Found latest version: $TAG"

# Find appropriate asset
ASSET_NAME=""
case "$OS-$ARCH" in
linux-amd64) ASSET_NAME="tooler-$TAG-x86_64-unknown-linux-gnu.tar.gz" ;;
linux-arm64) ASSET_NAME="tooler-$TAG-aarch64-unknown-linux-gnu.tar.gz" ;;
linux-arm) ASSET_NAME="tooler-$TAG-armv7-unknown-linux-gnueabihf.tar.gz" ;;
darwin-amd64) ASSET_NAME="tooler-$TAG-x86_64-apple-darwin.tar.gz" ;;
darwin-arm64) ASSET_NAME="tooler-$TAG-aarch64-apple-darwin.tar.gz" ;;
windows-amd64) ASSET_NAME="tooler-$TAG-x86_64-pc-windows-msvc.zip" ;;
esac

if [[ -z "$ASSET_NAME" ]]; then
  echo "❌ No pre-built binary available for $OS-$ARCH"
  echo "💡 Install from source instead:"
  echo "   git clone https://github.com/morgaesis/tooler"
  echo "   cd tooler"
  echo "   cargo install --path ."
  exit 1
fi

# Download URL
DOWNLOAD_URL="https://github.com/morgaesis/tooler/releases/download/$TAG/$ASSET_NAME"

# Create temporary directory
TEMP_DIR=$(mktemp -d)
cd "$TEMP_DIR"

echo "⬇️  Downloading $ASSET_NAME..."
curl -L -o "$ASSET_NAME" "$DOWNLOAD_URL"

# Extract
echo "📂 Extracting..."
if [[ "$ASSET_NAME" == *.tar.gz ]]; then
  tar -xzf "$ASSET_NAME"
elif [[ "$ASSET_NAME" == *.zip ]]; then
  unzip -q "$ASSET_NAME"
fi

# Install
INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"

echo "📋 Installing to $INSTALL_DIR..."
mv "$BINARY_NAME" "$INSTALL_DIR/tooler"

# Cleanup
cd /
rm -rf "$TEMP_DIR"

echo "✅ Installation complete!"
echo ""
echo "🎯 Add to PATH:"
echo "   export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "🚀 Run tooler:"
echo "   tooler --help"
