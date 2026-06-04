# Tooler

A CLI tool manager for GitHub Releases written in Rust.

## Features

- **Forge Support**: Seamlessly manages tools from GitHub Releases and direct URLs
- **Direct URL Installation**: Install and shim any binary or archive from the internet directly
- **Intelligent Discovery**: Automatically detects tool names and versions from URLs and attempts to discover updates via directory scraping
- **Release Body Parsing**: When GitHub releases lack direct asset downloads, parses release notes for download URLs and asks before downloading parsed binaries (e.g., Helm's get.helm.sh pattern)
- **Platform Detection**: Automatically detects your OS and architecture to download the right binaries
- **Archive Support**: Extracts tar.gz, tar.xz, and zip archives
- **Python Support**: Installs Python tools from wheel files with virtual environments
- **Auto-shimming**: Creates command-line shortcuts for installed tools
- **Update Checking**: Automatically checks for tool updates
- **Configuration**: Persistent configuration with environment variable overrides
- **Version Pinning**: Pin tools to specific versions to prevent auto-updates
- **Aliases**: Create short aliases for tools with `tooler alias <name> <target>`
- **Complex Version Support**: Handle GitHub release tags with slashes (e.g., [infisical-cli/v0.41.90](https://github.com/Infisical/infisical/releases/tag/infisical-cli/v0.41.90))

![Tooler Demo](./assets/demo.svg)

## Installation

Bootstrap (if tooler is already installed)

```bash
tooler pull morgaesis/tooler
```

or install from scratch

```bash
curl -sSL https://raw.githubusercontent.com/morgaesis/tooler/main/install.sh | bash
```

On Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/morgaesis/tooler/main/install.ps1 | iex
```

See [INSTALL.md](INSTALL.md) for platform details, from-source builds, and troubleshooting.

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

# Install and run a tool with different repo and binary name
tooler pull cli/cli # GitHub's `gh` CLI
tooler run gh --version
gh --version # Or just use the auto-shimmed binary

# Tools like Helm that host binaries externally (release body parsing)
tooler pull helm/helm
helm version

# Create aliases for shorter tool names
tooler alias cmk cloudstack-cloudmonkey
tooler run cmk --version
```

### Release Body Parsing

Some projects (like Helm) upload only signature files to GitHub releases and host actual binaries elsewhere. Tooler can parse release notes for download URLs and prompts before downloading a parsed binary by default:

```bash
# Ask before downloading a URL parsed from the release body (default)
tooler pull helm/helm
# Prompt choices: ask (approve once), always, never

# Choose release body parsing behavior globally
tooler config set parse-release-body ask
tooler config set parse-release-body always
tooler config set parse-release-body never

# Or disable per-command
tooler pull helm/helm --no-parse-release-body
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

# Set a setting (both formats supported, kebab/snake/camel accepted)
tooler config set update-check-days 14  # default: 60
tooler config set auto-shim false       # default: true
tooler config set auto-update false     # default: true
tooler config set parse-release-body ask    # default: ask
tooler config set bin-dir ~/.local/share/tooler/bin  # default: ~/.local/share/tooler/bin

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
- `parse-release-body`: Parse release notes for download URLs when assets don't match (`ask`, `always`, `never`; default: `ask`, env: `TOOLER_PARSE_RELEASE_BODY`)
- `bin-dir`: Directory for binaries and shims (default: `~/.local/share/tooler/bin`, env: `TOOLER_BIN_DIR`)

Logging level is controlled via `LOG_LEVEL` or `TOOLER_LOG_LEVEL`. Log output
goes to both stderr and the default log file unless changed with
`--log-destination`; pass a comma-separated list of `stderr`, `stdout`,
`logfile`, or `none`.

The default log file is `$XDG_STATE_HOME/tooler/tooler.log` or
`~/.local/state/tooler/tooler.log` on Unix, and the local application data
directory on Windows. Override the file path with `TOOLER_LOG_FILE`, or the
state directory with `TOOLER_STATE_DIR`.

Unix shim dispatch failures are also recorded locally at
`$XDG_STATE_HOME/tooler/shim.log` or `~/.local/state/tooler/shim.log`; override
with `TOOLER_SHIM_LOG` when debugging isolated runs.

## Development

```bash
cargo test
cargo clippy
cargo check
cargo run -- run nektos/act --help
cargo build --release
```

### Generating Demo Animation

The demo SVG is recorded with `asciinema` in an isolated tooler environment and converted with `svg-term-cli`:

```bash
# Requires: asciinema, svg-term-cli (npm i -g svg-term-cli)
./scripts/gen-demo.sh
```

## License

MIT (see the [LICENSE](LICENSE))
