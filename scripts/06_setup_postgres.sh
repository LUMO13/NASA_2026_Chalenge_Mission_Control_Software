#!/usr/bin/env bash
# PostgreSQL schema setup for the optional FPECS desktop runtime backend.
# Uses libpq PGHOST/PGPORT/PGUSER/PGPASSWORD and FPECS_DB_NAME (default lunar_hms).
# Can be rerun; never applies test/demo seed data or drops tables.
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/_common.sh"
if [[ "${1:-}" == '--help' ]]; then
  echo 'Usage: FPECS_DB_NAME=lunar_hms scripts/06_setup_postgres.sh [--dry-run]';
  echo 'Needs PostgreSQL CLI psql/createdb and permission to create DB; GUI uses it when DATABASE_URL is supplied.'; exit 0
fi
[[ "${1:-}" == '' || "${1:-}" == '--dry-run' ]] || fail 'Only --dry-run is accepted'
verify_root
DB_NAME="${FPECS_DB_NAME:-lunar_hms}"
[[ "$DB_NAME" =~ ^[a-z_][a-z0-9_]*$ ]] || fail 'FPECS_DB_NAME must contain lowercase a-z, digits and underscores, starting with a letter/underscore'
SCHEMA="${FPECS_ROOT}/migrations/001_initial_schema.sql"
RUNTIME_SCHEMA="${FPECS_ROOT}/migrations/002_runtime_snapshot.sql"
[[ -s "$SCHEMA" && -s "$RUNTIME_SCHEMA" ]] || fail 'Missing SQL migration'
if [[ "${1:-}" == '--dry-run' ]]; then
  info "DRY RUN: would check PostgreSQL database '${DB_NAME}', create if missing, then apply migrations 001 and 002"
  info 'Does not create accounts or seed profiles. To connect the desktop application set DATABASE_URL.'
  exit 0
fi
need psql
need createdb
info "Checking PostgreSQL database '${DB_NAME}'"
found="$(psql -X -v ON_ERROR_STOP=1 -d postgres -Atqc "SELECT 1 FROM pg_database WHERE datname='${DB_NAME}'")" || fail 'Could not query PostgreSQL; check PGHOST/PGUSER permissions'
if [[ "$found" != '1' ]]; then
  info "Creating database '${DB_NAME}'"
  createdb "$DB_NAME" || fail 'Database creation failed; grant CREATEDB or create database manually'
fi
info 'Applying schema (no seed accounts or astronauts)'
psql -X -v ON_ERROR_STOP=1 -d "$DB_NAME" -f "$SCHEMA"
psql -X -v ON_ERROR_STOP=1 -d "$DB_NAME" -f "$RUNTIME_SCHEMA"
info 'PostgreSQL schema prepared. Set DATABASE_URL=postgresql://USER:PASS@HOST:PORT/DB to enable runtime persistence.'
