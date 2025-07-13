#!/usr/bin/env python3
import argparse
import json
import logging
import os
import platform
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from datetime import datetime
from typing import Any, Dict, List, Optional, Tuple, Union

import requests
from tqdm import tqdm

# Conditional import for Python < 3.8
if sys.version_info < (3, 8):
    from typing_extensions import TypedDict
else:
    from typing import TypedDict


# --- Type Definitions for Configuration ---
class ToolInfo(TypedDict):
    tool_name: str
    repo: str
    version: str
    executable_path: str
    install_type: str
    installed_at: str
    last_accessed: str


class ToolerSettings(TypedDict):
    update_check_days: int


class ToolerConfig(TypedDict):
    tools: Dict[str, ToolInfo]
    settings: ToolerSettings


# --- Configuration Constants ---
APP_NAME = "tooler"
CONFIG_DIR_NAME = ".tooler"
TOOLS_DIR_NAME = "tools"
CONFIG_FILE_NAME = "config.json"
DEFAULT_UPDATE_CHECK_DAYS = 60

# --- Global Paths (Lazy Initialization) ---
_USER_DATA_DIR: Optional[str] = None
_USER_CONFIG_DIR: Optional[str] = None
_TOOLER_CONFIG_FILE_PATH: Optional[str] = None
_TOOLER_TOOLS_DIR: Optional[str] = None

# --- Logger Setup ---
logger = logging.getLogger(APP_NAME)
logger.setLevel(logging.DEBUG)


class ToolerFormatter(logging.Formatter):
    """Custom logger formatter to add color to output."""

    FORMATS = {
        logging.DEBUG: "\033[90mDEBUG> %(message)s\033[0m",
        logging.INFO: "\033[94mINFO> %(message)s\033[0m",
        logging.WARNING: "\033[93mWARN> %(message)s\033[0m",
        logging.ERROR: "\033[91mERROR> %(message)s\033[0m",
        logging.CRITICAL: "\033[91mCRIT> %(message)s\033[0m",
    }

    def format(self, record: logging.LogRecord) -> str:
        log_fmt = self.FORMATS.get(record.levelno)
        formatter = logging.Formatter(
            log_fmt if sys.stderr.isatty() else "%(levelname)s> %(message)s"
        )
        return formatter.format(record)


# --- Path Utility Functions ---
def get_user_data_dir() -> str:
    """Determines the user-specific data directory."""
    global _USER_DATA_DIR
    if _USER_DATA_DIR is None:
        if platform.system() == "Windows":
            _USER_DATA_DIR = os.path.join(
                os.environ.get("APPDATA", os.path.expanduser("~")), APP_NAME
            )
        elif platform.system() == "Darwin":
            _USER_DATA_DIR = os.path.join(
                os.path.expanduser("~/Library/Application Support"), APP_NAME
            )
        else:
            _USER_DATA_DIR = os.path.join(
                os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")),
                APP_NAME,
            )
        os.makedirs(_USER_DATA_DIR, exist_ok=True)
    return _USER_DATA_DIR


def get_user_config_dir() -> str:
    """Determines the user-specific config directory."""
    global _USER_CONFIG_DIR
    if _USER_CONFIG_DIR is None:
        if platform.system() == "Windows":
            _USER_CONFIG_DIR = os.path.join(
                os.environ.get("APPDATA", os.path.expanduser("~")), CONFIG_DIR_NAME
            )
        else:
            _USER_CONFIG_DIR = os.path.join(
                os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")),
                CONFIG_DIR_NAME,
            )
        os.makedirs(_USER_CONFIG_DIR, exist_ok=True)
    return _USER_CONFIG_DIR


def get_tooler_config_file_path() -> str:
    """Returns the path to the tooler's main configuration file."""
    global _TOOLER_CONFIG_FILE_PATH
    if _TOOLER_CONFIG_FILE_PATH is None:
        _TOOLER_CONFIG_FILE_PATH = os.path.join(get_user_config_dir(), CONFIG_FILE_NAME)
    return _TOOLER_CONFIG_FILE_PATH


def get_tooler_tools_dir() -> str:
    """Returns the path where actual tool binaries are stored."""
    global _TOOLER_TOOLS_DIR
    if _TOOLER_TOOLS_DIR is None:
        _TOOLER_TOOLS_DIR = os.path.join(get_user_data_dir(), TOOLS_DIR_NAME)
    os.makedirs(_TOOLER_TOOLS_DIR, exist_ok=True)
    return _TOOLER_TOOLS_DIR


# --- Configuration Management ---
def load_tool_configs() -> ToolerConfig:
    """Loads tool configurations from the JSON file."""
    config_path = get_tooler_config_file_path()
    if not os.path.exists(config_path):
        return ToolerConfig(
            tools={}, settings={"update_check_days": DEFAULT_UPDATE_CHECK_DAYS}
        )
    try:
        with open(config_path, "r") as f:
            config_data = json.load(f)
            config_data.setdefault("tools", {})
            config_data.setdefault("settings", {}).setdefault(
                "update_check_days", DEFAULT_UPDATE_CHECK_DAYS
            )
            return config_data  # type: ignore
    except (json.JSONDecodeError, TypeError):
        logger.error(
            f"Could not parse config file at {config_path}. Starting with empty config."
        )
        return ToolerConfig(
            tools={}, settings={"update_check_days": DEFAULT_UPDATE_CHECK_DAYS}
        )


def save_tool_configs(configs: ToolerConfig) -> None:
    """Saves tool configurations to the JSON file."""
    config_path = get_tooler_config_file_path()
    os.makedirs(os.path.dirname(config_path), exist_ok=True)
    with open(config_path, "w") as f:
        json.dump(configs, f, indent=2)


# --- Platform and Architecture Utilities ---
def get_system_arch() -> str:
    """Determines the standard system architecture string."""
    machine = platform.machine().lower()
    system = platform.system().lower()
    arch_map = {
        "linux": {
            "aarch64": "arm64",
            "arm": "arm",
            "x86_64": "amd64",
            "amd64": "amd64",
        },
        "darwin": {"aarch64": "arm64", "arm": "arm64", "x86_64": "amd64"},
        "windows": {"amd64": "amd64", "x86_64": "amd64"},
    }
    if system in arch_map:
        for keyword, arch in arch_map[system].items():
            if keyword in machine:
                return arch
    return machine


def map_arch_to_github_release(
    arch: str, system: str
) -> Optional[Union[str, Tuple[str, ...]]]:
    """Maps internal arch/system to common GitHub release file naming conventions."""
    system = system.lower()
    if system == "linux":
        if arch == "amd64":
            return "linux_amd64"
        if arch == "arm64":
            return "linux_arm64"
    elif system == "darwin":
        if arch == "amd64":
            return "darwin_amd64"
        if arch == "arm64":
            return "darwin_arm64"
    elif system == "windows" and arch == "amd64":
        return "windows_amd64"
    if arch == "amd64":
        return (f"{system}_x86_64", f"{system}-x86_64", f"{system}-amd64")
    if arch == "arm64":
        return (f"{system}_aarch64", f"{system}-aarch64", f"{system}-arm64")
    return None


def find_asset_for_platform(
    assets: List[Dict[str, Any]], repo_full_name: str, system_arch: str, system_os: str
) -> Tuple[Optional[str], Optional[str]]:
    """Finds a suitable binary asset from GitHub releases."""
    system_os_lower, _tool_name = system_os.lower(), repo_full_name.split("/")[1]
    possible_patterns: List[str] = [
        f"{system_os_lower}_{system_arch}",
        f"{system_os_lower}-{system_arch}",
    ]
    if mapped := map_arch_to_github_release(system_arch, system_os):
        if isinstance(mapped, str):
            possible_patterns.append(mapped)
        else:
            possible_patterns.extend(mapped)
    if system_os_lower == "windows":
        possible_patterns.append("windows")
    elif system_os_lower == "darwin":
        possible_patterns.append("macos")
    possible_patterns = sorted(list(set(possible_patterns)), key=len, reverse=True)
    logger.debug(f"Searching for assets with patterns: {possible_patterns}")

    for pattern in possible_patterns:
        for asset in assets:
            if pattern in asset["name"].lower() and (
                asset["name"].endswith((".tar.gz", ".zip"))
            ):
                logger.debug(f"Found archive asset: {asset['name']}")
                return asset["browser_download_url"], asset["name"]
    for pattern in possible_patterns:
        for asset in assets:
            if pattern in asset["name"].lower() and not any(
                ext in asset["name"].lower() for ext in [".tar.", ".zip", ".whl"]
            ):
                if (
                    system_os_lower == "windows" and asset["name"].endswith(".exe")
                ) or (system_os_lower != "windows" and "." not in asset["name"]):
                    logger.debug(f"Found executable asset: {asset['name']}")
                    return asset["browser_download_url"], asset["name"]
    for asset in assets:
        if asset["name"].endswith(".whl"):
            logger.info(f"Native binary not found. Using Python wheel: {asset['name']}")
            return asset["browser_download_url"], asset["name"]

    logger.error("No suitable binary asset (archive, executable, or .whl) found.")
    logger.error(f"Available assets on release: {[a['name'] for a in assets]}")
    return None, None


# --- Download and Extraction ---
def download_file(url: str, local_path: str) -> bool:
    """Downloads a file with a progress bar."""
    logger.info(f"Downloading {os.path.basename(local_path)}...")
    try:
        response = requests.get(url, stream=True)
        response.raise_for_status()
        total_size = int(response.headers.get("content-length", 0))
        with tqdm(
            total=total_size,
            unit="iB",
            unit_scale=True,
            desc="Progress >",
            file=sys.stderr,
            disable=logger.level > logging.INFO,
        ) as pbar:
            with open(local_path, "wb") as f:
                for data in response.iter_content(chunk_size=8192):
                    pbar.update(len(data))
                    f.write(data)
        if total_size != 0 and pbar.n != total_size:
            logger.warning("Download size mismatch.")
        return True
    except requests.exceptions.RequestException as e:
        logger.error(f"Error downloading {url}: {e}")
        return False


def extract_archive(archive_path: str, extract_dir: str) -> bool:
    """Extracts a tar.gz or zip archive with security checks."""
    logger.info(f"Extracting {os.path.basename(archive_path)}...")
    try:
        if archive_path.endswith(".tar.gz"):
            with tarfile.open(archive_path, "r:gz") as tar:
                for member in tar.getmembers():
                    if not os.path.realpath(
                        os.path.join(extract_dir, member.name)
                    ).startswith(os.path.realpath(extract_dir)):
                        logger.warning(f"Skipping malicious path in tar: {member.name}")
                    else:
                        tar.extract(member, path=extract_dir)
        elif archive_path.endswith(".zip"):
            with zipfile.ZipFile(archive_path, "r") as zip_ref:
                for member in zip_ref.infolist():
                    if not os.path.realpath(
                        os.path.join(extract_dir, member.filename)
                    ).startswith(os.path.realpath(extract_dir)):
                        logger.warning(
                            f"Skipping malicious path in zip: {member.filename}"
                        )
                    elif not member.is_dir():
                        zip_ref.extract(member, path=extract_dir)
        return True
    except Exception as e:
        logger.error(f"An unexpected error during extraction: {e}")
        return False


def find_executable_in_extracted(
    extract_dir: str, required_tool_name: str, os_system: str
) -> Optional[str]:
    """Finds the main executable within an extracted directory."""
    candidates: List[Tuple[int, str]] = []
    target_names = [required_tool_name.lower()]
    if os_system == "windows":
        target_names.append(f"{required_tool_name.lower()}.exe")
    for root, _, files in os.walk(extract_dir):
        for file in files:
            full_path = os.path.join(root, file)
            if system_is_executable(full_path, os_system):
                score = 0
                file_lower, rel_path = (
                    file.lower(),
                    os.path.relpath(full_path, extract_dir),
                )
                if file_lower in target_names:
                    score += 100
                elif os.path.splitext(file_lower)[0] in target_names:
                    score += 90
                elif required_tool_name.lower() in file_lower:
                    score += 50
                score -= rel_path.count(os.sep) * 10
                candidates.append((score, full_path))
    if candidates:
        candidates.sort(key=lambda x: x[0], reverse=True)
        logger.debug(f"Found executable candidate: {candidates[0][1]}")
        return candidates[0][1]
    return None


def system_is_executable(filepath: str, os_system: str) -> bool:
    """Checks if a file is executable on the current system."""
    if os_system == "windows":
        return filepath.lower().endswith((".exe", ".bat", ".cmd")) and os.path.isfile(
            filepath
        )
    return os.path.isfile(filepath) and os.access(filepath, os.X_OK)


# --- Python Tool Installation ---
def install_python_tool(
    tool_name: str, version: str, tool_dir: str, wheel_path: str
) -> Optional[str]:
    """Sets up a Python venv and installs the specified wheel file."""
    logger.info(f"Setting up Python environment for {tool_name} {version}...")
    venv_path = os.path.join(tool_dir, ".venv")
    try:
        subprocess.run(
            [sys.executable, "-m", "venv", venv_path], check=True, capture_output=True
        )
        pip_exec = os.path.join(
            venv_path,
            "bin/pip" if platform.system() != "Windows" else "Scripts/pip.exe",
        )
        subprocess.run(
            [pip_exec, "install", "--upgrade", "pip"], check=True, capture_output=True
        )
        logger.debug(f"Created pip venv at {venv_path}")
        logger.info(f"Installing local wheel {os.path.basename(wheel_path)}...")
        subprocess.run(
            [pip_exec, "install", wheel_path],
            check=True,
            capture_output=True,
            text=True,
        )
        logger.info(f"Successfully installed {tool_name} {version} via pip.")
    except (subprocess.CalledProcessError, FileNotFoundError) as e:
        err_msg = e.stderr if isinstance(e, subprocess.CalledProcessError) else str(e)
        logger.error(f"Failed to set up Python environment: {err_msg}")
        return None

    shim_path_base = os.path.join(tool_dir, tool_name)
    if platform.system() == "Windows":
        shim_content = f'@echo off\r\nset "PATH=%~dp0\\.venv\\Scripts;%PATH%"\r\n"%~dp0\\.venv\\Scripts\\{tool_name}.exe" %*\r\n'
        shim_path = shim_path_base + ".bat"
    else:
        shim_content = f'#!/bin/sh\n. "$(dirname "$0")/.venv/bin/activate"\nexec "$(dirname "$0")/.venv/bin/{tool_name}" "$@"\n'
        shim_path = shim_path_base
    try:
        with open(shim_path, "w", newline="\n") as f:
            f.write(shim_content)
        if platform.system() != "Windows":
            os.chmod(
                shim_path,
                stat.S_IRWXU
                | stat.S_IRGRP
                | stat.S_IXGRP
                | stat.S_IROTH
                | stat.S_IXOTH,
            )
        logger.info(f"Created shim script at {shim_path}")
    except Exception as e:
        logger.error(f"Failed to create shim script: {e}")
        return None
    return shim_path


# --- Core Tooler Logic ---
def get_gh_release_info(
    repo_full_name: str, version: Optional[str] = None
) -> Optional[Dict[str, Any]]:
    """Fetches GitHub release information."""
    if version and version.startswith("v"):
        url = f"https://api.github.com/repos/{repo_full_name}/releases/tags/{version}"
    elif version == "latest":
        url = f"https://api.github.com/repos/{repo_full_name}/releases/latest"
    elif version:
        url = f"https://api.github.com/repos/{repo_full_name}/releases/tags/{version}"
    else:
        url = f"https://api.github.com/repos/{repo_full_name}/releases/latest"
    logger.debug(f"Fetching GitHub release info from: {url}")
    try:
        headers = {"Accept": "application/vnd.github.v3+json"}
        if "GITHUB_TOKEN" in os.environ:
            headers["Authorization"] = f"token {os.environ['GITHUB_TOKEN']}"
            logger.debug("Using GITHUB_TOKEN.")
        response = requests.get(url, headers=headers)
        response.raise_for_status()
        return response.json()
    except requests.exceptions.RequestException as e:
        logger.error(f"Error fetching GitHub info: {e}")
        return None


def install_or_update_tool(
    tool_configs: ToolerConfig,
    tool_name: str,
    repo_full_name: str,
    version: str = "latest",
    force_update: bool = False,
) -> Optional[str]:
    """Downloads and prepares a tool from a GitHub release."""
    system_os, system_arch = platform.system(), get_system_arch()
    release_info = get_gh_release_info(repo_full_name, version)
    if not release_info:
        return None
    actual_version = release_info.get("tag_name")
    if not actual_version:
        return logger.error("Release has no tag_name, cannot proceed.")

    tool_key = (
        f"{repo_full_name}:v{actual_version}"
        if version == "latest"
        else f"{repo_full_name}:{version}"
    )
    tool_dir = os.path.join(
        get_tooler_tools_dir(), f"{repo_full_name.replace('/', '__')}", actual_version
    )
    if not force_update and (current_info := tool_configs["tools"].get(tool_key)):
        if (
            "executable_path" in current_info
            and os.path.exists(current_info["executable_path"])
            and system_is_executable(current_info["executable_path"], system_os)
        ):
            logger.info(f"Tool {tool_name} {actual_version} is already installed.")
            return current_info["executable_path"]
        else:
            logger.warning(
                f"Installation for {tool_name} {actual_version} is corrupted. Re-installing."
            )

    logger.info(f"Installing/Updating {tool_name} {actual_version}...")
    download_url, asset_name = find_asset_for_platform(
        release_info.get("assets", []), repo_full_name, system_arch, system_os
    )
    if not download_url or not asset_name:
        return None

    if os.path.exists(tool_dir):
        try:
            shutil.rmtree(tool_dir)
        except OSError as e:
            return logger.error(f"Error removing old directory {tool_dir}: {e}")
    os.makedirs(tool_dir, exist_ok=True)

    executable_path: Optional[str] = None
    install_type = "binary"
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_download_path = os.path.join(temp_dir, asset_name)
        if not download_file(download_url, temp_download_path):
            return None

        if asset_name.endswith(".whl"):
            install_type = "python-venv"
            executable_path = install_python_tool(
                tool_name, actual_version, tool_dir, temp_download_path
            )
        elif asset_name.endswith((".tar.gz", ".zip")):
            if not extract_archive(temp_download_path, tool_dir):
                return None
            executable_path = find_executable_in_extracted(
                tool_dir, tool_name, system_os
            )
        else:
            move_target_path = os.path.join(tool_dir, asset_name)
            try:
                shutil.move(temp_download_path, move_target_path)
                executable_path = move_target_path
            except shutil.Error as e:
                logger.error(f"Error moving file: {e}")
                return None

    if executable_path:
        if system_os != "Windows" and install_type == "binary":
            try:
                os.chmod(
                    executable_path,
                    stat.S_IRWXU
                    | stat.S_IRGRP
                    | stat.S_IXGRP
                    | stat.S_IROTH
                    | stat.S_IXOTH,
                )
            except OSError as e:
                logger.warning(f"Could not set permissions for {executable_path}: {e}")
        tool_configs["tools"][tool_key] = ToolInfo(
            tool_name=tool_name.lower(),
            repo=repo_full_name,
            version=actual_version,
            executable_path=executable_path,
            install_type=install_type,
            installed_at=datetime.now().isoformat(),
            last_accessed=datetime.now().isoformat(),
        )
        save_tool_configs(tool_configs)
        logger.info(
            f"Successfully installed {tool_name} {actual_version} to {executable_path}"
        )
        return executable_path
    else:
        logger.error(f"Installation failed for {tool_name} {actual_version}.")
        shutil.rmtree(tool_dir, ignore_errors=True)
        return None


def find_tool_executable(
    tool_configs: ToolerConfig, tool_query: str
) -> Optional[ToolInfo]:
    """Finds the tool_info dict for a tool based on the query."""
    repo, actual_key = tool_query.split(":")[0], None
    if ":" in tool_query:
        version = tool_query.split(":")[1]
        if not version.startswith("v") and version.lower() != "latest":
            version = f"v{version}"
        actual_key = f"{repo}:{version}"

    found_info: Optional[ToolInfo] = None
    if actual_key and actual_key in tool_configs["tools"]:
        found_info = tool_configs["tools"][actual_key]
        logger.debug(f"Direct match found for key: {actual_key}")
    elif not actual_key:  # Bare repo name provided
        latest_info: Optional[ToolInfo] = None
        last_time: Optional[datetime] = None
        for key, info in tool_configs["tools"].items():
            if info["repo"].lower() == repo.lower():
                current_time = datetime.fromisoformat(
                    info.get("last_accessed", "1970-01-01T00:00:00")
                )
                if not last_time or current_time > last_time:
                    latest_info, last_time, actual_key = info, current_time, key
        if latest_info:
            found_info = latest_info
            logger.info(
                f"Using version {found_info['version']} for {tool_query} (last accessed)."
            )

    if found_info and actual_key:
        if os.path.exists(found_info["executable_path"]) and system_is_executable(
            found_info["executable_path"], platform.system()
        ):
            found_info["last_accessed"] = datetime.now().isoformat()
            save_tool_configs(tool_configs)
            return found_info
        else:
            logger.warning(
                f"Executable for {found_info['repo']}@{found_info['version']} is missing/corrupt. Re-installing."
            )
            if actual_key in tool_configs["tools"]:
                del tool_configs["tools"][actual_key]
                save_tool_configs(tool_configs)

    return None


def check_for_updates(tool_configs: ToolerConfig) -> None:
    """Checks for updates for non-version-pinned tools."""
    if (update_days := tool_configs["settings"]["update_check_days"]) <= 0:
        return
    logger.info(f"Checking for tools not updated in >{update_days} days...")
    now, updates_found = datetime.now(), []
    for key, info in list(tool_configs["tools"].items()):
        if ":" in key:
            continue  # Skip fixed versions
        if (now - datetime.fromisoformat(info["last_accessed"])).days > update_days:
            if (release := get_gh_release_info(info["repo"], "latest")) and (
                tag := release.get("tag_name")
            ):
                if tag != info["version"]:
                    updates_found.append(
                        f"Tool {info['tool_name']} ({info['repo']}) has update: {info['version']} -> {tag}"
                    )
            info["last_accessed"] = now.isoformat()
            save_tool_configs(tool_configs)
    if updates_found:
        print("\n--- Tool Updates Available ---", file=sys.stderr)
        for msg in updates_found:
            print(f"  {msg}", file=sys.stderr)
        print(
            "To update, run `tooler update [repo/tool]` or `tooler update --all`.",
            file=sys.stderr,
        )
        print("----------------------------\n", file=sys.stderr)
    else:
        logger.info("No updates found or checks are not due.")


def list_installed_tools(tool_configs: ToolerConfig) -> None:
    """Lists all installed tools."""
    print("--- Installed Tooler Tools ---")
    if not tool_configs["tools"]:
        return print("  No tools installed yet.")
    for key, info in sorted(
        tool_configs["tools"].items(), key=lambda i: i[1]["repo"].lower()
    ):
        print(
            f"  - {info['repo']} (v{info['version']}) [type: {info.get('install_type', 'binary')}]"
        )
        print(f"    Path:    {info['executable_path']}\n")
    print("------------------------------")


def remove_tool(tool_configs: ToolerConfig, tool_query: str) -> bool:
    """Removes an installed tool."""
    keys_to_remove = [
        k
        for k, i in tool_configs["tools"].items()
        if k.lower() == tool_query.lower()
        or (":" not in tool_query and i["repo"].lower() == tool_query.lower())
    ]
    if not keys_to_remove:
        return not logger.error(f"Tool '{tool_query}' not found.")
    for key in keys_to_remove:
        info = tool_configs["tools"][key]
        tool_dir = (
            os.path.dirname(info["executable_path"])
            if info.get("install_type") == "python-venv"
            else os.path.dirname(os.path.dirname(info["executable_path"]))
        )
        if os.path.exists(tool_dir):
            logger.info(f"Removing directory: {tool_dir}")
            shutil.rmtree(tool_dir, ignore_errors=True)
        del tool_configs["tools"][key]
    save_tool_configs(tool_configs)
    logger.info(f"Tool(s) for '{tool_query}' removed successfully.")
    return True


# --- Main CLI ---
def main() -> None:
    """Main entrypoint for the tooler CLI."""
    parser = argparse.ArgumentParser(
        description="A CLI tool manager for GitHub Releases.",
        formatter_class=argparse.RawTextHelpFormatter,
        epilog="""Examples:
  tooler run nektos/act                 # Run latest version
  tooler run nektos/act:v0.2.79 -- help # Run specific version with args
  tooler run adrienverge/yamllint       # Run Python tool from .whl asset

  tooler list                           # List all installed tools
  tooler update nektos/act              # Update to latest version
  tooler update --all                   # Update all non-pinned tools
  tooler remove nektos/act              # Remove all versions of a tool
""",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="count",
        default=0,
        help="Increase verbosity (-vv for debug)",
    )
    parser.add_argument(
        "-q", "--quiet", action="store_true", help="Suppress all output except errors."
    )
    subparsers = parser.add_subparsers(
        dest="command", help="Available commands", required=True
    )

    run_parser = subparsers.add_parser("run", help="Run a tool.")
    run_parser.add_argument(
        "tool_id", help="GitHub repository (e.g., 'owner/repo:vX.Y.Z')."
    )
    run_parser.add_argument(
        "tool_args", nargs=argparse.REMAINDER, help="Arguments to pass to tool."
    )
    subparsers.add_parser("list", help="List all installed tools.")
    update_parser = subparsers.add_parser("update", help="Update one or all tools.")
    update_group = update_parser.add_mutually_exclusive_group(required=True)
    update_group.add_argument("tool_id", nargs="?", help="Specific tool to update.")
    update_group.add_argument(
        "--all", action="store_true", help="Update all applicable tools."
    )
    remove_parser = subparsers.add_parser("remove", help="Remove an installed tool.")
    remove_parser.add_argument("tool_id", help="Tool to remove (e.g., 'owner/repo').")
    config_parser = subparsers.add_parser(
        "config", help="Manage tooler's configuration."
    )
    config_subparsers = config_parser.add_subparsers(
        dest="config_command", help="Config commands", required=True
    )
    config_get_parser = config_subparsers.add_parser(
        "get", help="Get a configuration setting."
    )
    config_get_parser.add_argument(
        "key", nargs="?", help="Key to get (if omitted, shows all settings)."
    )
    config_set_parser = config_subparsers.add_parser(
        "set", help="Set a configuration setting."
    )
    config_set_parser.add_argument(
        "key_value", help="Key=Value pair (e.g., 'update_check_days=30')."
    )

    args = parser.parse_args()

    ch = logging.StreamHandler(sys.stderr)
    ch.setFormatter(ToolerFormatter())
    if args.quiet:
        ch.setLevel(logging.ERROR)
    elif args.verbose == 0:
        ch.setLevel(logging.WARNING)
    elif args.verbose == 1:
        ch.setLevel(logging.INFO)
    else:
        ch.setLevel(logging.DEBUG)
    if logger.hasHandlers():
        logger.handlers.clear()
    logger.addHandler(ch)

    tool_configs = load_tool_configs()
    if args.command in ["run", "update"]:
        check_for_updates(tool_configs)

    if args.command == "list":
        list_installed_tools(tool_configs)
    elif args.command == "remove":
        remove_tool(tool_configs, args.tool_id)
    elif args.command == "update":
        if args.all:
            logger.info("Updating all applicable tools...")
            updated_count = 0
            for key, info in list(tool_configs["tools"].items()):
                if ":" not in key:
                    updated_count += (
                        1
                        if install_or_update_tool(
                            tool_configs,
                            info["tool_name"],
                            info["repo"],
                            "latest",
                            True,
                        )
                        else 0
                    )
            logger.info(
                f"Update process finished. {updated_count} tool(s) were checked/updated."
            )
        elif args.tool_id:
            repo_full_name, tool_name = (
                args.tool_id.split(":")[0],
                args.tool_id.split("/")[-1].split(":")[0],
            )
            logger.info(f"Attempting to update {args.tool_id}...")
            if install_or_update_tool(
                tool_configs, tool_name, repo_full_name, "latest", True
            ):
                logger.info(f"{args.tool_id} updated successfully.")
            else:
                logger.error(f"Failed to update {args.tool_id}.")
                sys.exit(1)
    elif args.command == "config":
        if args.config_command == "get":
            if args.key:
                print(
                    tool_configs["settings"].get(
                        args.key, f"Setting '{args.key}' not found."
                    )
                )
            else:
                print(
                    "--- Tooler Settings ---\n"
                    + "\n".join(
                        f"  {k}: {v}" for k, v in tool_configs["settings"].items()
                    )
                )
        elif args.config_command == "set":
            key, value_str = (
                args.key_value.split("=", 1) if "=" in args.key_value else (None, None)
            )
            if key == "update_check_days" and value_str:
                try:
                    tool_configs["settings"]["update_check_days"] = int(value_str)
                    save_tool_configs(tool_configs)
                    logger.info(f"Setting '{key}' updated to '{value_str}'.")
                except ValueError:
                    logger.error(
                        "Invalid value for 'update_check_days'. Must be an integer."
                    )
            elif key:
                logger.error(
                    f"'{key}' is not a valid configuration setting. Valid settings: 'update_check_days'"
                )
            else:
                logger.error("Invalid format. Use 'key=value'.")
    elif args.command == "run":
        repo_full_name, tool_name = (
            args.tool_id.split(":")[0],
            args.tool_id.split("/")[-1].split(":")[0],
        )
        version_req = args.tool_id.split(":", 1)[1] if ":" in args.tool_id else "latest"
        tool_info = find_tool_executable(tool_configs, args.tool_id)
        if not tool_info:
            logger.info(
                f"Tool {args.tool_id} not found locally or is corrupted. Attempting to install..."
            )
            if not install_or_update_tool(
                tool_configs, tool_name, repo_full_name, version=version_req
            ):
                sys.exit(1)
            tool_info = find_tool_executable(load_tool_configs(), args.tool_id)
        if tool_info:
            cmd = [tool_info["executable_path"]] + args.tool_args
            logger.debug(f"Executing: {cmd}")
            try:
                sys.exit(subprocess.run(cmd).returncode)
            except Exception as e:
                logger.error(
                    f"Error executing tool '{tool_info['executable_path']}': {e}"
                )
                sys.exit(1)
        else:
            logger.error(f"Failed to find or install executable for {args.tool_id}.")
            sys.exit(1)


if __name__ == "__main__":
    main()
