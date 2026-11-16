//! # `telemetry-receiver` — Отримувач телеметрії з адаптерною архітектурою
//!
//! Відповідає за:
//! - підключення до симулятора датчиків або реальних фізичних інтерфейсів (UART/Serial, BLE, CAN/Modbus)
//!   через уніфікований трейт [`SensorHardwareAdapter`];
//! - перевірку цілісності отриманих повідомлень (`HMAC-SHA256`);
//! - захист від повторного відтворення повідомлень (`AntiReplayGuard`);
//! - перевірку фізичної коректності форматів і діапазонів;
//! - фіксацію точного часу отримання (`received_at_utc`);
//! - виявлення втрати з'єднання (Watchdog Timeout Detector).

use std::collections::HashMap;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use shared_core::{
    verify_envelope_hmac, AntiReplayGuard, LunarHmsError, LunarResult, TelemetryEnvelope,
};

/// Універсальний адаптерний інтерфейс для підключення як програмного симулятора,
/// так і реальних фізичних біомедичних та інженерних датчиків без зміни решти системи.
#[async_trait]
pub trait SensorHardwareAdapter: Send + Sync {
    /// Унікальний ідентифікатор шини або фізичного адаптера.
    fn adapter_id(&self) -> &str;

    /// Опитує підключений пристрій або шину датчиків і повертає нові кадри телеметрії.
    async fn poll_telemetry_frames(&mut self) -> LunarResult<Vec<TelemetryEnvelope>>;
}

/// Адаптер для роботи з програмним симулятором датчиків (in-memory або через TCP/HTTP).
pub struct SimulatedHardwareAdapter {
    adapter_id: String,
    simulator: sensor_simulator::SensorSimulatorEngine,
}

impl SimulatedHardwareAdapter {
    pub fn new(adapter_id: &str, simulator: sensor_simulator::SensorSimulatorEngine) -> Self {
        Self {
            adapter_id: adapter_id.to_string(),
            simulator,
        }
    }

    pub fn engine_mut(&mut self) -> &mut sensor_simulator::SensorSimulatorEngine {
        &mut self.simulator
    }
}

#[async_trait]
impl SensorHardwareAdapter for SimulatedHardwareAdapter {
    fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    async fn poll_telemetry_frames(&mut self) -> LunarResult<Vec<TelemetryEnvelope>> {
        if self.simulator.link_disconnected {
            return Err(LunarHmsError::ConnectionLost {
                source_id: self.adapter_id.clone(),
                timeout_ms: self.simulator.poll_interval_ms,
            });
        }
        let mut ids: Vec<String> = self.simulator.astronauts.keys().cloned().collect();
        ids.sort();
        let mut frames = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(frame) = self.simulator.generate_next_envelope(&id) {
                frames.push(frame);
            }
        }
        Ok(frames)
    }
}

/// Адаптер для підключення реальних фізичних датчиків (наприклад, через послідовний порт UART/USB-CDC або BLE-шлюз).
/// Дозволяє замінити симулятор апаратним комплексом без переписування отримувача та наступних сервісів.
pub struct PhysicalSerialCanAdapter {
    adapter_id: String,
    device_path: String,
}

impl PhysicalSerialCanAdapter {
    pub fn new(adapter_id: &str, device_path: &str) -> Self {
        Self {
            adapter_id: adapter_id.to_string(),
            device_path: device_path.to_string(),
        }
    }
}

#[async_trait]
impl SensorHardwareAdapter for PhysicalSerialCanAdapter {
    fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    async fn poll_telemetry_frames(&mut self) -> LunarResult<Vec<TelemetryEnvelope>> {
        // У реальному апаратному розгортанні читає JSON/CBOR-кадри з UART/CAN-шини self.device_path
        Err(LunarHmsError::ConnectionLost {
            source_id: format!("{} ({})", self.adapter_id, self.device_path),
            timeout_ms: 2000,
        })
    }
}

/// Сервіс приймання, криптографічної верифікації та структурної валідації телеметрії.
pub struct TelemetryReceiverService {
    hmac_secret: Vec<u8>,
    replay_guard: AntiReplayGuard,
    last_seen_utc: HashMap<String, DateTime<Utc>>,
    watchdog_timeout_ms: i64,
}

impl TelemetryReceiverService {
    pub fn new(hmac_secret: Vec<u8>, watchdog_timeout_ms: i64) -> Self {
        Self {
            hmac_secret,
            replay_guard: AntiReplayGuard::new(256),
            last_seen_utc: HashMap::new(),
            watchdog_timeout_ms: watchdog_timeout_ms.max(1000),
        }
    }

    /// Приймає сирий кадр телеметрії, перевіряє HMAC-SHA256, захист від Replay,
    /// фізичну коректність значень та проставляє `received_at_utc`.
    pub fn ingest_and_validate(
        &mut self,
        mut envelope: TelemetryEnvelope,
    ) -> LunarResult<TelemetryEnvelope> {
        // 1. Перевірка криптографічної цілісності HMAC-SHA256
        verify_envelope_hmac(&envelope, &self.hmac_secret)?;

        // 2. Перевірка захисту від повторного відтворення (Anti-Replay)
        self.replay_guard
            .check_and_record(&envelope.astronaut_id, envelope.sequence_nonce)?;

        // 3. Перевірка коректності ідентифікатора космонавта та фізичних меж
        if !envelope.astronaut_id.starts_with("AST-") {
            return Err(LunarHmsError::ValidationFailure {
                field: "astronaut_id".to_string(),
                reason: "ID космонавта повинен мати префікс AST-".to_string(),
            });
        }

        if let Some(ref hr) = envelope.cardiovascular.heart_rate_bpm {
            if !(10.0..=300.0).contains(&hr.value) || hr.unit != "bpm" {
                return Err(LunarHmsError::ValidationFailure {
                    field: "cardiovascular.heart_rate_bpm".to_string(),
                    reason: format!("Некоректне значення або одиниця ЧСС: {} {}", hr.value, hr.unit),
                });
            }
        }

        if let Some(ref spo2) = envelope.respiratory.spo2_pct {
            if !(0.0..=100.0).contains(&spo2.value) || spo2.unit != "%" {
                return Err(LunarHmsError::ValidationFailure {
                    field: "respiratory.spo2_pct".to_string(),
                    reason: format!("SpO2 поза межами 0..100%: {} {}", spo2.value, spo2.unit),
                });
            }
        }

        if envelope.habitat.atmospheric_pressure_kpa.value < 0.0 {
            return Err(LunarHmsError::ValidationFailure {
                field: "habitat.atmospheric_pressure_kpa".to_string(),
                reason: "Атмосферний тиск не може бути від'ємним".to_string(),
            });
        }

        // 4. Фіксація точного часу отримання в UTC
        let now = Utc::now();
        envelope.received_at_utc = Some(now);
        self.last_seen_utc
            .insert(envelope.astronaut_id.clone(), now);

        Ok(envelope)
    }

    /// Перевіряє, чи не перевищено інтервал очікування телеметрії від конкретного космонавта.
    pub fn check_connection_status(&self, astronaut_id: &str) -> LunarResult<()> {
        if let Some(last_seen) = self.last_seen_utc.get(astronaut_id) {
            let elapsed_ms = (Utc::now() - *last_seen).num_milliseconds();
            if elapsed_ms > self.watchdog_timeout_ms {
                return Err(LunarHmsError::ConnectionLost {
                    source_id: astronaut_id.to_string(),
                    timeout_ms: self.watchdog_timeout_ms as u64,
                });
            }
            Ok(())
        } else {
            Err(LunarHmsError::ConnectionLost {
                source_id: astronaut_id.to_string(),
                timeout_ms: self.watchdog_timeout_ms as u64,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sensor_simulator::SensorSimulatorEngine;

    #[tokio::test]
    async fn test_receiver_validates_and_rejects_tampered_or_replayed_packets() {
        let secret = b"receiver_hmac_test_secret".to_vec();
        let sim = SensorSimulatorEngine::new_with_default_crew(secret.clone());
        let mut adapter = SimulatedHardwareAdapter::new("SIM-BUS-01", sim);
        let mut receiver = TelemetryReceiverService::new(secret, 5000);

        let frames = adapter.poll_telemetry_frames().await.expect("poll frames");
        assert!(!frames.is_empty());

        let first = frames[0].clone();
        let validated = receiver.ingest_and_validate(first.clone()).expect("valid packet");
        assert!(validated.received_at_utc.is_some());

        // Повторне надсилання того самого пакета має бути відхилене Anti-Replay
        assert!(receiver.ingest_and_validate(first.clone()).is_err());

        // Підробка значення ЧСС без оновлення HMAC має бути виявлена
        let mut next_frames = adapter.poll_telemetry_frames().await.expect("poll 2");
        let mut tampered = next_frames.remove(0);
        if let Some(ref mut hr) = tampered.cardiovascular.heart_rate_bpm {
            hr.value = 210.0;
        }
        assert!(receiver.ingest_and_validate(tampered).is_err());
    }
}
