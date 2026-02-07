#!/bin/bash
set -e

# Installation script for tooler (Rust version)
# This script downloads and installs tooler
# If TOOLER_VERSION is set, it installs that specific version
# Otherwise it installs the latest release

# Embedded version (set during release build via sed substitution)
# RELEASE_VERSION_MARKER_START
TOOLER_VERSION=""
# RELEASE_VERSION_MARKER_END

if [[ -n "$TOOLER_VERSION" ]]; then
  echo "🚀 Installing tooler $TOOLER_VERSION..."
else
  echo "🚀 Installing tooler (latest)..."
fi

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

# Get version to install
if [[ -n "$TOOLER_VERSION" ]]; then
  # Embedded version from this script's release
  TAG="$TOOLER_VERSION"
  echo "📦 Installing release version: $TAG"
else
  # Fetch latest release info
  echo "📡 Fetching latest release..."
  RELEASE_INFO=$(curl -fsSL "https://api.github.com/repos/morgaesis/tooler/releases/latest")
  TAG=$(echo "$RELEASE_INFO" | grep -o '"tag_name": "[^"]*' | cut -d'"' -f4)

  if [[ -z "$TAG" ]]; then
    echo "❌ Failed to fetch release information"
    exit 1
  fi

  echo "📦 Found latest version: $TAG"
fi

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
curl -fsSL -o "$ASSET_NAME" "$DOWNLOAD_URL"

# Extract
echo "📂 Extracting..."
if [[ "$ASSET_NAME" == *.tar.gz ]]; then
  tar -xzf "$ASSET_NAME"
elif [[ "$ASSET_NAME" == *.zip ]]; then
  unzip -q "$ASSET_NAME"
fi

# Install
INSTALL_DIR="$HOME/.local/share/tooler/bin"
mkdir -p "$INSTALL_DIR"

echo "📋 Installing tooler to $INSTALL_DIR..."
# Remove existing installation if present
if [[ -f "$INSTALL_DIR/tooler" ]]; then
    echo "📝 Removing existing installation..."
    rm -f "$INSTALL_DIR/tooler"
fi
# Move new binary into place
mv "$BINARY_NAME" "$INSTALL_DIR/tooler"
chmod +x "$INSTALL_DIR/tooler"

# Update PATH in shell RC files
echo "📝 Updating PATH in shell configuration..."
PATH_LINE="export PATH=\"$INSTALL_DIR:\$PATH\""

update_rc() {
    local rc_file="$1"
    if [[ -f "$rc_file" ]]; then
        if ! grep -q "$INSTALL_DIR" "$rc_file"; then
            echo "" >> "$rc_file"
            echo "# Tooler PATH" >> "$rc_file"
            echo "$PATH_LINE" >> "$rc_file"
            echo "✅ Updated $rc_file"
        fi
    fi
}

update_rc "$HOME/.bashrc"
update_rc "$HOME/.zshrc"

# Cleanup
cd /
rm -rf "$TEMP_DIR"

echo "✅ Installation complete!"
echo "🚀 Tooler and its managed tools are now in your PATH."
echo "💡 Restart your shell or run: source ~/.bashrc (or ~/.zshrc)"
