//! # `access-control` — Централізована система авторизації (RBAC), Argon2id та аудиту безпеки
//!
//! Забезпечує:
//! - сувору реалізацію RBAC на серверному рівні для 6 ролей:
//!   1. `Commander` — загальний стан екіпажу, критичні сповіщення, стан системи;
//!   2. `FlightSurgeon` — повний доступ до медичних показників усіх космонавтів, історії та тенденцій;
//!   3. `Engineer` — технічні параметри датчиків, середовище, діагностика апаратури, БЕЗ доступу до приватної медичної історії;
//!   4. `CrewMember` — виключно власні медичні показники та власна історія;
//!   5. `MissionControl` — синхронізована телеметрія та загальний моніторинг місії;
//!   6. `Administrator` — управління обліковими записами, конфігурацією та дозволами, БЕЗ автоматичного доступу до медичного вмісту.
//! - хешування паролів через `Argon2id` з випадковою криптографічною сіллю;
//! - контроль невдалих спроб входу та автоматичне блокування облікових записів;
//! - обмеження частоти запитів (`RateLimiter`);
//! - повний журнал аудиту (`AuditLogEntry`).

use std::collections::HashMap;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use database::LunarDatabaseRepository;
use shared_core::{LunarHmsError, LunarResult, Role};

/// Обліковий запис користувача у системі.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccount {
    pub user_id: Uuid,
    pub username: String,
    pub password_hash_argon2id: String,
    pub role: Role,
    pub linked_astronaut_id: Option<String>,
    pub is_locked: bool,
    pub failed_attempts: u32,
    pub locked_until_utc: Option<DateTime<Utc>>,
    pub last_login_utc: Option<DateTime<Utc>>,
}

/// Активна авторизована сесія користувача.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub session_token: String,
    pub user_id: Uuid,
    pub username: String,
    pub role: Role,
    pub linked_astronaut_id: Option<String>,
    pub issued_at_utc: DateTime<Utc>,
    pub expires_at_utc: DateTime<Utc>,
}

/// Сервіс керування доступом, автентифікацією та RBAC.
pub struct AccessControlService {
    users_by_username: HashMap<String, UserAccount>,
    active_sessions: HashMap<String, AuthSession>,
    rate_limit_counters: HashMap<String, (DateTime<Utc>, u32)>,
    max_failed_attempts: u32,
    lockout_minutes: i64,
    session_ttl_minutes: i64,
    max_requests_per_minute: u32,
}

impl AccessControlService {
    pub fn new(
        max_failed_attempts: u32,
        lockout_minutes: i64,
        session_ttl_minutes: i64,
        max_requests_per_minute: u32,
    ) -> Self {
        Self {
            users_by_username: HashMap::new(),
            active_sessions: HashMap::new(),
            rate_limit_counters: HashMap::new(),
            max_failed_attempts: max_failed_attempts.max(3),
            lockout_minutes: lockout_minutes.max(1),
            session_ttl_minutes: session_ttl_minutes.max(5),
            max_requests_per_minute: max_requests_per_minute.max(10),
        }
    }

    /// Хешує пароль за допомогою алгоритму Argon2id та криптостійкої випадкової солі.
    pub fn hash_password_argon2id(plain_password: &str) -> LunarResult<String> {
        if plain_password.len() < 8 {
            return Err(LunarHmsError::ValidationFailure {
                field: "password".to_string(),
                reason: "Пароль повинен містити щонайменше 8 символів".to_string(),
            });
        }
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(plain_password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| LunarHmsError::ConfigError(format!("Помилка Argon2id: {}", e)))
    }

    /// Перевіряє відповідність пароля збереженому хешу Argon2id.
    pub fn verify_password_argon2id(plain_password: &str, hash_str: &str) -> bool {
        let Ok(parsed_hash) = PasswordHash::new(hash_str) else {
            return false;
        };
        Argon2::default()
            .verify_password(plain_password.as_bytes(), &parsed_hash)
            .is_ok()
    }

    /// Перевіряє обмеження частоти запитів (Rate Limiting) для захисту від brute-force та DoS.
    pub fn check_rate_limit(&mut self, client_id: &str) -> LunarResult<()> {
        let now = Utc::now();
        let entry = self
            .rate_limit_counters
            .entry(client_id.to_string())
            .or_insert((now, 0));

        if (now - entry.0).num_seconds() >= 60 {
            *entry = (now, 1);
            return Ok(());
        }

        entry.1 += 1;
        if entry.1 > self.max_requests_per_minute {
            return Err(LunarHmsError::RateLimitExceeded {
                client_id: client_id.to_string(),
            });
        }
        Ok(())
    }

    /// Створює новий обліковий запис користувача з хешуванням пароля Argon2id.
    pub fn register_user(
        &mut self,
        repo: &mut LunarDatabaseRepository,
        username: &str,
        plain_password: &str,
        role: Role,
        linked_astronaut_id: Option<String>,
    ) -> LunarResult<Uuid> {
        if self.users_by_username.contains_key(username) {
            return Err(LunarHmsError::ValidationFailure {
                field: "username".to_string(),
                reason: format!("Користувач '{}' вже існує", username),
            });
        }
        let password_hash_argon2id = Self::hash_password_argon2id(plain_password)?;
        let user_id = Uuid::new_v4();
        let account = UserAccount {
            user_id,
            username: username.to_string(),
            password_hash_argon2id,
            role,
            linked_astronaut_id,
            is_locked: false,
            failed_attempts: 0,
            locked_until_utc: None,
            last_login_utc: None,
        };
        self.users_by_username.insert(username.to_string(), account);
        repo.append_audit_log(
            "SYSTEM",
            "Administrator",
            "USER_CREATED",
            username,
            "SUCCESS",
            &format!("Створено обліковий запис із роллю {}", role.as_str()),
        );
        Ok(user_id)
    }

    /// Виконує автентифікацію користувача, контролює лічильник невдалих спроб та блокування.
    pub fn authenticate(
        &mut self,
        repo: &mut LunarDatabaseRepository,
        username: &str,
        plain_password: &str,
    ) -> LunarResult<AuthSession> {
        self.check_rate_limit(username)?;

        let Some(user) = self.users_by_username.get_mut(username) else {
            repo.append_audit_log(
                username,
                "UNKNOWN",
                "LOGIN_ATTEMPT",
                "AUTH_GATEWAY",
                "FAILED_UNKNOWN_USER",
                "Спроба входу з невідомим ім'ям користувача",
            );
            return Err(LunarHmsError::AuthenticationFailed);
        };

        let now = Utc::now();
        if user.is_locked {
            if let Some(until) = user.locked_until_utc {
                if now < until {
                    return Err(LunarHmsError::AccountLocked {
                        username: username.to_string(),
                        locked_until_utc: until.to_rfc3339(),
                    });
                } else {
                    user.is_locked = false;
                    user.failed_attempts = 0;
                    user.locked_until_utc = None;
                }
            } else {
                return Err(LunarHmsError::AccountLocked {
                    username: username.to_string(),
                    locked_until_utc: "indefinite".to_string(),
                });
            }
        }

        if !Self::verify_password_argon2id(plain_password, &user.password_hash_argon2id) {
            user.failed_attempts += 1;
            if user.failed_attempts >= self.max_failed_attempts {
                user.is_locked = true;
                let until = now + Duration::minutes(self.lockout_minutes);
                user.locked_until_utc = Some(until);
                repo.append_audit_log(
                    username,
                    user.role.as_str(),
                    "ACCOUNT_LOCKED",
                    username,
                    "LOCKED",
                    &format!(
                        "Обліковий запис заблоковано після {} невдалих спроб входу",
                        user.failed_attempts
                    ),
                );
                return Err(LunarHmsError::AccountLocked {
                    username: username.to_string(),
                    locked_until_utc: until.to_rfc3339(),
                });
            }
            repo.append_audit_log(
                username,
                user.role.as_str(),
                "LOGIN_ATTEMPT",
                "AUTH_GATEWAY",
                "FAILED_BAD_PASSWORD",
                &format!("Невдала спроба входу #{}/{}", user.failed_attempts, self.max_failed_attempts),
            );
            return Err(LunarHmsError::AuthenticationFailed);
        }

        user.failed_attempts = 0;
        user.last_login_utc = Some(now);

        let token = format!("astmed_sess_{}_{}", Uuid::new_v4(), Uuid::new_v4());
        let session = AuthSession {
            session_token: token.clone(),
            user_id: user.user_id,
            username: user.username.clone(),
            role: user.role,
            linked_astronaut_id: user.linked_astronaut_id.clone(),
            issued_at_utc: now,
            expires_at_utc: now + Duration::minutes(self.session_ttl_minutes),
        };

        self.active_sessions.insert(token, session.clone());
        repo.append_audit_log(
            username,
            user.role.as_str(),
            "LOGIN_SUCCESS",
            "AUTH_GATEWAY",
            "SUCCESS",
            "Успішна авторизація сесії",
        );
        Ok(session)
    }

    /// Завершує активну сесію користувача (Logout).
    pub fn terminate_session(
        &mut self,
        repo: &mut LunarDatabaseRepository,
        session_token: &str,
    ) -> LunarResult<()> {
        if let Some(sess) = self.active_sessions.remove(session_token) {
            repo.append_audit_log(
                &sess.username,
                sess.role.as_str(),
                "LOGOUT",
                "AUTH_GATEWAY",
                "SUCCESS",
                "Сесію користувача успішно завершено",
            );
        }
        Ok(())
    }

    /// Змінює пароль користувача після перевірки поточного пароля.
    pub fn change_password(
        &mut self,
        repo: &mut LunarDatabaseRepository,
        username: &str,
        old_password: &str,
        new_password: &str,
    ) -> LunarResult<()> {
        let user = self
            .users_by_username
            .get_mut(username)
            .ok_or(LunarHmsError::AuthenticationFailed)?;

        if !Self::verify_password_argon2id(old_password, &user.password_hash_argon2id) {
            return Err(LunarHmsError::AuthenticationFailed);
        }
        user.password_hash_argon2id = Self::hash_password_argon2id(new_password)?;
        repo.append_audit_log(
            username,
            user.role.as_str(),
            "PASSWORD_CHANGED",
            username,
            "SUCCESS",
            "Пароль змінено з перерахунком Argon2id хешу",
        );
        Ok(())
    }

    /// Перевіряє валідність токена сесії.
    pub fn validate_session(&self, session_token: &str) -> LunarResult<&AuthSession> {
        let sess = self
            .active_sessions
            .get(session_token)
            .ok_or(LunarHmsError::AuthenticationFailed)?;
        if Utc::now() > sess.expires_at_utc {
            return Err(LunarHmsError::AuthenticationFailed);
        }
        Ok(sess)
    }

    /// Серверна перевірка RBAC: чи має сесія право переглядати повні приватні медичні дані космонавта `target_astronaut_id`.
    pub fn authorize_medical_detail_access(
        &self,
        session_token: &str,
        target_astronaut_id: &str,
    ) -> LunarResult<()> {
        let sess = self.validate_session(session_token)?;
        match sess.role {
            Role::FlightSurgeon => Ok(()),
            Role::CrewMember => {
                if sess.linked_astronaut_id.as_deref() == Some(target_astronaut_id) {
                    Ok(())
                } else {
                    Err(LunarHmsError::AccessDenied {
                        role: sess.role.as_str().to_string(),
                        action: "VIEW_PRIVATE_MEDICAL_HISTORY".to_string(),
                        resource: target_astronaut_id.to_string(),
                    })
                }
            }
            // Commander бачить загальний стан та критичні сповіщення, але Engineer і Administrator НЕ мають доступу до медичної історії!
            Role::Engineer | Role::Administrator | Role::Commander | Role::MissionControl => {
                Err(LunarHmsError::AccessDenied {
                    role: sess.role.as_str().to_string(),
                    action: "VIEW_PRIVATE_MEDICAL_HISTORY".to_string(),
                    resource: target_astronaut_id.to_string(),
                })
            }
        }
    }

    /// Серверна перевірка RBAC: чи має сесія право переглядати інженерну діагностику датчиків та параметри середовища станції.
    pub fn authorize_sensor_and_habitat_access(&self, session_token: &str) -> LunarResult<()> {
        let sess = self.validate_session(session_token)?;
        match sess.role {
            Role::Engineer
            | Role::Commander
            | Role::FlightSurgeon
            | Role::MissionControl
            | Role::Administrator => Ok(()),
            Role::CrewMember => Err(LunarHmsError::AccessDenied {
                role: sess.role.as_str().to_string(),
                action: "VIEW_SENSOR_DIAGNOSTICS".to_string(),
                resource: "HARDWARE_BUS".to_string(),
            }),
        }
    }

    /// Серверна перевірка RBAC: чи має сесія право керувати обліковими записами та ролями.
    pub fn authorize_user_administration(&self, session_token: &str) -> LunarResult<()> {
        let sess = self.validate_session(session_token)?;
        if sess.role == Role::Administrator {
            Ok(())
        } else {
            Err(LunarHmsError::AccessDenied {
                role: sess.role.as_str().to_string(),
                action: "MANAGE_USERS_AND_ROLES".to_string(),
                resource: "RBAC_ADMIN".to_string(),
            })
        }
    }

    /// Restore password hashes but never restore active sessions or rate-limit state.
    pub fn restore_users(&mut self, users: Vec<UserAccount>) -> LunarResult<()> {
        if !self.users_by_username.is_empty() {
            return Err(LunarHmsError::AuthenticationFailed);
        }
        let mut restored = HashMap::new();
        for user in users {
            if user.username.trim().is_empty()
                || argon2::password_hash::PasswordHash::new(&user.password_hash_argon2id).is_err()
                || restored.contains_key(&user.username)
            {
                return Err(LunarHmsError::ValidationFailure {
                    field: "users".into(),
                    reason: "Invalid saved user accounts".into(),
                });
            }
            restored.insert(user.username.clone(), user);
        }
        self.users_by_username = restored;
        self.active_sessions.clear();
        self.rate_limit_counters.clear();
        Ok(())
    }

    pub fn list_users(&self) -> Vec<UserAccount> {
        self.users_by_username.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rbac_argon2id_and_lockout_enforcement() {
        let mut repo = LunarDatabaseRepository::new("data/test_rbac.sqlite3");
        let mut acl = AccessControlService::new(3, 15, 60, 100);

        // Створюємо користувачів для кожної ключової ролі
        acl.register_user(
            &mut repo,
            "dr_melnyk",
            "SurgeonPass#2026",
            Role::FlightSurgeon,
            Some("AST-002".to_string()),
        )
        .unwrap();
        acl.register_user(
            &mut repo,
            "eng_bondarenko",
            "EngineerPass#2026",
            Role::Engineer,
            Some("AST-003".to_string()),
        )
        .unwrap();
        acl.register_user(
            &mut repo,
            "crew_lysenko",
            "CrewPass#2026",
            Role::CrewMember,
            Some("AST-004".to_string()),
        )
        .unwrap();
        acl.register_user(
            &mut repo,
            "sys_admin",
            "AdminPass#2026",
            Role::Administrator,
            None,
        )
        .unwrap();

        let doc_sess = acl
            .authenticate(&mut repo, "dr_melnyk", "SurgeonPass#2026")
            .unwrap();
        let eng_sess = acl
            .authenticate(&mut repo, "eng_bondarenko", "EngineerPass#2026")
            .unwrap();
        let crew_sess = acl
            .authenticate(&mut repo, "crew_lysenko", "CrewPass#2026")
            .unwrap();
        let admin_sess = acl
            .authenticate(&mut repo, "sys_admin", "AdminPass#2026")
            .unwrap();

        // FlightSurgeon бачить медичні дані всіх космонавтів
        assert!(acl
            .authorize_medical_detail_access(&doc_sess.session_token, "AST-001")
            .is_ok());
        assert!(acl
            .authorize_medical_detail_access(&doc_sess.session_token, "AST-004")
            .is_ok());

        // CrewMember бачить ЛИШЕ власні медичні дані (AST-004), але НЕ чужі (AST-001)
        assert!(acl
            .authorize_medical_detail_access(&crew_sess.session_token, "AST-004")
            .is_ok());
        assert!(acl
            .authorize_medical_detail_access(&crew_sess.session_token, "AST-001")
            .is_err());

        // Engineer та Administrator НЕ мають доступу до приватної медичної історії
        assert!(acl
            .authorize_medical_detail_access(&eng_sess.session_token, "AST-001")
            .is_err());
        assert!(acl
            .authorize_medical_detail_access(&admin_sess.session_token, "AST-001")
            .is_err());

        // Проте Administrator має доступ до керування користувачами, а FlightSurgeon — ні
        assert!(acl
            .authorize_user_administration(&admin_sess.session_token)
            .is_ok());
        assert!(acl
            .authorize_user_administration(&doc_sess.session_token)
            .is_err());

        // Перевірка автоматичного блокування після 3 невдалих спроб входу
        assert!(acl.authenticate(&mut repo, "dr_melnyk", "wrong1").is_err());
        assert!(acl.authenticate(&mut repo, "dr_melnyk", "wrong2").is_err());
        let lock_res = acl.authenticate(&mut repo, "dr_melnyk", "wrong3");
        assert!(matches!(lock_res, Err(LunarHmsError::AccountLocked { .. })));
    }
}
