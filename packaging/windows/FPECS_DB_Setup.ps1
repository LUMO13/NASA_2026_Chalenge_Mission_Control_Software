# Optional Windows PostgreSQL schema setup helper, for an already running server.
# Runs only with privileges of the PostgreSQL role configured in PGUSER/PGPASSWORD.
param([switch]$DryRun)
$ErrorActionPreference = 'Stop'
$name = if ($env:FPECS_DB_NAME) { $env:FPECS_DB_NAME } else { 'lunar_hms' }
if ($name -cnotmatch '^[a-z_][a-z0-9_]*$') { throw 'Invalid database name' }
$schemaDir = Join-Path $PSScriptRoot 'migrations'
$initial = Join-Path $schemaDir '001_initial_schema.sql'
$runtime = Join-Path $schemaDir '002_runtime_snapshot.sql'
if (-not ((Test-Path $initial) -and (Test-Path $runtime))) { throw 'Missing SQL files' }
if ($DryRun) { Write-Host "[FPECS] DRY RUN: $name migrations 001, 002; no demo data"; exit }
foreach ($cmd in @('psql','createdb')) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) { throw "Missing $cmd (PostgreSQL command-line tools)" }
}
$exists = (& psql -X -v ON_ERROR_STOP=1 -d postgres -Atqc "SELECT 1 FROM pg_database WHERE datname='$name'")
if ($LASTEXITCODE -ne 0) { throw 'Connection or permissions failed' }
if (($exists | Out-String).Trim() -ne '1') {
    & createdb $name
    if ($LASTEXITCODE -ne 0) { throw 'Database creation failed' }
}
& psql -X -v ON_ERROR_STOP=1 -d $name -f $initial
if ($LASTEXITCODE -ne 0) { throw 'Initial schema failed' }
& psql -X -v ON_ERROR_STOP=1 -d $name -f $runtime
if ($LASTEXITCODE -ne 0) { throw 'Runtime schema failed' }
Write-Host '[FPECS] Database schema prepared. Set application role permissions, then configure in the GUI.'
