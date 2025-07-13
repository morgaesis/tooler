#!/bin/env python3

import os
import sys
import json
from typing import Any, Dict, Union
import requests
import shutil
import tarfile
import zipfile
import platform
import stat
from datetime import datetime, timedelta
import argparse
from tqdm import tqdm  # Minimalistic loading bar

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
            # Ensure 'tools' and 'settings' keys exist, initialize if not
            if "tools" not in config:
                config["tools"] = {}
            if "settings" not in config:
                config["settings"] = {"update_check_days": DEFAULT_UPDATE_CHECK_DAYS}
            elif "update_check_days" not in config["settings"]:
                config["settings"]["update_check_days"] = DEFAULT_UPDATE_CHECK_DAYS

            return config
    except json.JSONDecodeError:
        print(
            f"Error: Could not parse config file at {config_path}. It might be corrupted. Starting with an empty config.",
            file=sys.stderr,
        )
        return {
            "tools": {},
            "settings": {"update_check_days": DEFAULT_UPDATE_CHECK_DAYS},
        }


def save_tool_configs(configs):
    """Saves tool configurations to the JSON file."""
    config_path = get_tooler_config_file_path()
    os.makedirs(os.path.dirname(config_path), exist_ok=True)  # Ensure config dir exists
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
            return "386"  # Or sometimes "x86" is used

    # Fallback for unknown/uncommon architectures
    return machine


def map_arch_to_github_release(arch, system):
    """Maps internal arch/system to common GitHub release file naming conventions."""
    system = system.lower()

    # Common mappings
    if system == "linux":
        if arch == "amd64":
            return "linux_amd64"
        if arch == "arm64":
            return "linux_arm64"
        if arch == "arm":
            return "linux_arm"  # Less common for new tools, but good to have
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
            return "windows_386"  # Unlikely for act, but for generic

    # Common alternate patterns if direct match fails
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

    # Get common naming conventions for our platform
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

    # Add common general patterns
    if system_os_lower == "windows":
        possible_arch_patterns.append("windows")  # For simple .exe files
    elif system_os_lower == "darwin":
        possible_arch_patterns.append("macos")

    # Remove duplicates and ensure longer patterns are checked first
    possible_arch_patterns = sorted(
        list(set(possible_arch_patterns)), key=len, reverse=True
    )

    print(
        f"  Info: Searching for assets matching {system_os_lower} and {system_arch}",
        file=sys.stderr,
    )
    print(f"  Info: Potential patterns: {possible_arch_patterns}", file=sys.stderr)

    best_asset = None

    # Prioritize archives
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
                        print(
                            f"  Found potential archive asset: {asset['name']}",
                            file=sys.stderr,
                        )
                        break  # Found a good one, move to next pattern (or break outerloop if it's the best)
        if best_asset and (
            ".tar.gz" in best_asset["name"].lower()
            or ".zip" in best_asset["name"].lower()
        ):
            break  # Found a highly preferred archive, let's stick with it

    # Then prioritize uncompressed binaries if no suitable archive found
    if best_asset is None:
        for pattern in possible_arch_patterns:
            for asset in assets:
                asset_name_lower = asset["name"].lower()
                # Exclude archives as we already checked those
                if (
                    pattern in asset_name_lower
                    and ".tar.gz" not in asset_name_lower
                    and ".zip" not in asset_name_lower
                ):
                    # Check for direct executable names like 'act' or 'act.exe'
                    if (
                        tool_name.lower() in os.path.splitext(asset_name_lower)[0]
                    ):  # e.g. 'act' in 'act_linux_amd64'
                        if system_os_lower == "windows" and asset_name_lower.endswith(
                            ".exe"
                        ):
                            best_asset = asset
                            print(
                                f"  Found potential executable asset: {asset['name']}",
                                file=sys.stderr,
                            )
                            break
                        elif (
                            system_os_lower != "windows"
                            and not asset_name_lower.endswith(".exe")
                        ):  # Prefer non-.exe for non-Windows
                            best_asset = asset
                            print(
                                f"  Found potential executable asset: {asset['name']}",
                                file=sys.stderr,
                            )
                            break
            if best_asset:
                break  # Found a good one, break out

    if best_asset:
        return best_asset["browser_download_url"], best_asset["name"]

    print("  Error: No suitable asset found for your platform.", file=sys.stderr)
    print(f"  Available assets: {[a['name'] for a in assets]}", file=sys.stderr)
    return None, None


# --- Download and Extraction ---
def download_file(url, local_path):
    """Downloads a file with a minimalistic progress bar."""
    print(f"  Downloading {os.path.basename(local_path)}...", file=sys.stderr)
    try:
        response = requests.get(url, stream=True)
        response.raise_for_status()
        total_size_in_bytes = int(response.headers.get("content-length", 0))
        block_size = 8192  # 8 Kibibytes
        progress_bar = tqdm(
            total=total_size_in_bytes,
            unit="iB",
            unit_scale=True,
            desc="    Progress",
            file=sys.stderr,
        )
        with open(local_path, "wb") as file:
            for data in response.iter_content(block_size):
                progress_bar.update(len(data))
                file.write(data)
        progress_bar.close()
        if total_size_in_bytes != 0 and progress_bar.n != total_size_in_bytes:
            print("  Warning: Download size mismatch.", file=sys.stderr)
        return True
    except requests.exceptions.RequestException as e:
        print(f"  Error downloading {url}: {e}", file=sys.stderr)
        return False


def extract_archive(archive_path, extract_dir):
    """Extracts a tar.gz or zip archive."""
    print(
        f"  Extracting {os.path.basename(archive_path)} to {extract_dir}...",
        file=sys.stderr,
    )
    try:
        if archive_path.endswith(".tar.gz"):
            with tarfile.open(archive_path, "r:gz") as tar:
                # Security check: prevent path traversal
                for member in tar.getmembers():
                    if (
                        member.issym() or member.islnk()
                    ):  # Skip symbolic/hard links for safety
                        continue
                    if os.path.isabs(member.name) or ".." in member.name:
                        print(
                            f"    Warning: Skipping potentially malicious path in tar: {member.name}",
                            file=sys.stderr,
                        )
                        continue
                    tar.extract(member, path=extract_dir)
        elif archive_path.endswith(".zip"):
            with zipfile.ZipFile(archive_path, "r") as zip_ref:
                # Security check: prevent path traversal
                for member in zip_ref.infolist():
                    if member.is_dir():
                        continue  # Skip directories
                    if os.path.isabs(member.filename) or ".." in member.filename:
                        print(
                            f"    Warning: Skipping potentially malicious path in zip: {member.filename}",
                            file=sys.stderr,
                        )
                        continue
                    zip_ref.extract(member, path=extract_dir)
        elif archive_path.endswith(".exe") and platform.system() == "Windows":
            # This is a direct executable, no extraction needed.
            # We'll just move/rename it later.
            print(
                "  File is a direct executable (.exe), no archive extraction needed.",
                file=sys.stderr,
            )
            return True
        else:
            print(
                f"  Error: Unsupported archive format: {archive_path}", file=sys.stderr
            )
            return False
        return True
    except (tarfile.TarError, zipfile.BadZipFile) as e:
        print(f"  Error extracting archive {archive_path}: {e}", file=sys.stderr)
        return False
    except Exception as e:
        print(f"  An unexpected error occurred during extraction: {e}", file=sys.stderr)
        return False


def find_executable_in_extracted(extract_dir, required_tool_name, os_system):
    """
    Finds the main executable within an extracted directory.
    Prioritizes names matching the tool, then names without extensions.
    """
    candidates = []

    # Prefer exact match (case-insensitive) or common variants
    target_names = [required_tool_name.lower()]
    if os_system == "windows":
        target_names.append(f"{required_tool_name.lower()}.exe")

    for root, _, files in os.walk(extract_dir):
        for file in files:
            full_path = os.path.join(root, file)
            # Check if it's executable (on Unix-like systems) or has .exe on Windows
            if system_is_executable(full_path, os_system):
                relative_path = os.path.relpath(full_path, extract_dir)

                # Assign a score: Higher is better
                score = 0
                file_lower = file.lower()

                if file_lower in target_names:
                    score += 100  # Exact name match
                elif os.path.splitext(file_lower)[0] in target_names:
                    score += 90  # Name match ignoring extension
                elif required_tool_name.lower() in file_lower:
                    score += 50  # Contains tool name

                # Prioritize files closer to the root of the extraction
                score -= relative_path.count(os.sep) * 10  # Penalize for being deeper

                candidates.append((score, full_path))

    candidates.sort(key=lambda x: x[0], reverse=True)  # Sort by score, descending

    if candidates:
        print(f"  Found executable candidate: {candidates[0][1]}", file=sys.stderr)
        return candidates[0][1]

    return None


def system_is_executable(filepath, os_system):
    """Checks if a file is executable on the current system."""
    if os_system == "windows":
        return filepath.lower().endswith(".exe")
    return os.path.isfile(filepath) and os.access(filepath, os.X_OK)


# --- Main Tooler Logic ---
def get_gh_release_info(repo_full_name, version=None):
    """Fetches GitHub release information."""
    if version and version.startswith("v"):  # Try direct tag lookup first
        url = f"https://api.github.com/repos/{repo_full_name}/releases/tags/{version}"
    elif version == "latest":
        url = f"https://api.github.com/repos/{repo_full_name}/releases/latest"
    elif version:  # Assume it's a specific tag not starting with 'v' or a commit hash
        url = f"https://api.github.com/repos/{repo_full_name}/commits/{version}"  # This is for a commit hash, not a release.
        print(
            f"  Warning: Specific commit hash or non-'v' versioning is not directly supported for releases via GitHub API. Trying releases/tags/{version}",
            file=sys.stderr,
        )
        url = f"https://api.github.com/repos/{repo_full_name}/releases/tags/{version}"
    else:  # Default to latest if no version specified
        url = f"https://api.github.com/repos/{repo_full_name}/releases/latest"

    print(f"  Fetching GitHub release info from: {url}", file=sys.stderr)
    response: Union[requests.Response, None] = None
    try:
        headers = {"Accept": "application/vnd.github.v3+json"}
        # Check for GitHub token in environment for higher rate limits
        if os.environ.get("GITHUB_TOKEN"):
            headers["Authorization"] = f"token {os.environ['GITHUB_TOKEN']}"
            print("  (Using GITHUB_TOKEN for API requests)", file=sys.stderr)

        response = requests.get(url, headers=headers)
        response.raise_for_status()  # Raise an HTTPError for bad responses (4xx or 5xx)
        return response.json()
    except requests.exceptions.RequestException as e:
        print(
            f"  Error fetching GitHub release info for {repo_full_name}@{version or 'latest'}: {e}",
            file=sys.stderr,
        )
        if response is None:
            print(
                f"  (Hint: Try including 'v' (e.g., 'v{version}') if it's a version tag.)",
            )
        elif response.status_code == 404:
            if version and not version.startswith("v"):
                print(
                    f"  Hint: Tag '{version}' not found. Try including 'v' (e.g., 'v{version}') if it's a version tag.",
                    file=sys.stderr,
                )
            elif version:
                print(
                    f"  Hint: Tag '{version}' not found. It might be a pre-release or not a formal GitHub release.",
                    file=sys.stderr,
                )
        elif response.status_code == 403 and "ratelimit" in str(e).lower():
            print(
                "  Rate limit exceeded. Set GITHUB_TOKEN environment variable for higher limits.",
                file=sys.stderr,
            )

        return None


def install_or_update_tool(
    tool_configs, tool_name, repo_full_name, version="latest", force_update=False
):
    """
    Downloads and prepares a tool.
    Returns the path to the executable or None on failure.
    """
    system_os = platform.system()
    system_arch = get_system_arch()

    # Derive tool binary name from repository name
    _tool_binary_name = repo_full_name.split("/")[-1]  # e.g., 'act' from 'nektos/act'

    # Check if a specific version is requested, otherwise use the pinned/latest
    requested_version_string = f":v{version}" if version and version != "latest" else ""
    tool_key = f"{repo_full_name}{requested_version_string}"

    current_tool_info = tool_configs["tools"].get(tool_key)

    release_info = get_gh_release_info(repo_full_name, version)
    if not release_info:
        print(
            f"  Failed to get release info for {tool_key}. Cannot install/update.",
            file=sys.stderr,
        )
        return None

    # For 'latest', get the actual tag_name
    actual_version: str = release_info.get("tag_name", "unknown")
    if version == "latest":
        actual_version = actual_version.lstrip("v")  # Remove any 'v' prefix
        tool_key = (
            f"{repo_full_name}:v{actual_version}"  # Pin "latest" to exact version
        )

    # Re-check current tool info with the definitive key
    current_tool_info = tool_configs["tools"].get(tool_key)

    # Determine unique directory for this tool and version
    tool_dir = os.path.join(
        get_tooler_tools_dir(), f"{repo_full_name.replace('/', '__')}", actual_version
    )
    expected_executable_path = "NOT_YET_DETERMINED"  # Will be set after extraction

    # Check for existing installation
    if (
        current_tool_info
        and current_tool_info.get("version") == actual_version
        and not force_update
    ):
        if os.path.exists(current_tool_info.get("executable_path")):
            # Validate if executable is still there
            if system_is_executable(current_tool_info["executable_path"], system_os):
                print(
                    f"  Tool {tool_name} {actual_version} already installed and up-to-date at {current_tool_info['executable_path']}",
                    file=sys.stderr,
                )
                # Update last accessed for pinning purposes
                tool_configs["tools"][tool_key]["last_accessed"] = (
                    datetime.now().isoformat()
                )
                save_tool_configs(tool_configs)
                return current_tool_info["executable_path"]
            else:
                print(
                    f"  Warning: Executable for {tool_name} {actual_version} missing or not executable at {current_tool_info['executable_path']}. Re-installing.",
                    file=sys.stderr,
                )
                # Fall through to re-installation
        else:
            print(
                f"  Warning: Installation directory for {tool_name} {actual_version} missing. Re-installing.",
                file=sys.stderr,
            )
            # Fall through to re-installation

    # Proceed with installation/update
    print(f"  Installing/Updating {tool_name} {actual_version}...", file=sys.stderr)

    download_url, asset_name = find_asset_for_platform(
        release_info.get("assets", []), _tool_binary_name, system_arch, system_os
    )

    if not download_url:
        print(
            f"  Error: No suitable release asset found for {system_os} {system_arch} for {tool_key}.",
            file=sys.stderr,
        )
        return None

    temp_download_path = os.path.join(
        get_tooler_tools_dir(),
        f"temp_{_tool_binary_name}_{actual_version}_{os.path.basename(download_url)}",
    )

    if not download_file(download_url, temp_download_path):
        return None

    # Clear previous version's directory if it exists
    if os.path.exists(tool_dir):
        print(f"  Clearing old installation directory: {tool_dir}", file=sys.stderr)
        try:
            shutil.rmtree(tool_dir)
        except OSError as e:
            print(
                f"  Error removing old directory {tool_dir}: {e}. Please remove manually if issues persist.",
                file=sys.stderr,
            )
            return None

    os.makedirs(tool_dir, exist_ok=True)

    if asset_name.endswith(".tar.gz") or asset_name.endswith(".zip"):
        if not extract_archive(temp_download_path, tool_dir):
            if os.path.exists(tool_dir):
                shutil.rmtree(tool_dir)  # Cleanup broken install
            os.remove(temp_download_path)
            return None

        # Find the executable inside the extracted directory
        expected_executable_path = find_executable_in_extracted(
            tool_dir, _tool_binary_name, system_os
        )
        if not expected_executable_path:
            print(
                f"  Error: Could not find executable in {tool_dir} after extraction for {tool_key}.",
                file=sys.stderr,
            )
            if os.path.exists(tool_dir):
                shutil.rmtree(tool_dir)
            os.remove(temp_download_path)
            return None
    else:  # Assume it's a direct executable asset
        executable_filename = (
            f"{_tool_binary_name}.exe" if system_os == "Windows" else _tool_binary_name
        )
        expected_executable_path = os.path.join(tool_dir, executable_filename)
        try:
            shutil.move(temp_download_path, expected_executable_path)
        except shutil.Error as e:
            print(f"  Error moving downloaded file: {e}", file=sys.stderr)
            if os.path.exists(tool_dir):
                shutil.rmtree(tool_dir)
            os.remove(temp_download_path)
            return None

    # Ensure executable permissions on Unix-like systems
    if system_os != "Windows":
        try:
            current_mode = os.stat(expected_executable_path).st_mode
            os.chmod(
                expected_executable_path,
                current_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH,
            )
            print(
                f"  Set executable permissions for {expected_executable_path}",
                file=sys.stderr,
            )
        except OSError as e:
            print(
                f"  Warning: Could not set executable permissions for {expected_executable_path}: {e}",
                file=sys.stderr,
            )

    # Clean up temp archive
    if os.path.exists(temp_download_path):
        os.remove(temp_download_path)

    # Update configurations
    tool_configs["tools"][tool_key] = {
        "tool_name": tool_name.lower(),  # Store canonical common name
        "repo": repo_full_name,
        "version": actual_version,
        "executable_path": expected_executable_path,
        "installed_at": datetime.now().isoformat(),
        "last_accessed": datetime.now().isoformat(),
    }
    save_tool_configs(tool_configs)

    print(
        f"  Successfully installed {tool_name} {actual_version} at {expected_executable_path}",
        file=sys.stderr,
    )
    return expected_executable_path


def find_tool_executable(tool_configs, tool_query):
    """
    Finds the executable path for a tool based on the query.
    Query can be 'tool_name' or 'repo_full_name' or 'repo_full_name:vX.Y.Z'.
    """
    tool_name_part = tool_query.split("/")[
        0
    ].lower()  # e.g., 'nektos' or 'act' if just 'act'
    if ":" in tool_query:
        repo_full_name, version = tool_query.split(":", 1)
        # Standardize 'v' prefix
        if not version.startswith("v") and version != "latest":
            version = f"v{version}"
        target_key = f"{repo_full_name}:{version}"
    else:
        repo_full_name = tool_query  # Assume query is repo_full_name
        target_key = None  # Will find the latest pinned for this repo

    found_tool = None
    if target_key:  # Specific version requested
        if target_key in tool_configs["tools"]:
            found_tool = tool_configs["tools"][target_key]
        else:
            # If not found directly, assume it's for 'latest' and try to find the last-run 'latest' for this repo
            # No, if specific version is requested, we need to try and get that specific version (or fail)
            pass

    if not found_tool:  # If no specific version requested, or specific version not found, find the latest-accessed for the repo
        # Look for the last-accessed version for this repo
        latest_accessed = None
        for key, tool_info in tool_configs["tools"].items():
            if tool_info["repo"].lower() == repo_full_name.lower():
                if (
                    latest_accessed is None
                    or tool_info["last_accessed"] > latest_accessed["last_accessed"]
                ):
                    latest_accessed = tool_info
        if latest_accessed:
            found_tool = latest_accessed
            print(
                f"  Using pinned version {found_tool['version']} for {tool_query} (last accessed).",
                file=sys.stderr,
            )

    if found_tool:
        # Check if the executable still exists
        if os.path.exists(found_tool["executable_path"]):
            return found_tool
        else:
            print(
                f"  Warning: Pinned executable for {found_tool['repo']}@{found_tool['version']} is missing from {found_tool['executable_path']}. Will attempt to re-install.",
                file=sys.stderr,
            )
            # Remove this broken entry
            del tool_configs["tools"][f"{found_tool['repo']}:v{found_tool['version']}"]
            save_tool_configs(tool_configs)
            return None  # Indicate need for fresh install

    return None


def check_for_updates(tool_configs):
    """Checks for updates for previously installed tools that haven't been updated recently."""
    update_check_days = tool_configs["settings"]["update_check_days"]
    if update_check_days <= 0:
        print("  Update checks are disabled.", file=sys.stderr)
        return

    print(
        f"  Checking for updates for installed tools (last checked > {update_check_days} days ago)...",
        file=sys.stderr,
    )

    now = datetime.now()
    needs_update_info = []

    for key, tool_info in list(
        tool_configs["tools"].items()
    ):  # Iterate over a copy to allow modification
        last_accessed_dt = datetime.fromisoformat(
            tool_info.get("last_accessed")
            or tool_info.get("installed_at", now.isoformat())
        )

        # Only check tools that are not specifically version-pinned (e.g., nektos/act:v0.2.79)
        if last_accessed_dt + timedelta(days=update_check_days) < now:
            if (
                ":" not in key
            ):  # Not a specific version like tool:v1.2.3, implies it's the "latest" for that repo
                repo_full_name = tool_info["repo"]
                print(
                    f"  Checking latest version for {repo_full_name}...",
                    file=sys.stderr,
                )
                latest_release_info = get_gh_release_info(repo_full_name, "latest")
                if latest_release_info:
                    latest_tag = latest_release_info.get("tag_name")
                    if latest_tag and latest_tag != tool_info["version"]:
                        needs_update_info.append(
                            f"  Tool {tool_info['tool_name']} ({repo_full_name}) has an update: {tool_info['version']} -> {latest_tag}"
                        )
                        # Optionally: auto-update here if configured globally
                        # For now, just notify. Auto-update should be a conscious choice.
                # Update last accessed to prevent re-checking too soon, even if no update found
                tool_configs["tools"][key]["last_accessed"] = now.isoformat()
                save_tool_configs(
                    tool_configs
                )  # Save after each check to persist 'last_accessed'

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
        print("  No updates found or check not due.", file=sys.stderr)


def list_installed_tools(tool_configs):
    """Lists all installed tools."""
    print("--- Installed Tooler Tools ---", file=sys.stderr)
    if not tool_configs["tools"]:
        print("  No tools installed yet.", file=sys.stderr)
        return

    for key, tool_info in tool_configs["tools"].items():
        print(f"  - {tool_info['repo']}", file=sys.stderr)
        print(f"    Version: {tool_info['version']}", file=sys.stderr)
        print(f"    Path:    {tool_info['executable_path']}", file=sys.stderr)
        print(f"    Installed: {tool_info.get('installed_at', 'N/A')}", file=sys.stderr)
        print(
            f"    Last Run:  {tool_info.get('last_accessed', 'N/A')}", file=sys.stderr
        )
        print("", file=sys.stderr)
    print("------------------------------", file=sys.stderr)


def remove_tool(tool_configs, tool_query):
    """Removes an installed tool."""

    tool_key_to_remove = None
    removal_paths = []

    # Try to find an exact match first based on the query (repo or repo:v)
    for key, info in tool_configs["tools"].items():
        if key.lower() == tool_query.lower():
            tool_key_to_remove = key
            removal_paths.append(
                os.path.dirname(os.path.dirname(info["executable_path"]))
            )  # Root tool directory for this version
            break
        elif (
            info["repo"].lower() == tool_query.lower() and ":" not in tool_query
        ):  # User gave only repo name, remove ALL versions
            print(f"  Warning: Removing all versions of {tool_query}", file=sys.stderr)
            tool_key_to_remove = key  # Mark for deletion
            removal_paths.append(
                os.path.dirname(os.path.dirname(info["executable_path"]))
            )  # Root tool directory for this version

    if not tool_key_to_remove:
        # If no exact match and it's a simple name, try matching tool_name field
        for key, info in tool_configs["tools"].items():
            if info["tool_name"].lower() == tool_query.lower():
                print(
                    f"  Info: Matched Tool name '{tool_query}' to '{info['repo']}'. Removing all versions.",
                    file=sys.stderr,
                )
                tool_key_to_remove = key
                removal_paths.append(
                    os.path.dirname(os.path.dirname(info["executable_path"]))
                )

    if not tool_key_to_remove:
        print(
            f"  Error: Tool '{tool_query}' not found in installed list.",
            file=sys.stderr,
        )
        return False

    temp_keys_to_remove = []
    if (
        ":" not in tool_query and tool_key_to_remove
    ):  # If user provided repo name only, remove all related versions
        for key, info in tool_configs["tools"].items():
            if (
                info["repo"].lower() == tool_query.lower()
                or info["tool_name"].lower() == tool_query.lower()
            ):
                temp_keys_to_remove.append(key)
                removal_paths.append(
                    os.path.dirname(os.path.dirname(info["executable_path"]))
                )
    else:  # Specific version or exact match
        temp_keys_to_remove.append(tool_key_to_remove)

    unique_removal_paths = list(set(removal_paths))

    success = True
    for path in unique_removal_paths:
        if os.path.exists(path):
            print(f"  Removing directory: {path}", file=sys.stderr)
            try:
                shutil.rmtree(path)
            except OSError as e:
                print(f"  Error removing directory {path}: {e}", file=sys.stderr)
                success = False
        else:
            print(
                f"  Info: Directory not found, already removed?: {path}",
                file=sys.stderr,
            )

    if success:
        for key_to_del in temp_keys_to_remove:
            if key_to_del in tool_configs["tools"]:
                del tool_configs["tools"][key_to_del]
        save_tool_configs(tool_configs)
        print(f"  Tool(s) for '{tool_query}' removed successfully.", file=sys.stderr)
        return True
    else:
        print(f"  Failed to fully remove tool(s) for '{tool_query}'.", file=sys.stderr)
        return False


# --- Main execution function ---
def main():
    parser = argparse.ArgumentParser(
        description="Tooler: Download and manage CLI tools from GitHub releases.",
        formatter_class=argparse.RawTextHelpFormatter,  # Preserve newlines in help
        epilog="""
Examples:
  Run a tool:
    tooler nektos/act                       # Runs the latest pinned 'act' tool
    tooler nektos/act:v0.2.79               # Runs specific 'act' version v0.2.79
    tooler nektos/act -- install            # Explicitly installs/updates nektos/act (useful with arguments)

  Manage tools:
    tooler list                             # List all installed tools
    tooler update nektos/act                # Checks for and installs latest nektos/act
    tooler update --all                     # Checks for and installs latest for all non-pinned tools
    tooler remove nektos/act                # Removes all versions of nektos/act
    tooler remove nektos/act:v0.2.79        # Removes specific version of nektos/act

  Configuration:
    tooler config set update_check_days=30  # Set update check interval to 30 days
""",
    )

    # Subparsers for commands
    subparsers = parser.add_subparsers(dest="command", help="Available commands")

    # Command: run (default)
    run_parser = subparsers.add_parser(
        "run",
        help="Run a GitHub CLI tool. This is the default command if a tool_id is provided.",
    )
    run_parser.add_argument(
        "tool_id",
        nargs="?",  # Makes it optional
        help="GitHub repository (e.g., 'nektos/act') or full ID (e.g., 'nektos/act:v0.2.79').",
    )
    run_parser.add_argument(
        "tool_args", nargs=argparse.REMAINDER, help="Arguments to pass to the tool."
    )

    # Command: list
    list_parser = subparsers.add_parser("list", help="List all installed tools.")

    # Command: update
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
        help="Update all non-pinned tools to their latest versions.",
    )

    # Command: remove
    remove_parser = subparsers.add_parser(
        "remove", help="Remove an installed tool and its data."
    )
    remove_parser.add_argument(
        "tool_id", help="Tool to remove (e.g., 'nektos/act' or 'nektos/act:v0.2.79')."
    )

    # Command: config
    config_parser = subparsers.add_parser(
        "config", help="Manage tooler's configuration settings."
    )
    config_subparsers = config_parser.add_subparsers(
        dest="config_command", help="Config commands"
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

    # Parse initial arguments
    # If no command is given but has a positional argument that looks like a tool ID, default to 'run'
    if (
        len(sys.argv) > 1
        and sys.argv[1] not in subparsers.choices
        and not sys.argv[1].startswith("-")
    ):
        # Insert 'run' command at the second position (index 1)
        sys.argv.insert(1, "run")

    args = parser.parse_args()

    tool_configs = load_tool_configs()

    # Automatic update check notification
    if args.command in ["run", None]:  # Only check on 'run' command
        check_for_updates(tool_configs)

    if args.command == "list":
        list_installed_tools(tool_configs)
        sys.exit(0)
    elif args.command == "remove":
        remove_tool(tool_configs, args.tool_id)
        sys.exit(0)
    elif args.command == "update":
        if args.all:
            print("  Updating all applicable tools...", file=sys.stderr)
            updated_count = 0
            for key, tool_info in list(tool_configs["tools"].items()):
                # Only update tools not explicitly pinned and recently checked
                if ":" not in key:  # Not a specific version like tool:v1.2.3
                    print(
                        f"\n  Attempting to update {tool_info['repo']}...",
                        file=sys.stderr,
                    )
                    executable_path = install_or_update_tool(
                        tool_configs,
                        tool_info["tool_name"],
                        tool_info["repo"],
                        version="latest",
                        force_update=True,
                    )
                    if executable_path:
                        updated_count += 1
                else:
                    print(
                        f"\n  Tool {tool_info['repo']} is version pinned, skipping updating.",
                        file=sys.stderr,
                    )
            print(
                f"\n  Update process finished. {updated_count} tool(s) updated.",
                file=sys.stderr,
            )
            sys.exit(0)
        elif args.tool_id:
            repo_full_name = args.tool_id.split(":")[0]
            tool_name = repo_full_name.split("/")[-1]
            print(f"  Attempting to update {args.tool_id}...", file=sys.stderr)
            executable_path = install_or_update_tool(
                tool_configs,
                tool_name,
                repo_full_name,
                version="latest",
                force_update=True,
            )
            if executable_path:
                print(f"  {args.tool_id} updated successfully.", file=sys.stderr)
                sys.exit(0)
            else:
                print(f"  Failed to update {args.tool_id}.", file=sys.stderr)
                sys.exit(1)
    elif args.command == "config":
        if args.config_command == "get":
            if args.key:
                val = tool_configs["settings"].get(args.key)
                if val is not None:
                    print(f"{args.key}={val}", file=sys.stderr)
                else:
                    print(f"Setting '{args.key}' not found.", file=sys.stderr)
                    sys.exit(1)
            else:  # List all settings
                print("--- Tooler Settings ---", file=sys.stderr)
                for key, value in tool_configs["settings"].items():
                    print(f"  {key}: {value}", file=sys.stderr)
                print("-----------------------", file=sys.stderr)
            sys.exit(0)
        elif args.config_command == "set":
            if "=" in args.key_value:
                key, value_str = args.key_value.split("=", 1)
                try:
                    # Attempt to convert to appropriate type
                    if value_str.isdigit():
                        value = int(value_str)
                    elif value_str.lower() in ("true", "false"):
                        value = value_str.lower() == "true"
                    else:
                        value = value_str  # Keep as string

                    tool_configs["settings"][key] = value
                    save_tool_configs(tool_configs)
                    print(f"Setting '{key}' updated to '{value}'.", file=sys.stderr)
                    sys.exit(0)
                except ValueError:
                    print(
                        f"Could not parse value for '{key}'. Please provide integer, boolean, or string.",
                        file=sys.stderr,
                    )
                    sys.exit(1)
            else:
                print("Invalid format. Use 'key=value'.", file=sys.stderr)
                sys.exit(1)
        else:
            print(
                "Dumping entire config",
                file=sys.stderr,
            )
            print(json.dumps(tool_configs, indent=2), file=sys.stderr)
    elif args.command == "run" and args.tool_id:  # Main execution path
        repo_full_name = args.tool_id.split(":")[0]
        tool_version_req = (
            args.tool_id.split(":", 1)[1] if ":" in args.tool_id else "latest"
        )
        tool_name = repo_full_name.split("/")[-1]

        executable_path = None

        # 1. Try to find an existing, valid installation
        tool_info_found = find_tool_executable(tool_configs, args.tool_id)
        if tool_info_found:
            executable_path = tool_info_found["executable_path"]
            # Update last_accessed for the specific key that was resolved
            if (
                tool_version_req == "latest"
            ):  # If "latest" was requested, update specific version key
                resolved_key = (
                    f"{tool_info_found['repo']}:v{tool_info_found['version']}"
                )
            else:
                resolved_key = args.tool_id

            # Ensure the specific version key exists. Could happen if tool_id was "repo" then it picked one.
            if resolved_key not in tool_configs["tools"]:
                tool_configs["tools"][resolved_key] = (
                    tool_info_found  # Add the resolved one if it was just "repo"
                )
            tool_configs["tools"][resolved_key]["last_accessed"] = (
                datetime.now().isoformat()
            )
            save_tool_configs(tool_configs)

        # 2. If not found or broken, install/update
        if not executable_path:
            print(
                f"  Tool {args.tool_id} not found or corrupted. Attempting to install/update...",
                file=sys.stderr,
            )
            executable_path = install_or_update_tool(
                tool_configs, tool_name, repo_full_name, version=tool_version_req
            )

        if executable_path:
            # Prepare arguments for execution
            # In Windows, if a path has spaces, it needs to be quoted for subprocess correctly.
            # However, subprocess.Popen handles this when the command is a list.
            cmd = [executable_path] + args.tool_args

            print(f"  Running: {' '.join(cmd)}", file=sys.stderr)
            try:
                # Execute the tool
                # Use sys.executable and -c if the executable_path implies a Python script runner,
                # though for GitHub releases it's usually a compiled binary.
                os.execv(cmd[0], cmd)
                # os.execv replaces the current process, so code after this is only reached if it fails
            except FileNotFoundError:
                print(
                    f"  Error: Executable not found at {executable_path}. It might have been moved or deleted.",
                    file=sys.stderr,
                )
                sys.exit(127)  # Command not found exit code
            except OSError as e:
                print(f"  Error directly executing tool: {e}", file=sys.stderr)
                # Fallback to subprocess.run if os.execv fails (e.g., permissions, shell issues)
                try:
                    import subprocess

                    result = subprocess.run(cmd, check=False)
                    sys.exit(result.returncode)
                except Exception as sub_e:
                    print(f"  Error running via subprocess: {sub_e}", file=sys.stderr)
                    sys.exit(1)
        else:
            print(
                f"  Failed to get executable for {args.tool_id}. See errors above.",
                file=sys.stderr,
            )
            sys.exit(1)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
