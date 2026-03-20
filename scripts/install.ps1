# Savhub CLI installer for Windows.
# Usage:
#   irm https://raw.githubusercontent.com/savhub-ai/savhub/main/scripts/install.ps1 | iex
#   .\install.ps1 -Version v0.2.0
#   .\install.ps1 -InstallDir C:\custom\path

param(
    [string]$Version = "",
    [string]$InstallDir = ""
)

$ErrorActionPreference = "Stop"

$Repo = "savhub-ai/savhub"

if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA "savhub\bin"
}

function Get-Platform {
    $archCandidates = @()

    try {
        $archCandidates += [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    }
    catch {
    }

    try {
        $archCandidates += [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
    }
    catch {
    }

    $archCandidates += $env:PROCESSOR_ARCHITEW6432
    $archCandidates += $env:PROCESSOR_ARCHITECTURE

    foreach ($candidate in $archCandidates) {
        if ([string]::IsNullOrWhiteSpace($candidate)) {
            continue
        }

        switch ($candidate.Trim().ToUpperInvariant()) {
            { $_ -in @("X64", "AMD64", "X86_64") } {
                return "windows-x64"
            }
            { $_ -in @("ARM64", "AARCH64") } {
                return "windows-arm64"
            }
            { $_ -in @("X86", "I386", "I686", "ARM") } {
                throw "Unsupported architecture: $candidate. Savhub Windows releases are available for x64 and arm64."
            }
        }
    }

    $detected = $archCandidates |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        Select-Object -Unique

    if ($detected) {
        throw "Unsupported architecture: $($detected -join ', '). Savhub Windows releases are available for x64 and arm64."
    }

    throw "Could not determine Windows architecture. Checked RuntimeInformation and PROCESSOR_ARCHITECTURE environment variables."
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

    $archiveName = "savhub-$platform.zip"
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

        # Install binaries
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        foreach ($bin in @("savhub.exe", "savhub-desktop.exe")) {
            $found = Get-ChildItem -Path $tmpDir -Recurse -Filter $bin | Select-Object -First 1
            if ($found) {
                $destPath = Join-Path $InstallDir $bin
                Copy-Item -Path $found.FullName -Destination $destPath -Force
                Write-Host "Installed $bin to $destPath"
            }
        }

        if (-not (Test-Path (Join-Path $InstallDir "savhub.exe"))) {
            throw "savhub.exe not found in archive"
        }

        # Add to user PATH
        Add-ToPath

        # Create Start Menu shortcuts
        Add-StartMenuShortcuts

        # Create Desktop shortcut
        Add-DesktopShortcut

        # Install bundled skills into AI agent directories
        Write-Host "Installing savhub skills..."
        $savhubExe = Join-Path $InstallDir "savhub.exe"
        try { & $savhubExe pilot install 2>$null } catch {}

        Write-Host ""
        Write-Host "Savhub $Version installed successfully!" -ForegroundColor Green
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

function Add-StartMenuShortcuts {
    $desktopExe = Join-Path $InstallDir "savhub-desktop.exe"
    if (-not (Test-Path $desktopExe)) { return }

    $startMenuDir = Join-Path ([System.Environment]::GetFolderPath("Programs")) "Savhub"
    New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null

    $shell = New-Object -ComObject WScript.Shell

    $lnk = $shell.CreateShortcut((Join-Path $startMenuDir "Savhub Desktop.lnk"))
    $lnk.TargetPath = $desktopExe
    $lnk.WorkingDirectory = $InstallDir
    $lnk.Description = "Savhub Desktop"
    $lnk.Save()

    $cliExe = Join-Path $InstallDir "savhub.exe"
    if (Test-Path $cliExe) {
        $lnk = $shell.CreateShortcut((Join-Path $startMenuDir "Savhub CLI.lnk"))
        $lnk.TargetPath = $cliExe
        $lnk.WorkingDirectory = $InstallDir
        $lnk.Description = "Savhub CLI"
        $lnk.Save()
    }

    Write-Host "Created Start Menu shortcuts in $startMenuDir"
}

function Add-DesktopShortcut {
    $desktopExe = Join-Path $InstallDir "savhub-desktop.exe"
    if (-not (Test-Path $desktopExe)) { return }

    $desktopDir = [System.Environment]::GetFolderPath("Desktop")
    $shell = New-Object -ComObject WScript.Shell
    $lnk = $shell.CreateShortcut((Join-Path $desktopDir "Savhub Desktop.lnk"))
    $lnk.TargetPath = $desktopExe
    $lnk.WorkingDirectory = $InstallDir
    $lnk.Description = "Savhub Desktop"
    $lnk.Save()

    Write-Host "Created Desktop shortcut"
}

Install-Savhub
