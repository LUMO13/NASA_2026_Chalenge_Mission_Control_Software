## 2026-11-16 — Mission Control visual refresh & PostgreSQL connection wizard

- Redesigned the existing egui desktop layout with mission-status header, grouped navigation rail, telemetry cards, and more consistent charts across 11 application pages.
- Kept the same Rust modules, health data processing, role boundaries, account creation and local/offline storage.
- Added PostgreSQL connection wizard with `localhost:5432 / lunar_hms / fpecs` defaults, masked password, asynchronous read-only connection validation and confirmation before changing backend.
- Stored non-secret settings in user-local config; per-server credentials in GNOME Secret Service or Windows Credential Manager; added remote TLS VerifyFull requirement.
- Bundled SQL schema helpers in RPM/Windows installer and added FPECS icon assets for desktop and installer.
- No automatic data migration or seeded accounts; startup remains local by default.
- **Code compilation, GUI launch and RPM/Windows installer building still require verification on target systems.**

## 2026-11-15 — Submission review

- Revised first-run GUI: empty crew/accounts; separate admin, astronaut and user forms.
- Documented in-memory-only database implementation and unverified build status.
- Added clear demo and submission instructions; stripped development scripts and seed SQL from public archive.

# Changelog — ASTRA-MED / Lunar HMS

Усі значущі зміни цього проєкту документуються у цьому файлі.

## [1.0.0] — 2026-11-15
### Додано
- Повноцінну 9-компонентну мікросервісну архітектуру Cargo Workspace (`shared-core`, `sensor-simulator`, `telemetry-receiver`, `telemetry-distributor`, `database`, `health-analyzer`, `access-control`, `data-stream-manager`, `desktop-gui`).
- Симулятор біомедичних датчиків, середовища станції, радіації та скафандра з генерацією морфології ЕКГ (250 Гц) та 6 демонстраційними сценаріями (`NOMINAL`, `PHYSICAL_EXERTION`, `HIGH_STRESS`, `SLEEP_DEPRIVATION`, `EMERGENCY_HYPOXIA`, `SOLAR_PARTICLE_EVENT`).
- Адаптерний отримувач телеметрії (`SensorHardwareAdapter`) із перевіркою цілісності `HMAC-SHA256`, захистом від повторного відтворення (`AntiReplayGuard`) та детектором втрати з'єднання.
- Розподільник телеметрії з налаштовуваним повзунком часового вікна накопичення, пріоритетною чергою (`CRITICAL_IMMEDIATE` випереджає звичайну телеметрію), стисненням `Gzip` та роботою без зв'язку із Землею.
- Схему бази даних PostgreSQL (`migrations/001_initial_schema.sql`, `migrations/002_seed_demo_data.sql`) із тригером незмінності `astronaut_id`, індексами за `(astronaut_id, measured_at_utc DESC)` та локальним автономним буфером SQLite WAL.
- Детермінований аналізатор здоров'я (`INFO`, `WARNING`, `CRITICAL`), аналіз лінійного тренду та планувальник синхронізації у вікнах зв'язку із забороною видалення неперевірених даних.
- Систему RBAC для 6 ролей (`Commander`, `FlightSurgeon`, `Engineer`, `CrewMember`, `MissionControl`, `Administrator`) із хешуванням паролів `Argon2id`, блокуванням облікових записів та журналом аудиту.
- Графічний інтерфейс NASA Mission Control на базі `eframe`/`egui` + `egui_plot` з 11 сторінками, 8 часовими діапазонами та інспекцією точок.
- Маніфест Flatpak (`org.nasa.spaceapps.LunarHms.yml`) та 5 скриптів повної автоматизації життєвого циклу проєкту.
