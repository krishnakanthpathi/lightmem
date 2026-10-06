# LightMem (lmem) Universal Windows PowerShell Uninstaller
# Usage: irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/uninstall.ps1 | iex

param (
    [switch]$PurgeData,
    [switch]$Yes
)

$ErrorActionPreference = "Continue"

Write-Host ""
Write-Host "  ❖  L I G H T M E M   U N I N S T A L L E R  (Windows)" -ForegroundColor Red
Write-Host "  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor DarkGray
Write-Host "  ▸ Removing LightMem binary and shell configurations"
Write-Host ""

if (-not $Yes) {
    $confirm = Read-Host "  ? Proceed with uninstalling LightMem? [y/N]"
    if ($confirm -notmatch "^[yY](es)?$") {
        Write-Host "  Aborted by user." -ForegroundColor Gray
        exit 0
    }
}

# 1. Remove binary from ~/.local/bin and ~/.cargo/bin
$BinPaths = @(
    (Join-Path $HOME ".local\bin\lmem.exe"),
    (Join-Path $HOME ".cargo\bin\lmem.exe"),
    (Join-Path $HOME ".lightmem\bin\lmem.exe")
)

foreach ($path in $BinPaths) {
    if (Test-Path $path) {
        Remove-Item $path -Force -ErrorAction SilentlyContinue
        Write-Host "  ◈ Removed binary: $path" -ForegroundColor Green
    }
}

# 2. Check and remove Python package if present
if (Get-Command pip -ErrorAction SilentlyContinue) {
    $installed = pip show lmem 2>$null
    if ($installed) {
        Write-Host "  ▸ Uninstalling Python package (pip uninstall lmem)..." -ForegroundColor Yellow
        pip uninstall -y lmem | Out-Null
        Write-Host "  ◈ Uninstalled lmem Python package" -ForegroundColor Green
    }
}

# 3. Handle data directory
$LightMemDir = Join-Path $HOME ".lightmem"
if ($PurgeData) {
    if (Test-Path $LightMemDir) {
        Remove-Item $LightMemDir -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "  ◈ Purged all data & models: $LightMemDir" -ForegroundColor Red
    }
} else {
    $VaultDb = Join-Path $LightMemDir "memories.db"
    if (Test-Path $VaultDb) {
        Write-Host ""
        Write-Host "  ℹ Persistent vault preserved at: $VaultDb" -ForegroundColor Cyan
        Write-Host "    (To delete your memories vault as well, pass: -PurgeData)" -ForegroundColor Gray
    }
}

Write-Host ""
Write-Host "  ✔ LightMem has been successfully uninstalled." -ForegroundColor Green
Write-Host ""
