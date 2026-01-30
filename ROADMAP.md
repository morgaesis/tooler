# Tooler Roadmap

## Completed

- **Tool Aliases**: Added `config alias` command and automatic deduction of tool names from installed binary filenames.
- **Multi-binary Archives**: Enhanced search logic to find alternative binaries within a tool's installation directory (e.g., finding `kubeadm` if `kubectl` repository provides it).

## High Priority

- **Refine Version Recovery**: Improve `try_recover_tool` to handle deeply nested version structures (e.g., `infisical-cli/v0.41.90`).
- **Deduce Install Type**: Automatically detect if a recovered tool was a `binary`, `archive`, or `python-venv`.

## Enhancement

- Don't do github call for unknown and unqualified tools, e.g. `tooler run foo`. Currently gives `404` API call error.
- **GitHub Tag Lookups**: Implement "smart versioning" in GitHub API calls to handle tags with or without 'v' prefixes automatically.
- **Dead Code Cleanup**: Remove or properly integrate unused functions (`find_highest_version`, `api_version`, etc.).
- **Code Organization**: Refactor `main.rs` and `install.rs` into smaller, focused modules (e.g., `cmd/run.rs`, `cmd/list.rs`, `recovery.rs`).

## Future Ideas

- **Enhanced Forge Support**: Support for additional forges like GitLab or Gitea.
- **Export/Import Config**: Easier migration of toolsets between machines.
