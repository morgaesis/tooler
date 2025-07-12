#!/bin/bash

BINDIR="$HOME/.local/bin"
mkdir -p "$BINDIR"
cp ./tooler.py "$BINDIR/tooler"
chmod +x "$BINDIR/tooler"

pip install -r ./requirements.txt
