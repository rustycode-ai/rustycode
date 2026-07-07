# RustyCode installer for Windows
# Usage: irm https://rustycode-ai.github.io/rustycode/install.ps1 | iex
#   or:  iex "& { $(irm https://rustycode-ai.github.io/rustycode/install.ps1) } -Bin rustycode-mcp-computer-use"

param(
    [switch]$Nightly,
    [string]$InstallDir = "$env:USERPROFILE\.local\bin",
    [string]$Bin = ""
)

$ErrorActionPreference = "Stop"

$Repo = "rustycode-ai/rustycode"
$Headers = @{
    "User-Agent" = "RustyCode-Installer"
}

function Get-LatestNightlyTag {
    $Releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases?per_page=100" -Headers $Headers
    $NightlyRelease = $Releases | Where-Object { $_.tag_name -like "nightly-*" } | Select-Object -First 1
    return $NightlyRelease.tag_name
}

# Resolve version
if ($Nightly) {
    $Tag = Get-LatestNightlyTag
} else {
    $Latest = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers $Headers
    $Tag = $Latest.tag_name
}

if (-not $Tag) {
    Write-Error "Could not determine release version"
    exit 1
}

Write-Host "Installing RustyCode $Tag for Windows x64..."

$Filename = "rustycode-windows-x64.zip"
$Url = "https://github.com/$Repo/releases/download/$Tag/$Filename"

# Download
$TempDir = [System.IO.Path]::GetTempPath() + [System.IO.Path]::GetRandomFileName()
New-Item -ItemType Directory -Path $TempDir | Out-Null

$ZipPath = Join-Path $TempDir $Filename
Write-Host "Downloading $Url..."
Invoke-WebRequest -Uri $Url -OutFile $ZipPath -Headers $Headers

# Extract
Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

# Find binaries
$Binaries = Get-ChildItem -Path $TempDir -Recurse -File | Where-Object { $_.Name -like "rustycode*.exe" }

if (-not $Binaries) {
    Write-Error "Could not find any RustyCode binaries in archive"
    exit 1
}

# Filter if --Bin was specified
if ($Bin) {
    $Selected = $Binaries | Where-Object { $_.Name -eq "$Bin.exe" }
    if (-not $Selected) {
        Write-Error "Binary '$Bin.exe' not found in archive. Available: $($Binaries.Name -join ', ')"
        exit 1
    }
    $Binaries = $Selected
}

# Install
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$Installed = @()
foreach ($Binary in $Binaries) {
    $DestPath = Join-Path $InstallDir $Binary.Name
    Copy-Item $Binary.FullName -Destination $DestPath -Force
    $Installed += $DestPath
}

# Add to PATH if not already there
$PathDirs = $env:PATH -split ";"
if ($PathDirs -notcontains $InstallDir) {
    $env:PATH = "$InstallDir;$env:PATH"
    $UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("PATH", "$InstallDir;$UserPath", "User")
    }
    Write-Host "Added $InstallDir to PATH"
}

# Verify
foreach ($DestPath in $Installed) {
    $Version = & $DestPath --version 2>$null
    Write-Host "Installed: $DestPath ($Version)"
}
