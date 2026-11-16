# On Windows (PowerShell), build native EXE files and optionally make an Inno Setup installer.
# Run: powershell -ExecutionPolicy Bypass -File .\scripts\04_build_release.ps1
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not (Test-Path (Join-Path $root 'crates/desktop-gui/Cargo.toml'))) { throw 'Not an existing FPECS workspace.' }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'cargo not found: install Rust with MSVC toolchain.' }
Push-Location $root
try {
    & cargo build --release --workspace
    if ($LASTEXITCODE -ne 0) { throw 'Cargo Windows release build failed.' }
    $output = Join-Path $root 'dist/windows-x86_64'
    New-Item -ItemType Directory -Force -Path $output | Out-Null
    foreach ($name in @('desktop-gui','sensor-simulator','telemetry-receiver','telemetry-distributor')) {
        $source = Join-Path $root "target/release/$name.exe"
        if (-not (Test-Path $source)) { throw "Missing Windows binary: $source" }
        Copy-Item -LiteralPath $source -Destination (Join-Path $output "$name.exe") -Force
    }
    Write-Host '[FPECS] Windows EXE files copied to dist/windows-x86_64/'
    $iss = Join-Path $root 'packaging/windows/FPECS.iss'
    $isccCmd = Get-Command 'ISCC.exe' -ErrorAction SilentlyContinue
    $iscc = if ($isccCmd) { $isccCmd.Source } else { $null }
    if (-not $iscc) {
        foreach ($candidate in @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe")) {
            if (Test-Path $candidate) { $iscc = $candidate; break }
        }
    }
    if ($iscc) {
        $cargoManifest = Get-Content 'Cargo.toml' -Raw
        if ($cargoManifest -notmatch '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"') {
            throw 'Cannot determine the workspace version.'
        }
        $version = $Matches[1]
        & $iscc "/DAppVersion=$version" $iss
        if ($LASTEXITCODE -ne 0) { throw 'Inno Setup installer compilation failed.' }
        Write-Host "[FPECS] Windows installer created in $output"
    } else {
        Write-Host '[FPECS] SKIP installer: Inno Setup 6 (ISCC.exe) not installed. Raw EXEs were created.'
    }
} finally { Pop-Location }
