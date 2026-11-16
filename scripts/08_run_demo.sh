#!/usr/bin/env bash
# Launch existing Rust GUI in place. Does not overwrite source or create fake users.
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/_common.sh"
verify_root
need cargo
cd "$FPECS_ROOT"
info 'Starting FPECS Mission Control (synthetic demo; no real astronaut data)'
info 'Optional PostgreSQL: set DATABASE_URL and run 06_setup_postgres.sh first.'
exec cargo run -p desktop-gui
