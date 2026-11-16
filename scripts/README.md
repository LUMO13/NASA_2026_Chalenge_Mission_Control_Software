# FPECS automation: `scripts/` (restored and made non-destructive)

The directory is named **`scripts/`** (plural) because that is the original project path referenced by the CI workflow. Run scripts from either the project root or `scripts/`; each script derives the project root from its own location. Commands never reset the project or overwrite Rust source, TOML settings, SQL schema or documentation.

## Linux / Fedora

```bash
./scripts/01_create_project.sh            # validate structure; does NOT create another project
./scripts/02_generate_source.sh           # validate source; does NOT regenerate code
./scripts/03_build_and_test.sh           # cargo fmt --check, check, clippy, test, build
./scripts/04_build_release.sh            # native Linux release binaries; RPM if rpmbuild installed
./scripts/05_prepare_github.sh           # submission validation, no git push, no deletion
./scripts/06_setup_postgres.sh --dry-run # preview optional DB creation & schema apply
./scripts/06_setup_postgres.sh           # create DB if absent + apply 001 schema
./scripts/07_package_submission.sh       # make a sanitized public source ZIP in dist/
./scripts/08_run_demo.sh                  # trial launch desktop GUI
```

Fedora setup for packaging: `sudo dnf install rpm-build`. A Rust toolchain and platform GUI libraries are needed before compiling. RPMs are **built on x86_64 Linux**, and require runtime testing on the intended distribution. If `rpmbuild` is not installed, the script reports a skipped RPM rather than claiming it was created.

## Windows (PowerShell)

```powershell
.\scripts\03_build_and_test.ps1
.\scripts\04_build_release.ps1
.\scripts\06_setup_postgres.ps1 -DryRun
.\scripts\06_setup_postgres.ps1
.\scripts\08_run_demo.ps1
```

The Windows release script creates **native `.exe` programs** using a Windows MSVC Rust toolchain. If **Inno Setup 6** (`ISCC.exe`) is also installed, it attempts a graphical `FPECS_Setup_..._x64.exe` installer. Without Inno Setup, only the raw executable files are produced. A Fedora/Linux shell alone cannot reliably create an MSVC Windows installer.

## PostgreSQL connection

Use the new graphical **Settings → Storage & Connection** wizard to avoid embedding passwords in command lines or scripts. Provision the server/role first; see `docs/DATABASE_WIZARD.md`. The wizard checks schema access in the background, securely saves credentials in GNOME Keyring/Windows Credential Manager, and applies backend changes after restarting FPECS. Local storage remains the default.


`06_setup_postgres` provisions both `migrations/001_initial_schema.sql` and `002_runtime_snapshot.sql`. It does **not** create pre-made users, astronauts, or demo passwords. The desktop runtime connects using `DATABASE_URL=postgresql://USER:PASSWORD@HOST:PORT/lunar_hms`; without this variable it writes a private local snapshot for the current operating-system user. A database connection failure triggers an explicit GUI warning and offline storage. Avoid committing connection URLs with credentials. The original normalized tables receive profile/account/key telemetry fields; the snapshot holds complete serialized frame data.

```bash
./scripts/06_setup_postgres.sh --dry-run
./scripts/06_setup_postgres.sh
./scripts/08_run_demo.sh
```

See the top-level `README.md` for setup details and the remaining prototype limitations.

## Verification status

The automation was inspected and Bash scripts were syntax-checked in the ZIP preparation environment. Actual Cargo builds, PostgreSQL actions, RPM assembly and Windows Installer compilation must be run on appropriate machines with the required tools installed. Do not present an untested build as successful.



## 2026-11-16 — FPECS persistent Mission Control update

The **current code** provides a dark NASA-style desktop UI, 11 non-placeholder views, automatically scheduled simulated readings, explicit administrator and crew/account registration, local snapshot persistence, and optional PostgreSQL storage via `DATABASE_URL`. These updates were made in source; **the Rust compilation, actual PostgreSQL connection, RPM and Windows installer are not verified in the packaging environment**. Do not treat this note as a successful integration test.

Start the GUI from the project root with `cargo run -p desktop-gui`, or use `scripts/08_run_demo.sh` on Fedora / `scripts/08_run_demo.ps1` on Windows. Without `DATABASE_URL`, data is stored at `$XDG_DATA_HOME/fpecs/state.json` or `$HOME/.local/share/fpecs/state.json` on Linux, and `%APPDATA%/FPECS/state.json` on Windows. Set `FPECS_STATE_FILE` to use a custom path. Local state includes **Argon2id password hashes** and synthetic telemetry: keep it private. PostgreSQL mode stores a JSONB restore snapshot and mirrors profile/account/key telemetry fields into the original normalized tables; the JSONB payload holds full frame detail. To provision, run `scripts/06_setup_postgres.sh` or the PowerShell equivalent before starting the GUI. If PostgreSQL cannot connect, an explicit warning is shown and it falls back to the local file.

**Operational boundaries:** Earth uplink remains a local **simulation**; no NASA communication link or astronaut sensor is connected. No claim of clinical/flight readiness is made. Native RPM/EXE builds remain platform-specific and must be compiled and tested on Fedora / Windows respectively. AI-assisted code and documentation must be disclosed appropriately in the submission.
