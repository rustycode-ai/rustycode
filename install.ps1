# RustyCode installer for Windows
# Usage: irm https://rustycode-ai.github.io/install.ps1 | iex
#   or:  irm https://rustycode-ai.github.io/install.ps1 | iex; & $args[0] --nightly

param(
    [switch]$Nightly,
    [string]$InstallDir = "$env:USERPROFILE\.local\bin"
)

$ErrorActionPreference = "Stop"

$Repo = "rustycode-ai/rustycode"
$BinaryName = "rustycode.exe"

# Resolve version
if ($Nightly) {
    $Tag = "nightly"
} else {
    $Latest = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $Tag = $Latest.tag_name
    if (-not $Tag) {
        Write-Error "Could not determine latest version"
        exit 1
    }
}

Write-Host "Installing RustyCode $Tag for Windows x64..."

$Filename = "rustycode-windows-x64.zip"
$Url = "https://github.com/$Repo/releases/download/$Tag/$Filename"

# Download
$TempDir = [System.IO.Path]::GetTempPath() + [System.IO.Path]::GetRandomFileName()
New-Item -ItemType Directory -Path $TempDir | Out-Null

$ZipPath = Join-Path $TempDir $Filename
Write-Host "Downloading $Url..."
Invoke-WebRequest -Uri $Url -OutFile $ZipPath

# Extract
Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

# Find the binary
$Binary = Get-ChildItem -Path $TempDir -Filter "*.exe" -Recurse | Where-Object { $_.Name -like "rustycode*" } | Select-Object -First 1

if (-not $Binary) {
    Write-Error "Could not find rustycode binary in archive"
    exit 1
}

# Install
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$DestPath = Join-Path $InstallDir $BinaryName
Copy-Item $Binary.FullName -Destination $DestPath -Force

# Add to PATH if not already there
$PathDirs = $env:PATH -split ";"
if ($PathDirs -notcontains $InstallDir) {
    $env:PATH = "$InstallDir;$env:PATH"
    # Persist for future sessions
    $UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("PATH", "$InstallDir;$UserPath", "User")
    }
    Write-Host "Added $InstallDir to PATH"
}

# Verify
$Version = & $DestPath --version 2>$null
Write-Host "RustyCode installed: $DestPath ($Version)"
