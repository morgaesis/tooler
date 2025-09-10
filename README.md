# 🚀 Tooler: Your CLI Tool Sidekick

Tired of juggling CLI tools? `tooler` simplifies managing external binaries from GitHub Releases.
Never manually download, extract, or mess with `$PATH` again.

## ✨ Features

- **One-command run:** `tooler run owner/repo:vX.Y.Z` just works.
- **Version Pinning:** `nektos/act:v0.2.79` for consistency.
- **Smart Updates:** Auto-notifies for new versions. `tooler update --all` for the win.
- **Cleanliness:** Organizes tools in your user data dir. Your `$PATH` stays pristine.
- **Cross-Platform:** Linux 🐧, macOS 🍎, Windows 🪟.
- **Dev-Friendly Logs:** Configurable verbosity. Errors/warnings by default.

## 🛠️ Install

Just use `tooler` to run itself:

```bash
tooler run morgaesis/tooler -h
```

For the first run:

```bash
curl -sLo- https://raw.githubusercontent.com/morgaesis/tooler/refs/heads/main/install.sh | bash
```

## 🚀 Usage

```bash
tooler <command> [options]
```

### Commands

- **`tooler run <tool_id> [args...]`**: Execute a tool. Auto-downloads if missing.

  - `tool_id` can be `owner/repo` (latest) or `owner/repo:vX.Y.Z`.
  - `args...` are passed directly to the tool.
  - **Examples:**

    ```bash
    tooler run nektos/act --version
    tooler run cli/cli:v2.40.0 feedback
    ```

- **`tooler list`**: See what's installed. 📋

- **`tooler update <tool_id|--all>`**: Get latest versions.

  - `tooler update cli/cli`
  - `tooler update --all` (Only updates non-pinned tools)

- **`tooler remove <tool_id>`**: Delete tools and their files. 🗑️

  - `tooler remove nektos/act` (all versions)
  - `tooler remove nektos/act:v0.2.79` (specific version)

- **`tooler config <get|set> [key[=value]]`**: Manage `tooler` itself. ⚙️
  - `tooler config get update_check_days`
  - `tooler config set update_check_days=30`

### Log Verbosity

Logs go to `stderr`. Default: ⚠️`WARNING` & ❌`ERROR`.

- `-v`: `INFO` & above.
- `-vv`: `DEBUG` & above (talkative).
- `-q`: Just ❌`ERROR`.

### GitHub API Rate Limits

Heavy usage? Set your `GITHUB_TOKEN` ENV var:

```bash
export GITHUB_TOKEN="ghp_YOUR_TOKEN_HERE" # PAT with 'public_repo' scope
```
