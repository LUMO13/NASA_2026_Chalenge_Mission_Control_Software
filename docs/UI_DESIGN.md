# FPECS Mission Control — GUI visual system

The existing native Rust/egui desktop program now uses a consistent dark command-deck visual language throughout all 11 views. The app is a **functional desktop application**, not a static screenshot or web mockup.

## Layout

- **Top command bar**: FPECS brand, synthetic-mission warning, live/paused indicator, active storage backend and signed-in account.
- **Navigation rail**: grouped Mission Overview / Live Systems / Administration links with highlighted active page.
- **Main dashboard**: a mission-context banner, key system metrics, permission-gated heart-rate trend and crew-status cards.
- **Telemetry**: consistent information cards and color-coded history charts for heart rate, oxygen saturation, temperature and pressure.
- **Other screens**: Alerts, Sensor Diagnostics, Habitat, Radiation, Access, Simulator, Earth Link, Settings and Audit keep the same visual components and page headings.
- **Setup**: first-run admin creation plus the optional database wizard, preserving the fact that a clean install has no pre-created accounts.

## Palette

| Name | RGB | Use |
|---|---|---|
| Midnight | `#080f1c` | Content background |
| Navigation | `#0a1524` | Header & sidebar |
| Card | `#111f31` | Panel surfaces |
| Panel accent | `#162a3e` | Metrics, active navigation |
| Cyan | `#67ebe6` | Primary highlight, data labels |
| Green | `#70eab5` | Healthy/connected status |
| Amber | `#fac56e` | Demo warnings, paused status |
| Red | `#ff7c96` | Errors/alerts |
| Text | `#eaf4fc` | Main content |

## Accessibility and integrity

All telemetry is visibly labeled **simulated**. Medical readings remain role-gated; the dashboard trend does not bypass the access policy. The backend status in the top bar is the *actual active* backend, not merely the backend selected in Settings. Storage changes apply after restart and do not delete old records.

The PostgreSQL connection form masks the password and writes non-secret settings to a private JSON file; passwords go to the system keyring. Database checks are read-only and performed off the UI thread. The project does not claim clinical certification, real astronaut telemetry, or a real NASA DSN connection.

## Source files

- `crates/desktop-gui/src/main.rs` — theme, layout, all pages, connection wizard UI.
- `crates/desktop-gui/src/db_config.rs` — preferences and secret-store credentials.
- `crates/desktop-gui/src/persistence.rs` — local snapshots and PostgreSQL connection tests/runtime.
- `packaging/assets/` — app icon PNG/SVG/ICO.
- `docs/DATABASE_WIZARD.md` — setup details and access permissions.
