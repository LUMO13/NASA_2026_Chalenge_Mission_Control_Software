//! User-specific, non-secret database preferences and OS credential storage.
//! Passwords are never written to the JSON config or included in log messages.
use std::{env, fs, io::Write, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "org.fpecs.mission-control.postgresql";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseMode {
    #[default]
    Local,
    Postgres,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseConfig {
    pub mode: DatabaseMode,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    /// Path to the trusted CA certificate for remote VerifyFull TLS.
    /// This is public configuration, never a private key.
    pub ca_cert_path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            mode: DatabaseMode::Local,
            host: "localhost".into(),
            port: 5432,
            database: "lunar_hms".into(),
            username: "fpecs".into(),
            ca_cert_path: String::new(),
        }
    }
}

impl DatabaseConfig {
    pub fn is_loopback(&self) -> bool {
        matches!(self.host.trim().to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]")
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.host.trim().is_empty() || self.host.len() > 253 || self.host.contains(char::is_whitespace) {
            return Err("Host/IP має бути непорожнім (без пробілів).".into());
        }
        if self.port == 0 { return Err("Порт має бути між 1 та 65535.".into()); }
        for (value, field) in [(&self.database, "База"), (&self.username, "Користувач")] {
            if value.trim().is_empty() || value.len() > 128 || value.contains(char::is_whitespace) {
                return Err(format!("{field}: вкажіть непорожнє значення без пробілів (до 128 символів)."));
            }
        }
        if !self.is_loopback() {
            if self.ca_cert_path.trim().is_empty() {
                return Err("Для віддаленого сервера вкажіть шлях до довіреного CA-сертифіката (TLS VerifyFull).".into());
            }
            if !Path::new(self.ca_cert_path.trim()).is_file() {
                return Err("CA-сертифікат не знайдено. Вкажіть наявний файл на цьому комп'ютері.".into());
            }
        }
        Ok(())
    }

    pub fn load() -> Result<Self, String> {
        let path = config_path();
        if !path.exists() { return Ok(Self::default()); }
        let bytes = fs::read(path).map_err(|e| format!("Не вдалося відкрити конфігурацію: {e}"))?;
        let config: Self = serde_json::from_slice(&bytes).map_err(|e| format!("Некоректна конфігурація БД: {e}"))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), String> {
        self.validate()?;
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let path = config_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("Не вдалося створити директорію налаштувань: {e}"))?;
        }
        let temp = path.with_extension("json.tmp");
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)] {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|e| e.to_string())?;
        file.write_all(&data).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temp, &path).map_err(|e| format!("Не вдалося зберегти конфігурацію: {e}"))?;
        Ok(())
    }
}

pub fn config_path() -> PathBuf {
    if let Some(path) = env::var_os("FPECS_DB_CONFIG_FILE") {
        return PathBuf::from(path);
    }
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("fpecs").join("database.json");
    }
    if let Some(path) = env::var_os("APPDATA") {
        return PathBuf::from(path).join("FPECS").join("database.json");
    }
    if let Some(path) = env::var_os("HOME") {
        return PathBuf::from(path).join(".config/fpecs/database.json");
    }
    PathBuf::from("fpecs_database.json")
}

fn credential_entry(config: &DatabaseConfig) -> Result<keyring::Entry, String> {
    // A credential is tied to a specific target. Never send the password saved for
    // one server/account to a different host after editing the connection form.
    let account = format!("{}@{}:{}/{}", config.username, config.host, config.port, config.database);
    keyring::Entry::new(KEYRING_SERVICE, &account)
        .map_err(|_| "Системне сховище секретів недоступне.".to_owned())
}

pub fn load_password(config: &DatabaseConfig) -> Result<Option<String>, String> {
    match credential_entry(config)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("Не вдалося прочитати пароль зі сховища секретів GNOME/Windows.".into()),
    }
}

pub fn save_password(config: &DatabaseConfig, password: &str) -> Result<(), String> {
    credential_entry(config)?.set_password(password)
        .map_err(|_| "Не вдалося зберегти пароль у системному сховищі секретів. Перевірте, чи розблоковано GNOME Keyring/Windows Credential Manager.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_local_with_requested_postgres_values() {
        let cfg = DatabaseConfig::default();
        assert_eq!(cfg.mode, DatabaseMode::Local);
        assert_eq!(cfg.host, "localhost");
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.database, "lunar_hms");
        assert_eq!(cfg.username, "fpecs");
    }

    #[test]
    fn config_never_contains_password_field() {
        let encoded = serde_json::to_string(&DatabaseConfig::default()).unwrap();
        assert!(!encoded.contains("password"));
    }

    #[test]
    fn remote_requires_ca_certificate() {
        let mut cfg = DatabaseConfig::default();
        cfg.host = "db.example.org".into();
        assert!(cfg.validate().is_err());
    }
}
