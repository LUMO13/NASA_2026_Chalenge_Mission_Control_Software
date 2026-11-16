# Launch existing FPECS desktop-GUI with Cargo. Run from any directory.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Rust Cargo not installed.' }
Push-Location $root
try {
    Write-Host '[FPECS] Starting synthetic mission demo. Set DATABASE_URL for PostgreSQL persistence.'
    & cargo run -p desktop-gui
    if ($LASTEXITCODE -ne 0) { throw 'cargo run failed' }
} finally { Pop-Location }
