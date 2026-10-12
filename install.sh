#!/bin/bash
set -eo pipefail

# Installation script for tooler (Rust version)
# This script downloads and installs tooler
# If TOOLER_VERSION is set, it installs that specific version
# Otherwise it installs the latest release

# Embedded version (set during release build via sed substitution)
# RELEASE_VERSION_MARKER_START
TOOLER_VERSION="${TOOLER_VERSION:-}"
# RELEASE_VERSION_MARKER_END

if [[ -n "$TOOLER_VERSION" ]]; then
  echo "Installing tooler $TOOLER_VERSION..."
else
  echo "Installing tooler (latest)..."
fi

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')

case $OS in
mingw* | msys* | cygwin*) OS="windows" ;;
esac

if [[ "$OS" == "windows" ]]; then
  NATIVE_ARCH=$(printf '%s' "${PROCESSOR_ARCHITEW6432:-${PROCESSOR_ARCHITECTURE:-}}" | tr '[:upper:]' '[:lower:]')
  case $NATIVE_ARCH in
  amd64 | x86_64) ARCH="amd64" ;;
  aarch64 | arm64) ARCH="arm64" ;;
  esac
fi

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

# Convert native Windows paths for Git Bash, MSYS, and Cygwin.
windows_shell_path() {
  if command -v cygpath &>/dev/null; then
    cygpath -u -- "$1"
  else
    local path="${1//\\//}"
    if [[ "$path" == [[:alpha:]]:/* ]]; then
      local drive
      drive=$(printf '%s' "${path:0:1}" | tr '[:upper:]' '[:lower:]')
      path="/$drive/${path:3}"
    fi
    printf '%s\n' "$path"
  fi
}

if [[ -n "${TOOLER_BIN_DIR:-}" ]]; then
  INSTALL_DIR="$TOOLER_BIN_DIR"
elif [[ "$OS" == "windows" ]]; then
  LOCAL_APP_DATA="${LOCALAPPDATA:-}"
  if [[ -z "$LOCAL_APP_DATA" && -n "${USERPROFILE:-}" ]]; then
    LOCAL_APP_DATA="$USERPROFILE/AppData/Local"
  fi
  if [[ -z "$LOCAL_APP_DATA" ]]; then
    echo "ERROR: Set LOCALAPPDATA, USERPROFILE, or TOOLER_BIN_DIR for Windows installation."
    exit 1
  fi
  INSTALL_DIR="$LOCAL_APP_DATA/tooler/bin"
else
  INSTALL_DIR="$HOME/.local/share/tooler/bin"
fi
if [[ "$OS" == "windows" ]]; then
  INSTALL_DIR=$(windows_shell_path "$INSTALL_DIR")
fi
# Resolve relative overrides before changing to the extraction directory.
if [[ "$INSTALL_DIR" != /* ]]; then
  INSTALL_DIR="$PWD/$INSTALL_DIR"
fi
INSTALL_PATH="$INSTALL_DIR/$BINARY_NAME"

# Get version to install
if [[ -n "$TOOLER_VERSION" ]]; then
  # Embedded version from this script's release
  TAG="$TOOLER_VERSION"
  echo "Installing release version: $TAG"
else
  # Fetch latest release info - try gh CLI first (authenticated), fall back to curl
  echo "Fetching latest release..."
  if command -v gh &>/dev/null; then
    TAG=$(gh api repos/morgaesis/tooler/releases/latest --jq '.tag_name' 2>/dev/null || true)
  fi

  if [[ -z "$TAG" ]]; then
    CURL_HEADERS=(-H "User-Agent: tooler-install.sh")
    if [[ -n "${GITHUB_TOKEN:-}" ]]; then
      CURL_HEADERS+=(-H "Authorization: Bearer $GITHUB_TOKEN")
    fi

    RELEASE_INFO=$(curl -fsSL "${CURL_HEADERS[@]}" "https://api.github.com/repos/morgaesis/tooler/releases/latest" 2>/dev/null || true)
    if command -v jq &>/dev/null; then
      TAG=$(echo "$RELEASE_INFO" | jq -r '.tag_name // empty' 2>/dev/null || true)
    fi
    if [[ -z "$TAG" ]]; then
      TAG=$(echo "$RELEASE_INFO" | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/' || true)
    fi
  fi

  if [[ -z "$TAG" ]]; then
    echo "ERROR: Failed to fetch release information."
    echo "This is likely due to GitHub API rate limiting for unauthenticated requests."
    echo ""
    echo "Workarounds:"
    echo "  1. Install 'gh' CLI and authenticate: gh auth login"
    echo "  2. Download the latest release directly from: https://github.com/morgaesis/tooler/releases/latest"
    echo "  3. Set TOOLER_VERSION manually to a release tag from: https://github.com/morgaesis/tooler/releases"
    exit 1
  fi

  echo "Found latest version: $TAG"
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
windows-arm64) ASSET_NAME="tooler-$TAG-aarch64-pc-windows-msvc.zip" ;;
esac

if [[ -z "$ASSET_NAME" ]]; then
  echo "ERROR: No pre-built binary available for $OS-$ARCH"
  echo "Install from source instead:"
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

echo "Downloading $ASSET_NAME..."
curl -fsSL -o "$ASSET_NAME" "$DOWNLOAD_URL"

# Extract
echo "Extracting..."
if [[ "$ASSET_NAME" == *.tar.gz ]]; then
  tar -xzf "$ASSET_NAME"
elif [[ "$ASSET_NAME" == *.zip ]]; then
  unzip -q "$ASSET_NAME"
fi

# Install
mkdir -p "$INSTALL_DIR"

echo "Installing tooler to $INSTALL_DIR..."
# Move new binary into place
mv "$BINARY_NAME" "$INSTALL_PATH"
chmod +x "$INSTALL_PATH"
if [[ "$OS" == "windows" ]]; then
  # Preserve a legacy native manager without shadowing the current .exe.
  # Leave extensionless shell wrappers untouched.
  # MSYS file tests can resolve "tooler" to "tooler.exe". Enumerate the
  # actual directory entry so a fresh installation cannot archive itself.
  for LEGACY_PATH in "$INSTALL_DIR"/tooler*; do
    if [[ "${LEGACY_PATH##*/}" == tooler && -f "$LEGACY_PATH" && "$(head -c 2 "$LEGACY_PATH")" == MZ ]]; then
      LEGACY_BACKUP=$(mktemp "$INSTALL_DIR/.tooler-legacy-XXXXXX")
      mv "$LEGACY_PATH" "$LEGACY_BACKUP"
      echo "Preserved the extensionless executable at $LEGACY_BACKUP"
    fi
  done
fi

# Update PATH in shell RC files
echo "Updating PATH in shell configuration..."
printf -v PATH_LINE 'export PATH=%q:"$PATH"' "$INSTALL_DIR"

update_rc() {
    local rc_file="$1"
    if [[ -f "$rc_file" ]]; then
        if ! grep -Fxq -- "$PATH_LINE" "$rc_file"; then
            echo "" >> "$rc_file"
            echo "# Tooler PATH" >> "$rc_file"
            echo "$PATH_LINE" >> "$rc_file"
            echo "Updated $rc_file"
        fi
    fi
}

update_rc "$HOME/.bashrc"
update_rc "$HOME/.zshrc"

# Cleanup
cd /
rm -rf "$TEMP_DIR"

# Bootstrap: register tooler as a tool managed by itself for future self-updates
export PATH="$INSTALL_DIR:$PATH"
if [[ "$OS" == "windows" ]] && command -v cygpath &>/dev/null; then
  export TOOLER_BIN_DIR="$(cygpath -w -- "$INSTALL_DIR")"
elif [[ "$OS" == "windows" && "$INSTALL_DIR" == /[[:alpha:]]/* ]]; then
  export TOOLER_BIN_DIR="${INSTALL_DIR:1:1}:${INSTALL_DIR:2}"
else
  export TOOLER_BIN_DIR="$INSTALL_DIR"
fi
if [[ "${TOOLER_NO_BOOTSTRAP:-}" == "1" || "${TOOLER_NO_BOOTSTRAP:-}" == "true" ]]; then
  echo "Skipping self-update registration."
else
  if [[ ! -x "$INSTALL_PATH" ]]; then
    echo "ERROR: Installed tooler binary was not found at $INSTALL_PATH"
    exit 1
  fi
  echo "Registering tooler for self-updates..."
  "$INSTALL_PATH" pull morgaesis/tooler 2>/dev/null || true
fi

echo "Installation complete!"
echo "Tooler and its managed tools are now in your PATH."
echo "Restart your shell or run: source ~/.bashrc (or ~/.zshrc)"
echo ""
echo "Future updates: tooler update tooler"
