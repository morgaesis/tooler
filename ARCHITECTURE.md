# Tooler Architecture

## Overview

Tooler is a CLI tool manager that automates the downloading, extraction, and execution of binaries from GitHub releases or direct URLs. It focuses on zero-configuration usage and is robust to config migration between hosts.

## Tool Resolution & Aliases

Tooler uses a multi-layered approach to resolve a tool name (e.g., `gh`) to an executable:

1. **Aliases**: Check the `aliases` map in `config.json`. If an alias exists, resolve the target repo (e.g., `gh` -> `cli/cli`).
2. **Registry Lookup**: Search `config.json` for tools where the `repo` or `tool_name` matches the query.
3. **Binary Name Deduction**: Search `config.json` for any tool whose actual binary filename matches the query.
4. **Deep Search**: For each installed tool, check its installation directory for an executable matching the query. This allows running secondary binaries (e.g., `kubeadm`) that were packaged with a primary tool (e.g., `kubectl`).
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

When installing a tool from GitHub, Tooler scores all available release assets to find the best match for the current platform:

- Assets matching the OS (e.g., `linux`, `darwin`) and architecture (e.g., `arm64`, `x86_64`) exactly receive the highest scores.
- Common aliases are handled (e.g., `aarch64` matching `arm64`).
- Extension scoring: Binaries and archives are preferred over installers or checksum files.

## Execution & Self-Healing

Running a tool via `tooler <tool>` or `tooler run <tool>` follows these steps:

1. **Validation**: Check if the tool is in `config.json`.
2. **Pre-flight Check**: If in config, verify the executable path exists and is a valid binary.
3. **Recovery**: If the tool is missing or corrupt, `try_recover_tool` scans the filesystem for an existing installation matching the `repo` part exactly.
4. **Auto-Install**: If recovery fails, Tooler attempts to fetch the latest version from the forge.
5. **Execution**: The binary is executed as a child process, delegating all arguments.

## Recovery Logic Details

The recovery system (`try_recover_tool`) is designed to handle "orphaned" tools that exist on disk but are not in the configuration:

- It scans all forge directories for matching `author__repo__arch` names.
- It recursively searches for subdirectories that look like version strings (matching `v?\d+\.\d+`).
- It identifies the best candidate binary using a scoring system based on the tool name, repo name, and common binary aliases.
- It deduces the original install type:
  - `python-venv` if a `.venv` directory is present.
  - `binary` if the directory contains few files (standalone binary).
  - `archive` otherwise.
- **Precedence**: If multiple authors provide the same tool name and both are orphaned, the recovery logic will pick the first one encountered during the forge/directory scan (typically alphabetical).

## URL-based Tools

Tools installed via direct URL are treated as "direct" author tools. They are stored under the `url/` forge and use the filename as the repo name. Updating these tools is supported if version patterns can be discovered at the same URL location.
