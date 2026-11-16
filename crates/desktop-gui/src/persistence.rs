//! Persistent storage for a simulated mission, with no hardcoded database secrets.
//! PostgreSQL is opt-in through DATABASE_URL; otherwise a user-local JSON file is used.
use std::{env, fs, io::Write, path::{Path, PathBuf}, time::Duration};

use desktop_gui::{FpecsSnapshot, MissionControlAppState};
use sqlx::{postgres::{PgConnectOptions, PgPoolOptions, PgSslMode}, PgPool, Row};
use crate::db_config::{DatabaseConfig, DatabaseMode, load_password};
use tokio::runtime::{Builder, Runtime};

pub enum StorageBackend {
    Local(PathBuf),
    Postgres { runtime: Runtime, pool: PgPool },
}

pub struct Persistence {
    backend: StorageBackend,
    pub last_error: Option<String>,
    pub description: String,
}

fn local_path() -> PathBuf {
    if let Some(custom) = env::var_os("FPECS_STATE_FILE") {
        return PathBuf::from(custom);
    }
    if let Some(xdg) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("fpecs").join("state.json");
    }
    if let Some(appdata) = env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("FPECS").join("state.json");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/fpecs/state.json");
    }
    PathBuf::from("fpecs_state.json")
}

fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("create storage directory: {e}"))?;
    }
    let temp = path.with_extension("json.tmp");
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}

/// Build SQLx options without embedding credentials in URLs or process arguments.
fn connection_options(config: &DatabaseConfig, password: &str) -> Result<PgConnectOptions, String> {
    config.validate()?;
    let mut options = PgConnectOptions::new_without_pgpass()
        .host(config.host.trim())
        .port(config.port)
        .database(config.database.trim())
        .username(config.username.trim())
        .password(password)
        .application_name("FPECS Mission Control");
    if config.is_loopback() {
        // Localhost is never routed over the public Internet; TLS is optional here.
        options = options.ssl_mode(PgSslMode::Prefer);
    } else {
        // Never silently downgrade a remote connection to plaintext or unverified TLS.
        options = options.ssl_mode(PgSslMode::VerifyFull)
            .ssl_root_cert(config.ca_cert_path.trim());
    }
    Ok(options)
}

fn sanitize_error(error: impl std::fmt::Display, password: &str) -> String {
    let message = error.to_string();
    if password.is_empty() { message } else { message.replace(password, "[REDACTED]") }
}

fn connect_snapshot(options: PgConnectOptions, password: &str) -> Result<(Runtime, PgPool, Option<FpecsSnapshot>), String> {
    let runtime = Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let (pool, saved) = runtime.block_on(async {
        let pool = PgPoolOptions::new().max_connections(2)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options).await.map_err(|e| sanitize_error(e, password))?;
        // Read only: a missing schema must be fixed by running 06_setup_postgres,
        // not by creating undocumented tables silently during login.
        let row = sqlx::query("SELECT payload FROM fpecs_runtime_state WHERE id=1")
            .fetch_optional(&pool).await.map_err(|e| sanitize_error(e, password))?;
        let saved = match row {
            Some(row) => {
                let payload: serde_json::Value = row.try_get("payload").map_err(|e| sanitize_error(e, password))?;
                Some(serde_json::from_value(payload).map_err(|e| format!("Invalid saved snapshot: {e}"))?)
            }
            None => None,
        };
        Ok::<_, String>((pool, saved))
    })?;
    Ok((runtime, pool, saved))
}

/// Read-only connection test: checks credentials and the schema required by the app.
/// Never creates databases, tables, users, or test records.
pub fn test_postgres(config: &DatabaseConfig, password: &str) -> Result<String, String> {
    let runtime = Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let options = connection_options(config, password)?;
    runtime.block_on(async {
        let pool = PgPoolOptions::new().max_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options).await.map_err(|e| sanitize_error(e, password))?;
        sqlx::query("SELECT 1").execute(&pool).await.map_err(|e| sanitize_error(e, password))?;
        for table in ["fpecs_runtime_state", "astronauts", "users", "roles_permissions", "telemetry"] {
            let found: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
                .bind(format!("public.{table}"))
                .fetch_one(&pool).await.map_err(|e| sanitize_error(e, password))?;
            if !found {
                return Err(format!("Не знайдено таблицю {table}. Спочатку застосуйте migrations/001 та 002 через scripts/06_setup_postgres."));
            }
        }
        sqlx::query("SELECT payload FROM fpecs_runtime_state WHERE id=1")
            .fetch_optional(&pool).await.map_err(|e| sanitize_error(e, password))?;
        pool.close().await;
        Ok(format!("Підключення й читання схеми успішні: {}:{}/{}. Права запису ще не перевірені.",
            config.host, config.port, config.database))
    })
}

impl Persistence {
    pub fn initialize(state: &mut MissionControlAppState) -> Self {
        // Explicit environment configuration remains supported for scripts and automation.
        let from_env = env::var("DATABASE_URL").ok().filter(|url| !url.trim().is_empty());
        let configured = if let Some(url) = from_env {
            match url.parse::<PgConnectOptions>() {
                Ok(options) => Some((options, String::new(), "PostgreSQL / DATABASE_URL".to_string())),
                Err(_) => {
                    let mut local = Self::local(state);
                    local.last_error = Some("DATABASE_URL has an invalid format. Running with local storage.".into());
                    return local;
                }
            }
        } else {
            match DatabaseConfig::load() {
                Ok(config) if config.mode == DatabaseMode::Postgres => {
                    match load_password(&config) {
                        Ok(secret) => match connection_options(&config, secret.as_deref().unwrap_or("")) {
                            Ok(options) => Some((options, secret.unwrap_or_default(),
                                format!("PostgreSQL / {}:{}/{}",config.host, config.port, config.database))),
                            Err(e) => {
                                let mut local = Self::local(state);
                                local.last_error = Some(format!("Database settings invalid: {e}. Local storage in use."));
                                return local;
                            }
                        },
                        Err(e) => {
                            let mut local = Self::local(state);
                            local.last_error = Some(format!("{e} Локальне сховище активне; пароль можна переввести у майстрі."));
                            return local;
                        }
                    }
                }
                Ok(_) => None,
                Err(e) => {
                    let mut local = Self::local(state);
                    local.last_error = Some(format!("{e}. Локальне сховище активне."));
                    return local;
                }
            }
        };
        if let Some((options, password, description)) = configured {
            match connect_snapshot(options, &password) {
                Ok((runtime, pool, saved)) => {
                    let mut last_error = None;
                    if let Some(snapshot) = saved {
                        if let Err(e) = state.restore_snapshot(snapshot) {
                            last_error = Some(format!("Restoring database snapshot failed: {e}"));
                        }
                    }
                    state.database.set_postgres_online(true);
                    state.status_banner = "PostgreSQL підключено. Дані зберігаються в базі.".into();
                    return Self { backend: StorageBackend::Postgres { runtime, pool }, last_error, description };
                }
                Err(e) => {
                    let mut fallback = Self::local(state);
                    fallback.last_error = Some(format!("PostgreSQL unavailable ({e}); local storage in use. Check Settings → Database."));
                    return fallback;
                }
            }
        }
        Self::local(state)
    }

    fn local(state: &mut MissionControlAppState) -> Self {
        let path = local_path();
        let mut error = None;
        if path.exists() {
            match fs::read(&path).map_err(|e| e.to_string())
                .and_then(|b| serde_json::from_slice::<FpecsSnapshot>(&b).map_err(|e| e.to_string()))
                .and_then(|s| state.restore_snapshot(s).map_err(|e| e.to_string()))
            {
                Ok(_) => state.status_banner = "Відновлено локальні дані. Увійдіть у свій обліковий запис.".into(),
                Err(e) => error = Some(format!("Local snapshot not loaded: {e}")),
            }
        }
        Self { description: format!("Local ({}); PostgreSQL optional", path.display()), backend: StorageBackend::Local(path), last_error: error }
    }

    pub fn is_postgres(&self) -> bool {
        matches!(self.backend, StorageBackend::Postgres { .. })
    }

    pub fn save(&mut self, state: &MissionControlAppState) -> Result<(), String> {
        let snapshot = state.export_snapshot();
        let data = serde_json::to_value(&snapshot).map_err(|e| e.to_string())?;
        let result = match &mut self.backend {
            StorageBackend::Local(path) => {
                let pretty = serde_json::to_vec_pretty(&snapshot).map_err(|e| e.to_string())?;
                write_private_atomic(path, &pretty)
            }
            StorageBackend::Postgres { runtime, pool } => runtime.block_on(async {
                let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
                // Write domain records to original normalized schema, without deleting historical rows.
                for astronaut in snapshot.database.astronauts.iter() {
                    sqlx::query("INSERT INTO astronauts (astronaut_id,full_name,mission_role,callsign,height_cm,baseline_mass_kg,assigned_suit_id,is_active,created_at_utc) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (astronaut_id) DO NOTHING")
                        .bind(astronaut.astronaut_id()).bind(&astronaut.full_name).bind(&astronaut.mission_role)
                        .bind(&astronaut.callsign).bind(astronaut.height_cm).bind(astronaut.baseline_mass_kg)
                        .bind(&astronaut.assigned_suit_id).bind(astronaut.is_active).bind(astronaut.created_at_utc)
                        .execute(&mut *tx).await.map_err(|e| format!("astronauts: {e}"))?;
                }
                for user in snapshot.users.iter() {
                    let role_code = user.role.as_str();
                    sqlx::query("INSERT INTO roles_permissions (role_code,display_name,description) VALUES ($1,$1,'FPECS role-based access') ON CONFLICT (role_code) DO NOTHING")
                        .bind(role_code).execute(&mut *tx).await.map_err(|e| format!("roles: {e}"))?;
                    sqlx::query("INSERT INTO users (user_id,username,password_hash_argon2id,role_code,linked_astronaut_id,is_locked,failed_login_attempts,locked_until_utc,last_login_utc) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (user_id) DO UPDATE SET password_hash_argon2id=EXCLUDED.password_hash_argon2id,role_code=EXCLUDED.role_code,linked_astronaut_id=EXCLUDED.linked_astronaut_id,is_locked=EXCLUDED.is_locked,failed_login_attempts=EXCLUDED.failed_login_attempts,locked_until_utc=EXCLUDED.locked_until_utc,last_login_utc=EXCLUDED.last_login_utc")
                        .bind(user.user_id).bind(&user.username).bind(&user.password_hash_argon2id)
                        .bind(role_code).bind(user.linked_astronaut_id.as_deref()).bind(user.is_locked)
                        .bind(user.failed_attempts as i32).bind(user.locked_until_utc.clone()).bind(user.last_login_utc.clone())
                        .execute(&mut *tx).await.map_err(|e| format!("users: {e}"))?;
                }
                for record in snapshot.database.telemetry.iter() {
                    let hr = record.cardiovascular.heart_rate_bpm.as_ref().map(|m| m.value);
                    let spo2 = record.respiratory.spo2_pct.as_ref().map(|m| m.value);
                    let temp = record.physiological.core_body_temp_c.as_ref().map(|m| m.value);
                    sqlx::query("INSERT INTO telemetry (record_id,sequence_nonce,astronaut_id,measured_at_utc,simulation_scenario,heart_rate_bpm,spo2_pct,core_body_temp_c,hmac_sha256_signature) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
                        .bind(record.record_id).bind(record.sequence_nonce as i64).bind(&record.astronaut_id)
                        .bind(record.measured_at_utc).bind(record.scenario.as_code()).bind(hr).bind(spo2).bind(temp)
                        .bind(&record.hmac_sha256_signature).execute(&mut *tx).await.map_err(|e| format!("telemetry: {e}"))?;
                }
                sqlx::query("INSERT INTO fpecs_runtime_state (id,payload,updated_at_utc) VALUES (1,$1,now()) ON CONFLICT (id) DO UPDATE SET payload=EXCLUDED.payload,updated_at_utc=now()")
                    .bind(data).execute(&mut *tx).await.map_err(|e| format!("snapshot: {e}"))?;
                tx.commit().await.map_err(|e| e.to_string())?;
                Ok::<(), String>(())
            }),
        };
        self.last_error = result.as_ref().err().cloned();
        result
    }
}
