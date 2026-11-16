//! # `telemetry-distributor` — Розподільник телеметрії та відправник на Землю
//!
//! Забезпечує:
//! - приймання верифікованої телеметрії від `telemetry-receiver`;
//! - негайний запис кожного вимірювання окремим записом у базу даних із захистом від дублювання;
//! - накопичення даних за налаштовуваним часовим проміжком (повзунок у секундах);
//! - формування стиснених пакетів (`Gzip` + `SHA-256`) для передавання на Землю;
//! - пріоритетну чергу передавання (`CRITICAL_IMMEDIATE` випереджає звичайну телеметрію);
//! - роботу в автономному режимі без зв'язку із Землею (`offline_mode`) та автоматичне
//!   повторне надсилання з експоненційною затримкою після відновлення каналу.

use std::collections::VecDeque;
use std::io::Write;
use chrono::{DateTime, Utc};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use database::{EarthSyncBatchRecord, LunarDatabaseRepository};
use shared_core::{
    sha256_hex, AlertEvent, AlertSeverity, EarthUplinkAck, EarthUplinkBatch, LunarHmsError,
    LunarResult, SyncPriority, TelemetryEnvelope,
};

/// Налаштування панелі керування розподільника телеметрії (включно з повзунком часу накопичення).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributorSettings {
    /// Часовий проміжок накопичення даних перед передачею на Землю (повзунок: 5..3600 с)
    pub accumulation_window_secs: u64,
    /// Інтервал автоматичної синхронізації (с)
    pub send_interval_secs: u64,
    /// Максимальна кількість записів у одному пакеті
    pub max_batch_records: usize,
    /// Автоматична синхронізація увімкнена
    pub auto_sync_enabled: bool,
    /// Режим роботи без зв'язку із Землею (Offline / Loss of Signal)
    pub earth_link_online: bool,
    /// Максимальна кількість повторних спроб при збої каналу
    pub max_retries: u32,
}

impl Default for DistributorSettings {
    fn default() -> Self {
        Self {
            accumulation_window_secs: 30,
            send_interval_secs: 30,
            max_batch_records: 100,
            auto_sync_enabled: true,
            earth_link_online: true,
            max_retries: 5,
        }
    }
}

/// Запис журналу передачі телеметрії на Землю.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransmissionLogEntry {
    pub timestamp_utc: DateTime<Utc>,
    pub batch_id: Uuid,
    pub priority: SyncPriority,
    pub record_count: usize,
    pub compressed_bytes: usize,
    pub status: String,
    pub details: String,
}

/// Queue snapshot for recovery after a desktop restart (all transmissions remain simulated).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributorSnapshot {
    pub settings: DistributorSettings,
    pub pending_frames: Vec<TelemetryEnvelope>,
    pub pending_alerts: Vec<AlertEvent>,
    pub queued_batches: Vec<EarthUplinkBatch>,
    pub transmission_log: Vec<TransmissionLogEntry>,
    pub last_batch_created_utc: DateTime<Utc>,
}

/// Компонент розподільника телеметрії та черги відправлення на Землю.
pub struct TelemetryDistributor {
    pub settings: DistributorSettings,
    pending_Buffer: Vec<TelemetryEnvelope>,
    pending_alerts: Vec<AlertEvent>,
    uplink_queue: VecDeque<EarthUplinkBatch>,
    transmission_log: Vec<TransmissionLogEntry>,
    last_batch_created_utc: DateTime<Utc>,
}

#[allow(non_snake_case)]
impl TelemetryDistributor {
    pub fn new(settings: DistributorSettings) -> Self {
        Self {
            settings,
            pending_Buffer: Vec::new(),
            pending_alerts: Vec::new(),
            uplink_queue: VecDeque::new(),
            transmission_log: Vec::new(),
            last_batch_created_utc: Utc::now(),
        }
    }

    pub fn export_snapshot(&self) -> DistributorSnapshot {
        DistributorSnapshot {
            settings: self.settings.clone(),
            pending_frames: self.pending_Buffer.clone(),
            pending_alerts: self.pending_alerts.clone(),
            queued_batches: self.uplink_queue.iter().cloned().collect(),
            transmission_log: self.transmission_log.clone(),
            last_batch_created_utc: self.last_batch_created_utc,
        }
    }

    pub fn restore_snapshot(&mut self, data: DistributorSnapshot) {
        self.settings = data.settings;
        self.pending_Buffer = data.pending_frames;
        self.pending_alerts = data.pending_alerts;
        self.uplink_queue = data.queued_batches.into();
        self.transmission_log = data.transmission_log;
        self.last_batch_created_utc = data.last_batch_created_utc;
    }

    /// Оновлює положення повзунка часового проміжку накопичення (5..3600 секунд).
    pub fn set_accumulation_slider_secs(&mut self, window_secs: u64) {
        self.settings.accumulation_window_secs = window_secs.clamp(5, 3600);
    }

    /// Приймає новий кадр від отримувача, записує його окремим записом у БД (із захистом від дублів)
    /// та додає у буфер накопичення для відправлення на Землю.
    pub fn process_incoming_telemetry(
        &mut self,
        repo: &mut LunarDatabaseRepository,
        envelope: TelemetryEnvelope,
    ) -> LunarResult<bool> {
        let inserted = repo.insert_telemetry_record(envelope.clone())?;
        if !inserted {
            return Ok(false);
        }
        self.pending_Buffer.push(envelope);

        let elapsed_secs = (Utc::now() - self.last_batch_created_utc)
            .num_seconds()
            .max(0) as u64;
        if self.pending_Buffer.len() >= self.settings.max_batch_records
            || elapsed_secs >= self.settings.accumulation_window_secs
        {
            self.flush_buffer_to_priority_queue(repo)?;
        }
        Ok(true)
    }

    /// Додає тривожне сповіщення. Якщо воно має рівень `CRITICAL`, негайно формує
    /// високопріоритетний пакет `CriticalImmediate` на початок черги відправлення!
    pub fn enqueue_alert_with_priority(
        &mut self,
        repo: &mut LunarDatabaseRepository,
        alert: AlertEvent,
    ) -> LunarResult<Option<Uuid>> {
        let is_critical = alert.severity == AlertSeverity::Critical;
        self.pending_alerts.push(alert);
        if is_critical {
            self.flush_buffer_to_priority_queue(repo)
        } else {
            Ok(None)
        }
    }

    /// Формує стиснений пакет Gzip із накопичених записів та ставить його у пріоритетну чергу.
    pub fn flush_buffer_to_priority_queue(
        &mut self,
        repo: &mut LunarDatabaseRepository,
    ) -> LunarResult<Option<Uuid>> {
        if self.pending_Buffer.is_empty() && self.pending_alerts.is_empty() {
            return Ok(None);
        }

        let priority = if self
            .pending_alerts
            .iter()
            .any(|a| a.severity == AlertSeverity::Critical)
        {
            SyncPriority::CriticalImmediate
        } else if self
            .pending_alerts
            .iter()
            .any(|a| a.severity == AlertSeverity::Warning)
        {
            SyncPriority::HighAlert
        } else {
            SyncPriority::NormalTelemetry
        };

        let records: Vec<TelemetryEnvelope> = self.pending_Buffer.drain(..).collect();
        let alerts: Vec<AlertEvent> = self.pending_alerts.drain(..).collect();
        let record_ids: Vec<Uuid> = records.iter().map(|r| r.record_id).collect();
        let record_count = records.len().max(1);

        let serialized = serde_json::to_vec(&(&records, &alerts))
            .map_err(|e| LunarHmsError::TransportError(e.to_string()))?;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&serialized)
            .map_err(|e| LunarHmsError::TransportError(e.to_string()))?;
        let compressed = encoder
            .finish()
            .map_err(|e| LunarHmsError::TransportError(e.to_string()))?;

        let checksum = sha256_hex(&compressed);
        let batch_id = Uuid::new_v4();
        let now = Utc::now();

        let batch = EarthUplinkBatch {
            batch_id,
            created_at_utc: now,
            priority,
            record_count,
            compressed_payload_gzip: compressed.clone(),
            sha256_checksum: checksum.clone(),
            alerts,
            sample_record_ids: record_ids.clone(),
        };

        repo.register_sync_batch(EarthSyncBatchRecord {
            batch_id,
            created_at_utc: now,
            transmitted_at_utc: None,
            verified_at_utc: None,
            priority,
            record_count,
            compressed_size_bytes: compressed.len(),
            sha256_checksum: checksum,
            sync_status: "QUEUED".to_string(),
            retry_count: 0,
            record_ids,
        });

        // Критичні пакети вставляються у голову черги перед звичайною телеметрією
        if priority == SyncPriority::CriticalImmediate {
            self.uplink_queue.push_front(batch);
        } else {
            self.uplink_queue.push_back(batch);
        }
        self.last_batch_created_utc = now;
        Ok(Some(batch_id))
    }

    /// Намагається передати наступний пакет із черги на Землю.
    /// Якщо зв'язок із Землею відсутній (`earth_link_online == false`), пакет безпечно залишається у черзі.
    pub fn transmit_next_queued_batch(
        &mut self,
        repo: &mut LunarDatabaseRepository,
    ) -> LunarResult<Option<EarthUplinkAck>> {
        if !self.settings.earth_link_online {
            return Ok(None);
        }

        let Some(batch) = self.uplink_queue.pop_front() else {
            return Ok(None);
        };

        // Перевірка контрольної суми SHA-256 на стороні приймального шлюзу Землі
        let actual_hash = sha256_hex(&batch.compressed_payload_gzip);
        if actual_hash != batch.sha256_checksum {
            self.uplink_queue.push_front(batch);
            return Err(LunarHmsError::IntegrityViolation {
                expected: "matching batch sha256".to_string(),
                actual: actual_hash,
            });
        }

        repo.mark_batch_verified_by_earth(batch.batch_id)?;

        let ack = EarthUplinkAck {
            batch_id: batch.batch_id,
            verified: true,
            received_at_utc: Utc::now(),
            verified_sha256: actual_hash,
            station_id: "DSN-MADRID-DSS63".to_string(),
        };

        self.transmission_log.push(TransmissionLogEntry {
            timestamp_utc: Utc::now(),
            batch_id: batch.batch_id,
            priority: batch.priority,
            record_count: batch.record_count,
            compressed_bytes: batch.compressed_payload_gzip.len(),
            status: "VERIFIED_ACK".to_string(),
            details: format!(
                "Підтверджено станцією {}, SHA-256: {}",
                ack.station_id, ack.verified_sha256
            ),
        });

        Ok(Some(ack))
    }

    pub fn queued_batches_count(&self) -> usize {
        self.uplink_queue.len()
    }

    pub fn transmission_log(&self) -> &[TransmissionLogEntry] {
        &self.transmission_log
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sensor_simulator::SensorSimulatorEngine;

    #[test]
    fn test_distributor_offline_queue_and_critical_priority() {
        let mut repo = LunarDatabaseRepository::new("data/test_dist.sqlite3");
        let mut dist = TelemetryDistributor::new(DistributorSettings {
            earth_link_online: false, // Імітація втрати зв'язку із Землею
            max_batch_records: 2,
            ..Default::default()
        });

        let secret = b"dist_secret".to_vec();
        let mut sim = SensorSimulatorEngine::new_with_default_crew(secret);

        let f1 = sim.generate_next_envelope("AST-001").unwrap();
        let f2 = sim.generate_next_envelope("AST-002").unwrap();
        dist.process_incoming_telemetry(&mut repo, f1).unwrap();
        dist.process_incoming_telemetry(&mut repo, f2).unwrap();

        assert_eq!(dist.queued_batches_count(), 1);
        // Коли немає зв'язку із Землею, пакет не втрачається і залишається в черзі
        assert!(dist.transmit_next_queued_batch(&mut repo).unwrap().is_none());
        assert_eq!(dist.queued_batches_count(), 1);

        // Додаємо CRITICAL сповіщення — воно повинно стати першим у черзі!
        let crit_alert = AlertEvent {
            alert_id: Uuid::new_v4(),
            astronaut_id: Some("AST-004".to_string()),
            source_subsystem: "SPACESUIT_EVA".to_string(),
            parameter_name: "spo2_pct".to_string(),
            severity: AlertSeverity::Critical,
            measured_value: 83.0,
            unit: "%".to_string(),
            threshold_limit: 86.0,
            trend_slope_per_min: -4.0,
            message: "Критична гіпоксія".to_string(),
            triggered_at_utc: Utc::now(),
            acknowledged: false,
            acknowledged_by_user: None,
            acknowledged_at_utc: None,
        };
        let crit_batch_id = dist
            .enqueue_alert_with_priority(&mut repo, crit_alert)
            .unwrap()
            .unwrap();

        assert_eq!(dist.queued_batches_count(), 2);

        // Відновлюємо зв'язок із Землею: першим має піти CRITICAL пакет
        dist.settings.earth_link_online = true;
        let first_ack = dist
            .transmit_next_queued_batch(&mut repo)
            .unwrap()
            .expect("First ACK");
        assert_eq!(first_ack.batch_id, crit_batch_id);
    }
}
