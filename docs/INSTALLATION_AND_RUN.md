# Installation and first run — FPECS / ASTRA-MED

> **Prototype:** The supplied GUI currently stores everything **in memory**, not in PostgreSQL or SQLite. Creating PostgreSQL tables will not connect them to the app. Rust build/run steps below have **not been verified** in the environment used to prepare this ZIP.

## 1. Dependencies

- Rust and Cargo installed via `rustup` (stable recommended); GUI requires a desktop session.
- Linux needs compiler/pkg-config and window-system libraries for `eframe`/`egui`. On Fedora a typical starting point is:

```bash
sudo dnf install gcc pkgconf-pkg-config libXcursor-devel libXi-devel libXinerama-devel libXrandr-devel libxkbcommon-devel wayland-devel mesa-libGL-devel
```

On Windows, install the stable Rust MSVC toolchain and the Microsoft C++ build tools. Building for Windows on Linux is **not** guaranteed by simply adding the Windows target.

## 2. Run and test

From the extracted `lunar-hms/` directory:

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo run -p desktop-gui
```

The first GUI run has **no crew accounts or profiles**. Open navigation item **7. Користувачі та екіпаж**, create a local administrator and create an astronaut. Then add a **separate** `FlightSurgeon` or linked `CrewMember` account to view medical telemetry. Enter that account on the dashboard; use `Крок симуляції` to generate synthetic data.

**Important:** restart clears astronauts, users, records and alerts because the repository is in-memory. Do not use real passwords or real medical data. The separately launched HTTP simulator is an older **demo fixture** with example astronauts and is not integrated with the GUI's local accounts.

## 3. PostgreSQL schema (optional, not wired to runtime)

If you want to inspect the proposed SQL data model and already have a PostgreSQL installation:

```bash
createdb lunar_hms
psql -d lunar_hms -f migrations/001_initial_schema.sql
```

The SQL migration will create tables and indexes. It **does not automatically store GUI telemetry**. Do not expect this step to change the desktop application's memory-only behavior. The public archive excludes `002_seed_demo_data.sql` so no example users or passwords are inserted.

## 4. Release artifacts

The workspace includes an **unverified** Flatpak manifest. The restored `scripts/` directory adds Linux/Fedora RPM and Windows EXE/installer recipes. Native build experiments may start with:

```bash
cargo build --release -p desktop-gui
```

On Fedora/Linux, run `./scripts/03_build_and_test.sh` and then `./scripts/04_build_release.sh` (RPM requires `sudo dnf install rpm-build`). On Windows PowerShell, run `.\scripts\03_build_and_test.ps1` and `.\scripts\04_build_release.ps1` (graphical installer requires Inno Setup 6). Use `scripts/06_setup_postgres.sh` or `scripts/06_setup_postgres.ps1` for an optional schema-only PostgreSQL setup.

There is **no tested `.rpm`, `.exe` installer or `.flatpak` binary inside this source ZIP**. The recipes are not evidence that builds succeeded. For script details see [`scripts/README.md`](../scripts/README.md).

## 5. Security and troubleshooting

- No default administrator username or password is provided. Pick your own for the session.
- The optional `LUNAR_HMAC_SECRET` environment variable is used by the GUI simulation. If absent, a per-run random local demonstration secret is created.
- Standalone simulator and receiver network services require an explicit shared `LUNAR_HMAC_SECRET` environment variable. They are not recommended for public network exposure without additional security work.
- If building fails, report the exact Rust/Cargo error log rather than assuming the system is operational.



## 2026-11-16 — FPECS persistent Mission Control update

The **current code** provides a dark NASA-style desktop UI, 11 non-placeholder views, automatically scheduled simulated readings, explicit administrator and crew/account registration, local snapshot persistence, and optional PostgreSQL storage via `DATABASE_URL`. These updates were made in source; **the Rust compilation, actual PostgreSQL connection, RPM and Windows installer are not verified in the packaging environment**. Do not treat this note as a successful integration test.

Start the GUI from the project root with `cargo run -p desktop-gui`, or use `scripts/08_run_demo.sh` on Fedora / `scripts/08_run_demo.ps1` on Windows. Without `DATABASE_URL`, data is stored at `$XDG_DATA_HOME/fpecs/state.json` or `$HOME/.local/share/fpecs/state.json` on Linux, and `%APPDATA%/FPECS/state.json` on Windows. Set `FPECS_STATE_FILE` to use a custom path. Local state includes **Argon2id password hashes** and synthetic telemetry: keep it private. PostgreSQL mode stores a JSONB restore snapshot and mirrors profile/account/key telemetry fields into the original normalized tables; the JSONB payload holds full frame detail. To provision, run `scripts/06_setup_postgres.sh` or the PowerShell equivalent before starting the GUI. If PostgreSQL cannot connect, an explicit warning is shown and it falls back to the local file.

**Operational boundaries:** Earth uplink remains a local **simulation**; no NASA communication link or astronaut sensor is connected. No claim of clinical/flight readiness is made. Native RPM/EXE builds remain platform-specific and must be compiled and tested on Fedora / Windows respectively. AI-assisted code and documentation must be disclosed appropriately in the submission.
