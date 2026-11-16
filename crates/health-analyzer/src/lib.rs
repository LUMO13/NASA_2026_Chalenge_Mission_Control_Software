//! # `health-analyzer` — Аналізатор стану екіпажу, тенденцій та керування життєвим циклом даних
//!
//! Забезпечує:
//! - детермінований реальний аналіз біомедичних показників, параметрів середовища,
//!   радіаційного навантаження та скафандра (без жодної випадкової генерації аварій);
//! - обчислення швидкості зміни показників (лінійний тренд за хвилину `slope_per_min`)
//!   на ковзному вікні історичних вимірювань;
//! - генерацію 3 рівнів сповіщень (`INFO`, `WARNING`, `CRITICAL`) на основі налаштовуваних порогів;
//! - керування життєвим циклом даних та планувальник синхронізації у вікнах зв'язку із Землею.

use std::collections::{HashMap, VecDeque};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use database::LunarDatabaseRepository;
use shared_core::{
    AlertEvent, AlertSeverity, LunarResult, ParameterThreshold, SyncPriority, TelemetryEnvelope,
};

/// Набір конфігурованих порогів аналізатора (демонстраційний прототип).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigurableThresholdSet {
    pub heart_rate: ParameterThreshold,
    pub spo2: ParameterThreshold,
    pub core_temp: ParameterThreshold,
    pub co2_ppm: ParameterThreshold,
    pub radiation_rate: ParameterThreshold,
    pub suit_pressure: ParameterThreshold,
}

impl Default for ConfigurableThresholdSet {
    fn default() -> Self {
        Self {
            heart_rate: ParameterThreshold::default_heart_rate(),
            spo2: ParameterThreshold::default_spo2(),
            core_temp: ParameterThreshold::default_core_temp(),
            co2_ppm: ParameterThreshold::default_co2_ppm(),
            radiation_rate: ParameterThreshold::default_radiation_rate(),
            suit_pressure: ParameterThreshold::default_suit_pressure(),
        }
    }
}

/// Вікно зв'язку із Землею для планувальника синхронізації.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationWindow {
    pub window_id: String,
    pub ground_station: String,
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
    pub bandwidth_bytes_per_sec: usize,
    pub channel_healthy: bool,
}

/// План передачі пакетів під час активного вікна зв'язку.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledSyncPlan {
    pub window_id: String,
    pub selected_batch_ids: Vec<Uuid>,
    pub total_bytes_scheduled: usize,
    pub estimated_duration_secs: f64,
}

/// Аналізатор фізіологічного стану, тенденцій та життєвого циклу бази даних.
pub struct HealthAnalyzerEngine {
    pub thresholds: ConfigurableThresholdSet,
    history_window: HashMap<(String, String), VecDeque<(DateTime<Utc>, f64)>>,
    max_window_samples: usize,
}

impl HealthAnalyzerEngine {
    pub fn new(thresholds: ConfigurableThresholdSet) -> Self {
        Self {
            thresholds,
            history_window: HashMap::new(),
            max_window_samples: 30,
        }
    }

    /// Оновлює історію параметра та обчислює лінійний нахил тренду (зміна за 1 хвилину).
    fn record_and_compute_slope(
        &mut self,
        astronaut_id: &str,
        param_name: &str,
        ts: DateTime<Utc>,
        value: f64,
    ) -> f64 {
        let key = (astronaut_id.to_string(), param_name.to_string());
        let series = self.history_window.entry(key).or_default();
        series.push_back((ts, value));
        if series.len() > self.max_window_samples {
            series.pop_front();
        }
        if series.len() < 2 {
            return 0.0;
        }
        let (first_ts, first_val) = series.front().copied().unwrap();
        let (last_ts, last_val) = series.back().copied().unwrap();
        let dt_mins = ((last_ts - first_ts).num_milliseconds() as f64) / 60_000.0;
        if dt_mins.abs() < 1e-6 {
            0.0
        } else {
            ((last_val - first_val) / dt_mins * 100.0).round() / 100.0
        }
    }

    /// Оцінює одне виміряне значення відносно конфігурованого порогу `ParameterThreshold`.
    fn evaluate_single_metric(
        &mut self,
        astronaut_id: &str,
        subsystem: &str,
        ts: DateTime<Utc>,
        value: f64,
        threshold: &ParameterThreshold,
    ) -> Option<AlertEvent> {
        let slope =
            self.record_and_compute_slope(astronaut_id, &threshold.parameter_name, ts, value);

        let (severity, limit, desc) = if value <= threshold.critical_low {
            (
                AlertSeverity::Critical,
                threshold.critical_low,
                format!(
                    "Критично низьке значення {} ({:.2} {} <= критичного мінімуму {:.2} {})",
                    threshold.parameter_name, value, threshold.unit, threshold.critical_low, threshold.unit
                ),
            )
        } else if value >= threshold.critical_high && threshold.critical_high < 99.999_f64.max(threshold.warning_high + 0.001) || (threshold.parameter_name != "spo2_pct" && value >= threshold.critical_high) {
            (
                AlertSeverity::Critical,
                threshold.critical_high,
                format!(
                    "Критично високе значення {} ({:.2} {} >= критичного максимуму {:.2} {})",
                    threshold.parameter_name, value, threshold.unit, threshold.critical_high, threshold.unit
                ),
            )
        } else if value <= threshold.warning_low {
            (
                AlertSeverity::Warning,
                threshold.warning_low,
                format!(
                    "Потенційно небезпечне зниження {} ({:.2} {} <= порогу WARNING {:.2} {})",
                    threshold.parameter_name, value, threshold.unit, threshold.warning_low, threshold.unit
                ),
            )
        } else if threshold.parameter_name != "spo2_pct" && value >= threshold.warning_high {
            (
                AlertSeverity::Warning,
                threshold.warning_high,
                format!(
                    "Потенційно небезпечне перевищення {} ({:.2} {} >= порогу WARNING {:.2} {})",
                    threshold.parameter_name, value, threshold.unit, threshold.warning_high, threshold.unit
                ),
            )
        } else if value < threshold.info_low {
            (
                AlertSeverity::Info,
                threshold.info_low,
                format!(
                    "Інформаційне відхилення {} нижче штатного діапазону ({:.2} {} < {:.2} {})",
                    threshold.parameter_name, value, threshold.unit, threshold.info_low, threshold.unit
                ),
            )
        } else if threshold.parameter_name != "spo2_pct" && value > threshold.info_high {
            (
                AlertSeverity::Info,
                threshold.info_high,
                format!(
                    "Інформаційне відхилення {} вище штатного діапазону ({:.2} {} > {:.2} {})",
                    threshold.parameter_name, value, threshold.unit, threshold.info_high, threshold.unit
                ),
            )
        } else {
            return None;
        };

        Some(AlertEvent {
            alert_id: Uuid::new_v4(),
            astronaut_id: Some(astronaut_id.to_string()),
            source_subsystem: subsystem.to_string(),
            parameter_name: threshold.parameter_name.clone(),
            severity,
            measured_value: value,
            unit: threshold.unit.clone(),
            threshold_limit: limit,
            trend_slope_per_min: slope,
            message: format!("[{}] Космонавт {}: {}", severity.as_str(), astronaut_id, desc),
            triggered_at_utc: Utc::now(),
            acknowledged: false,
            acknowledged_by_user: None,
            acknowledged_at_utc: None,
        })
    }

    /// Виконує повний детермінований аналіз кадру телеметрії та зберігає виявлені події в БД.
    pub fn analyze_envelope(
        &mut self,
        repo: &mut LunarDatabaseRepository,
        env: &TelemetryEnvelope,
    ) -> Vec<AlertEvent> {
        let mut generated = Vec::new();
        let ast_id = &env.astronaut_id;
        let ts = env.measured_at_utc;

        if let Some(ref hr) = env.cardiovascular.heart_rate_bpm {
            let th = self.thresholds.heart_rate.clone();
            if let Some(alert) = self.evaluate_single_metric(ast_id, "BIOMEDICAL", ts, hr.value, &th) {
                repo.insert_alert(alert.clone());
                generated.push(alert);
            }
        }

        if let Some(ref spo2) = env.respiratory.spo2_pct {
            let th = self.thresholds.spo2.clone();
            if let Some(alert) =
                self.evaluate_single_metric(ast_id, "BIOMEDICAL", ts, spo2.value, &th)
            {
                repo.insert_alert(alert.clone());
                generated.push(alert);
            }
        }

        if let Some(ref temp) = env.physiological.core_body_temp_c {
            let th = self.thresholds.core_temp.clone();
            if let Some(alert) =
                self.evaluate_single_metric(ast_id, "BIOMEDICAL", ts, temp.value, &th)
            {
                repo.insert_alert(alert.clone());
                generated.push(alert);
            }
        }

        {
            let th = self.thresholds.co2_ppm.clone();
            let co2_val = env.habitat.co2_concentration_ppm.value;
            if let Some(alert) =
                self.evaluate_single_metric(ast_id, "HABITAT_ECLSS", ts, co2_val, &th)
            {
                repo.insert_alert(alert.clone());
                generated.push(alert);
            }
        }

        {
            let th = self.thresholds.radiation_rate.clone();
            let rad_val = env.radiation.equivalent_dose_rate_usv_h.value;
            if let Some(alert) =
                self.evaluate_single_metric(ast_id, "RADIATION_DOSIMETRY", ts, rad_val, &th)
            {
                repo.insert_alert(alert.clone());
                generated.push(alert);
            }
        }

        {
            let th = self.thresholds.suit_pressure.clone();
            let suit_p = env.spacesuit.suit_internal_pressure_kpa.value;
            if let Some(alert) =
                self.evaluate_single_metric(ast_id, "SPACESUIT_EVA", ts, suit_p, &th)
            {
                repo.insert_alert(alert.clone());
                generated.push(alert);
            }
        }

        generated
    }

    /// Планувальник синхронізації із Землею: відбирає пакети для передачі у вікні зв'язку
    /// з урахуванням пріоритету (`CRITICAL_IMMEDIATE` -> `HIGH_ALERT` -> `NORMAL_TELEMETRY` -> `ARCHIVE_BULK`),
    /// пропускної здатності каналу та тривалості сеансу зв'язку.
    pub fn plan_sync_for_window(
        &self,
        repo: &LunarDatabaseRepository,
        window: &CommunicationWindow,
    ) -> LunarResult<ScheduledSyncPlan> {
        if !window.channel_healthy || window.end_utc <= window.start_utc {
            return Ok(ScheduledSyncPlan {
                window_id: window.window_id.clone(),
                selected_batch_ids: Vec::new(),
                total_bytes_scheduled: 0,
                estimated_duration_secs: 0.0,
            });
        }

        let window_secs = (window.end_utc - window.start_utc).num_seconds().max(1) as usize;
        let max_window_bytes = window_secs.saturating_mul(window.bandwidth_bytes_per_sec);

        let mut queued: Vec<_> = repo
            .list_sync_batches()
            .into_iter()
            .filter(|b| b.sync_status == "QUEUED" || b.sync_status == "FAILED_RETRY")
            .collect();

        // Сортування за спаданням пріоритету, а всередині пріоритету — за часом створення (FIFO)
        queued.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at_utc.cmp(&b.created_at_utc))
        });

        let mut selected_batch_ids = Vec::new();
        let mut total_bytes = 0usize;

        for batch in queued {
            if total_bytes + batch.compressed_size_bytes <= max_window_bytes {
                total_bytes += batch.compressed_size_bytes;
                selected_batch_ids.push(batch.batch_id);
            }
        }

        let est_secs = if window.bandwidth_bytes_per_sec > 0 {
            (total_bytes as f64) / (window.bandwidth_bytes_per_sec as f64)
        } else {
            0.0
        };

        Ok(ScheduledSyncPlan {
            window_id: window.window_id.clone(),
            selected_batch_ids,
            total_bytes_scheduled: total_bytes,
            estimated_duration_secs: (est_secs * 100.0).round() / 100.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sensor_simulator::SensorSimulatorEngine;
    use shared_core::{SimulationScenario, SimulatorControlCommand};

    #[test]
    fn test_analyzer_detects_real_thresholds_and_levels() {
        let mut repo = LunarDatabaseRepository::new("data/test_analyzer.sqlite3");
        let mut analyzer = HealthAnalyzerEngine::new(ConfigurableThresholdSet::default());
        let secret = b"analyzer_test_key".to_vec();
        let mut sim = SensorSimulatorEngine::new_with_default_crew(secret);
        sim.error_margin_pct = 0.0;

        // 1. Номінальний стан — критичних чи попереджувальних тривог немає
        let nom = sim.generate_next_envelope("AST-001").unwrap();
        let alerts_nom = analyzer.analyze_envelope(&mut repo, &nom);
        assert!(alerts_nom.is_empty());

        // 2. Фізичне навантаження — очікуємо WARNING за підвищеною ЧСС
        sim.apply_command(SimulatorControlCommand::SetScenario {
            astronaut_id: "AST-001".to_string(),
            scenario: SimulationScenario::PhysicalExertion,
        });
        let exert = sim.generate_next_envelope("AST-001").unwrap();
        let alerts_exert = analyzer.analyze_envelope(&mut repo, &exert);
        assert!(alerts_exert
            .iter()
            .any(|a| a.severity == AlertSeverity::Warning && a.parameter_name == "heart_rate_bpm"));

        // 3. Аварійна гіпоксія — очікуємо CRITICAL за падінням SpO2 та тиску у скафандрі
        sim.apply_command(SimulatorControlCommand::SetScenario {
            astronaut_id: "AST-001".to_string(),
            scenario: SimulationScenario::EmergencyHypoxia,
        });
        let emerg = sim.generate_next_envelope("AST-001").unwrap();
        let alerts_emerg = analyzer.analyze_envelope(&mut repo, &emerg);
        assert!(alerts_emerg
            .iter()
            .any(|a| a.severity == AlertSeverity::Critical && a.parameter_name == "spo2_pct"));
        assert!(alerts_emerg.iter().any(|a| a.severity == AlertSeverity::Critical
            && a.parameter_name == "suit_internal_pressure_kpa"));
    }
}
