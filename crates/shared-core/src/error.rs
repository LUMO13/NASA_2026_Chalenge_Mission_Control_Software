//! Єдина ієрархія помилок системи ASTRA-MED / Lunar HMS.

use thiserror::Error;

/// Централізований тип помилки для взаємодії між компонентами системи.
#[derive(Debug, Error)]
pub enum LunarHmsError {
    #[error("Помилка перевірки цілісності повідомлення HMAC-SHA256: очікувалось {expected}, отримано {actual}")]
    IntegrityViolation { expected: String, actual: String },

    #[error("Виявлено спробу повторного відтворення пакета (Replay Attack): космонавт {astronaut_id}, nonce {nonce}")]
    ReplayDetected { astronaut_id: String, nonce: u64 },

    #[error("Некоректний формат або фізично неможливе значення телеметрії у полі '{field}': {reason}")]
    ValidationFailure { field: String, reason: String },

    #[error("Втрачено зв'язок із джерелом телеметрії '{source_id}' (таймаут {timeout_ms} мс)")]
    ConnectionLost { source_id: String, timeout_ms: u64 },

    #[error("Відмова в доступі (RBAC): роль '{role}' не має дозволу на дію '{action}' для ресурсу '{resource}'")]
    AccessDenied {
        role: String,
        action: String,
        resource: String,
    },

    #[error("Обліковий запис '{username}' заблоковано до {locked_until_utc} через перевищення ліміту невдалих входів")]
    AccountLocked {
        username: String,
        locked_until_utc: String,
    },

    #[error("Недійсні облікові дані або прострочений токен сесії")]
    AuthenticationFailed,

    #[error("Перевищено ліміт частоти запитів (Rate Limit Exceeded) для клієнта '{client_id}'")]
    RateLimitExceeded { client_id: String },

    #[error("Заборона видалення даних: пакет {batch_id} ще не синхронізовано та не підтверджено Землею")]
    UnverifiedPurgeForbidden { batch_id: String },

    #[error("Помилка підсистеми зберігання даних: {0}")]
    StorageError(String),

    #[error("Помилка конфігурації: {0}")]
    ConfigError(String),

    #[error("Помилка мережевого транспорту: {0}")]
    TransportError(String),
}

pub type LunarResult<T> = Result<T, LunarHmsError>;
