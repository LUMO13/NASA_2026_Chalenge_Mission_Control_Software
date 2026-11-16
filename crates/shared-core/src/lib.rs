//! # `shared-core` — Спільне ядро системи моніторингу здоров'я космонавтів ASTRA-MED / Lunar HMS
//!
//! Цей крейт визначає доменні типи даних з явно зафіксованими фізичними одиницями вимірювання,
//! метаданими апаратних сенсорів, криптографічні примітиви перевірки цілісності (`HMAC-SHA256`),
//! захист від повторного відтворення повідомлень (`Anti-Replay Window`), конфігураційні структури
//! та єдину модель помилок для всіх мікросервісів комплексу.

pub mod config;
pub mod error;
pub mod models;
pub mod protocol;
pub mod security;

pub use config::*;
pub use error::*;
pub use models::*;
pub use protocol::*;
pub use security::*;
