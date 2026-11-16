# FPECS — Current implementation status (2026-11-16)

## Implemented in source

- Nine original Rust crates retained with synthetic astronaut telemetry, simulator scenarios, HMAC frame validation, rule-based alerts, access control and simulated Earth-link priority queue.
- Mission Control desktop interface: all eleven navigation screens have explicit page implementations (crew, medical graphs, alerts, sensors, habitat, radiation, management, simulation, Earth-link simulation, settings and audit).
- Dark graphite/navy appearance, cyan accents, card layout and graphical historical telemetry.
- No predefined user accounts or astronauts in desktop setup. Users are created separately from astronaut profiles; first-launch wizard creates a personalized administrator.
- Argon2id-hashed user accounts and role-based access to individual medical information. Saved state does not contain plain-text passwords or active session tokens.
- On-disk local JSON snapshot of profiles, accounts, synthetic telemetry, analyzer settings and Earth-link queue. Optional PostgreSQL runtime path reads/writes a JSONB snapshot and mirrors selected fields to the original schema tables. `DATABASE_URL` must be provided and both migrations applied.
- Existing Linux/Windows scripts retained, plus `08_run_demo` trial launcher. RPM and Windows installer packaging recipes retained.

## Not verified / not implemented as real mission infrastructure

- **Rust build and runtime are unverified** in the package-generation environment: no installed `cargo`/`rustc` or supported GUI runtime there.
- No actual PostgreSQL database connection was available for integration tests. SQL and Rust integration have been authored, not end-to-end tested.
- No actual `.rpm`, `.exe` or Flatpak was built or run; package templates remain unverified.
- No real human medical devices, flight hardware, NASA datasets, clinical validation, remote ground link or authenticated external mission control server are integrated.
- PostgreSQL captures full readings in JSONB and selected measurements in the relational `telemetry` table. It does not yet populate every specialized habitat, spacesuit and radiation column, and is not designed for production multi-writer synchronization.
- Simulation time and data are artificial, and any generated Earth acknowledgement is simulated rather than evidence of delivery to NASA or Earth.

## How to verify on Fedora

```bash
cargo fmt --all
./scripts/03_build_and_test.sh
./scripts/08_run_demo.sh
```

For PostgreSQL (optional), configure a local database and `DATABASE_URL` as described in `README.md`; repeat a trial launch to verify that astronaut profiles, accounts and records survive restarting the application. Source-only ZIPs are created by `07_package_submission.sh` and exclude `.env`, generated binaries, application state and private credentials. Any failed tests should be fixed before describing the result as a completed and tested app.
