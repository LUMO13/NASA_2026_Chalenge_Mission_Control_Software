> **Submission note (2026-11-15):** The current desktop application uses an **in-memory repository**, not connected PostgreSQL or SQLite. Remote Earth transmission and some GUI pages remain simulations/placeholders. This document describes the intended architecture or schema; see [CURRENT_STATUS.md](CURRENT_STATUS.md) for implemented functionality.

# Документація API та RBAC-протоколів

## 1. Ендпоінти симулятора датчиків (`sensor-simulator`, порт `7001`)

- `GET /api/v1/simulator/poll/:astronaut_id` — отримання чергового підписаного `HMAC-SHA256` кадру `TelemetryEnvelope` для обраного космонавта.
- `GET /api/v1/simulator/poll-all` — пакетне опитування всіх активних космонавтів.
- `POST /api/v1/simulator/control` — керування параметрами симуляції (`SimulatorControlCommand`):
  ```json
  {
    "SetScenario": {
      "astronaut_id": "AST-004",
      "scenario": "EmergencyHypoxia"
    }
  }
  ```

## 2. Ендпоінти отримувача телеметрії (`telemetry-receiver`, порт `7002`)

- `POST /api/v1/receiver/ingest` — приймає `TelemetryEnvelope`, перевіряє `HMAC-SHA256`, захист від Replay-атак за `sequence_nonce`, валідує фізичні одиниці та фіксує `received_at_utc`.

## 3. Ендпоінти розподільника телеметрії (`telemetry-distributor`, порт `7003`)

- `GET /api/v1/distributor/status` — поточний розмір черги синхронізації із Землею, значення повзунка накопичення (`accumulation_window_secs`) та стан каналу зв'язку (`earth_link_online`).
- `POST /api/v1/distributor/sync-now` — примусове формування стисненого пакета `Gzip` та передавання найвищого за пріоритетом пакета на Землю з перевіркою `SHA-256`.

## 4. Матриця серверного контролю доступу RBAC (`access-control` + `data-stream-manager`)

| Роль | Загальний стан екіпажу | Приватні медичні дані всіх космонавтів | Власні медичні дані | Діагностика датчиків і середовище станції | Керування обліковими записами і ролями | Синхронізація із Землею |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Commander** | ✅ | ❌ | ✅ | ✅ | ❌ | ✅ |
| **FlightSurgeon** | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| **Engineer** | ❌ | ❌ | ✅ | ✅ | ❌ | ✅ |
| **CrewMember** | ❌ | ❌ | ✅ (лише свій ID) | ❌ (лише середовище) | ❌ | ❌ |
| **MissionControl** | ✅ | ❌ (за повноваженнями) | ❌ | ✅ | ❌ | ✅ |
| **Administrator** | ❌ | ❌ | ❌ | ✅ (лише технічні) | ✅ | ✅ |
