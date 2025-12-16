#!/bin/bash

set -euo pipefail

# Installation script for tooler (Rust version)
# This script downloads and installs the latest release of tooler

: "${REPO_URL:=https://github.com/morgaesis/tooler}"
: "${BIN_DIR:=${XDG_BIN_DIR:-$HOME/.local/bin}}"
: "${TOOLER_REPO_DIR:=${XDG_DATA_DIR:-$HOME/.local/share/tooler/.repo}}"
: "${VENV_DIR:=${TOOLER_REPO_DIR}/.venv}"

# Prepare workdir
rm -rf "${TOOLER_REPO_DIR}"
mkdir -p "${TOOLER_REPO_DIR}"
git clone "${REPO_URL}" "${TOOLER_REPO_DIR}"
cd "${TOOLER_REPO_DIR}"

# Install/setup
git stash || :
git pull
cargo install --path .

echo "✅ Installation complete!"
echo ""
echo "🎯 Add to PATH:"
echo "   export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "🚀 Run tooler:"
echo "   tooler --help"
