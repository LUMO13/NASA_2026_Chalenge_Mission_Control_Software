# FPECS database connection wizard — Fedora, Windows and remote PostgreSQL

## Defaults and starting mode

The program launches in **Local / Offline** mode unless a user has saved PostgreSQL settings or set `DATABASE_URL`. It does not create demo users or astronaut profiles. First launch requires manual admin creation. In **Settings → Storage & Connection**, or in the first-run wizard, choose PostgreSQL to configure:

| Field | Default |
|---|---|
| Host | `localhost` |
| Port | `5432` |
| Database | `lunar_hms` |
| Username | `fpecs` |
| Password | Empty: enter your own secret |

The password is a masked text field; it is saved only to the desktop OS credential store, never to `database.json` or the package. Each server/user/database combination has its own credential record. Linux requires a functional Secret Service (typically GNOME Keyring); Windows uses Credential Manager. The user-specific non-secret settings are in `$XDG_CONFIG_HOME/fpecs/database.json` / `~/.config/fpecs/database.json`, or `%APPDATA%/FPECS/database.json`.

## Server setup (admin task, one time)

The wizard does **not** install or configure a server, create tables, users or roles. Provision with the original scripts:

```bash
# Run from the source checkout with a PostgreSQL admin role configured:
./scripts/06_setup_postgres.sh
# Or, after installing RPM and configuring PostgreSQL admin privileges:
fpecs-db-setup
```

For a **new** local server on Fedora, an administrator can create an application role with an interactive password prompt, then grant limited access. Example (do not execute `createuser` again if `fpecs` already exists):

```bash
sudo -u postgres createuser --login --pwprompt fpecs
sudo -u postgres psql -d lunar_hms <<'SQL'
GRANT CONNECT ON DATABASE lunar_hms TO fpecs;
GRANT USAGE ON SCHEMA public TO fpecs;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO fpecs;
GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO fpecs;
SQL
```

If the role already exists, set a new password with `sudo -u postgres psql -c '\password fpecs'`. Check your `pg_hba.conf` for password-based localhost TCP access (e.g. SCRAM); do not weaken authentication. The above assumes the administrator owns the tables. If the SQL schema is owned by a different role, the owner must grant access.

For Windows, use the installed `FPECS_DB_Setup.ps1` with PostgreSQL client utilities, or `scripts/06_setup_postgres.ps1` in source checkout. Create the application login role and privileges using an admin SQL console, without publishing the chosen password.

## Connect through the GUI

1. Start FPECS and open **Settings → Database / Connection Wizard**. Before the first admin is created, the connection wizard is visible on the welcome screen.
2. Select **PostgreSQL**, enter connection values, then fill in a password (masked).
3. Click **Перевірити підключення**. The test runs in a separate thread; it performs only `SELECT`, validates required tables and displays a result. It does **not** test INSERT/UPDATE privileges.
4. Click **Зберегти конфігурацію** and restart FPECS. If switching with existing users, you must confirm that you have a backup; the application never silently merges the two stores.
5. Verify the top bar says **POSTGRESQL**, create a user/astronaut, restart and check persistence. If it shows **LOCAL MODE** or an error, the database has not been activated.

If `DATABASE_URL` is set in the process environment it overrides wizard preferences. Unset it before testing GUI settings. The credentials in `DATABASE_URL` are not secured by the wizard.

## Remote PostgreSQL safety

Remote hosts require TLS `VerifyFull` and a local trusted CA certificate path, configured via the wizard. Verify the correct hostname and do not expose PostgreSQL directly to the public internet. Use a VPN or a trusted private network and narrow firewall rules. Avoid connecting from shared or untrusted PCs.

## Data migration and backups

Switching storage does **not** migrate records. Existing local state remains in a private JSON file, and the PostgreSQL schema is not wiped. Back up all user state before changing backends. Creating a fresh admin on a separate, empty database is expected. Do not upload user data, `.env`, database dumps, DB passwords or the private `state.json` to a public source-code ZIP.

## Failure and diagnostics

- `role "fpecs" does not exist`: create a PostgreSQL application role (admin step).
- `password authentication failed`: re-enter the role's password and check the server auth policy.
- `connection refused`: start PostgreSQL / check host, port and firewall.
- `relation fpecs_runtime_state does not exist`: apply migrations 001 and 002.
- `permission denied`: grant necessary table privileges to the application role.
- `Secret Service unavailable`: unlock/setup GNOME Keyring or check the Windows Credential Manager; the app can still run in Local / Offline mode.

**Limitations:** This is synthetic mission telemetry, not clinical, flight-certified or a real NASA Earth communications link. DB test is read-only; operational multi-user concurrency and write privileges require actual post-install testing.
