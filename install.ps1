param(
    # Embedded version (set during release build via sed substitution)
    # RELEASE_VERSION_MARKER_START
    [string]$ToolerVersion = "",
    # RELEASE_VERSION_MARKER_END
    [string]$InstallDir = "$env:LOCALAPPDATA\tooler\bin",
    [switch]$NoPathUpdate,
    [switch]$NoBootstrap
)

$ErrorActionPreference = "Stop"

if (-not $ToolerVersion -and $env:TOOLER_VERSION) {
    $ToolerVersion = $env:TOOLER_VERSION
}

if ($ToolerVersion) {
    Write-Host "Installing tooler $ToolerVersion..."
} else {
    Write-Host "Installing tooler (latest)..."
}

if (-not ('ToolerNativeArchitecture' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ToolerNativeArchitecture {
    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern bool IsWow64Process2(IntPtr process, out ushort processMachine, out ushort nativeMachine);
    public static string Detect() {
        ushort processMachine, nativeMachine;
        try {
            if (!IsWow64Process2(new IntPtr(-1), out processMachine, out nativeMachine))
                throw new System.ComponentModel.Win32Exception();
        } catch (EntryPointNotFoundException) {
            return "Unavailable";
        }
        switch (nativeMachine) {
            case 0x8664: return "X64";
            case 0xAA64: return "Arm64";
            default: throw new NotSupportedException("Unsupported native Windows architecture");
        }
    }
}
'@
}
$nativeArchitecture = [ToolerNativeArchitecture]::Detect()
if ($nativeArchitecture -eq "Unavailable") {
    Write-Warning "Native Windows architecture query unavailable; using runtime architecture hints."
    $nativeArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
}
$arch = switch ($nativeArchitecture) {
    "X64" { "x86_64" }
    "Arm64" { "aarch64" }
    default { throw "No pre-built Windows binary is available for architecture '$($_)'." }
}

if ($ToolerVersion) {
    $tag = $ToolerVersion
    Write-Host "Installing release version: $tag"
} else {
    Write-Host "Fetching latest release..."
    $tag = $null

    if (Get-Command gh -ErrorAction SilentlyContinue) {
        try {
            $tag = (& gh api repos/morgaesis/tooler/releases/latest --jq '.tag_name' 2>$null).Trim()
        } catch {
            $tag = $null
        }
    }

    if (-not $tag) {
        try {
            $headers = @{
                "User-Agent" = "tooler-install.ps1"
            }
            if ($env:GITHUB_TOKEN) {
                $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
            }

            $release = Invoke-RestMethod -Uri "https://api.github.com/repos/morgaesis/tooler/releases/latest" -Headers $headers
            $tag = $release.tag_name
        } catch {
            throw @"
Failed to fetch release information.

This is likely due to GitHub API rate limiting for unauthenticated requests.

Workarounds:
  1. Install GitHub CLI and authenticate: gh auth login
  2. Download the latest release directly from: https://github.com/morgaesis/tooler/releases/latest
  3. Set TOOLER_VERSION manually to a release tag from: https://github.com/morgaesis/tooler/releases
"@
        }
    }

    if (-not $tag) {
        throw "Latest release response did not include a tag name."
    }

    Write-Host "Found latest version: $tag"
}

$assetName = "tooler-$tag-$arch-pc-windows-msvc.zip"
$downloadUrl = "https://github.com/morgaesis/tooler/releases/download/$tag/$assetName"
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("tooler-install-" + [System.Guid]::NewGuid())

New-Item -ItemType Directory -Path $tempDir | Out-Null

try {
    $zipPath = Join-Path $tempDir $assetName
    Write-Host "Downloading $assetName..."
    Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -Headers @{
        "User-Agent" = "tooler-install.ps1"
    }

    Write-Host "Extracting..."
    Expand-Archive -Path $zipPath -DestinationPath $tempDir -Force

    $sourceExe = Get-ChildItem -Path $tempDir -Filter "tooler.exe" -Recurse | Select-Object -First 1
    if (-not $sourceExe) {
        throw "Archive did not contain tooler.exe."
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $targetExe = Join-Path $InstallDir "tooler.exe"

    Write-Host "Installing tooler to $targetExe..."
    Copy-Item -Path $sourceExe.FullName -Destination $targetExe -Force

    if (-not $NoPathUpdate) {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $pathEntries = @()
        if ($userPath) {
            $pathEntries = $userPath -split ";" | Where-Object { $_ }
        }

        if ($pathEntries -notcontains $InstallDir) {
            Write-Host "Adding $InstallDir to the user PATH..."
            $newPath = (@($pathEntries) + $InstallDir) -join ";"
            [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        }

        $env:Path = "$InstallDir;$env:Path"
    }

    if (-not $NoBootstrap -and $env:TOOLER_NO_BOOTSTRAP -ne "1" -and $env:TOOLER_NO_BOOTSTRAP -ne "true") {
        if (-not (Test-Path $targetExe -PathType Leaf)) {
            throw "Installed tooler.exe was not found at $targetExe."
        }

        Write-Host "Registering tooler for self-updates..."
        try {
            & $targetExe pull morgaesis/tooler 2>$null
            if ($LASTEXITCODE -ne 0) {
                Write-Warning "Self-update registration skipped: tooler exited with code $LASTEXITCODE"
                $global:LASTEXITCODE = 0
            }
        } catch {
            Write-Warning "Self-update registration skipped: $($_.Exception.Message)"
            $global:LASTEXITCODE = 0
        }
    } else {
        Write-Host "Skipping self-update registration."
    }

    Write-Host "Installation complete."
    Write-Host "Open a new PowerShell session, or run: `$env:Path = '$InstallDir;' + `$env:Path"
    Write-Host "Future updates: tooler update tooler"
} finally {
    Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}
