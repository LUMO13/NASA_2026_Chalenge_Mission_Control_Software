# FPECS Mission Control UI + PostgreSQL wizard — validation report

Date: 2026-11-16

## Completed in the artifact-preparation environment

- Examined the existing FPECS source tree and its GUI, persistent storage, RPM and Windows packaging.
- Edited the existing Rust workspace in place; did not regenerate the project, seed accounts, or remove schema files.
- Ran `bash -n` on all Linux scripts and the RPM database helper: passed.
- Ran `scripts/01_create_project.sh`, `scripts/02_generate_source.sh` and `scripts/05_prepare_github.sh`: structural checks passed.
- Parsed the Rust workspace and desktop package manifests as TOML: passed.
- Ran a lexical Rust delimiter check on changed UI and database code: passed. This is **not** a Rust compiler check.
- Verified SHA-256 identity of the existing database migrations, core models and access-control implementation against the starting archive: unchanged.
- Generated and inspected the presence of application icon assets (PNG/SVG/ICO) and their installer references.
- Checked ZIP archive structural integrity after packaging.

## Not testable here

No Rust/Cargo compiler, running desktop UI, PostgreSQL server, Fedora rpmbuild, Windows/MSVC toolchain or Inno Setup were available in the file-generation environment. Therefore compilation, installation and runtime connection tests are **not claimed**.

## Required tests on Fedora

From the extracted `lunar-hms` directory:

```bash
sudo dnf install dbus-devel pkgconf-pkg-config
cargo fmt --all
./scripts/03_build_and_test.sh
cargo run -p desktop-gui
```

1. Start with the default local store, create a fresh admin, exit and reopen. Verify persistence.
2. Open Settings > Storage & Connection. Check PostgreSQL defaults `localhost:5432`, `lunar_hms`, `fpecs`.
3. On a *test* PostgreSQL database with schema, create a restricted app role and enter its password. Test connection; observe success, then save and **restart**.
4. Verify POSTGRESQL badge in the running app and persistence after restart. Make sure no password is present in `~/.config/fpecs/database.json`.
5. Test invalid credentials / unreachable server / missing schema; the UI must show an error and offline mode, not a false success.
6. Verify existing user/crew permissions, simulator, alerts and plotted medical data.
7. Build RPM with `./scripts/04_build_release.sh`, install it on a clean Fedora user profile and test GNOME launch icon and clean first-run state.
8. Build Windows EXE with `scripts/04_build_release.ps1` on Windows; test Windows Credential Manager and optional Inno Setup installer.

## Security boundaries

- Connection tests are read-only. `Test connection` does not verify INSERT/UPDATE privilege.
- Local and PostgreSQL data are **not** migrated when switching stores.
- SQL migrations are optional administrator actions and do not create default astronaut accounts.
- External `DATABASE_URL` takes precedence over GUI settings, which the app warns about.
- Medical access control remains enforced; program data are **simulated**, not flight/medical-grade.
