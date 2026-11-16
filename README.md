## About FPECS

**FPECS Mission Control** is a desktop application developed for the **NASA Space Apps Challenge 2026**.

The project demonstrates astronaut health monitoring during simulated space missions. It includes health telemetry visualization, astronaut management, mission status monitoring, and data storage.

FPECS is an independent educational prototype, not an official NASA application or a certified medical device. All telemetry is simulated.

## Installation and Usage

Download the latest version from the [Releases](https://github.com/LUMO13/NASA_2026_Chalenge_Mission_Control_Software/releases) page.

For Fedora Linux, install the downloaded RPM package:

```bash
sudo dnf install ./fpecs-*.rpm
```

Launch **FPECS Mission Control** from your applications menu.

To run the application from source, install Rust and execute:

```bash
cargo run -p desktop-gui
```

## Database Setup

FPECS supports two storage modes:

- **Local Storage (default):** Works without PostgreSQL or additional configuration.
- **PostgreSQL:** Allows users to connect the application to their own database.

### Setting up PostgreSQL

The repository includes PostgreSQL setup scripts for Linux and Windows in the `scripts/` directory.

**Linux:**

```bash
./scripts/06_setup_postgres.sh
```

**Windows (PowerShell):**

```powershell
.\scripts\06_setup_postgres.ps1
```

These scripts prepare the `lunar_hms` database and apply the required SQL schema.

PostgreSQL must be installed and running, and the database user must have the necessary permissions.

### Connecting the application

Once the database is ready:

1. Open **FPECS Mission Control**.
2. Navigate to **Settings → Storage & Connection**.
3. Select **PostgreSQL**.
4. Enter your database connection details.
5. Click **Test Connection**.
6. Save the configuration and restart the application.

Default connection settings:

| Parameter | Value |
|---|---|
| Host | localhost |
| Port | 5432 |
| Database | lunar_hms |
| Username | fpecs |

The username and password must match the PostgreSQL account configured on your system.

**Note:** PostgreSQL is optional. FPECS works with local storage without requiring a database server.
