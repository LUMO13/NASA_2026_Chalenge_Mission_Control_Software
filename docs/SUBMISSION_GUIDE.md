# NASA Space Apps Challenge 2026 — what to submit

## Project Demonstration

Use the public link to the team's **presentation (maximum 7 slides, including title and introduction)** or a short demo video (maximum 30 seconds) according to the submission form instructions. The presentation link supplied by the team is:

https://drive.google.com/file/d/16noSVVcs_dxmFGuXZNDKSCadkpX1GZte/view?usp=sharing

Check that viewers can open it in a private browser window **without signing in**. We did not inspect its slide count or public permissions while repackaging the source.

## Project

Upload the updated judges' archive `FPECS_NASA_Space_Apps_2026_With_Scripts.zip` to Google Drive. Enable **Anyone with the link → Viewer**, then paste that archive's full URL in the **Project** field. Open the link while signed out to verify it works.

## Suggested short description (English)

> FPECS (ASTRA-MED) is a Rust-based prototype for simulated astronaut health monitoring during lunar missions. It contains modular components for synthetic physiological and environmental telemetry, validation, rule-based alerts, in-memory analysis, and a desktop dashboard. The project also includes a proposed PostgreSQL schema for future persistence. The current release demonstrates the software architecture and user workflows; database integration, real sensor connectivity, and remote transmission are planned rather than completed.

## Last checks

- The source archive is public and contains no passwords, real astronaut records or private keys.
- Both presentation and project links are accessible without Google account login.
- Declare the actual AI-tool use in the submission form and any generated-media/code metadata.
- Be accurate about actual NASA datasets or resources used. This archive alone **does not prove NASA-data usage**; synthetic telemetry must not be represented as a NASA dataset.
- Click **Save Progress** and then **Submit for Judging** when ready. Saving alone does not submit the project.

## Build automation included

The source ZIP includes the restored `scripts/` folder; see `scripts/README.md`. These scripts validate existing source without regenerating it, run Rust checks on machines with Cargo, prepare optional PostgreSQL schema only, and provide platform-specific RPM / Windows installer recipes. **No precompiled executable or verified installer is included.**



## 2026-11-16 — FPECS persistent Mission Control update

The **current code** provides a dark NASA-style desktop UI, 11 non-placeholder views, automatically scheduled simulated readings, explicit administrator and crew/account registration, local snapshot persistence, and optional PostgreSQL storage via `DATABASE_URL`. These updates were made in source; **the Rust compilation, actual PostgreSQL connection, RPM and Windows installer are not verified in the packaging environment**. Do not treat this note as a successful integration test.

Start the GUI from the project root with `cargo run -p desktop-gui`, or use `scripts/08_run_demo.sh` on Fedora / `scripts/08_run_demo.ps1` on Windows. Without `DATABASE_URL`, data is stored at `$XDG_DATA_HOME/fpecs/state.json` or `$HOME/.local/share/fpecs/state.json` on Linux, and `%APPDATA%/FPECS/state.json` on Windows. Set `FPECS_STATE_FILE` to use a custom path. Local state includes **Argon2id password hashes** and synthetic telemetry: keep it private. PostgreSQL mode stores a JSONB restore snapshot and mirrors profile/account/key telemetry fields into the original normalized tables; the JSONB payload holds full frame detail. To provision, run `scripts/06_setup_postgres.sh` or the PowerShell equivalent before starting the GUI. If PostgreSQL cannot connect, an explicit warning is shown and it falls back to the local file.

**Operational boundaries:** Earth uplink remains a local **simulation**; no NASA communication link or astronaut sensor is connected. No claim of clinical/flight readiness is made. Native RPM/EXE builds remain platform-specific and must be compiled and tested on Fedora / Windows respectively. AI-assisted code and documentation must be disclosed appropriately in the submission.
