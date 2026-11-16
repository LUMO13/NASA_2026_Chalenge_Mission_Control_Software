//! # `database` — in-memory domain repository with serializable snapshots.
//!
//! Desktop app handles local-file or SQLx PostgreSQL persistence for these structures.
//! SQL migrations live under `migrations/`; this crate itself does not issue SQL.
//! Присутні базова дедуплікація, незмінність ID і перевірки перед видаленням.

use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shared_core::{
    AlertEvent, AstronautProfile, LunarHmsError, LunarResult, SyncPriority, TelemetryEnvelope,
};

/// Запис у таблиці синхронізації із Землею (`earth_sync_batches`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarthSyncBatchRecord {
    pub batch_id: Uuid,
    pub created_at_utc: DateTime<Utc>,
    pub transmitted_at_utc: Option<DateTime<Utc>>,
    pub verified_at_utc: Option<DateTime<Utc>>,
    pub priority: SyncPriority,
    pub record_count: usize,
    pub compressed_size_bytes: usize,
    pub sha256_checksum: String,
    pub sync_status: String,
    pub retry_count: u32,
    pub record_ids: Vec<Uuid>,
}

/// Запис журналу аудиту безпеки та операцій (`audit_logs`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub log_id: Uuid,
    pub occurred_at_utc: DateTime<Utc>,
    pub actor_username: String,
    pub actor_role: String,
    pub action_type: String,
    pub target_resource: String,
    pub status: String,
    pub details: String,
}

/// In-memory domain store. The desktop app now persists a versioned snapshot to local JSON
/// or PostgreSQL; no background server is implied by this structure.

/// Serializable and backward-compatible snapshot of the running demonstration.
/// No credentials or session tokens are stored in this database structure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatabaseSnapshot {
    pub astronauts: Vec<AstronautProfile>,
    pub telemetry: Vec<TelemetryEnvelope>,
    pub alerts: Vec<AlertEvent>,
    pub sync_batches: Vec<EarthSyncBatchRecord>,
    pub audit_logs: Vec<AuditLogEntry>,
    pub synced_record_ids: Vec<Uuid>,
}

#[derive(Debug, Default)]
pub struct LunarDatabaseRepository {
    astronauts: HashMap<String, AstronautProfile>,
    telemetry_records: Vec<TelemetryEnvelope>,
    seen_record_ids: HashSet<Uuid>,
    seen_astronaut_nonces: HashSet<(String, u64)>,
    synced_record_ids: HashSet<Uuid>,
    alerts: Vec<AlertEvent>,
    sync_batches: HashMap<Uuid, EarthSyncBatchRecord>,
    audit_logs: Vec<AuditLogEntry>,
    postgres_online: bool,
    local_wal_buffer_path: String,
}

impl LunarDatabaseRepository {
    pub fn new(local_wal_buffer_path: &str) -> Self {
        let repo = Self {
            astronauts: HashMap::new(),
            telemetry_records: Vec::new(),
            seen_record_ids: HashSet::new(),
            seen_astronaut_nonces: HashSet::new(),
            synced_record_ids: HashSet::new(),
            alerts: Vec::new(),
            sync_batches: HashMap::new(),
            audit_logs: Vec::new(),
            postgres_online: false,
            local_wal_buffer_path: local_wal_buffer_path.to_string(),
        };
        repo
    }

    /// Return only the domain data that must survive process restart.
    pub fn export_snapshot(&self) -> DatabaseSnapshot {
        DatabaseSnapshot {
            astronauts: self.list_astronauts(),
            telemetry: self.telemetry_records.clone(),
            alerts: self.alerts.clone(),
            sync_batches: self.sync_batches.values().cloned().collect(),
            audit_logs: self.audit_logs.clone(),
            synced_record_ids: self.synced_record_ids.iter().copied().collect(),
        }
    }

    /// Reconstitute indexes from persisted rows; do not change astronaut identifiers.
    pub fn import_snapshot(&mut self, data: DatabaseSnapshot) -> LunarResult<()> {
        let mut fresh = Self::new(&self.local_wal_buffer_path);
        for astronaut in data.astronauts {
            fresh.insert_astronaut(astronaut)?;
        }
        for frame in data.telemetry {
            if fresh.astronauts.contains_key(&frame.astronaut_id) {
                fresh.insert_telemetry_record(frame)?;
            }
        }
        fresh.alerts = data.alerts;
        fresh.audit_logs = data.audit_logs;
        fresh.sync_batches = data.sync_batches.into_iter().map(|b| (b.batch_id, b)).collect();
        fresh.synced_record_ids = data.synced_record_ids.into_iter().collect();
        *self = fresh;
        Ok(())
    }

    pub fn set_postgres_online(&mut self, online: bool) {
        self.postgres_online = online;
    }

    pub fn is_postgres_online(&self) -> bool {
        self.postgres_online
    }

    pub fn local_wal_path(&self) -> &str {
        &self.local_wal_buffer_path
    }

    /// Додає нового космонавта. Перевіряє формат та унікальність `astronaut_id`.
    /// Існуючий `astronaut_id` не може бути перезаписаний або змінений.
    pub fn insert_astronaut(&mut self, profile: AstronautProfile) -> LunarResult<()> {
        let id = profile.astronaut_id().to_string();
        let suffix = id.strip_prefix("AST-").unwrap_or("");
        if !(3..=6).contains(&suffix.len()) || !suffix.bytes().all(|c| c.is_ascii_digit()) {
            return Err(LunarHmsError::ValidationFailure {
                field: "astronaut_id".to_string(),
                reason: "Незмінний ID космонавта повинен мати формат AST-XXX".to_string(),
            });
        }
        if !profile.height_cm.is_finite() || !(100.0..250.0).contains(&profile.height_cm)
            || !profile.baseline_mass_kg.is_finite()
            || !(30.0..200.0).contains(&profile.baseline_mass_kg)
        {
            return Err(LunarHmsError::ValidationFailure {
                field: "height_cm/baseline_mass_kg".to_string(),
                reason: "Зріст або маса виходять за межі схеми БД".to_string(),
            });
        }
        if self.astronauts.contains_key(&id) {
            return Err(LunarHmsError::ValidationFailure {
                field: "astronaut_id".to_string(),
                reason: format!("Космонавт із незмінним ID {} вже зареєстрований", id),
            });
        }
        self.astronauts.insert(id, profile);
        Ok(())
    }

    /// Оновлює метадані космонавта (ПІБ, роль, масу), гарантуючи незмінність `astronaut_id`.
    pub fn update_astronaut_metadata(
        &mut self,
        astronaut_id: &str,
        attempted_new_id: &str,
        full_name: String,
        mission_role: String,
        baseline_mass_kg: f64,
    ) -> LunarResult<()> {
        if astronaut_id != attempted_new_id {
            return Err(LunarHmsError::ValidationFailure {
                field: "astronaut_id".to_string(),
                reason: "Заборонено змінювати незмінний унікальний ID космонавта".to_string(),
            });
        }
        let entry = self.astronauts.get_mut(astronaut_id).ok_or_else(|| {
            LunarHmsError::ValidationFailure {
                field: "astronaut_id".to_string(),
                reason: format!("Космонавта {} не знайдено", astronaut_id),
            }
        })?;
        entry.full_name = full_name;
        entry.mission_role = mission_role;
        entry.baseline_mass_kg = baseline_mass_kg;
        Ok(())
    }

    pub fn list_astronauts(&self) -> Vec<AstronautProfile> {
        let mut list: Vec<AstronautProfile> = self.astronauts.values().cloned().collect();
        list.sort_by(|a, b| a.astronaut_id().cmp(b.astronaut_id()));
        list
    }

    /// Записує нове вимірювання телеметрії окремим записом із захистом від дублювання.
    /// Повертає `Ok(true)`, якщо запис новий, або `Ok(false)`, якщо дублікат було відхилено.
    pub fn insert_telemetry_record(&mut self, envelope: TelemetryEnvelope) -> LunarResult<bool> {
        if self.seen_record_ids.contains(&envelope.record_id) {
            return Ok(false);
        }
        let nonce_key = (envelope.astronaut_id.clone(), envelope.sequence_nonce);
        if self.seen_astronaut_nonces.contains(&nonce_key) {
            return Ok(false);
        }

        self.seen_record_ids.insert(envelope.record_id);
        self.seen_astronaut_nonces.insert(nonce_key);
        self.telemetry_records.push(envelope);
        Ok(true)
    }

    /// Повертає записи телеметрії космонавта за вказаний часовий інтервал UTC.
    pub fn query_telemetry_by_astronaut_and_range(
        &self,
        astronaut_id: &str,
        from_utc: DateTime<Utc>,
        to_utc: DateTime<Utc>,
    ) -> Vec<TelemetryEnvelope> {
        self.telemetry_records
            .iter()
            .filter(|r| {
                r.astronaut_id == astronaut_id
                    && r.measured_at_utc >= from_utc
                    && r.measured_at_utc <= to_utc
            })
            .cloned()
            .collect()
    }

    /// Повертає всі записи телеметрії, які ще не були синхронізовані із Землею.
    pub fn get_unsynced_telemetry(&self, limit: usize) -> Vec<TelemetryEnvelope> {
        self.telemetry_records
            .iter()
            .filter(|r| !self.synced_record_ids.contains(&r.record_id))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Реєструє пакет синхронізації із Землею.
    pub fn register_sync_batch(&mut self, batch: EarthSyncBatchRecord) {
        self.sync_batches.insert(batch.batch_id, batch);
    }

    /// Позначає пакет синхронізації як успішно переданий та криптографічно підтверджений Землею (`VERIFIED_ACK`).
    pub fn mark_batch_verified_by_earth(&mut self, batch_id: Uuid) -> LunarResult<()> {
        let batch = self.sync_batches.get_mut(&batch_id).ok_or_else(|| {
            LunarHmsError::StorageError(format!("Пакет синхронізації {} не знайдено", batch_id))
        })?;
        batch.sync_status = "VERIFIED_ACK".to_string();
        batch.verified_at_utc = Some(Utc::now());
        for rec_id in &batch.record_ids {
            self.synced_record_ids.insert(*rec_id);
        }
        Ok(())
    }

    /// Видаляє застарілі записи згідно з політикою зберігання ЛИШЕ за умови,
    /// що кожен запис уже був успішно переданий на Землю та підтверджений (`synced_record_ids`).
    pub fn purge_expired_verified_records(
        &mut self,
        older_than_utc: DateTime<Utc>,
    ) -> LunarResult<usize> {
        // Якщо серед записів, старіших за older_than_utc, є несинхронізовані — видалення цих записів заборонено!
        let unsynced_old_exists = self
            .telemetry_records
            .iter()
            .any(|r| r.measured_at_utc < older_than_utc && !self.synced_record_ids.contains(&r.record_id));

        if unsynced_old_exists {
            return Err(LunarHmsError::UnverifiedPurgeForbidden {
                batch_id: "UNSYNCED_TELEMETRY_RECORDS".to_string(),
            });
        }

        let before = self.telemetry_records.len();
        self.telemetry_records
            .retain(|r| r.measured_at_utc >= older_than_utc);
        let removed = before.saturating_sub(self.telemetry_records.len());
        Ok(removed)
    }

    pub fn insert_alert(&mut self, alert: AlertEvent) {
        self.alerts.push(alert);
    }

    pub fn list_alerts(&self) -> &[AlertEvent] {
        &self.alerts
    }

    pub fn acknowledge_alert(&mut self, alert_id: Uuid, user_id: Uuid) -> LunarResult<()> {
        let alert = self
            .alerts
            .iter_mut()
            .find(|a| a.alert_id == alert_id)
            .ok_or_else(|| LunarHmsError::StorageError("Сповіщення не знайдено".to_string()))?;
        alert.acknowledged = true;
        alert.acknowledged_by_user = Some(user_id);
        alert.acknowledged_at_utc = Some(Utc::now());
        Ok(())
    }

    pub fn append_audit_log(
        &mut self,
        actor_username: &str,
        actor_role: &str,
        action_type: &str,
        target_resource: &str,
        status: &str,
        details: &str,
    ) {
        self.audit_logs.push(AuditLogEntry {
            log_id: Uuid::new_v4(),
            occurred_at_utc: Utc::now(),
            actor_username: actor_username.to_string(),
            actor_role: actor_role.to_string(),
            action_type: action_type.to_string(),
            target_resource: target_resource.to_string(),
            status: status.to_string(),
            details: details.to_string(),
        });
    }

    pub fn list_audit_logs(&self) -> &[AuditLogEntry] {
        &self.audit_logs
    }

    pub fn list_sync_batches(&self) -> Vec<EarthSyncBatchRecord> {
        let mut out: Vec<_> = self.sync_batches.values().cloned().collect();
        out.sort_by(|a, b| b.created_at_utc.cmp(&a.created_at_utc));
        out
    }

    pub fn total_telemetry_count(&self) -> usize {
        self.telemetry_records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sensor_simulator::SensorSimulatorEngine;

    #[test]
    fn test_database_deduplication_and_immutable_astronaut_id() {
        let mut repo = LunarDatabaseRepository::new("data/test_wal.sqlite3");
        let secret = b"db_test_secret".to_vec();
        let mut sim = SensorSimulatorEngine::new_with_default_crew(secret);
        let profile = sim.astronauts.get("AST-001").unwrap().profile.clone();
        repo.insert_astronaut(profile).unwrap();

        let frame = sim.generate_next_envelope("AST-001").unwrap();
        assert_eq!(repo.insert_telemetry_record(frame.clone()).unwrap(), true);
        // Повторна вставка того самого запису ігнорується (захист від дублювання)
        assert_eq!(repo.insert_telemetry_record(frame.clone()).unwrap(), false);

        // Спроба змінити незмінний ID космонавта завершується помилкою
        let res = repo.update_astronaut_metadata(
            "AST-001",
            "AST-999",
            "Олександр Коваленко".to_string(),
            "Commander".to_string(),
            80.0,
        );
        assert!(res.is_err());

        // Спроба видалити несинхронізовані із Землею записи заборонена
        let future_cutoff = Utc::now() + chrono::Duration::hours(1);
        assert!(repo.purge_expired_verified_records(future_cutoff).is_err());
    }
}
