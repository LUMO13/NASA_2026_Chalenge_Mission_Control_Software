# FPECS — Mission Control

FPECS is a desktop application developed for the **NASA Space Apps Challenge 2026** hackathon. It demonstrates simulated astronaut health monitoring for a lunar mission: crew profiles, telemetry dashboards, alerts, and mission status.

This is an independent hackathon prototype, **not an official NASA application or a medical device**. All telemetry is simulated.

## Run the app

The project is written in Rust. From the repository folder:

```bash
cargo run -p desktop-gui
```

For Fedora, building may require `sudo dnf install dbus-devel pkgconf-pkg-config` for the desktop keyring integration. Build and packaging scripts for Linux and Windows are included in `scripts/`.

## Connect your own PostgreSQL database

**PostgreSQL is optional.** FPECS uses private local storage by default, so no database server is needed for a quick demo.

To use PostgreSQL instead:

1. Install/start PostgreSQL. Create a database and a login with permission to apply the schema and read/write the application data. The default connection values in FPECS are `localhost`, port `5432`, database `lunar_hms`, username `fpecs`. You can use different values for your own server.
2. Apply the two SQL files from this repository to your database (replace the example host, user and database name as needed):

   ```bash
   psql -h localhost -U fpecs -d lunar_hms -f migrations/001_initial_schema.sql
   psql -h localhost -U fpecs -d lunar_hms -f migrations/002_runtime_snapshot.sql
   ```

   Alternatively, an authorized PostgreSQL administrator can use `./scripts/06_setup_postgres.sh` to prepare a local database. The script does not create demo users.
3. Open **Settings → Storage & Connection** in FPECS, select **PostgreSQL**, enter your host, port, database, user and password, then click **Test Connection** and **Save**. Restart the application.
4. Confirm the storage status shows **PostgreSQL**. The connection test is read-only; verify that new data is actually saved before relying on it.

For a remote server, provide a trusted CA certificate for TLS verification. Passwords entered in the wizard are saved to the operating system's credential store, not the source repository. `DATABASE_URL`, if set, overrides the GUI settings. Switching from local storage does **not** automatically migrate old records.

Do not publish passwords, database dumps, user records, local `state.json`, or other private data.

## License

MIT — see [LICENSE](LICENSE).
