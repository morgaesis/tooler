#!/bin/bash

set -euo pipefail

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
[[ -d "$VENV_DIR" ]] || python3 -m venv "$VENV_DIR"
# shellcheck disable=SC1090,SC1091
. "${VENV_DIR}/bin/activate"
python -m ensurepip
pip install -r "${TOOLER_REPO_DIR}/requirements.txt"
cat >"${BIN_DIR}/tooler" <<EOF
#!/bin/bash
. "${VENV_DIR}/bin/activate"
exec python3 "${TOOLER_REPO_DIR}/tooler.py" "\$@"
EOF
chmod +x "${BIN_DIR}/tooler"
