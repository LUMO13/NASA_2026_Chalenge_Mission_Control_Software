//! Визначення мережевих пакетів та протоколів обміну між мікросервісами.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{AlertEvent, SimulationScenario, SyncPriority, TelemetryEnvelope};

/// Команда керування симулятором датчиків (від панелі керування або GUI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimulatorControlCommand {
    SetRunning { running: bool },
    SetPollIntervalMs { interval_ms: u64 },
    SetScenario { astronaut_id: String, scenario: SimulationScenario },
    SetErrorMarginPct { error_margin_pct: f64 },
    SetSensorEnabled { astronaut_id: String, sensor_name: String, enabled: bool },
    SetSignalInstability { packet_loss_pct: f64, disconnected: bool },
    SetManualOverride {
        astronaut_id: String,
        parameter: String,
        manual_value: Option<f64>,
    },
}

/// Пакет накопиченої телеметрії для передавання на Землю.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarthUplinkBatch {
    pub batch_id: Uuid,
    pub created_at_utc: DateTime<Utc>,
    pub priority: SyncPriority,
    pub record_count: usize,
    pub compressed_payload_gzip: Vec<u8>,
    pub sha256_checksum: String,
    pub alerts: Vec<AlertEvent>,
    pub sample_record_ids: Vec<Uuid>,
}

/// Відповідь наземного центру управління (Mission Control Earth Station) на пакет синхронізації.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarthUplinkAck {
    pub batch_id: Uuid,
    pub verified: bool,
    pub received_at_utc: DateTime<Utc>,
    pub verified_sha256: String,
    pub station_id: String,
}

/// Повідомлення потоку даних реального часу для клієнтів GUI (через WebSocket / внутрішній канал).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RealtimeStreamEvent {
    TelemetryUpdate(Box<TelemetryEnvelope>),
    AlertTriggered(AlertEvent),
    SyncStatusChanged {
        batch_id: Uuid,
        status: String,
        pending_batches: usize,
        link_online: bool,
    },
}
