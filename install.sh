#!/bin/bash

: "${BINDIR:=$HOME/.local/bin}"
: "${RAW_URL_BASE:=https://raw.githubusercontent.com/morgaesis/tooler/refs/heads/main}"
: "${TOOLER_URL:=$RAW_URL_BASE/tooler.py}"
: "${REQUIREMENTS_URL:=$RAW_URL_BASE/requirements.txt}"

mkdir -p "$BINDIR"
curl -sfLo "$BINDIR/tooler" "$TOOLER_URL"
chmod +x "$BINDIR/tooler"

if grep -Ei 'ID="?ubuntu"?' /etc/os-release >/dev/null; then
  apt-get install -y python3-tqdm
else
  req_file=$(mktemp 'tooler-requirements-XXXXXX' --tmpdir)
  curl -sfLo "$req_file" "$REQUIREMENTS_URL"
  python3 -m pip install -r "$req_file"
  rm "$req_file"
fi
