# Installation

## Quick Install

Download and install the latest release:

```bash
curl -sSL https://raw.githubusercontent.com/morgaesis/tooler/main/install.sh | bash
```

This installs tooler to `~/.local/share/tooler/bin/` and adds it to your PATH via `.bashrc`/`.zshrc`.

After installation, restart your shell or run:

```bash
source ~/.bashrc  # or ~/.zshrc
```

## Bootstrap (self-managed)

If tooler is already installed, it can update itself:

```bash
tooler pull morgaesis/tooler
tooler update tooler
```

The install script registers tooler as a self-managed tool automatically.

## From Source

Requires Rust toolchain (rustup.rs):

```bash
git clone https://github.com/morgaesis/tooler
cd tooler
cargo install --path .
```

## Specific Version

Set `TOOLER_VERSION` before running the install script:

```bash
TOOLER_VERSION=v0.6.3 curl -sSL https://raw.githubusercontent.com/morgaesis/tooler/main/install.sh | bash
```

Or download a release directly from [GitHub Releases](https://github.com/morgaesis/tooler/releases).

## Supported Platforms

| Platform | Architecture | Asset |
|----------|-------------|-------|
| Linux | x86_64 | `tooler-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | aarch64 | `tooler-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` |
| macOS | x86_64 | `tooler-vX.Y.Z-x86_64-apple-darwin.tar.gz` |
| macOS | aarch64 | `tooler-vX.Y.Z-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `tooler-vX.Y.Z-x86_64-pc-windows-msvc.zip` |
| Windows | aarch64 | `tooler-vX.Y.Z-aarch64-pc-windows-msvc.zip` |

## Shell Integration

The install script adds this automatically. If you installed from source, add to your shell profile:

```bash
export PATH="$HOME/.local/share/tooler/bin:$PATH"
```

## File Locations

| Path | Purpose |
|------|---------|
| `~/.local/share/tooler/bin/` | Tooler binary and tool shims |
| `~/.local/share/tooler/tools/` | Downloaded tool binaries (by forge/author/version) |
| `~/.config/tooler/config.json` | Configuration and tool registry |

## Uninstall

```bash
rm -rf ~/.local/share/tooler
rm -f ~/.config/tooler/config.json
```

Remove the PATH line from your `.bashrc`/`.zshrc`.

## Troubleshooting

**Install script fails with "Failed to fetch release information"**

GitHub API rate limits unauthenticated requests (60/hour). Workarounds:

1. Install `gh` CLI and authenticate (`gh auth login`) before running the script
2. Set the version explicitly: `TOOLER_VERSION=v0.6.3 bash install.sh`
3. Download from [GitHub Releases](https://github.com/morgaesis/tooler/releases) directly

**GPG-signed git commits fail**

Tooler never bypasses GPG signing. Unlock your GPG keyring before running git operations.

**`tooler update tooler` doesn't update the version**

Versions before v0.6.3 don't support self-update. Reinstall via the install script or `cargo install --path .` to get self-update support.
