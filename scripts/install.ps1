# Savhub CLI installer for Windows.
# Usage:
#   irm https://raw.githubusercontent.com/savhub-ai/savhub-client/main/scripts/install.ps1 | iex
#   .\install.ps1 -Version v0.2.0
#   .\install.ps1 -InstallDir C:\custom\path

param(
    [string]$Version = "",
    [string]$InstallDir = ""
)

$ErrorActionPreference = "Stop"

$Repo = "savhub-ai/client"

if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "savhub\bin"
}

function Get-Platform {
    $arch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        "X64"   { "x64" }
        "Arm64" { "arm64" }
        default { throw "Unsupported architecture: $_" }
    }
    return "windows-$arch"
}

function Get-LatestVersion {
    $url = "https://api.github.com/repos/$Repo/releases/latest"
    $response = Invoke-RestMethod -Uri $url -UseBasicParsing
    return $response.tag_name
}

function Install-Savhub {
    $platform = Get-Platform
    Write-Host "Detected platform: $platform"

    if (-not $Version) {
        Write-Host "Fetching latest version..."
        $Version = Get-LatestVersion
        if (-not $Version) {
            throw "Could not determine latest version"
        }
    }
    Write-Host "Installing savhub $Version..."

    $archiveName = "savhub-cli-$platform.zip"
    $downloadUrl = "https://github.com/$Repo/releases/download/$Version/$archiveName"

    $tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) "savhub-install-$(Get-Random)"
    New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

    try {
        $archivePath = Join-Path $tmpDir $archiveName
        Write-Host "Downloading $downloadUrl..."
        Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath -UseBasicParsing

        Write-Host "Extracting..."
        Expand-Archive -Path $archivePath -DestinationPath $tmpDir -Force

        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

        # Find the binary
        $binary = Get-ChildItem -Path $tmpDir -Recurse -Filter "savhub.exe" | Select-Object -First 1
        if (-not $binary) {
            throw "savhub.exe not found in archive"
        }

        $destPath = Join-Path $InstallDir "savhub.exe"
        Copy-Item -Path $binary.FullName -Destination $destPath -Force

        Write-Host "Installed savhub to $destPath"

        # Add to user PATH
        Add-ToPath

        Write-Host ""
        Write-Host "savhub $Version installed successfully!" -ForegroundColor Green
        Write-Host ""

        # Check if available immediately
        $currentPath = $env:PATH -split ";"
        if ($currentPath -notcontains $InstallDir) {
            # Update current session PATH
            $env:PATH = "$InstallDir;$env:PATH"
            Write-Host "Added $InstallDir to current session PATH."
            Write-Host "New terminal windows will also have savhub in PATH."
        }
    }
    finally {
        Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Add-ToPath {
    $userPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")
    if ($userPath -and ($userPath -split ";" | Where-Object { $_ -eq $InstallDir })) {
        Write-Host "$InstallDir is already in user PATH"
        return
    }

    if ($userPath) {
        $newPath = "$InstallDir;$userPath"
    } else {
        $newPath = $InstallDir
    }

    [System.Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
    Write-Host "Added $InstallDir to user PATH (persistent)"
}

Install-Savhub
