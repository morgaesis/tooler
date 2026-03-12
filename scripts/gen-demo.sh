#!/bin/bash
set -e

# Generate the README demo SVG in an isolated tooler environment.
# Requires: asciinema, svg-term (npm i -g svg-term-cli)

DEMO_DIR="/tmp/tooler-demo"
CAST_FILE="$DEMO_DIR/demo.cast"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

rm -rf "$DEMO_DIR"
mkdir -p "$DEMO_DIR"/{data,bin}

cat > "$DEMO_DIR/run.sh" << 'DEMO'
type_cmd() {
    local cmd="$1"
    printf '\033[1;32m%s@tooler\033[0m \033[1;34m~\033[0m \033[0;33m$\033[0m ' "$USER"
    for (( i=0; i<${#cmd}; i++ )); do
        printf '%s' "${cmd:$i:1}"
        sleep 0.03
    done
    echo
    eval "$cmd"
    sleep 0.5
}

sleep 0.5
type_cmd "tooler run nektos/act@v0.2.79 --version"
type_cmd "tooler pin nektos/act@v0.2.79"
type_cmd "tooler run infisical/infisical@infisical-cli/v0.41.90 --version"
type_cmd "tooler run https://dl.k8s.io/release/v1.31.0/bin/linux/arm64/kubectl version --client"
type_cmd "tooler list"
type_cmd "tooler pull cli/cli"
type_cmd "gh --version"
sleep 1
DEMO

echo "Recording demo..."
TOOLER_CONFIG_PATH="$DEMO_DIR/config.json" \
TOOLER_DATA_DIR="$DEMO_DIR/data" \
TOOLER_BIN_DIR="$DEMO_DIR/bin" \
PATH="$DEMO_DIR/bin:$PATH" \
  asciinema rec --overwrite --cols 100 --rows 22 -c "bash $DEMO_DIR/run.sh" "$CAST_FILE"

echo "Converting to SVG..."
svg-term --in "$CAST_FILE" --out "$SCRIPT_DIR/../assets/demo.svg" --window --no-cursor --padding 20 --width 100 --height 22

rm -rf "$DEMO_DIR"
echo "Done: assets/demo.svg"
