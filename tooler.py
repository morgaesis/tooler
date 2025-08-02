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
from importlib import metadata
from typing import Any, Dict, List, Optional, Set, Tuple

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

    # Standardize to "amd64", "arm64", or "arm" for internal use
    if (
        "aarch64" in machine
        or "arm64" in machine
        or (system == "darwin" and "arm" in machine)
    ):
        return "arm64"
    if "x86_64" in machine or "amd64" in machine:
        return "amd64"
    if "armv7" in machine or "arm" in machine:  # Fallback for 32-bit ARM on Linux
        return "arm"

    return machine  # Return raw machine if not specifically mapped


# --- Centralized Alias and Configuration ---
PLATFORM_ALIASES = {
    "os": {
        "linux": {
            "aliases": ["linux", "unknown-linux", "pc-linux"],
            "distros": ["gnu", "musl"],
        },
        "darwin": {"aliases": ["macos", "darwin", "apple-darwin"]},
        "windows": {
            "aliases": ["windows", "win", "pc-windows"],
            "distros": ["msvc", "gnu"],
        },
    },
    "arch": {
        "amd64": {"aliases": ["amd64", "x86_64", "x64"]},
        "arm64": {"aliases": ["arm64", "aarch64"]},
        "arm": {"aliases": ["arm", "armv7", "armv7l"]},
    },
}

# The new scoring system replaces the rigid, verbose tier list.
# Adjust weights to change matching priorities.
SCORING_WEIGHTS = {
    "os": 10,
    "arch": 10,
    "distro": 5,
    "tool_name": 3,
    "version": 2,
}

# Extensions that disqualify an asset from being a primary executable/archive.
# .whl is handled separately as a fallback.
INVALID_EXTENSIONS = (
    ".sha256",
    ".asc",
    ".sig",
    ".pem",
    ".pub",
    ".md",
    ".txt",
    ".pom",
    ".xml",
    ".json",
    ".whl",
)


def get_system_info() -> Tuple[str, str]:
    """Determines the standardized OS and architecture."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    if "aarch64" in machine or "arm64" in machine:
        arch = "arm64"
    elif "x86_64" in machine or "amd64" in machine:
        arch = "amd64"
    elif "arm" in machine:  # Broad catch for armv7l etc.
        arch = "arm"
    else:
        arch = machine

    return system, arch


def _get_platform_keywords(
    platform_type: str, platform_name: str
) -> Tuple[Set[str], Set[str]]:
    """Helper to get aliases and distros for a given platform."""
    config = PLATFORM_ALIASES.get(platform_type, {}).get(platform_name.lower(), {})
    aliases = set(config.get("aliases", [platform_name.lower()]))
    distros = set(config.get("distros", []))
    return aliases, distros


def find_asset_for_platform(
    assets: List[Dict[str, Any]],
    repo_full_name: str,
    system_os: str,
    system_arch: str,
) -> Tuple[Optional[str], Optional[str]]:
    """
    Finds the best binary asset by prioritizing archives over packages.

    Matching Priority:
    1. Archive (OS + Arch)
    2. Package (OS + Arch)
    3. Archive (OS only)
    4. Package (OS only)
    5. And so on for Arch only, then .whl fallback.

    Returns:
        A tuple containing the asset's download URL and name, or (None, None).
    """
    system_os, system_arch = system_os.lower(), system_arch.lower()

    asset_names = [a.get("name", "unnamed-asset") for a in assets]
    logger.debug(f"Available assets for '{repo_full_name}': {asset_names}")

    os_aliases, _ = _get_platform_keywords("os", system_os)
    arch_aliases, _ = _get_platform_keywords("arch", system_arch)

    # Define preferred asset types
    ARCHIVE_EXTS = (".tar.gz", ".zip", ".tar.xz", ".tgz")
    PACKAGE_EXTS = (".apk", ".deb", ".rpm")

    # --- 1. Categorize all assets in a single pass ---
    candidates = {
        "os_arch_archive": [],
        "os_arch_package": [],
        "os_only_archive": [],
        "os_only_package": [],
        "arch_only_archive": [],
        "arch_only_package": [],
    }

    for asset in assets:
        name_lower = asset.get("name", "").lower()
        if name_lower.endswith(INVALID_EXTENSIONS):
            continue

        has_os = any(alias in name_lower for alias in os_aliases)
        has_arch = any(alias in name_lower for alias in arch_aliases)

        is_archive = name_lower.endswith(ARCHIVE_EXTS)
        is_package = name_lower.endswith(PACKAGE_EXTS)

        # Assign asset to the correct category
        if has_os and has_arch:
            if is_archive:
                candidates["os_arch_archive"].append(asset)
            elif is_package:
                candidates["os_arch_package"].append(asset)
        elif has_os:
            if is_archive:
                candidates["os_only_archive"].append(asset)
            elif is_package:
                candidates["os_only_package"].append(asset)
        elif has_arch:
            if is_archive:
                candidates["arch_only_archive"].append(asset)
            elif is_package:
                candidates["arch_only_package"].append(asset)

    # --- 2. Select the first asset from the highest-priority list found ---
    # The order of this list defines our preference
    priority_order = [
        "os_arch_archive",
        "os_arch_package",
        "os_only_archive",
        "os_only_package",
        "arch_only_archive",
        "arch_only_package",
    ]

    best_candidates = []
    for category in priority_order:
        if candidates[category]:
            logger.debug(
                f"Found {len(candidates[category])} candidates in category '{category}'."
            )
            best_candidates = candidates[category]
            break

    if best_candidates:
        # Simply pick the first asset from the highest-priority list
        best_asset_dict = best_candidates[0]
        logger.info(f"Found best match: '{best_asset_dict['name']}'")
        return best_asset_dict.get("browser_download_url"), best_asset_dict.get("name")

    # --- 3. Final fallback for cases where no other logic finds a match ---
    logger.warning("No asset matched platform criteria. Checking for fallbacks.")
    for asset in assets:
        if asset.get("name", "").lower().endswith(".whl"):
            logger.warning("Falling back to Python wheel.")
            return asset.get("browser_download_url"), asset.get("name")
        elif (
            asset.get("name", "").lower().split(".tar")[0]
            == repo_full_name.split("/").pop()
        ):
            return asset.get("browser_download_url"), asset.get("name")

    logger.error("No suitable asset found after all checks.")
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


def extract_archive(
    archive_path: str, extract_dir: str, tool_name: str, os_system: str
) -> Optional[str]:
    """
    Extracts a tar/zip archive and attempts to find the main executable within it.
    Returns the path to the discovered executable or None if not found/error.
    """
    logger.info(f"Extracting {os.path.basename(archive_path)}...")
    try:
        if archive_path.endswith((".tar.gz", ".tar.xz", ".tgz")):
            with tarfile.open(
                archive_path, "r:*"
            ) as tar:  # r:* auto-detects compression
                # Security check for path traversal
                for member in tar.getmembers():
                    member_path = os.path.join(extract_dir, member.name)
                    if not os.path.realpath(member_path).startswith(
                        os.path.realpath(extract_dir)
                    ):
                        logger.warning(f"Skipping malicious path in tar: {member.name}")
                        continue
                    tar.extract(member, path=extract_dir)
        elif archive_path.endswith(".zip"):
            with zipfile.ZipFile(archive_path, "r") as zip_ref:
                # Security check for path traversal
                for member in zip_ref.infolist():
                    member_path = os.path.join(extract_dir, member.filename)
                    if not os.path.realpath(member_path).startswith(
                        os.path.realpath(extract_dir)
                    ):
                        logger.warning(
                            f"Skipping malicious path in zip: {member.filename}"
                        )
                        continue
                    # Only extract files, not directories (they are created by extracting files)
                    if not member.is_dir():
                        zip_ref.extract(member, path=extract_dir)
        else:
            logger.error(f"Unsupported archive format: {archive_path}")
            return None

        # Find the executable in the extracted directory
        executable_path = find_executable_in_extracted(
            extract_dir, tool_name, os_system
        )
        if not executable_path:
            logger.error(
                f"Could not find executable for {tool_name} in extracted archive."
            )
            return None

        # Make executable if on Unix-like system
        if os_system != "Windows":
            try:
                os.chmod(executable_path, 0o755)  # rwxr-xr-x
            except OSError as e:
                logger.warning(
                    f"Could not set execute permissions on {executable_path}: {e}"
                )

        logger.info(f"Successfully extracted and found executable: {executable_path}")
        return executable_path

    except (tarfile.ReadError, zipfile.BadZipFile) as e:
        logger.error(f"Failed to extract archive {os.path.basename(archive_path)}: {e}")
        return None
    except Exception as e:
        logger.error(f"An unexpected error occurred during extraction: {e}")
        return None


def find_executable_in_extracted(
    extract_dir: str, required_tool_name: str, os_system: str
) -> Optional[str]:
    """Finds the main executable within an extracted directory."""
    candidates: List[Tuple[int, str]] = []
    target_names = [required_tool_name.lower()]
    if os_system == "windows":
        target_names.append(f"{required_tool_name.lower()}.exe")

    # Prioritize executables closer to the root, matching target names
    for root, _, files in os.walk(extract_dir):
        for file in files:
            full_path = os.path.join(root, file)
            if system_is_executable(full_path, os_system):
                score = 0
                file_lower, rel_path = (
                    file.lower(),
                    os.path.relpath(full_path, extract_dir),
                )

                # Higher score for direct name match (e.g., "logu" or "logu.exe")
                if file_lower in target_names:
                    score += 100
                # Next highest for name match without extension (e.g., "logu" for "logu.sh")
                elif os.path.splitext(file_lower)[0] in target_names:
                    score += 90
                # Decent score for tool name being a substring
                elif required_tool_name.lower() in file_lower:
                    score += 50

                # Penalize deeper paths
                score -= rel_path.count(os.sep) * 10
                candidates.append((score, full_path))

    if candidates:
        candidates.sort(key=lambda x: x[0], reverse=True)
        logger.debug(
            f"Selected executable candidate: {candidates[0][1]} (score: {candidates[0][0]})"
        )
        return candidates[0][1]
    return None


def system_is_executable(filepath: str, os_system: str) -> bool:
    """Checks if a file is executable on the current system, considering common executable extensions."""
    if os_system == "Windows":
        return filepath.lower().endswith((".exe", ".bat", ".cmd")) and os.path.isfile(
            filepath
        )
    # For Unix-like systems, check if it's a regular file and has execute permission
    # Also, avoid common library extensions explicitly for robustness
    if os.path.isfile(filepath) and os.access(filepath, os.X_OK):
        # Exclude shared libraries unless explicitly looking for them (which we are not here)
        if filepath.lower().endswith((".dll", ".so", ".dylib")):
            return False
        return True
    return False


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

    # Create a shim script that activates the venv and runs the tool
    # The tool's actual executable would be within .venv/bin/tool_name or .venv/Scripts/tool_name.exe
    shim_path_base = os.path.join(tool_dir, tool_name)
    if platform.system() == "Windows":
        # Point to the actual executable in the venv's Scripts folder
        # For Python tools, the console entry point is usually tool_name.exe or tool_name.cmd/tool_name.py
        # We assume tool_name.exe is what pip creates for console scripts on Windows
        shim_content = f'@echo off\r\n"%~dp0\\.venv\\Scripts\\{tool_name}.exe" %*\r\n'
        shim_path = (
            shim_path_base + ".cmd"
        )  # Using .cmd is more common for simple entry points
    else:
        # For Unix-like, directly execute the script from the venv's bin folder
        shim_content = f'#!/bin/sh\nexec "$(dirname "$0")/.venv/bin/{tool_name}" "$@"\n'
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
        logger.error("Release has no tag_name, cannot proceed.")
        return None

    tool_key = (
        f"{repo_full_name}:v{actual_version}"
        if version == "latest"
        else f"{repo_full_name}:{version}"
    )
    # Install tools into a path like: ~/.local/share/tooler/tools/owner__repo/vX.Y.Z/
    # This directory will contain extracted contents or the direct binary.
    tool_install_base_dir = os.path.join(
        get_tooler_tools_dir(), repo_full_name.replace("/", "__")
    )
    tool_version_dir = os.path.join(tool_install_base_dir, actual_version)

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

    # Find the most suitable asset for the platform
    download_url, asset_name = find_asset_for_platform(
        release_info.get("assets", []),
        repo_full_name,
        system_arch,
        system_os,
    )
    if not download_url or not asset_name:
        logger.error(
            f"No suitable asset found for {repo_full_name} {actual_version} for your platform."
        )
        return None

    # Clean up any existing installation directory for this version
    if os.path.exists(tool_version_dir):
        try:
            shutil.rmtree(tool_version_dir)
        except OSError as e:
            logger.error(f"Error removing old directory {tool_version_dir}: {e}")
            return None
    os.makedirs(
        tool_version_dir, exist_ok=True
    )  # Create the version-specific installation directory

    executable_path: Optional[str] = None
    install_type = "binary"  # Default install type

    with tempfile.TemporaryDirectory() as temp_dir:
        temp_download_path = os.path.join(temp_dir, asset_name)
        if not download_file(download_url, temp_download_path):
            return None

        if asset_name.lower().endswith(".whl"):
            install_type = "python-venv"
            executable_path = install_python_tool(
                tool_name, actual_version, tool_version_dir, temp_download_path
            )
        elif any(
            asset_name.lower().endswith(ext)
            for ext in [".tar.gz", ".zip", ".tar.xz", ".tgz"]
        ):
            # Extract and find executable within the extracted directory
            executable_path = extract_archive(
                temp_download_path, tool_version_dir, tool_name, system_os
            )
        else:
            # Assume it's a direct executable (e.g., .exe, or a binary with no extension)
            # Move it directly into the tool_version_dir, renaming it to the tool_name for simplicity
            final_binary_name = (
                f"{tool_name}.exe" if system_os == "Windows" else tool_name
            )
            move_target_path = os.path.join(tool_version_dir, final_binary_name)
            try:
                shutil.move(temp_download_path, move_target_path)
                executable_path = move_target_path
                if system_os != "Windows":
                    os.chmod(executable_path, 0o755)  # Make executable on Unix-like
                logger.info(f"Installed direct executable to: {executable_path}")
            except shutil.Error as e:
                logger.error(f"Error moving direct executable {asset_name}: {e}")
                return None

    if executable_path:
        # Update config with new installation details
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
        logger.error(
            f"Installation failed for {tool_name} {actual_version}. Cleaning up."
        )
        shutil.rmtree(tool_version_dir, ignore_errors=True)  # Clean up partial install
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
    elif not actual_key:  # Bare repo name provided, find the latest installed version
        latest_info: Optional[ToolInfo] = None
        last_time: Optional[datetime] = None
        for key, info in tool_configs["tools"].items():
            # Check if this tool is for the same repo, ignoring version suffix
            # e.g., "owner/repo:v1.0.0" and "owner/repo" should match the repo part
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
    # Using list() to iterate over a copy of keys, allowing modification during iteration
    for key, info in list(tool_configs["tools"].items()):
        # Only check tools installed as 'latest' or without a specific version in their key
        # (This logic implies that 'latest' tools don't have a version in their key, which is defined by tool_key logic)
        # If the tool_key contains a specific version (e.g., "repo:v1.2.3"), it's considered pinned.
        if ":" in key:  # This means it's a version-pinned tool
            continue

        last_accessed_dt = datetime.fromisoformat(info["last_accessed"])
        if (now - last_accessed_dt).days > update_days:
            logger.debug(
                f"Checking for update for {info['repo']} (current: {info['version']})"
            )
            if (release := get_gh_release_info(info["repo"], "latest")) and (
                tag := release.get("tag_name")
            ):
                if tag != info["version"]:
                    updates_found.append(
                        f"Tool {info['tool_name']} ({info['repo']}) has update: {info['version']} -> {tag}"
                    )
                # Always update last_accessed after checking, regardless of if an update was found
                info["last_accessed"] = now.isoformat()
                save_tool_configs(
                    tool_configs
                )  # Save config after each update check to persist last_accessed
            else:
                logger.warning(
                    f"Could not get latest release for {info['repo']} during update check."
                )

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
    for _, info in sorted(
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
        # The tool_dir now correctly points to the version-specific installation folder
        # For python-venv, the shim is in tool_dir, and .venv is inside.
        # For binary, the executable is directly in tool_dir or a subfolder.
        # In both cases, removing `tool_version_dir` (which is `os.path.dirname(info["executable_path"])`
        # or its parent if the binary is nested one level down from the version dir) is appropriate.

        # Determine the root installation directory for this version
        # It's safest to base this off the 'executable_path' recorded.
        # The structure is `get_tooler_tools_dir()/repo__name/version/executable_path_inside`
        # So we need to go up from `executable_path` until we hit the `version` directory.

        # A safer approach for removal: reconstruct the base tool version directory
        tool_install_base_dir = os.path.join(
            get_tooler_tools_dir(), info["repo"].replace("/", "__")
        )
        tool_version_dir = os.path.join(tool_install_base_dir, info["version"])

        if os.path.exists(tool_version_dir):
            logger.info(f"Removing directory: {tool_version_dir}")
            shutil.rmtree(tool_version_dir, ignore_errors=True)
        else:
            logger.warning(
                f"Tool directory not found, assuming already removed: {tool_version_dir}"
            )

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
  tooler run nektos/act              # Run latest version
  tooler run nektos/act:v0.2.79 -- help # Run specific version with args
  tooler run adrienverge/yamllint    # Run Python tool from .whl asset

  tooler list                        # List all installed tools
  tooler update nektos/act           # Update to latest version
  tooler update --all                # Update all non-pinned tools
  tooler remove nektos/act           # Remove all versions of a tool
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
    subparsers.add_parser(
        "version",
        help="Show the current version",
    )

    args = parser.parse_args()

    if args.command == "version":
        try:
            version = metadata.version("tooler")
        except metadata.PackageNotFoundError:
            version = "unknown"
        print(f"tooler v{version}")
        return

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
    # Perform update check only if `run` or `update` command is used,
    # and if it's not a `run` with a pinned version.
    if args.command in ["run", "update"]:
        if (
            args.command == "run" and ":" in args.tool_id
        ):  # A specific version is requested
            logger.debug(f"Skipping update check for pinned tool: {args.tool_id}")
        else:
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
                # Only update tools that are not version-pinned in the config
                if ":" not in key:
                    updated = install_or_update_tool(
                        tool_configs,
                        info["tool_name"],
                        info["repo"],
                        "latest",
                        True,
                    )
                    if updated:
                        updated_count += 1
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
            tool_info_path = install_or_update_tool(  # Catch the returned path
                tool_configs, tool_name, repo_full_name, version=version_req
            )
            if (
                not tool_info_path
            ):  # Check if install_or_update_tool returned a valid path
                sys.exit(1)
            # Re-load config and find tool info after installation to ensure it's up-to-date
            tool_info = find_tool_executable(load_tool_configs(), args.tool_id)

        if (
            tool_info and tool_info["executable_path"]
        ):  # Ensure executable_path is not None
            cmd = [tool_info["executable_path"]] + args.tool_args
            logger.debug(f"Executing: {cmd}")
            try:
                # Use subprocess.run directly as we expect the tool's output to go to stdout/stderr
                # and its exit code to be the primary result.
                sys.exit(subprocess.run(cmd).returncode)
            except FileNotFoundError:
                logger.error(
                    f"Executable not found at '{tool_info['executable_path']}'. It might have been moved or deleted."
                )
                sys.exit(1)
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
