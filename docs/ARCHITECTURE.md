> **Submission note (2026-11-15):** The current desktop application uses an **in-memory repository**, not connected PostgreSQL or SQLite. Remote Earth transmission and some GUI pages remain simulations/placeholders. This document describes the intended architecture or schema; see [CURRENT_STATUS.md](CURRENT_STATUS.md) for implemented functionality.

# Архітектура системи ASTRA-MED (`lunar-hms`)

## 1. Огляд та декомпозиція мікросервісів

Система спроєктована за принципами слабкої зв'язаності (Loose Coupling), явної типізації фізичних одиниць та автономності (Offline-First) для роботи в умовах місячної бази із затримками та перериваннями зв'язку із Землею.

### 1.1. Таблиця компонентів та їх відповідальності

| Компонент | Відповідальність | Вхідні дані | Вихідні дані | Протокол взаємодії | Обробка помилок |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`shared-core`** | Спільні моделі даних, одиниці вимірювання, HMAC-SHA256, Anti-Replay, конфігурація | Сирі структури конфігурації та телеметрії | Валідовані доменні типи, підписи `HMAC-SHA256` | Внутрішній Rust ABI | Типізований `LunarHmsError` (`thiserror`) |
| **`sensor-simulator`** | Імітація фізичних біомедичних датчиків, середовища, радіації та скафандра для N космонавтів | Команди `SimulatorControlCommand`, частота опитування, похибка, сценарій | Підписані кадри `TelemetryEnvelope` | HTTP/JSON REST або TCP Stream (`127.0.0.1:7001`) | Відновлення генерації після збоїв, контроль меж шуму |
| **`telemetry-receiver`** | Приймання телеметрії через адаптер `SensorHardwareAdapter`, перевірка HMAC, Anti-Replay, контроль таймауту | Кадри `TelemetryEnvelope` від симулятора або UART/CAN/BLE адаптера | Верифіковані кадри з фіксованим `received_at_utc` | Адаптерний трейт + HTTP REST (`127.0.0.1:7002`) | Відхилення пошкоджених/повторних пакетів, подія `ConnectionLost` |
| **`telemetry-distributor`** | Буферизація за часовим повзунком, запис у БД, стиснення Gzip, пріоритетна черга на Землю | Верифіковані `TelemetryEnvelope` та тривоги `AlertEvent` | Записи у БД + стиснені пакети `EarthUplinkBatch` | HTTP/TLS (`127.0.0.1:7003`) + SQLx | Локальна черга при втраті зв'язку із Землею, повторні спроби з backoff |
| **`database`** | Збереження даних у PostgreSQL + автономний буфер SQLite WAL, дедуплікація, незмінність ID | Параметризовані моделі телеметрії, тривог, користувачів, пакетів | Історичні вибірки за UTC, звіти синхронізації | PostgreSQL Wire Protocol (TLS) / SQLite WAL | Автоматичний перехід на локальний буфер при недоступності PostgreSQL |
| **`health-analyzer`** | Реальний аналіз показників за порогами, обчислення лінійного тренду, керування життєвим циклом | Історичні та поточні записи з `database`, `thresholds.toml` | Події `AlertEvent` (`INFO`, `WARNING`, `CRITICAL`), плани синхронізації | Внутрішній сервісний API (`127.0.0.1:7004`) | Заборона видалення неперевірених даних (`UnverifiedPurgeForbidden`) |
| **`access-control`** | RBAC для 6 ролей, хешування `Argon2id`, сесії, блокування облікових записів, Rate Limiting, аудит | Облікові дані, токени сесій, запити на перевірку прав | Токени `AuthSession`, рішення RBAC, записи в `audit_logs` | HTTP REST (`127.0.0.1:7005`) | Блокування облікового запису після N невдалих спроб, логування відмов |
| **`data-stream-manager`** | Серверна фільтрація та маскування потоків телеметрії згідно з RBAC-роллю клієнта | Повні `TelemetryEnvelope` + токен сесії `AuthSession` | Відфільтровані структури `ScopedTelemetryView` | HTTP/WebSocket (`127.0.0.1:7006`) | Вилучення приватних медичних полів до відправлення по мережі |
| **`desktop-gui`** | Візуалізація 11 сторінок у стилі NASA Mission Control, графіки, керування симулятором та синхронізацією | `ScopedTelemetryView`, `AlertEvent`, журнали синхронізації та аудиту | Команди оператора, підтвердження тривог, форми налаштувань | REST / WebSocket / In-Process шина | Відображення банерів помилок, індикація втрати зв'язку, локальний часовий пояс |

## 2. Заміна симулятора реальними фізичними датчиками

Компонент `telemetry-receiver` взаємодіє з джерелами даних виключно через асинхронний трейт `SensorHardwareAdapter`:

```rust
#[async_trait]
pub trait SensorHardwareAdapter: Send + Sync {
    fn adapter_id(&self) -> &str;
    async fn poll_telemetry_frames(&mut self) -> LunarResult<Vec<TelemetryEnvelope>>;
}
```

Для підключення реального апаратного комплексу (наприклад, нагрудного ЕКГ/ФПГ-патча через BLE або шлюзу скафандра через шину CAN / RS-485) достатньо підключити реалізацію `PhysicalSerialCanAdapter` без жодної модифікації `telemetry-distributor`, `database`, `health-analyzer`, `access-control` чи `desktop-gui`.
