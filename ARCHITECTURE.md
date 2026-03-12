# Tooler Architecture

## Overview

Tooler is a CLI tool manager that automates the downloading, extraction, and execution of binaries from GitHub releases or direct URLs. It focuses on zero-configuration usage and is robust to config migration between hosts.

## Module Structure

```
src/
  main.rs          Entry point, CLI dispatch, shim/symlink creation, self-update, platform suffix stripping
  cli.rs           Clap command and argument definitions
  config.rs        Config file I/O, path resolution, key normalization, env var overrides
  download.rs      HTTP download with progress, archive extraction (zip/tar.gz/tar.xz), executable scoring
  platform.rs      OS/arch detection, asset-to-platform matching, release body URL parsing, musl detection, ELF/Mach-O arch verification
  tool_id.rs       ToolIdentifier parsing (owner/repo@version, short names, URLs)
  types.rs         Shared types: ToolerConfig, ToolInfo, ToolerSettings, Forge, PlatformInfo, AssetInfo, GitHubRelease/Asset
  install/
    mod.rs         Install/update orchestration, tool lookup (find_tool_entry/find_tool_executable), recovery, pinning, removal, update checking
    github.rs      GitHub API URL construction, release fetching, error types, URL version discovery stub
build.rs           Build script embedding git metadata (commit, branch, tag) into compile-time env vars
```

## Tool Resolution & Aliases

Tooler uses a multi-layered approach to resolve a tool name (e.g., `gh`) to an executable:

1. **Aliases**: Check the `aliases` map in `config.json`. If an alias exists, resolve the target repo (e.g., `gh` -> `cli/cli`).
2. **Registry Lookup**: Search `config.json` for tools where the `repo` or `tool_name` matches the query.
3. **Binary Name Deduction**: Search `config.json` for any tool whose actual binary filename matches the query.
4. **Deep Search**: For each installed tool, check its installation directory for an executable matching the query. This handles platform-suffixed names (e.g., searching for `cmk` matches `cmk.linux.x86-64`) via `has_matching_binary`, which splits filenames on `.`, `-`, `_` and compares the base segment. This allows running secondary binaries (e.g., `kubeadm`) that were packaged with a primary tool (e.g., `kubectl`).
5. **Recovery & Install**: If not found in config, attempt to recover from disk or install from a forge.

## Storage Structure

Tools are stored in the user's data directory (typically `~/.local/share/tooler/tools/`) using the following convention:
`<forge>/<author>__<repo>__<arch>/<version>/`

- **forge**: `github` or `url`.
- **author**: The organization or user (e.g., `nektos`, `kubernetes`).
- **repo**: The tool name.
- **arch**: The architecture the tool was downloaded for (e.g., `arm64`, `amd64`).
- **version**: The specific version tag (e.g., `v0.2.79`, `1.31.0`).

## Asset Selection

When installing a tool from GitHub, Tooler categorizes all release assets into a 3x3 matrix of (os+arch, os-only, arch-only) x (archive, binary, package). Categories are checked in priority order: `os_arch_archive` > `os_arch_binary` > `os_arch_package`.

Within a category:
- OS and architecture are matched using alias tables (e.g., `aarch64` = `arm64`, `darwin` = `macos`).
- 32-bit assets are rejected on 64-bit systems.
- musl vs glibc libc variants are scored based on the host system (detected via `ldd --version`).
- If no asset matches and `parse_release_body` is enabled, markdown links in the release body are parsed for download URLs matching the platform.
- `.whl` (Python wheel) files serve as a last-resort fallback.

## Executable Scoring

After extraction, `find_executable_in_extracted` walks the extracted directory and scores each executable file:

- Base score: 10 for any executable.
- +20 for files in a `bin/` directory.
- +100 for exact tool name match, +90 for stem match, +85 for base-name match (first segment before `.`/`-`/`_`, e.g., `cmk.linux.x86-64` base is `cmk`).
- +80/+70 for matching parts of the archive filename.
- +50 for correct architecture (verified by reading ELF/Mach-O headers), -500 for mismatch.
- +30 for fuzzy tool name containment.
- -5 per directory depth level (prefer shallow paths).

Files are filtered by `is_executable`, which rejects known non-executables (LICENSE, README, docs), libraries (`.so`, `.dylib`, `.dll`), and files without the Unix execute bit.

## Execution & Self-Healing

Running a tool via `tooler <tool>` or `tooler run <tool>` follows these steps:

1. **Validation**: Check if the tool is in `config.json`.
2. **Pre-flight Check**: If in config, verify the executable path exists and is a valid binary.
3. **Recovery**: If the tool is missing or corrupt, `try_recover_tool` scans the filesystem for an existing installation matching the `repo` part exactly.
4. **Auto-Install**: If recovery fails, Tooler attempts to fetch the latest version from the forge.
5. **Shimming**: If `auto_shim` is enabled, create the shim script and symlinks (see below).
6. **Execution**: The binary is executed as a child process, delegating all arguments. The process exits with the child's exit code.

## Recovery Logic Details

The recovery system (`try_recover_tool`) is designed to handle "orphaned" tools that exist on disk but are not in the configuration:

- It scans all forge directories for matching `author__repo__arch` names.
- It recursively searches for subdirectories that look like version strings (matching `v?\d+\.\d+`).
- It identifies the best candidate binary using the executable scoring system.
- Recovered binaries are verified against the host architecture via ELF header inspection.
- It deduces the original install type:
  - `python-venv` if a `.venv` directory is present.
  - `binary` if the directory contains few files (standalone binary).
  - `archive` otherwise.
- **Precedence**: If multiple authors provide the same tool name and both are orphaned, the recovery logic picks the first one encountered during the forge/directory scan (typically alphabetical).
- `recover_all_installed_tools` runs on `tooler list` to re-register any orphaned tools before display.

## URL-based Tools

Tools installed via direct URL are treated as "direct" author tools. They are stored under the `url/` forge and use the filename as the repo name. Updating these tools is supported if version patterns can be discovered at the same URL location.

## Self-Update Mechanism

When `tooler update` or `tooler pull` targets tooler itself (`morgaesis/tooler`), `handle_self_update` replaces the running binary in-place:

1. Detect self-update by checking if `tool_name == "tooler"` and `author == "morgaesis"`.
2. Locate the current executable via `std::env::current_exe()`.
3. Copy the newly downloaded binary to a `.new` temporary path alongside the current executable.
4. Set executable permissions (0o755) on the temporary file.
5. Atomically rename the temporary file over the current executable. The old inode remains alive until the current process exits, so the rename is safe on Unix.
6. Print a message instructing the user to restart.

## Shimming & Symlink Creation

When `auto_shim` is enabled (default: true, Unix only), Tooler creates a shim-based dispatch system in `bin_dir` (default: `~/.local/share/tooler/bin/`):

### Shim Script

`tooler-shim` is a bash script that delegates to `tooler run`:
```bash
#!/bin/bash
tool_name=$(basename "$0")
exec tooler run "$tool_name" "$@"
```

Created once by `create_shim_script`. If the file exists but is not a valid bash script, it is recreated.

### Symlink Creation

For each tool, `create_tool_symlink` creates a symlink from `bin_dir/<tool_name>` pointing to `tooler-shim`. When the symlink is invoked, `basename $0` resolves to the tool name, which `tooler run` then resolves through the standard tool resolution chain.

### Multi-Binary Symlinks

After a `pull` or `run`, `find_all_executables_in_tool_dir` walks the tool's installation directory (max depth 2) and returns all executable filenames. Symlinks are created for every binary found, not just the primary one.

### Platform Suffix Stripping

`strip_platform_suffix` creates additional convenience symlinks for binaries with platform-specific names. It splits the filename on `.` separators and removes trailing segments composed entirely of platform tokens (`linux`, `darwin`, `amd64`, `x86_64`, `arm64`, `musl`, `gnu`, etc.) or numeric-only segments. For example:
- `cmk.linux.x86-64` -> additional symlink `cmk`
- `wt-cli` -> no change (no platform tokens in dot-separated segments)

This runs during `pull` only (not during `run`).

## Build System

`build.rs` runs at compile time and embeds git metadata into the binary via `cargo:rustc-env`:

- `TOOLER_GIT_COMMIT`: Short hash of HEAD (`git rev-parse --short HEAD`).
- `TOOLER_GIT_BRANCH`: Current branch name (`git branch --show-current`).
- `TOOLER_GIT_TAG`: Tag at HEAD if one exists (`git tag --points-at HEAD`). Only set when building on a tagged commit.

The `version` command and clap's `--version` flag use these values:
- Tagged builds display just the tag (e.g., `v0.6.3`).
- Dev builds display `v<cargo_version>-<commit> (<branch>)`.

Rebuild is triggered by changes to `.git/HEAD` or `.git/refs/`.

## Configuration

Stored as JSON at `~/.config/tooler/config.json` (overridable via `TOOLER_CONFIG` or `TOOLER_CONFIG_PATH` env vars).

### Settings

| Key | Default | Description |
|-----|---------|-------------|
| `update_check_days` | 60 | Days before a tool is considered stale |
| `auto_shim` | true | Create shim symlinks automatically |
| `auto_update` | true | Auto-update stale tools on run |
| `parse_release_body` | true | Parse release notes for download URLs when no asset matches |
| `bin_dir` | `~/.local/share/tooler/bin/` | Directory for shim symlinks |

All settings can be overridden by environment variables: `TOOLER_UPDATE_CHECK_DAYS`, `TOOLER_AUTO_SHIM`, `TOOLER_AUTO_UPDATE`, `TOOLER_BIN_DIR`. Data and config directories are overridable via `TOOLER_DATA_DIR` and `TOOLER_CONFIG_DIR`.

Key normalization converts between `kebab-case`, `snake_case`, and `camelCase` transparently.

### Install Script Bootstrapping

When tooler is first installed (e.g., via a curl|bash install script), the install script registers tooler as a self-managed tool in its own config. This means subsequent `tooler update tooler` or `tooler update all` will update tooler itself through the standard self-update mechanism.
