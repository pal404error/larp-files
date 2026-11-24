# Install a larp release binary on Windows after verifying its SHA-256 checksum.
# Usage:
#   irm https://raw.githubusercontent.com/pal404error/larp-files/main/install.ps1 | iex
#   (safer: download this script, read it, then run it)
#
# Optional environment variables:
#   $env:INSTALL_DIR         (default: $HOME\.cargo\bin if directory exists, else $HOME\AppData\Local\Programs\larp\bin)
#   $env:LARP_VERSION        (default: "latest")
#   $env:LARP_DOWNLOAD_BASE  (default: https://github.com/<repo>/releases; for mirrors and tests)
#   $env:LARP_NO_MODIFY_PATH (set to "1" to leave the user PATH unchanged)
#
# The binary must match the SHA256SUMS file published with the same release,
# or nothing is installed. For a stronger check (build provenance signed by
# GitHub Actions): gh attestation verify <path-to-larp.exe> --repo pal404error/larp-files

$ErrorActionPreference = "Stop"

$Repo = "pal404error/larp-files"
$Asset = "larp-Windows.exe"
$Version = if ($env:LARP_VERSION) { $env:LARP_VERSION } else { "latest" }
$Base = if ($env:LARP_DOWNLOAD_BASE) { $env:LARP_DOWNLOAD_BASE } else { "https://github.com/$Repo/releases" }

if ($env:INSTALL_DIR) {
    $TargetDir = $env:INSTALL_DIR
} elseif (Test-Path "$HOME\.cargo\bin") {
    $TargetDir = "$HOME\.cargo\bin"
} else {
    $TargetDir = "$HOME\AppData\Local\Programs\larp\bin"
}

if ($Version -eq "latest") {
    $ReleaseUrl = "$Base/latest/download"
} else {
    $ReleaseUrl = "$Base/download/$Version"
}

$TmpDir = Join-Path ([IO.Path]::GetTempPath()) ("larp-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TmpDir | Out-Null
try {
    $TmpExe = Join-Path $TmpDir $Asset
    $TmpSums = Join-Path $TmpDir "SHA256SUMS"
    Write-Host "Downloading $Asset and SHA256SUMS from $ReleaseUrl ..." -ForegroundColor Cyan
    Invoke-WebRequest -Uri "$ReleaseUrl/$Asset" -OutFile $TmpExe -UseBasicParsing
    try {
        Invoke-WebRequest -Uri "$ReleaseUrl/SHA256SUMS" -OutFile $TmpSums -UseBasicParsing
    } catch {
        throw "This release has no SHA256SUMS file; refusing to install an unverified binary."
    }

    $Expected = $null
    foreach ($Line in Get-Content $TmpSums) {
        $Parts = $Line.Trim() -split '\s+', 2
        if ($Parts.Count -eq 2 -and $Parts[1].TrimStart('*') -eq $Asset) {
            $Expected = $Parts[0].ToLowerInvariant()
        }
    }
    if (-not $Expected) {
        throw "No checksum for $Asset in SHA256SUMS; refusing to install."
    }
    $Actual = (Get-FileHash -Path $TmpExe -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Expected -ne $Actual) {
        throw "Checksum mismatch for $Asset (expected $Expected, got $Actual); refusing to install."
    }

    if (-not (Test-Path $TargetDir)) {
        New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
    }
    $ExePath = Join-Path $TargetDir "larp.exe"
    Move-Item -Path $TmpExe -Destination $ExePath -Force
    Write-Host "Installed $ExePath (sha256 $Actual)" -ForegroundColor Green
} finally {
    Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
}

# Add to the user PATH only if this exact directory is not already an entry
# (a substring test would wrongly match e.g. C:\tools\bin2 for C:\tools\bin).
# Set $env:LARP_NO_MODIFY_PATH = "1" to skip this step entirely.
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
$Entries = @()
if ($UserPath) { $Entries = @($UserPath -split ';' | Where-Object { $_ }) }
if ($env:LARP_NO_MODIFY_PATH -eq "1") {
    Write-Host "Skipping PATH update (LARP_NO_MODIFY_PATH=1)." -ForegroundColor Yellow
} elseif ($Entries -notcontains $TargetDir) {
    Write-Host "`nAdding $TargetDir to your User PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", (($Entries + $TargetDir) -join ';'), "User")
    $env:PATH = "$env:PATH;$TargetDir"
}

Write-Host "Verify with: larp --version" -ForegroundColor Cyan
