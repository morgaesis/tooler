# Tooler

A CLI tool manager for GitHub Releases written in Rust.

## Features

- **Tool Management**: Install, update, and remove tools from GitHub releases
- **Platform Detection**: Automatically detects your OS and architecture to download the right binaries
- **Archive Support**: Extracts tar.gz, tar.xz, and zip archives
- **Python Support**: Installs Python tools from wheel files with virtual environments
- **Auto-shimming**: Creates command-line shortcuts for installed tools
- **Update Checking**: Automatically checks for tool updates
- **Configuration**: Persistent configuration with environment variable overrides
- **Version Pinning**: Pin tools to specific versions to prevent auto-updates
- **Complex Version Support**: Handle GitHub release tags with slashes (e.g., infisical-cli/v0.41.90)

## Installation

### Quick Install (curl-pipe)

```bash
curl -sSL https://github.com/morgaesis/tooler/releases/latest/download/install.sh | bash
```

### From Source

```bash
git clone https://github.com/morgaesis/tooler
cd tooler
cargo install --path .
```

### From Source (recommended)

```bash
git clone https://github.com/morgaesis/tooler
cd tooler
cargo install --path .
```

## Usage

### Basic Commands

```bash
# Run a specific version
tooler run nektos/act@v0.2.79 -- --help

# Run with complex GitHub tag (tags with slashes)
tooler run infisical/infisical@infisical-cli/v0.41.90 -- --help
```

### Configuration

```bash
# Show all settings
tooler config get

# Get a specific setting
tooler config get update_check_days

# Set a setting
tooler config set auto_shim=true
tooler config set shim_dir=/home/user/.local/bin

# Unset a setting (revert to default)
tooler config unset shim_dir
```

### Advanced Usage

```bash
# Run with explicit asset selection
tooler run argoproj/argo-cd --asset argocd-darwin-amd64

# Verbose output
tooler -v run act

# Quiet mode (errors only)
tooler -q list

# Run a previously installed tool by short name
tooler run act -- --help
```

## Configuration

Settings are stored in `~/.config/.tooler/config.json` and can be overridden with environment variables:

- `update_check_days`: Days between update checks (default: 60, env: `TOOLER_UPDATE_CHECK_DAYS`)
- `auto_shim`: Create command-line shims (default: false, env: `TOOLER_AUTO_SHIM`)
- `shim_dir`: Directory for shims (default: `~/.local/bin`, env: `TOOLER_SHIM_DIR`)

## Architecture Support

Tooler supports automatic detection and downloading for:

### Operating Systems

- Linux (gnu, musl)
- macOS (darwin)
- Windows (msvc, gnu)

### Architectures

- amd64 (x86_64)
- arm64 (aarch64)
- arm (armv7, armv7l)

## Asset Selection

Tooler prioritizes assets in this order:

1. **Archive with OS + Arch** (tar.gz, zip, tar.xz, tgz)
2. **Binary with OS + Arch** (direct executables)
3. **Package with OS + Arch** (apk, deb, rpm)
4. **Archive with OS only**
5. **Binary with OS only**
6. **Package with OS only**
7. **Archive with Arch only**
8. **Binary with Arch only**
9. **Package with Arch only**
10. **Python wheel** (fallback)

## Development

### Building

```bash
cargo build --release
```

### Testing

```bash
cargo test
```

### Running from source

```bash
cargo run -- run nektos/act -- --help
```

## Migration from Python

This Rust version maintains compatibility with the Python tooler configuration and data formats. Simply replace the Python installation with the Rust binary, and your existing tools and settings will continue to work.

## License

MIT
