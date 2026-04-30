# scripts/install.ps1
$ErrorActionPreference = "Stop"
Write-Host "Installing RustyCode for Windows..."

if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    Write-Error "Cargo not found. Please install Rust first: https://rustup.rs/"
    exit 1
}

$buildDir = Join-Path $env:USERPROFILE ".rustycode-build"
if (-not (Test-Path $buildDir)) { New-Item -Path $buildDir -ItemType Directory }
Set-Location $buildDir

if (Test-Path "rustycode") {
    Set-Location rustycode
    git pull origin main
} else {
    git clone https://github.com/luengnat/rustycode.git
    Set-Location rustycode
}

Write-Host "Building RustyCode..."
cargo build --release --package rustycode-cli

$binPath = Join-Path $env:USERPROFILE ".local\bin"
if (-not (Test-Path $binPath)) { New-Item -Path $binPath -ItemType Directory }
Copy-Item -Path "target\release\rustycode.exe" -Destination (Join-Path $binPath "rustycode.exe")
Write-Host "Done! RustyCode installed to $binPath\rustycode.exe"
