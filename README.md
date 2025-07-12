# Tooler: CLI Tool Management from GitHub Releases

`tooler` is a command-line utility designed for DevOps engineers to effortlessly download, manage, and run CLI
tools distributed via GitHub Releases. It simplifies your workflow by handling architecture-specific downloads,
local storage, and version pinning, freeing you from manual releases page navigation and PATH management.

## Features

- **Effortless Execution:** Run `tooler run <repo/tool>` to automatically download (if not present) and execute
  a tool.
- **Version Pinning:** Specify exact versions (e.g., `tooler run nektos/act:v0.2.79`).
- **Auto-Update Checks:** Notifies you when new versions of non-pinned tools are available.
- **Centralized Storage:** Tools are stored in a dedicated user data directory, keeping your system PATH clean.
- **Cross-Platform:** Works on Linux, macOS, and Windows.
- **Clean Logging:** Configurable verbosity, defaults to essential warnings/errors.

## Installation

Run `./install.sh` for automatic installation.

1. **Download:** Save the `tooler.py` script to a directory of your choice (e.g., `~/bin/tooler.py` or `C:\tools\tooler.py`).
2. **Make Executable (Linux/macOS):**

   ```bash
   mv /path/to/tooler{.py,}
   chmod +x /path/to/tooler
   ```

3. **Add to PATH:** Add the directory containing `tooler.py` to your system's `PATH` environment variable.
   - **Linux/macOS:** Add `export PATH="/path/to/script_directory:$PATH"` to your shell profile
     (`.bashrc`, `.zshrc`, etc.) and `source` it.
   - **Windows:** Add the directory via System Environment Variables or PowerShell
     (`[Environment]::SetEnvironmentVariable("PATH", "$env:PATH;C:\path\to\script_directory", "User")`).
     Remember to restart your terminal.

## Usage

```bash
tooler <command> [options]
```

### Commands

- **`tooler run <tool_id> [args...]`**

  - Downloads (if necessary) and executes the specified tool.
  - `<tool_id>` can be a GitHub repository (`owner/repo` for latest) or a specific version (`owner/repo:vX.Y.Z`).
  - `[args...]` are passed directly to the tool.
  - **Examples:**

    ```bash
    tooler run nektos/act --version
    tooler run nektos/act:v0.2.79 --matrix '{"os": ["ubuntu-latest"]}' build
    ```

- **`tooler list`**

  - Lists all installed tools, their versions, and paths.

- **`tooler update <tool_id>`**

  - Checks for and installs the latest version of a specific tool.

- **`tooler update --all`**

  - Checks for and installs the latest versions for all _non-pinned_ installed tools.

- **`tooler remove <tool_id>`**

  - Removes one or all versions of an installed tool and its data.
  - **Examples:**

    ```bash
    tooler remove nektos/act          # Removes all versions of nektos/act
    tooler remove nektos/act:v0.2.79  # Removes specific version
    ```

- **`tooler config <get|set> [key[=value]]`**

  - Manages Tooler's internal configuration settings.
  - **Examples:**

    ```bash
    tooler config get update_check_days       # Get a specific setting
    tooler config set update_check_days=30    # Set update check interval to 30 days
    tooler config get                         # Show all settings
    ```

### Verbosity Options

Control the log output (logs go to `stderr`):

- Default: Shows `WARNING` and `ERROR` messages.
- `-v` / `--verbose`: Shows `INFO` and higher.
- `-vv` / `--verbose --verbose`: Shows `DEBUG` and higher (most verbose).
- `-q` / `--quiet`: Shows only `ERROR` messages.

### GitHub API Rate Limits

For frequent use, set a GitHub Personal Access Token in your environment:

```bash
export GITHUB_TOKEN="your_github_pat_here"
```

(Ensure your PAT has `public_repo` scope for public repositories, or broader access if you intend to use private
ones.)
