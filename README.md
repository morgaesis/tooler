# Tooler

A CLI tool manager for GitHub Releases written in Rust.

## Features

- **Forge Support**: Seamlessly manages tools from GitHub Releases and direct URLs
- **Direct URL Installation**: Install and shim any binary or archive from the internet directly
- **Intelligent Discovery**: Automatically detects tool names and versions from URLs and attempts to discover updates via directory scraping
- **Platform Detection**: Automatically detects your OS and architecture to download the right binaries
- **Archive Support**: Extracts tar.gz, tar.xz, and zip archives
- **Python Support**: Installs Python tools from wheel files with virtual environments
- **Auto-shimming**: Creates command-line shortcuts for installed tools
- **Update Checking**: Automatically checks for tool updates
- **Configuration**: Persistent configuration with environment variable overrides
- **Version Pinning**: Pin tools to specific versions to prevent auto-updates
- **Complex Version Support**: Handle GitHub release tags with slashes (e.g., infisical-cli/v0.41.90)

![Tooler Demo](./assets/demo.svg)

## Installation

### Bootstrapped

```bash
tooler pull morgaesis/tooler
```

### Quick Install (curl-pipe)

```bash
curl -sSL https://raw.githubusercontent.com/morgaesis/tooler/main/install.sh | bash
```

### From Source

```bash
git clone https://github.com/morgaesis/tooler
cd tooler
cargo install --path .
```

## Usage

### Basic Commands

```bash
# Run a specific version from GitHub
tooler run nektos/act@v0.2.79 --version

# Run with complex GitHub tag (tags with slashes)
tooler run infisical/infisical@infisical-cli/v0.41.90 --version

# Install and run kubectl from official source
tooler run https://dl.k8s.io/release/v1.31.0/bin/linux/arm64/kubectl --version

# Install a tool from a specific archive URL
tooler run https://example.com/downloads/mytool-v1.0.0-linux-amd64.tar.gz --help
```

### Advanced Usage

```bash
# Run with explicit asset selection from the tool's GitHub release
tooler run argoproj/argo-cd --asset argocd-darwin-amd64

# Verbose output
tooler -v run act

# Quiet mode (errors only)
tooler -q list

# Run a previously installed tool by short name
tooler run act --help
```

### Configuration Details

```bash
# Show full configuration (plain text is the default)
tooler config show

# Export configuration as JSON
tooler config show --format json

# Get a specific setting
tooler config get update-check-days

# Set a setting (both formats supported)
tooler config set auto-shim=true
tooler config set auto-update true

# Unset a setting (revert to default)
tooler config unset auto-shim
```

### Shell Integration

To use `tooler` and the tools it manages, add the following to your shell profile (`.bashrc`, `.zshrc`, etc.). The installation script handles this automatically for standard setups:

```bash
# Path for tooler and managed tool shims
export PATH="$HOME/.local/share/tooler/bin:$PATH"
```

## How it Works

Tooler abstracts the installation and update process through a shimming system. When you run a tool, a transparent shim invokes `tooler run`, which checks for stale binaries and auto-updates them from the forge (GitHub or direct URL). Binaries are stored in versioned, isolated directories to prevent conflicts.

Assets are prioritized based on **specificity**: matches for both OS and Architecture are ranked highest. Archives (tar.gz, zip) are preferred over direct binaries to ensure all metadata is captured, with system packages and Python wheels serving as fallbacks.

## Settings

Settings are stored in `~/.config/tooler/config.json`. Overrides are supported via environment variables:

- `update-check-days`: Days between update checks (default: 60, env: `TOOLER_UPDATE_CHECK_DAYS`)
- `auto-shim`: Create command-line shims (default: true, env: `TOOLER_AUTO_SHIM`)
- `auto-update`: Automatically update tools on run (default: true, env: `TOOLER_AUTO_UPDATE`)
- `bin-dir`: Directory for binaries and shims (default: `~/.local/share/tooler/bin`, env: `TOOLER_BIN_DIR`)

Logging is controlled via `LOG_LEVEL` or `TOOLER_LOG_LEVEL`.

## Development

```bash
cargo test
cargo clippy
cargo check
cargo run -- run nektos/act --help
cargo build --release
```

### Generating Demo Animation

The animated demo in the README is generated using `scripts/gen_demo.js` and `svg-term-cli`.

```bash
node scripts/gen_demo.js
npx svg-term-cli --in demo.cast --out assets/demo.svg --window
rm demo.cast
```

## License

MIT (see the [LICENSE](LICENSE))
