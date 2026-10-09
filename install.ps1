# LightMem (lmem) Universal Windows PowerShell Installer
# Usage: irm https://raw.githubusercontent.com/krishnakanthpathi/lightmem/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "krishnakanthpathi/lightmem"
$Branch = "main"
$ReleaseTag = if ($env:LIGHTMEM_VERSION) { $env:LIGHTMEM_VERSION } else { "v0.2.8" }
$InstallDir = Join-Path $HOME ".local\bin"
$Target = "x86_64-pc-windows-msvc"
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
        $env:PATH = "$HOME\.cargo\bin;$env:PATH"
    }
    Write-Host "  ◈ Building lmem from $Repo ($Branch)..." -ForegroundColor Cyan
    & cargo install --git "https://github.com/$Repo.git" --branch $Branch --locked --force
    $CargoBin = Join-Path $HOME ".cargo\bin\lmem.exe"
    if (Test-Path $CargoBin) {
        Copy-Item $CargoBin $ExePath -Force
    }
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

Write-Host "  ◈ Pre-caching default ONNX embedding (bge-small) and Extractive QA (minilm-squad2) models..." -ForegroundColor Cyan
$PrevErrPref = $ErrorActionPreference
$ErrorActionPreference = "Continue"
& $ExePath config --download bge-small
& $ExePath config --download minilm-squad2
$ErrorActionPreference = $PrevErrPref

# Interactive LLM Setup
$LlmChoice = if ($env:LIGHTMEM_LLM) { $env:LIGHTMEM_LLM } else { "" }
if ([string]::IsNullOrWhiteSpace($LlmChoice) -and [Environment]::UserInteractive) {
    Write-Host ""
    Write-Host "  ❖ Select LLM Engine for Memory Ingestion & QA:" -ForegroundColor Red
    Write-Host "    [1] Local Ollama (http://localhost:11434 · Auto-detects models · Recommended)"
    Write-Host "    [2] Ollama Cloud (https://ollama.com · gemma4:31b-cloud)"
    Write-Host "    [3] Custom Remote LLM (URL + Model)"
    Write-Host "    [4] Offline Local ONNX (100% offline, no LLM required)"
    Write-Host ""
    $PromptChoice = Read-Host "  ▸ Enter choice [1-4, default=1]"
    $LlmChoice = if ([string]::IsNullOrWhiteSpace($PromptChoice)) { "1" } else { $PromptChoice.Trim() }
}
if ([string]::IsNullOrWhiteSpace($LlmChoice)) { $LlmChoice = "1" }

switch -Wildcard ($LlmChoice) {
    "2" {
        $CloudModel = Read-Host "  ▸ Enter Ollama Cloud model [default: gemma4:31b-cloud]"
        if ([string]::IsNullOrWhiteSpace($CloudModel)) { $CloudModel = "gemma4:31b-cloud" }
        & $ExePath config --preset ollama-cloud --reranker "ollama:$CloudModel" --yes
        Write-Host "  ✓ Configured Ollama Cloud ($CloudModel)" -ForegroundColor Green
    }
    "3" {
        $CustomUrl = Read-Host "  ▸ Enter LLM endpoint URL [default: http://localhost:11434]"
        if ([string]::IsNullOrWhiteSpace($CustomUrl)) { $CustomUrl = "http://localhost:11434" }
        $CustomModel = Read-Host "  ▸ Enter LLM model name [default: qwen2.5:3b]"
        if ([string]::IsNullOrWhiteSpace($CustomModel)) { $CustomModel = "qwen2.5:3b" }
        & $ExePath config --url $CustomUrl --reranker "ollama:$CustomModel" --yes
        Write-Host "  ✓ Configured custom LLM ($CustomUrl · $CustomModel)" -ForegroundColor Green
    }
    "4" {
        & $ExePath config --preset local --yes
        Write-Host "  ✓ Configured 100% offline local ONNX mode" -ForegroundColor Green
    }
    Default {
        & $ExePath config --preset ollama-local --yes
        try {
            $testResp = Invoke-WebRequest -Uri "http://localhost:11434/api/tags" -TimeoutSec 2 -UseBasicParsing -ErrorAction Stop
            Write-Host "  ✓ Configured Local Ollama (connected at http://localhost:11434)" -ForegroundColor Green
        } catch {
            Write-Host "  ! Configured Local Ollama (http://localhost:11434 · start with 'ollama serve')" -ForegroundColor Yellow
        }
    }
}

Write-Host ""
Write-Host "  ✦ LightMem ($ReleaseTag) is ready! Run: lmem" -ForegroundColor Green
Write-Host ""
