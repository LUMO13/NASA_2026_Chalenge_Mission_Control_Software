# PostgreSQL DB schema setup for optional FPECS desktop runtime.
# Uses PGHOST, PGPORT, PGUSER, PGPASSWORD and FPECS_DB_NAME, default lunar_hms.
param([switch]$DryRun)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$name = if ($env:FPECS_DB_NAME) { $env:FPECS_DB_NAME } else { 'lunar_hms' }
if ($name -cnotmatch '^[a-z_][a-z0-9_]*$') { throw 'Invalid FPECS_DB_NAME; use lowercase letters, digits and underscores.' }
$schema = Join-Path $root 'migrations/001_initial_schema.sql'
$runtimeSchema = Join-Path $root 'migrations/002_runtime_snapshot.sql'
if (-not ((Test-Path $schema) -and (Test-Path $runtimeSchema))) { throw 'SQL schema not found.' }
if ($DryRun) {
    Write-Host "[FPECS] DRY RUN: create DB $name (if needed), apply migrations 001 and 002. No demo users."
    Write-Host '[FPECS] Set DATABASE_URL before launching desktop-gui to enable PostgreSQL persistence.'
    exit 0
}
foreach ($commandName in @('psql','createdb')) {
    if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) { throw "Missing command: $commandName" }
}
$exists = (& psql -X -v ON_ERROR_STOP=1 -d postgres -Atqc "SELECT 1 FROM pg_database WHERE datname='$name'")
if ($LASTEXITCODE -ne 0) { throw 'Cannot query PostgreSQL, check PGHOST/PGUSER/PGPASSWORD.' }
if (($exists | Out-String).Trim() -ne '1') {
    & createdb $name
    if ($LASTEXITCODE -ne 0) { throw 'Database creation failed.' }
}
& psql -X -v ON_ERROR_STOP=1 -d $name -f $schema
if ($LASTEXITCODE -ne 0) { throw 'Applying the SQL schema failed.' }
& psql -X -v ON_ERROR_STOP=1 -d $name -f $runtimeSchema
if ($LASTEXITCODE -ne 0) { throw 'Applying the runtime migration failed.' }
Write-Host '[FPECS] Schemas applied; no demo accounts or astronauts created. Set DATABASE_URL for desktop persistence.'
