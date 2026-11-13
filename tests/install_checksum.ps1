# Offline test for install.ps1 checksum verification (LARP-06).
# Runs the real installer against a local file:// "release". It sets
# LARP_NO_MODIFY_PATH=1, so the user PATH is never touched.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File tests\install_checksum.ps1
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$Work = Join-Path ([IO.Path]::GetTempPath()) ("larp-ps-test-" + [guid]::NewGuid().ToString("N"))
$Rel = Join-Path $Work "rel\latest\download"
$Out = Join-Path $Work "out"
$Log = Join-Path $Work "log.txt"
$ErrLog = Join-Path $Work "err.txt"
New-Item -ItemType Directory -Path $Rel -Force | Out-Null

function Write-Sums {
    $h = (Get-FileHash (Join-Path $Rel "larp-Windows.exe") -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -Path (Join-Path $Rel "SHA256SUMS") -Value "$h  larp-Windows.exe" -Encoding ascii
}
function Invoke-Install {
    # Child process: environment is inherited, so set it only for the child.
    $env:INSTALL_DIR = $Out
    $env:LARP_DOWNLOAD_BASE = ([Uri](Join-Path $Work "rel")).AbsoluteUri
    $env:LARP_NO_MODIFY_PATH = "1"
    try {
        # Start-Process keeps the child's stderr out of this script's error
        # stream (a refused install writes to stderr by design).
        $p = Start-Process -FilePath "powershell" -Wait -PassThru -NoNewWindow `
            -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$(Join-Path $Root 'install.ps1')`"") `
            -RedirectStandardOutput $Log -RedirectStandardError $ErrLog
        return ($p.ExitCode -eq 0)
    } finally {
        Remove-Item Env:INSTALL_DIR, Env:LARP_DOWNLOAD_BASE, Env:LARP_NO_MODIFY_PATH -ErrorAction SilentlyContinue
    }
}

try {
    # 1. Matching checksum installs.
    Set-Content -Path (Join-Path $Rel "larp-Windows.exe") -Value "genuine larp binary" -Encoding ascii
    Write-Sums
    if (-not (Invoke-Install)) { throw "genuine binary was refused: $(Get-Content $ErrLog -Raw)" }
    if ((Get-Content (Join-Path $Out "larp.exe") -Raw) -notmatch "genuine") { throw "installed file differs" }

    # 2. Tampered binary is refused; previous install untouched.
    Set-Content -Path (Join-Path $Rel "larp-Windows.exe") -Value "evil binary" -Encoding ascii
    if (Invoke-Install) { throw "tampered binary was installed" }
    if ((Get-Content $ErrLog -Raw) -notmatch "Checksum mismatch") { throw "no mismatch message" }
    if ((Get-Content (Join-Path $Out "larp.exe") -Raw) -notmatch "genuine") { throw "previous install was overwritten" }

    # 3. Release without SHA256SUMS is refused.
    Remove-Item (Join-Path $Out "larp.exe")
    Remove-Item (Join-Path $Rel "SHA256SUMS")
    if (Invoke-Install) { throw "unverifiable binary was installed" }
    if (Test-Path (Join-Path $Out "larp.exe")) { throw "file installed without verification" }

    Write-Host "install.ps1 checksum tests: ok"
} finally {
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}
