# Install the latest larp release binary on Windows.
# Usage:
#   irm https://raw.githubusercontent.com/pal404error/larp-files/main/install.ps1 | iex
#
# Optional environment variables:
#   $env:INSTALL_DIR   (default: $HOME\.cargo\bin if directory exists, else $HOME\AppData\Local\Programs\larp\bin)
#   $env:LARP_VERSION  (default: "latest")

$ErrorActionPreference = "Stop"

$Repo = "pal404error/larp-files"
$Version = if ($env:LARP_VERSION) { $env:LARP_VERSION } else { "latest" }

if ($env:INSTALL_DIR) {
    $TargetDir = $env:INSTALL_DIR
} elseif (Test-Path "$HOME\.cargo\bin") {
    $TargetDir = "$HOME\.cargo\bin"
} else {
    $TargetDir = "$HOME\AppData\Local\Programs\larp\bin"
}

if ($Version -eq "latest") {
    $DownloadUrl = "https://github.com/$Repo/releases/latest/download/larp-Windows.exe"
} else {
    $DownloadUrl = "https://github.com/$Repo/releases/download/$Version/larp-Windows.exe"
}

if (-not (Test-Path $TargetDir)) {
    New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
}

$ExePath = Join-Path $TargetDir "larp.exe"
Write-Host "Downloading larp-Windows.exe to $ExePath ..." -ForegroundColor Cyan

Invoke-WebRequest -Uri $DownloadUrl -OutFile $ExePath

# Verify PATH
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$TargetDir*") {
    Write-Host "`nAdding $TargetDir to your User PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$TargetDir", "User")
    $env:PATH = "$env:PATH;$TargetDir"
}

Write-Host "Successfully installed larp!" -ForegroundColor Green
Write-Host "Verify with: larp --version" -ForegroundColor Cyan
