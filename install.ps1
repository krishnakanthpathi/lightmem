# LightMem (lmem) Universal Windows PowerShell Installer
# Usage: irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.ps1 | iex

param(
    [switch]$SkipModels
)

$ErrorActionPreference = "Stop"

$Repo = "krishnakanthpathi/lightmem"
$ReleaseTag = if ($env:LIGHTMEM_VERSION) { $env:LIGHTMEM_VERSION } else { "v0.8.4" }
$InstallDir = Join-Path $HOME ".local\bin"
$Arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
$Target = "$Arch-pc-windows-msvc"
$ArchiveUrl = "https://github.com/$Repo/releases/download/$ReleaseTag/lmem-$Target.zip"

Write-Host ""
Write-Host "  ❖  L I G H T M E M   I N S T A L L E R  ($ReleaseTag · Windows)" -ForegroundColor Red
Write-Host "  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor DarkGray
Write-Host "  ▸ Target Architecture: $Target"
Write-Host "  ▸ Install Directory:   $InstallDir"
Write-Host ""

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$ExePath = Join-Path $InstallDir "lmem.exe"
$TmpZip = Join-Path $env:TEMP "lmem-$Target.zip"

$Downloaded = $false
try {
    Write-Host "  ◈ Downloading prebuilt binary from $ArchiveUrl ..." -ForegroundColor Cyan
    Invoke-WebRequest -Uri $ArchiveUrl -OutFile $TmpZip -UseBasicParsing
    $ChecksumContent = Invoke-RestMethod -Uri "https://github.com/$Repo/releases/download/$ReleaseTag/SHA256SUMS"
    $ChecksumPattern = '^([0-9a-fA-F]{64})\s+\*?' + [regex]::Escape("lmem-$Target.zip") + '$'
    $ChecksumLines = @($ChecksumContent -split '\r?\n' | Where-Object { $_ -match $ChecksumPattern })
    if ($ChecksumLines.Count -ne 1) { throw "Missing or ambiguous release checksum" }
    $ExpectedDigest = [regex]::Match($ChecksumLines[0], $ChecksumPattern).Groups[1].Value
    $ActualDigest = (Get-FileHash -Path $TmpZip -Algorithm SHA256).Hash
    if ($ActualDigest -ne $ExpectedDigest) { throw "Release archive SHA-256 mismatch" }
    Expand-Archive -Path $TmpZip -DestinationPath $InstallDir -Force
    Remove-Item $TmpZip -Force -ErrorAction SilentlyContinue
    $Downloaded = $true
    Write-Host "  ✓ Installed prebuilt binary to $ExePath" -ForegroundColor Green
} catch {
    Write-Host "  ! Prebuilt Windows archive not found at release URL; falling back to cargo build from source..." -ForegroundColor Yellow
}

if (-not $Downloaded) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Host "  ◈ Rust toolchain not found. Installing rustup..." -ForegroundColor Cyan
        $RustupInit = Join-Path $env:TEMP "rustup-init.exe"
        Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $RustupInit -UseBasicParsing
        & $RustupInit -y
        if ($LASTEXITCODE -ne 0) { throw "Native installation command failed (exit $LASTEXITCODE)" }
        $env:PATH = "$HOME\.cargo\bin;$env:PATH"
    }
    Write-Host "  ◈ Building lmem from $Repo ($ReleaseTag)..." -ForegroundColor Cyan
    & cargo install --git "https://github.com/$Repo.git" --tag $ReleaseTag --locked --force
    if ($LASTEXITCODE -ne 0) { throw "Native installation command failed (exit $LASTEXITCODE)" }
    $CargoBin = Join-Path $HOME ".cargo\bin\lmem.exe"
    if (-not (Test-Path $CargoBin)) { throw "cargo succeeded but lmem.exe was not found" }
    Copy-Item $CargoBin $ExePath -Force
}

if (-not (Test-Path $ExePath)) { throw "Installed lmem.exe was not found" }
$ExpectedVersion = $ReleaseTag -replace '^v', ''
$InstalledVersion = & $ExePath --version
if ($LASTEXITCODE -ne 0) { throw "lmem --version failed (exit $LASTEXITCODE)" }
if (($InstalledVersion | Out-String).Trim() -ne "lmem $ExpectedVersion") {
    throw "Installed version mismatch: expected lmem $ExpectedVersion; got $InstalledVersion"
}

# Add InstallDir to User PATH if not already present
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($null -eq $UserPath) { $UserPath = "" }
if ($UserPath -notlike "*$InstallDir*") {
    $NewPath = if ($UserPath.EndsWith(";") -or $UserPath.Length -eq 0) { "$UserPath$InstallDir" } else { "$UserPath;$InstallDir" }
    [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
    $env:PATH = "$InstallDir;$env:PATH"
    Write-Host "  ✓ Added $InstallDir to your User PATH" -ForegroundColor Green
}

# Use the same defaults and short prompts as lmem setup.
$SkipModelsFlag = $SkipModels -or ($env:LIGHTMEM_SKIP_MODELS -eq "1") -or ($env:LIGHTMEM_NO_MODELS -eq "1") -or ($env:LIGHTMEM_MODEL -in @("none", "skip", "5"))
$LlmChoice = $env:LIGHTMEM_LLM
if (-not [string]::IsNullOrWhiteSpace($LlmChoice)) {
    $SetupPreset = switch ($LlmChoice) {
        { $_ -in @("1", "ollama", "ollama-local") } { "ollama-local" }
        { $_ -in @("2", "cloud", "ollama-cloud") } { "ollama-cloud" }
        { $_ -in @("4", "offline", "local") } { "local" }
        { $_ -in @("demo", "hash") } { "demo" }
        { $_ -in @("3", "custom") } { "" }
        default { throw "Unknown LIGHTMEM_LLM choice. Run lmem setup." }
    }
    if ($SetupPreset) {
        & $ExePath setup --preset $SetupPreset --entity-extraction off --yes
        if ($LASTEXITCODE -ne 0) { throw "LightMem setup failed (exit $LASTEXITCODE)" }
    } else {
        Write-Host "Custom model setup: run lmem setup and choose LLM answers."
    }
} elseif (-not $SkipModelsFlag -and -not $env:LIGHTMEM_MODEL -and [Environment]::UserInteractive -and -not [Console]::IsInputRedirected) {
    & $ExePath setup
    if ($LASTEXITCODE -ne 0) { Write-Host "Setup was not completed. Run lmem setup when ready." }
} else {
    Write-Host "Configure when ready: lmem setup (default: local search and answers)."
}

if ($SkipModelsFlag) {
    Write-Host "Local model downloads skipped. Prepare later: lmem config --download all"
} elseif ($env:LIGHTMEM_MODEL) {
    $EmbeddingChoice = switch ($env:LIGHTMEM_MODEL) {
        { $_ -in @("1", "bge-small", "4", "all") } { "bge-small" }
        { $_ -in @("2", "minilm") } { "minilm" }
        { $_ -in @("3", "nomic") } { "nomic" }
        default { throw "Unknown LIGHTMEM_MODEL choice." }
    }
    & $ExePath config --backend onnx --onnx-model $EmbeddingChoice --yes
    if ($LASTEXITCODE -ne 0) { throw "Model configuration failed (exit $LASTEXITCODE)" }
    if ($env:LIGHTMEM_MODEL -in @("4", "all")) {
        & $ExePath config --download all
        if ($LASTEXITCODE -ne 0) { throw "Model download failed (exit $LASTEXITCODE)" }
    } else {
        & $ExePath config --download $EmbeddingChoice
        if ($LASTEXITCODE -ne 0) { throw "Embedding download failed (exit $LASTEXITCODE)" }
        & $ExePath config --download minilm-squad2
        if ($LASTEXITCODE -ne 0) { throw "Answer-model download failed (exit $LASTEXITCODE)" }
    }
}

# Deploy LightMem skill to detected AI agent platforms
$PrevErrPref = $ErrorActionPreference
$ErrorActionPreference = "Continue"
& $ExePath connect | Out-Null
if ($LASTEXITCODE -ne 0) { throw "Native installation command failed (exit $LASTEXITCODE)" }
$ErrorActionPreference = $PrevErrPref
Write-Host "  ✓ Deployed LightMem skill to detected AI agents" -ForegroundColor Green

# Optional MCP Server configuration
$McpPrompt = Read-Host "  ▸ Configure Model Context Protocol (MCP) in AI clients (Claude Desktop, Cursor, Antigravity)? [y/N, default: N]"
if ($McpPrompt -match "^[yY]") {
    $ErrorActionPreference = "Continue"
    & $ExePath connect --mcp | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Native installation command failed (exit $LASTEXITCODE)" }
    $ErrorActionPreference = $PrevErrPref
    Write-Host "  ✓ Configured LightMem MCP in detected client configs" -ForegroundColor Green
} else {
    Write-Host "  ↷ Skipped MCP configuration (run 'lmem connect --mcp' anytime to enable)" -ForegroundColor DarkGray
}

Write-Host ""
Write-Host "  ✦ LightMem ($ReleaseTag) is ready! Run: lmem" -ForegroundColor Green
Write-Host ""
