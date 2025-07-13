#!/usr/bin/env python3

import os
import sys
import json
import requests
import shutil
import tarfile
import zipfile
import platform
import stat
from datetime import datetime, timedelta
import argparse
from tqdm import tqdm  # Minimalistic loading bar
import logging
import subprocess  # For executing pip and Python commands
import tempfile  # For safely handling temporary wheel files

# --- Configuration Constants ---
APP_NAME = "tooler"
CONFIG_DIR_NAME = ".tooler"
TOOLS_DIR_NAME = "tools"
CONFIG_FILE_NAME = "config.json"
DEFAULT_UPDATE_CHECK_DAYS = 60  # Days before notifying about potential updates

# --- Global Paths (Lazy Initialization) ---
_USER_DATA_DIR = None
_USER_CONFIG_DIR = None
_TOOLER_CONFIG_FILE_PATH = None
_TOOLER_TOOLS_DIR = None

# --- Logger Setup ---
logger = logging.getLogger(APP_NAME)
logger.setLevel(
    logging.DEBUG
)  # Internal logger level set to DEBUG to capture all messages


class ToolerFormatter(logging.Formatter):
    FORMATS = {
        logging.DEBUG: "\033[90mDEBUG> %(message)s\033[0m",  # Gray
        logging.INFO: "\033[94mINFO> %(message)s\033[0m",  # Blue
        logging.WARNING: "\033[93mWARN> %(message)s\033[0m",  # Yellow
        logging.ERROR: "\033[91mERROR> %(message)s\033[0m",  # Red
        logging.CRITICAL: "\033[91mCRIT> %(message)s\033[0m",  # Red
    }

    def format(self, record):
        log_fmt = self.FORMATS.get(record.levelno)
        # Apply color only if output is a TTY
        if sys.stderr.isatty():
            formatter = logging.Formatter(log_fmt)
        else:
            formatter = logging.Formatter(
                "%(levelname)s> %(message)s"
            )  # No colors if not TTY
        return formatter.format(record)


# --- Global Functions (Lazy Initialization for Paths) ---
def get_user_data_dir():
    """Determines the user-specific data directory."""
    global _USER_DATA_DIR
    if _USER_DATA_DIR is None:
        if platform.system() == "Windows":
            _USER_DATA_DIR = os.path.join(
                os.environ.get("APPDATA", os.path.expanduser("~")), APP_NAME
            )
        elif platform.system() == "Darwin":  # macOS
            _USER_DATA_DIR = os.path.join(
                os.path.expanduser("~/Library/Application Support"), APP_NAME
            )
        else:  # Linux/Unix
            _USER_DATA_DIR = os.path.join(
                os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")),
                APP_NAME,
            )
    os.makedirs(_USER_DATA_DIR, exist_ok=True)
    return _USER_DATA_DIR


def get_user_config_dir():
    """Determines the user-specific config directory."""
    global _USER_CONFIG_DIR
    if _USER_CONFIG_DIR is None:
        if platform.system() == "Windows":
            _USER_CONFIG_DIR = os.path.join(
                os.environ.get("APPDATA", os.path.expanduser("~")), CONFIG_DIR_NAME
            )
        else:  # macOS, Linux/Unix
            _USER_CONFIG_DIR = os.path.join(
                os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")),
                CONFIG_DIR_NAME,
            )
    os.makedirs(_USER_CONFIG_DIR, exist_ok=True)
    return _USER_CONFIG_DIR


def get_tooler_config_file_path():
    """Returns the path to the tooler's main configuration file."""
    global _TOOLER_CONFIG_FILE_PATH
    if _TOOLER_CONFIG_FILE_PATH is None:
        _TOOLER_CONFIG_FILE_PATH = os.path.join(get_user_config_dir(), CONFIG_FILE_NAME)
    return _TOOLER_CONFIG_FILE_PATH


def get_tooler_tools_dir():
    """Returns the path where actual tool binaries are stored."""
    global _TOOLER_TOOLS_DIR
    if _TOOLER_TOOLS_DIR is None:
        _TOOLER_TOOLS_DIR = os.path.join(get_user_data_dir(), TOOLS_DIR_NAME)
    os.makedirs(_TOOLER_TOOLS_DIR, exist_ok=True)
    return _TOOLER_TOOLS_DIR


# --- Tooler Configuration Management ---
def load_tool_configs():
    """Loads tool configurations from the JSON file."""
    config_path = get_tooler_config_file_path()
    if not os.path.exists(config_path):
        return {
            "tools": {},
            "settings": {"update_check_days": DEFAULT_UPDATE_CHECK_DAYS},
        }
    try:
        with open(config_path, "r") as f:
            config = json.load(f)
            if "tools" not in config:
                config["tools"] = {}
            if "settings" not in config:
                config["settings"] = {"update_check_days": DEFAULT_UPDATE_CHECK_DAYS}
            elif "update_check_days" not in config["settings"]:
                config["settings"]["update_check_days"] = DEFAULT_UPDATE_CHECK_DAYS
            return config
    except json.JSONDecodeError:
        logger.error(
            f"Could not parse config file at {config_path}. It might be corrupted. Starting with an empty config."
        )
        return {
            "tools": {},
            "settings": {"update_check_days": DEFAULT_UPDATE_CHECK_DAYS},
        }


def save_tool_configs(configs):
    """Saves tool configurations to the JSON file."""
    config_path = get_tooler_config_file_path()
    os.makedirs(os.path.dirname(config_path), exist_ok=True)
    with open(config_path, "w") as f:
        json.dump(configs, f, indent=2)


# --- Platform and Architecture Mapping ---
def get_system_arch():
    """Determines the standard system architecture string."""
    machine = platform.machine().lower()
    system = platform.system().lower()

    if system == "linux":
        if "arm" in machine or "aarch64" in machine:
            return "arm64" if "64" in machine else "arm"
        elif "x86_64" in machine or "amd64" in machine:
            return "amd64"
        elif "x86" in machine:
            return "386"
    elif system == "darwin":  # macOS
        if "arm" in machine or "aarch64" in machine:
            return "arm64"
        elif "x86_64" in machine or "amd64" in machine:
            return "amd64"
    elif system == "windows":
        if "arm" in machine or "aarch64" in machine:
            return "arm64"
        elif "x86_64" in machine or "amd64" in machine:
            return "amd64"
        elif "x86" in machine:
            return "386"

    return machine


def map_arch_to_github_release(arch, system):
    """Maps internal arch/system to common GitHub release file naming conventions."""
    system = system.lower()

    if system == "linux":
        if arch == "amd64":
            return "linux_amd64"
        if arch == "arm64":
            return "linux_arm64"
        if arch == "arm":
            return "linux_arm"
        if arch == "386":
            return "linux_386"
    elif system == "darwin":
        if arch == "amd64":
            return "darwin_amd64"
        if arch == "arm64":
            return "darwin_arm64"
    elif system == "windows":
        if arch == "amd64":
            return "windows_amd64"
        if arch == "arm64":
            return "windows_arm64"
        if arch == "386":
            return "windows_386"

    if arch == "amd64":
        return (
            f"{system}_x86_64",
            f"{system}-x86_64",
            f"{system}-amd64",
            f"{system}_64",
            f"{system}64",
        )
    if arch == "arm64":
        return f"{system}_aarch64", f"{system}-aarch64", f"{system}-arm64"
    if arch == "386":
        return f"{system}_i386", f"{system}-i386", f"{system}_x86", f"{system}-x86"

    return None


def find_asset_for_platform(assets, tool_name, system_arch, system_os):
    """
    Finds the most suitable asset URL for the given platform from a list of GitHub release assets.
    Prioritizes .tar.gz and .zip over other formats.
    """
    system_os_lower = system_os.lower()

    possible_arch_patterns = [
        f"{system_os_lower}_{system_arch}",
        f"{system_os_lower}-{system_arch}",
    ]
    mapped_patterns = map_arch_to_github_release(system_arch, system_os)
    if mapped_patterns:
        if isinstance(mapped_patterns, str):
            possible_arch_patterns.append(mapped_patterns)
        else:
            possible_arch_patterns.extend(mapped_patterns)

    if system_os_lower == "windows":
        possible_arch_patterns.append("windows")
    elif system_os_lower == "darwin":
        possible_arch_patterns.append("macos")

    possible_arch_patterns = sorted(
        list(set(possible_arch_patterns)), key=len, reverse=True
    )

    logger.debug(f"Searching for assets matching {system_os_lower} and {system_arch}")
    logger.debug(f"Potential patterns: {possible_arch_patterns}")

    best_asset = None

    # Priority 1: Archives
    for pattern in possible_arch_patterns:
        for asset in assets:
            asset_name_lower = asset["name"].lower()
            if pattern in asset_name_lower:
                if ".tar.gz" in asset_name_lower or ".zip" in asset_name_lower:
                    if best_asset is None or (
                        ".tar.gz" in asset_name_lower
                        and ".zip" not in best_asset["name"].lower()
                    ):
                        best_asset = asset
                        logger.debug(f"Found potential archive asset: {asset['name']}")
                        break
        if best_asset and (
            ".tar.gz" in best_asset["name"].lower()
            or ".zip" in best_asset["name"].lower()
        ):
            break

    # Priority 2: Direct executables (if no suitable archive found)
    if best_asset is None:
        for pattern in possible_arch_patterns:
            for asset in assets:
                asset_name_lower = asset["name"].lower()
                if (
                    pattern in asset_name_lower
                    and ".tar.gz" not in asset_name_lower
                    and ".zip" not in asset_name_lower
                    and ".whl" not in asset_name_lower
                    and ".tar" not in asset_name_lower
                ):
                    if tool_name.lower() in os.path.splitext(asset_name_lower)[0]:
                        if system_os_lower == "windows" and asset_name_lower.endswith(
                            ".exe"
                        ):
                            best_asset = asset
                            logger.debug(
                                f"Found potential executable asset: {asset['name']}"
                            )
                            break
                        elif (
                            system_os_lower != "windows"
                            and not asset_name_lower.endswith(".exe")
                        ):
                            best_asset = asset
                            logger.debug(
                                f"Found potential executable asset: {asset['name']}"
                            )
                            break
            if best_asset:
                break

    # Priority 3: Python wheels/source (lowest priority)
    if best_asset is None:
        for asset in assets:
            asset_name_lower = asset["name"].lower()
            if tool_name.lower() in asset_name_lower:
                if ".whl" in asset_name_lower:
                    logger.info(f"Detected Python wheel asset: {asset['name']}")
                    best_asset = asset
                    break
                elif ".tar.gz" in asset_name_lower and (
                    "source" in asset_name_lower or "src" in asset_name_lower
                ):
                    logger.info(
                        f"Detected Python source distribution (.tar.gz): {asset['name']}"
                    )
                    if not best_asset:
                        best_asset = asset

    if best_asset:
        return best_asset["browser_download_url"], best_asset["name"]

    logger.debug(
        "No pre-packaged asset found. Checking if this is a known direct-to-PyPI tool."
    )
    known_pypi_tools = ["adrienverge/yamllint"]
    if any(repo.endswith(f"/{tool_name}") for repo in known_pypi_tools):
        logger.info(
            f"Tool `{tool_name}` is a known Python tool. Will install directly from PyPI."
        )
        return "PyPI", f"{tool_name}-from-pypi"

    logger.error("No suitable asset found for your platform.")
    logger.error(f"Available assets: {[a['name'] for a in assets]}")
    return None, None


# --- Download and Extraction ---
def download_file(url, local_path):
    """Downloads a file with a minimalistic progress bar."""
    logger.info(f"Downloading {os.path.basename(local_path)}...")
    try:
        response = requests.get(url, stream=True)
        response.raise_for_status()
        total_size_in_bytes = int(response.headers.get("content-length", 0))
        block_size = 8192
        progress_bar = tqdm(
            total=total_size_in_bytes,
            unit="iB",
            unit_scale=True,
            desc="    Progress",
            file=sys.stderr,
            disable=logger.level > logging.INFO,
        )
        with open(local_path, "wb") as file:
            for data in response.iter_content(block_size):
                progress_bar.update(len(data))
                file.write(data)
        progress_bar.close()
        if total_size_in_bytes != 0 and progress_bar.n != total_size_in_bytes:
            logger.warning("Download size mismatch.")
        return True
    except requests.exceptions.RequestException as e:
        logger.error(f"Error downloading {url}: {e}")
        return False


def extract_archive(archive_path, extract_dir):
    """Extracts a tar.gz or zip archive."""
    logger.info(f"Extracting {os.path.basename(archive_path)} to {extract_dir}...")
    try:
        if archive_path.endswith(".tar.gz"):
            with tarfile.open(archive_path, "r:gz") as tar:
                # Security check for path traversal
                for member in tar.getmembers():
                    member_path = os.path.join(extract_dir, member.name)
                    if not os.path.realpath(member_path).startswith(
                        os.path.realpath(extract_dir)
                    ):
                        logger.warning(
                            f"Skipping potentially malicious path in tar: {member.name}"
                        )
                        continue
                    if member.issym() or member.islnk():
                        logger.warning(f"Skipping symbolic/hard link: {member.name}")
                        continue
                    tar.extract(member, path=extract_dir)
        elif archive_path.endswith(".zip"):
            with zipfile.ZipFile(archive_path, "r") as zip_ref:
                for member in zip_ref.infolist():
                    member_path = os.path.join(extract_dir, member.filename)
                    if not os.path.realpath(member_path).startswith(
                        os.path.realpath(extract_dir)
                    ):
                        logger.warning(
                            f"Skipping potentially malicious path in zip: {member.filename}"
                        )
                        continue
                    if member.is_dir():
                        continue
                    zip_ref.extract(member, path=extract_dir)
        elif archive_path.endswith(".exe") and platform.system() == "Windows":
            logger.info(
                "File is a direct executable (.exe), no archive extraction needed."
            )
            return True
        else:
            logger.error(
                f"Unsupported archive format for direct extraction: {archive_path}"
            )
            return False
        return True
    except (tarfile.TarError, zipfile.BadZipFile) as e:
        logger.error(f"Error extracting archive {archive_path}: {e}")
        return False
    except Exception as e:
        logger.error(f"An unexpected error occurred during extraction: {e}")
        return False


def find_executable_in_extracted(extract_dir, required_tool_name, os_system):
    """
    Finds the main executable within an extracted directory.
    Prioritizes names matching the tool, then names without extensions.
    """
    candidates = []

    target_names = [required_tool_name.lower()]
    if os_system == "windows":
        target_names.append(f"{required_tool_name.lower()}.exe")

    for root, _, files in os.walk(extract_dir):
        for file in files:
            full_path = os.path.join(root, file)
            if system_is_executable(full_path, os_system):
                relative_path = os.path.relpath(full_path, extract_dir)

                score = 0
                file_lower = file.lower()

                if file_lower in target_names:
                    score += 100
                elif os.path.splitext(file_lower)[0] in target_names:
                    score += 90
                elif required_tool_name.lower() in file_lower:
                    score += 50

                score -= relative_path.count(os.sep) * 10

                candidates.append((score, full_path))

    candidates.sort(key=lambda x: x[0], reverse=True)

    if candidates:
        logger.debug(f"Found executable candidate: {candidates[0][1]}")
        return candidates[0][1]

    return None


def system_is_executable(filepath, os_system):
    """Checks if a file is executable on the current system."""
    if os_system == "windows":
        return filepath.lower().endswith(".exe") and os.path.isfile(filepath)
    return os.path.isfile(filepath) and os.access(filepath, os.X_OK)


# --- Python Tool Setup ---
def install_python_tool(
    tool_configs,
    tool_name,
    pypi_package_name,
    version,
    tool_dir,
    download_path,
    asset_name,
):
    """
    Sets up a Python virtual environment and installs the specified Python package.
    `pypi_package_name` is the name used on PyPI (e.g., 'yamllint').
    Returns the path to the shim executable.
    """
    logger.info(f"Setting up Python environment for {tool_name} {version}...")
    venv_path = os.path.join(tool_dir, ".venv")

    try:
        # Create a virtual environment
        subprocess.run(
            [sys.executable, "-m", "venv", venv_path], check=True, capture_output=True
        )
        # Upgrade pip within the venv
        pip_exec = (
            os.path.join(venv_path, "bin", "pip")
            if platform.system() != "Windows"
            else os.path.join(venv_path, "Scripts", "pip.exe")
        )
        subprocess.run(
            [pip_exec, "install", "--upgrade", "pip"], check=True, capture_output=True
        )
        logger.debug(f"Created and upgraded pip in virtual environment at {venv_path}")
    except subprocess.CalledProcessError as e:
        logger.error(
            f"Failed to create virtual environment or upgrade pip: {e.stderr.decode()}"
        )
        return None
    except FileNotFoundError:
        logger.error(
            f"Python interpreter '{sys.executable}' or 'venv' module not found. Is Python installed correctly?"
        )
        return None

    # Determine paths within the venv
    pip_exec = (
        os.path.join(venv_path, "bin", "pip")
        if platform.system() != "Windows"
        else os.path.join(venv_path, "Scripts", "pip.exe")
    )

    # 2. Install the Python package using pip
    install_target = ""
    if download_path and asset_name.endswith(".whl"):
        install_target = download_path
        logger.info(
            f"Installing local wheel {os.path.basename(download_path)} into virtual environment..."
        )
    else:  # Fallback to PyPI install
        install_target = (
            f"{pypi_package_name}=={version.lstrip('v')}"
            if version != "latest"
            else pypi_package_name
        )
        logger.info(
            f"Installing {pypi_package_name} from PyPI into virtual environment..."
        )

    if not install_target:
        logger.error("No valid installation target for Python tool identified.")
        return None

    try:
        install_cmd = [pip_exec, "install", "--force-reinstall", install_target]
        result = subprocess.run(
            install_cmd, check=True, capture_output=True, text=True
        )  # text=True for cleaner output
        logger.debug(f"Pip install stdout:\n{result.stdout}")
        if result.stderr:
            logger.debug(f"Pip install stderr:\n{result.stderr}")
        logger.info(f"Successfully installed {pypi_package_name} {version} via pip.")
    except subprocess.CalledProcessError as e:
        logger.error(f"Failed to install Python package '{install_target}': {e.stderr}")
        return None

    # 3. Create the shim script
    shim_name = tool_name
    shim_path_base = os.path.join(tool_dir, shim_name)

    if platform.system() == "Windows":
        shim_content = f"""@echo off
set "VENV_PATH=%~dp0\\.venv"
set "PATH=%VENV_PATH%\\Scripts;%PATH%"
"%VENV_PATH%\\Scripts\\{tool_name}.exe" %*
if errorlevel 1 exit /b %errorlevel%
"""
        shim_path = shim_path_base + ".bat"
    else:
        shim_content = f"""#!/bin/sh
# This shim activates the virtual environment and runs the tool.
VENV_PATH="$( cd "$( dirname "$0" )" && pwd )/.venv"
. "$VENV_PATH/bin/activate"
exec "$VENV_PATH/bin/{tool_name}" "$@"
"""
        shim_path = shim_path_base

    try:
        with open(shim_path, "w") as f:
            f.write(shim_content)
        if platform.system() != "Windows":
            os.chmod(
                shim_path,
                os.stat(shim_path).st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH,
            )
        logger.info(f"Created shim script at {shim_path}")
    except Exception as e:
        logger.error(f"Failed to create shim script at {shim_path}: {e}")
        return None

    return shim_path


# --- Main Tooler Logic ---
def get_gh_release_info(repo_full_name, version=None):
    """Fetches GitHub release information."""
    if version and version.startswith("v"):
        url = f"https://api.github.com/repos/{repo_full_name}/releases/tags/{version}"
    elif version == "latest":
        url = f"https://api.github.com/repos/{repo_full_name}/releases/latest"
    elif version:
        logger.warning(
            f"Specific version reference '{version}' is not 'latest' and doesn't start with 'v'. "
            "Assuming it's a tag name and attempting to fetch as a release tag."
        )
        url = f"https://api.github.com/repos/{repo_full_name}/releases/tags/{version}"
    else:
        url = f"https://api.github.com/repos/{repo_full_name}/releases/latest"

    logger.debug(f"Fetching GitHub release info from: {url}")
    response = None
    try:
        headers = {"Accept": "application/vnd.github.v3+json"}
        if os.environ.get("GITHUB_TOKEN"):
            headers["Authorization"] = f"token {os.environ['GITHUB_TOKEN']}"
            logger.debug("Using GITHUB_TOKEN for API requests.")

        response = requests.get(url, headers=headers)
        response.raise_for_status()
        return response.json()
    except requests.exceptions.RequestException as e:
        logger.error(
            f"Error fetching GitHub release info for {repo_full_name}@{version or 'latest'}: {e}"
        )
        if response is None:
            logger.error("No response received from GitHub API.")
        elif response.status_code == 404:
            if version and not version.startswith("v"):
                logger.error(
                    f"Tag '{version}' not found. Try including 'v' (e.g., 'v{version}') if it's a version tag, or ensure it's a published release."
                )
            elif version:
                logger.error(
                    f"Tag '{version}' not found. It might be a pre-release or not a formal GitHub release."
                )
        elif response.status_code == 403 and "ratelimit" in str(e).lower():
            logger.error(
                "GitHub API rate limit exceeded. Set GITHUB_TOKEN environment variable for higher limits."
            )
        return None


def install_or_update_tool(
    tool_configs, tool_name, repo_full_name, version="latest", force_update=False
):
    """
    Downloads and prepares a tool. Central logic for binary and Python installations.
    """
    system_os = platform.system()
    system_arch = get_system_arch()

    _tool_binary_name = repo_full_name.split("/")[-1]

    release_info = get_gh_release_info(repo_full_name, version)
    if not release_info:
        logger.error(
            f"Failed to get release info for {repo_full_name}. Cannot install/update."
        )
        return None

    actual_version = release_info.get(
        "tag_name", version if version != "latest" else "unknown"
    )
    if actual_version == "unknown":
        logger.error(
            "Could not determine actual version from GitHub release. Cannot proceed."
        )
        return None

    if version == "latest":
        tool_key = f"{repo_full_name}:v{actual_version}"
    else:
        tool_key = f"{repo_full_name}:{version}"

    current_tool_info = tool_configs["tools"].get(tool_key)

    tool_dir = os.path.join(
        get_tooler_tools_dir(), f"{repo_full_name.replace('/', '__')}", actual_version
    )

    if (
        current_tool_info
        and current_tool_info.get("version") == actual_version
        and not force_update
    ):
        if os.path.exists(
            current_tool_info.get("executable_path")
        ) and system_is_executable(current_tool_info["executable_path"], system_os):
            logger.info(
                f"Tool {tool_name} {actual_version} is already installed and up-to-date."
            )
            tool_configs["tools"][tool_key]["last_accessed"] = (
                datetime.now().isoformat()
            )
            save_tool_configs(tool_configs)
            return current_tool_info["executable_path"]
        else:
            logger.warning(
                f"Tool installation for {tool_name} {actual_version} is corrupted. Re-installing."
            )

    logger.info(f"Installing/Updating {tool_name} {actual_version}...")

    download_url, asset_name = find_asset_for_platform(
        release_info.get("assets", []), _tool_binary_name, system_arch, system_os
    )

    is_pypi_install = download_url == "PyPI"
    is_wheel_install = asset_name and asset_name.endswith(".whl")

    if os.path.exists(tool_dir):
        logger.info(f"Clearing old installation directory: {tool_dir}")
        try:
            shutil.rmtree(tool_dir)
        except OSError as e:
            logger.error(
                f"Error removing old directory {tool_dir}: {e}. Please remove manually if issues persist."
            )
            return None
    os.makedirs(tool_dir, exist_ok=True)

    executable_path = None
    install_type = "binary"

    with tempfile.TemporaryDirectory() as temp_dir:
        temp_download_path: str | None = None
        if is_pypi_install:
            install_type = "python-venv"
        elif download_url:
            if is_wheel_install:
                temp_download_path = os.path.join(temp_dir, asset_name)
            else:
                temp_download_filename = os.path.basename(download_url).split("?")[0]
                temp_download_path = os.path.join(
                    temp_dir,
                    f"temp_{_tool_binary_name}_{actual_version}_{temp_download_filename}",
                )

            if not download_file(download_url, temp_download_path):
                return None

        # --- Installation Logic ---
        if is_pypi_install or is_wheel_install:
            install_type = "python-venv"
            executable_path = install_python_tool(
                tool_configs,
                tool_name,
                tool_name,
                actual_version,
                tool_dir,
                temp_download_path,
                asset_name,
            )
        elif asset_name.endswith(".tar.gz") or asset_name.endswith(".zip"):
            if temp_download_path and not extract_archive(temp_download_path, tool_dir):
                return None
            executable_path = find_executable_in_extracted(
                tool_dir, _tool_binary_name, system_os
            )
        else:  # Direct executable
            executable_filename = (
                f"{_tool_binary_name}.exe"
                if system_os == "Windows"
                else _tool_binary_name
            )
            move_target_path = os.path.join(tool_dir, executable_filename)
            if temp_download_path:  # FIX IS HERE
                try:
                    shutil.move(temp_download_path, move_target_path)
                    executable_path = move_target_path
                except shutil.Error as e:
                    logger.error(f"Error moving downloaded file: {e}")
                    return None
            else:
                logger.error(
                    "Internal Error: No temporary download path available for moving."
                )
                return None

    # --- Post-installation steps ---
    if executable_path:
        if system_os != "Windows" and install_type == "binary":
            try:
                os.chmod(
                    executable_path,
                    os.stat(executable_path).st_mode
                    | stat.S_IXUSR
                    | stat.S_IXGRP
                    | stat.S_IXOTH,
                )
                logger.info(f"Set executable permissions for {executable_path}")
            except OSError as e:
                logger.warning(
                    f"Could not set executable permissions for {executable_path}: {e}"
                )

        tool_configs["tools"][tool_key] = {
            "tool_name": tool_name.lower(),
            "repo": repo_full_name,
            "version": actual_version,
            "executable_path": executable_path,
            "installed_at": datetime.now().isoformat(),
            "last_accessed": datetime.now().isoformat(),
            "install_type": install_type,
        }
        save_tool_configs(tool_configs)

        logger.info(
            f"Successfully installed {tool_name} {actual_version} at {executable_path}"
        )
        return executable_path
    else:
        logger.error(
            f"Installation failed. No executable path was determined for {tool_name} {actual_version}."
        )
        if os.path.exists(tool_dir) and not os.listdir(tool_dir):
            shutil.rmtree(tool_dir)
        return None


def find_tool_executable(tool_configs, tool_query):
    """
    Finds the executable path for a tool based on the query. Returns the tool_info dict on success.
    """
    target_key = None
    repo_full_name_from_query = tool_query

    if ":" in tool_query:
        parts = tool_query.split(":", 1)
        repo_full_name_from_query = parts[0]
        version_req = parts[1]
        if not version_req.startswith("v") and version_req.lower() != "latest":
            version_req = f"v{version_req}"
        target_key = f"{repo_full_name_from_query}:{version_req}"

    found_tool = None
    actual_tool_key_found = None

    if target_key and target_key in tool_configs["tools"]:
        found_tool = tool_configs["tools"][target_key]
        actual_tool_key_found = target_key
        logger.debug(f"Direct match found for key: {target_key}")
    elif ":" not in tool_query:  # Bare repo name passed, find last used
        latest_accessed = None
        for key, tool_info in tool_configs["tools"].items():
            if tool_info["repo"].lower() == repo_full_name_from_query.lower():
                current_accessed_dt = datetime.fromisoformat(
                    tool_info.get("last_accessed", "1970-01-01T00:00:00")
                )
                if (
                    latest_accessed is None
                    or current_accessed_dt
                    > datetime.fromisoformat(
                        latest_accessed.get("last_accessed", "1970-01-01T00:00:00")
                    )
                ):
                    latest_accessed = tool_info
                    actual_tool_key_found = key
        if latest_accessed:
            found_tool = latest_accessed
            logger.info(
                f"No specific version requested. Using pinned version {found_tool['version']} for {tool_query} (last accessed)."
            )

    if found_tool:
        if os.path.exists(found_tool["executable_path"]) and system_is_executable(
            found_tool["executable_path"], platform.system()
        ):
            if actual_tool_key_found:
                tool_configs["tools"][actual_tool_key_found]["last_accessed"] = (
                    datetime.now().isoformat()
                )
                save_tool_configs(tool_configs)
            return found_tool
        else:
            logger.warning(
                f"Pinned executable for {found_tool['repo']}@{found_tool['version']} is missing or not executable at {found_tool['executable_path']}. Will attempt to re-install."
            )
            if actual_tool_key_found and actual_tool_key_found in tool_configs["tools"]:
                del tool_configs["tools"][actual_tool_key_found]
                save_tool_configs(tool_configs)
            return None

    return None


def check_for_updates(tool_configs):
    """Checks for updates for previously installed tools that haven't been updated recently."""
    update_check_days = tool_configs["settings"].get(
        "update_check_days", DEFAULT_UPDATE_CHECK_DAYS
    )
    if update_check_days <= 0:
        logger.info("Update checks are disabled via configuration.")
        return

    logger.info(
        f"Checking for updates for installed tools (last checked > {update_check_days} days ago)..."
    )

    now = datetime.now()
    needs_update_info = []

    for key, tool_info in list(tool_configs["tools"].items()):
        is_fixed_version = (
            tool_info["version"].lower() != "latest"
            and tool_info["version"].lower() != "unknown"
        )

        if is_fixed_version:
            logger.debug(f"Skipping update check for fixed version: {key}")
            continue

        last_accessed_dt = datetime.fromisoformat(
            tool_info.get("last_accessed")
            or tool_info.get("installed_at", now.isoformat())
        )

        if last_accessed_dt + timedelta(days=update_check_days) < now:
            repo_full_name = tool_info["repo"]
            logger.debug(
                f"Fetching latest release info for {repo_full_name} for update check..."
            )
            latest_release_info = get_gh_release_info(repo_full_name, "latest")
            if latest_release_info:
                latest_tag = latest_release_info.get("tag_name")
                if latest_tag and latest_tag != tool_info["version"]:
                    needs_update_info.append(
                        f"Tool {tool_info['tool_name']} ({repo_full_name}) has an update: {tool_info['version']} -> {latest_tag}"
                    )
            tool_configs["tools"][key]["last_accessed"] = now.isoformat()
            save_tool_configs(tool_configs)

    if needs_update_info:
        print("\n--- Tool Updates Available ---", file=sys.stderr)
        for msg in needs_update_info:
            print(msg, file=sys.stderr)
        print(
            "To update, run `tooler update [repo/tool]` or `tooler update --all`.",
            file=sys.stderr,
        )
        print("----------------------------\n", file=sys.stderr)
    else:
        logger.info(
            "No updates found or all checks are still pending for their next cycle."
        )


def list_installed_tools(tool_configs):
    """Lists all installed tools."""
    print("--- Installed Tooler Tools ---")
    if not tool_configs["tools"]:
        print("  No tools installed yet.")
        return

    sorted_tools = sorted(
        tool_configs["tools"].items(), key=lambda item: item[1]["repo"].lower()
    )

    for key, tool_info in sorted_tools:
        install_type_display = (
            f" ({tool_info.get('install_type', 'binary')})"
            if tool_info.get("install_type")
            else ""
        )
        print(f"  - {tool_info['repo']}{install_type_display}")
        print(f"    Version: {tool_info['version']}")
        print(f"    Path:    {tool_info['executable_path']}")
        print(f"    Installed: {tool_info.get('installed_at', 'N/A')}")
        print(f"    Last Run:  {tool_info.get('last_accessed', 'N/A')}")
        print("")
    print("------------------------------")


def remove_tool(tool_configs, tool_query):
    """Removes an installed tool."""

    tools_to_remove_keys = []
    removal_dirs = set()

    found_any = False
    for key, info in tool_configs["tools"].items():
        if key.lower() == tool_query.lower():
            tools_to_remove_keys.append(key)
            if info.get("install_type") == "python-venv":
                removal_dirs.add(os.path.dirname(info["executable_path"]))
            else:
                removal_dirs.add(
                    os.path.dirname(os.path.dirname(info["executable_path"]))
                )
            found_any = True
            break
        elif (
            info["repo"].lower() == tool_query.split(":")[0].lower()
            and ":" not in tool_query
        ):
            tools_to_remove_keys.append(key)
            if info.get("install_type") == "python-venv":
                removal_dirs.add(os.path.dirname(info["executable_path"]))
            else:
                removal_dirs.add(
                    os.path.dirname(os.path.dirname(info["executable_path"]))
                )
            found_any = True
        elif (
            info["tool_name"].lower() == tool_query.lower()
            and ":" not in tool_query
            and "/" not in tool_query
        ):
            tools_to_remove_keys.append(key)
            if info.get("install_type") == "python-venv":
                removal_dirs.add(os.path.dirname(info["executable_path"]))
            else:
                removal_dirs.add(
                    os.path.dirname(os.path.dirname(info["executable_path"]))
                )
            found_any = True

    if not found_any:
        logger.error(f"Tool '{tool_query}' not found in installed list.")
        return False

    if ":" not in tool_query and len(tools_to_remove_keys) > 1:
        logger.info(
            f"Removing {len(tools_to_remove_keys)} installations for '{tool_query}' across different versions/repos."
        )

    success = True
    for path in removal_dirs:
        if os.path.exists(path):
            logger.info(f"Removing directory: {path}")
            try:
                shutil.rmtree(path)
            except OSError as e:
                logger.error(f"Error removing directory {path}: {e}")
                success = False
        else:
            logger.info(f"Directory not found, already removed?: {path}")

    if success:
        for key_to_del in tools_to_remove_keys:
            if key_to_del in tool_configs["tools"]:
                del tool_configs["tools"][key_to_del]
        save_tool_configs(tool_configs)
        logger.info(f"Tool(s) for '{tool_query}' removed successfully.")
        return True
    else:
        logger.error(f"Failed to fully remove tool(s) for '{tool_query}'.")
        return False


# --- Main execution function ---
def main():
    parser = argparse.ArgumentParser(
        description="Tooler: Download and manage CLI tools from GitHub releases.",
        formatter_class=argparse.RawTextHelpFormatter,
        epilog="""
Examples:
  Run a tool:
    tooler run nektos/act                   # Runs the latest pinned 'act' tool
    tooler run nektos/act:v0.2.79           # Runs specific 'act' version v0.2.79
    tooler run nektos/act -- install        # Pass arguments to the tool

  Python Example:
    tooler run adrienverge/yamllint         # Installs yamllint in a managed Python env

  Manage tools:
    tooler list                             # List all installed tools
    tooler update nektos/act                # Checks for and installs latest nektos/act
    tooler update --all                     # Checks for and installs latest for all non-pinned tools
    tooler remove nektos/act                # Removes all versions of nektos/act
    tooler remove nektos/act:v0.2.79        # Removes specific version of nektos/act

  Configuration:
    tooler config set update_check_days=30  # Set update check interval to 30 days
    tooler config get                       # Show all settings

Verbosity levels (default: WARNING):
    Less verbose: `tooler -q ...` (ERROR only)
    More verbose: `tooler -v ...` (INFO and above)
    Debug:        `tooler -vv ...` (DEBUG and above)
""",
    )

    parser.add_argument(
        "-v",
        "--verbose",
        action="count",
        default=0,
        help="Increase verbosity (can be used multiple times, e.g., -vv for debug)",
    )
    parser.add_argument(
        "-q",
        "--quiet",
        action="store_true",
        help="Suppress output, show only errors.",
    )

    subparsers = parser.add_subparsers(
        dest="command", help="Available commands", required=True
    )

    run_parser = subparsers.add_parser(
        "run",
        help="Run a GitHub CLI tool.",
    )
    run_parser.add_argument(
        "tool_id",
        help="GitHub repository (e.g., 'nektos/act') or full ID (e.g., 'nektos/act:v0.2.79').",
    )
    run_parser.add_argument(
        "tool_args", nargs=argparse.REMAINDER, help="Arguments to pass to the tool."
    )

    list_parser = subparsers.add_parser("list", help="List all installed tools.")

    update_parser = subparsers.add_parser(
        "update", help="Update one or all installed tools to their latest version."
    )
    update_group = update_parser.add_mutually_exclusive_group(required=True)
    update_group.add_argument(
        "tool_id",
        nargs="?",
        help="Specific tool to update (e.g., 'nektos/act'). Cannot be combined with --all.",
    )
    update_group.add_argument(
        "--all",
        action="store_true",
        help="Update all applicable tools to their latest versions.",
    )

    remove_parser = subparsers.add_parser(
        "remove", help="Remove an installed tool and its data."
    )
    remove_parser.add_argument(
        "tool_id", help="Tool to remove (e.g., 'nektos/act' or 'nektos/act:v0.2.79')."
    )

    config_parser = subparsers.add_parser(
        "config", help="Manage tooler's configuration settings."
    )
    config_subparsers = config_parser.add_subparsers(
        dest="config_command", help="Config commands", required=True
    )

    config_get_parser = config_subparsers.add_parser(
        "get", help="Get a configuration setting."
    )
    config_get_parser.add_argument(
        "key",
        nargs="?",
        help="Key to get (e.g., 'update_check_days'). If omitted, shows all settings.",
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

    for handler in logger.handlers[:]:
        logger.removeHandler(handler)
    logger.addHandler(ch)

    tool_configs = load_tool_configs()

    if args.command in ["run", "update"]:
        check_for_updates(tool_configs)

    if args.command == "list":
        list_installed_tools(tool_configs)
        sys.exit(0)
    elif args.command == "remove":
        remove_tool(tool_configs, args.tool_id)
        sys.exit(0)
    elif args.command == "update":
        if args.all:
            logger.info("Updating all applicable tools...")
            updated_count = 0
            for key, tool_info in list(tool_configs["tools"].items()):
                is_fixed_version = (
                    tool_info["version"].lower() != "latest"
                    and tool_info["version"].lower() != "unknown"
                )

                if is_fixed_version:
                    logger.info(
                        f"Tool {tool_info['repo']} ({tool_info['version']}) is a fixed version, skipping update."
                    )
                    continue

                logger.info(f"Attempting to update {tool_info['repo']}...")
                install_or_update_tool(
                    tool_configs,
                    tool_info["tool_name"],
                    tool_info["repo"],
                    version="latest",
                    force_update=True,
                )
                updated_count += 1
            logger.info(
                f"Update process finished. {updated_count} tool(s) were checked/updated."
            )
            sys.exit(0)
        elif args.tool_id:
            repo_full_name = args.tool_id.split(":")[0]
            tool_name = repo_full_name.split("/")[-1]
            logger.info(f"Attempting to update {args.tool_id}...")
            executable_path = install_or_update_tool(
                tool_configs,
                tool_name,
                repo_full_name,
                version="latest",
                force_update=True,
            )
            if executable_path:
                logger.info(f"{args.tool_id} updated successfully.")
                sys.exit(0)
            else:
                logger.error(f"Failed to update {args.tool_id}.")
                sys.exit(1)
    elif args.command == "config":
        if args.config_command == "get":
            if args.key:
                val = tool_configs["settings"].get(args.key)
                if val is not None:
                    print(val)
                else:
                    logger.error(f"Setting '{args.key}' not found.")
                    sys.exit(1)
            else:
                print("--- Tooler Settings ---")
                for key, value in tool_configs["settings"].items():
                    print(f"  {key}: {value}")
                print("-----------------------")
            sys.exit(0)
        elif args.config_command == "set":
            if "=" in args.key_value:
                key, value_str = args.key_value.split("=", 1)
                try:
                    if value_str.isdigit():
                        value = int(value_str)
                    elif value_str.lower() in ("true", "false"):
                        value = value_str.lower() == "true"
                    else:
                        value = value_str

                    tool_configs["settings"][key] = value
                    save_tool_configs(tool_configs)
                    logger.info(f"Setting '{key}' updated to '{value}'.")
                    sys.exit(0)
                except ValueError:
                    logger.error(
                        f"Could not parse value for '{key}'. Please provide integer, boolean, or string."
                    )
                    sys.exit(1)
            else:
                logger.error("Invalid format. Use 'key=value'.")
                sys.exit(1)
    elif args.command == "run":
        repo_full_name = args.tool_id.split(":")[0]
        tool_version_req = (
            args.tool_id.split(":", 1)[1] if ":" in args.tool_id else "latest"
        )
        tool_name = repo_full_name.split("/")[-1]

        tool_info_found = find_tool_executable(tool_configs, args.tool_id)

        if not tool_info_found:
            logger.info(
                f"Tool {args.tool_id} not found locally or is corrupted. Attempting to install/update..."
            )
            install_or_update_tool(
                tool_configs, tool_name, repo_full_name, version=tool_version_req
            )
            # Re-fetch the info AFTER installation to get the correct path and type.
            tool_info_found = find_tool_executable(load_tool_configs(), args.tool_id)

        if tool_info_found:
            executable_path = tool_info_found["executable_path"]
            cmd = [executable_path] + args.tool_args

            logger.debug(f"Executing: {cmd}")
            try:
                # Use subprocess.run for shims/batch files for portability
                if (
                    tool_info_found.get("install_type") == "python-venv"
                    or platform.system() == "Windows"
                ):
                    result = subprocess.run(cmd, check=False)
                    sys.exit(result.returncode)
                else:  # Use os.execv for direct native binaries on Unix-like systems for efficiency
                    os.execv(cmd[0], cmd)
            except FileNotFoundError:
                logger.error(
                    f"Executable not found at {executable_path}. It might have been moved or deleted."
                )
                sys.exit(127)
            except OSError as e:
                logger.error(f"Error executing tool: {e}")
                sys.exit(1)
        else:
            logger.error(
                f"Failed to get executable for {args.tool_id}. See errors above."
            )
            sys.exit(1)


if __name__ == "__main__":
    main()
