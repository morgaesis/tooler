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

### Windows PowerShell

From PowerShell, install the latest Windows release with:

```powershell
irm https://raw.githubusercontent.com/morgaesis/tooler/main/install.ps1 | iex
```

This installs `tooler.exe` to `%LOCALAPPDATA%\tooler\bin`, adds that directory to your user PATH, and registers tooler as a self-managed tool. Open a new PowerShell session after installation so PATH changes are picked up.

To install a specific version:

```powershell
$env:TOOLER_VERSION = 'v0.7.1'
irm https://raw.githubusercontent.com/morgaesis/tooler/main/install.ps1 | iex
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

On Windows, the default `x86_64-pc-windows-gnu` Rust toolchain requires a compatible MinGW GCC toolchain on PATH. If builds fail with `dlltool.exe: program not found` or missing `-lgcc`/`-lgcc_eh`, install MSYS2 and the MinGW GCC package:

```powershell
winget install --id MSYS2.MSYS2 --exact
C:\msys64\usr\bin\bash.exe -lc "pacman --noconfirm -Sy mingw-w64-x86_64-gcc"
$env:Path = 'C:\msys64\mingw64\bin;' + $env:Path
cargo build --release
```

The LLVM MinGW package provides `dlltool.exe`, but it does not provide the GCC runtime libraries expected by Rust's GNU target.

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

On Windows PowerShell, add tooler's bin directory to your user PATH:

```powershell
[Environment]::SetEnvironmentVariable(
  'Path',
  [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + "$env:LOCALAPPDATA\tooler\bin",
  'User'
)
```

## File Locations

| Path | Purpose |
|------|---------|
| `~/.local/share/tooler/bin/` | Tooler binary and tool shims |
| `~/.local/share/tooler/tools/` | Downloaded tool binaries (by forge/author/version) |
| `~/.config/tooler/config.json` | Configuration and tool registry |

On Windows, default locations are:

| Path | Purpose |
|------|---------|
| `%LOCALAPPDATA%\tooler\bin\` | Tooler binary and `.cmd` tool shims |
| `%LOCALAPPDATA%\tooler\tools\` | Downloaded tool binaries (by forge/author/version) |
| `%APPDATA%\tooler\config.json` | Configuration and tool registry |

## Uninstall

```bash
rm -rf ~/.local/share/tooler
rm -f ~/.config/tooler/config.json
```

Remove the PATH line from your `.bashrc`/`.zshrc`.

On Windows:

```powershell
Remove-Item "$env:LOCALAPPDATA\tooler" -Recurse -Force
Remove-Item "$env:APPDATA\tooler\config.json" -Force
```

Then remove `%LOCALAPPDATA%\tooler\bin` from your user PATH.

## Troubleshooting

**Install script fails with "Failed to fetch release information"**

GitHub API rate limits unauthenticated requests (60/hour). Workarounds:

1. Install `gh` CLI and authenticate (`gh auth login`) before running the script
2. Set the version explicitly: `TOOLER_VERSION=v0.6.3 bash install.sh`
3. Download from [GitHub Releases](https://github.com/morgaesis/tooler/releases) directly

**Windows source build fails before compiling tooler**

If Cargo reports `dlltool.exe: program not found`, the Rust GNU linker tools are missing. Install MSYS2 and `mingw-w64-x86_64-gcc`, then prepend `C:\msys64\mingw64\bin` to PATH for the build.

If Cargo reports missing `-lgcc` or `-lgcc_eh` after installing LLVM MinGW, switch to the MSYS2 GCC package. LLVM MinGW is not enough for the active Rust GNU toolchain.

**Windows shims are not found**

Tooler creates `.cmd` shims such as `gh.cmd` in the configured `bin-dir`. Make sure that directory is on PATH in a new shell session and that `.CMD` is present in `PATHEXT`.

**GPG-signed git commits fail**

Tooler never bypasses GPG signing. Unlock your GPG keyring before running git operations.

**`tooler update tooler` doesn't update the version**

Versions before v0.6.3 don't support self-update. Reinstall via the install script or `cargo install --path .` to get self-update support.
