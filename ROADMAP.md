# Tooler Roadmap

## High Priority

- **Refine Version Recovery**: Improve `try_recover_tool` to handle deeply nested version structures (e.g., `infisical-cli/v0.41.90`).
- **Deduce Install Type**: Automatically detect if a recovered tool was a `binary`, `archive`, or `python-venv`.
- **Tool Aliases**: Automatically set if known or deducible, or allow user to set tool aliases where the repo is not the same as the tool name.
  For example, the `gh` GitHub CLI, is in `cli/cli`.

## Enhancement

- Don't do github call for unknown and unqualified tools, e.g. `tooler run foo`. Currently gives `404` API call error.
- **GitHub Tag Lookups**: Implement "smart versioning" in GitHub API calls to handle tags with or without 'v' prefixes automatically.
- **Dead Code Cleanup**: Remove or properly integrate unused functions (`find_highest_version`, `api_version`, etc.).
- **Code Organization**: Refactor `main.rs` and `install.rs` into smaller, focused modules (e.g., `cmd/run.rs`, `cmd/list.rs`, `recovery.rs`).

## Future Ideas

- **Multi-binary Archives**: Better support for archives containing multiple useful binaries (e.g., `kubectl` and `kubeadm`).
- **Enhanced Forge Support**: Support for additional forges like GitLab or Gitea.
- **Export/Import Config**: Easier migration of toolsets between machines.
