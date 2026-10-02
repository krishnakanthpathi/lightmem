<#
.SYNOPSIS
    Build LightMem native DLL, import library, and .NET bindings on Windows.
#>

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$WindowsDir = Split-Path -Parent $ScriptDir
$RepoRoot = Split-Path -Parent (Split-Path -Parent $WindowsDir)

Write-Host "=== Building LightMem FFI for Windows (MSVC) ===" -ForegroundColor Cyan

Set-Location $RepoRoot
cargo build --release -p lightmem-ffi

$BinDir = Join-Path $WindowsDir "bin"
$LibDir = Join-Path $WindowsDir "lib"
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
New-Item -ItemType Directory -Force -Path $LibDir | Out-Null

$BuiltDll = Join-Path $RepoRoot "target\release\lightmem_ffi.dll"
$BuiltLib = Join-Path $RepoRoot "target\release\lightmem_ffi.dll.lib"

if (Test-Path $BuiltDll) {
    Copy-Item $BuiltDll (Join-Path $BinDir "lightmem.dll") -Force
    Write-Host "✔ Copied native DLL to: $BinDir\lightmem.dll" -ForegroundColor Green
}

if (Test-Path $BuiltLib) {
    Copy-Item $BuiltLib (Join-Path $LibDir "lightmem.lib") -Force
    Write-Host "✔ Copied import library to: $LibDir\lightmem.lib" -ForegroundColor Green
}

# Build .NET Library if dotnet is available
if (Get-Command dotnet -ErrorAction SilentlyContinue) {
    Write-Host "=== Building .NET SDK Library ===" -ForegroundColor Cyan
    $CsProj = Join-Path $WindowsDir "dotnet\LightMem.csproj"
    dotnet build $CsProj -c Release
    Write-Host "✔ .NET SDK library built successfully." -ForegroundColor Green
}

Write-Host "✔ Windows build completed successfully!" -ForegroundColor Green
