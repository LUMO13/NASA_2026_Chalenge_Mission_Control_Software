//! # `sensor-simulator` — Симулятор апаратного комплексу біомедичних та інженерних датчиків космонавта
//!
//! Забезпечує:
//! - реалістичну генерацію взаємопов'язаних фізіологічних параметрів (ЧСС, АТ, дихання, SpO2,
//!   температура тіла, глюкоза, лактат, маса/ІМТ, тривалість та якість сну, стрес);
//! - синтетичну морфологію сигналу ЕКГ (комплекс P-QRS-T, 250 Гц);
//! - окрему генерацію параметрів житлового модуля станції (`HabitatEnvironmentTelemetry`),
//!   радіаційного фону (`RadiationTelemetry`) та контуру скафандра (`SpacesuitTelemetry`);
//! - підтримку декількох космонавтів із незмінними ID (`AST-001` .. `AST-004`);
//! - налаштування інтервалу опитування, похибки сенсорів, ручного перевизначення значень,
//!   відключення окремих датчиків, втрати зв'язку та нестабільного сигналу.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shared_core::{
    compute_envelope_hmac, AstronautProfile, CardiovascularMetrics, HabitatEnvironmentTelemetry,
    MeasuredMetric, PhysiologicalAndSleepMetrics, RadiationTelemetry, RespiratoryMetrics,
    SimulationScenario, SimulatorControlCommand, SpacesuitTelemetry, TelemetryEnvelope,
};

/// Стан симуляції окремого космонавта у симуляторі.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstronautSimState {
    pub profile: AstronautProfile,
    pub scenario: SimulationScenario,
    pub sequence_nonce: u64,
    pub disabled_sensors: HashSet<String>,
    pub manual_overrides: HashMap<String, f64>,
    pub cumulative_absorbed_dose_mgy: f64,
    pub cumulative_equivalent_dose_msv: f64,
    pub phase_rad: f64,
}

impl AstronautSimState {
    pub fn new(profile: AstronautProfile) -> Self {
        Self {
            profile,
            scenario: SimulationScenario::Nominal,
            sequence_nonce: 1000,
            disabled_sensors: HashSet::new(),
            manual_overrides: HashMap::new(),
            cumulative_absorbed_dose_mgy: 12.0,
            cumulative_equivalent_dose_msv: 26.4,
            phase_rad: 0.0,
        }
    }
}

/// Головний рушій симулятора апаратних датчиків.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorSimulatorEngine {
    pub running: bool,
    pub poll_interval_ms: u64,
    pub error_margin_pct: f64,
    pub packet_loss_pct: f64,
    pub link_disconnected: bool,
    pub astronauts: HashMap<String, AstronautSimState>,
    hmac_secret: Vec<u8>,
}

impl SensorSimulatorEngine {
    /// Ініціалізація без попередньо створених космонавтів: профілі додаються через GUI.
    pub fn new_empty(hmac_secret: Vec<u8>) -> Self {
        Self {
            running: true,
            poll_interval_ms: 1000,
            error_margin_pct: 1.5,
            packet_loss_pct: 0.0,
            link_disconnected: false,
            astronauts: HashMap::new(),
            hmac_secret,
        }
    }

    /// Створює симулятор зі штатним екіпажем із 4 космонавтів місії Artemis-IV.
    pub fn new_with_default_crew(hmac_secret: Vec<u8>) -> Self {
        let mut astronauts = HashMap::new();
        let default_crew = vec![
            AstronautProfile::new(
                "AST-001".to_string(),
                "Олександр Коваленко".to_string(),
                "Commander".to_string(),
                "VULCAN-1".to_string(),
                182.0,
                79.5,
                "EVA-SUIT-01".to_string(),
            ),
            AstronautProfile::new(
                "AST-002".to_string(),
                "Ірина Мельник".to_string(),
                "Flight Surgeon / Science Officer".to_string(),
                "VULCAN-2".to_string(),
                168.0,
                61.2,
                "EVA-SUIT-02".to_string(),
            ),
            AstronautProfile::new(
                "AST-003".to_string(),
                "Тарас Бондаренко".to_string(),
                "Lunar Systems Engineer".to_string(),
                "EVA-SUIT-03".to_string(),
                178.0,
                75.8,
                "EVA-SUIT-03".to_string(),
            ),
            AstronautProfile::new(
                "AST-004".to_string(),
                "Софія Лисенко".to_string(),
                "EVA Payload Specialist".to_string(),
                "VULCAN-4".to_string(),
                171.0,
                64.0,
                "EVA-SUIT-04".to_string(),
            ),
        ];

        for profile in default_crew {
            astronauts.insert(
                profile.astronaut_id().to_string(),
                AstronautSimState::new(profile),
            );
        }

        Self {
            running: true,
            poll_interval_ms: 1000,
            error_margin_pct: 1.5,
            packet_loss_pct: 0.0,
            link_disconnected: false,
            astronauts,
            hmac_secret,
        }
    }

    /// Застосовує команду керування від панелі керування симулятором.
    pub fn apply_command(&mut self, cmd: SimulatorControlCommand) {
        match cmd {
            SimulatorControlCommand::SetRunning { running } => {
                self.running = running;
            }
            SimulatorControlCommand::SetPollIntervalMs { interval_ms } => {
                self.poll_interval_ms = interval_ms.clamp(100, 60_000);
            }
            SimulatorControlCommand::SetScenario {
                astronaut_id,
                scenario,
            } => {
                if let Some(state) = self.astronauts.get_mut(&astronaut_id) {
                    state.scenario = scenario;
                }
            }
            SimulatorControlCommand::SetErrorMarginPct { error_margin_pct } => {
                self.error_margin_pct = error_margin_pct.clamp(0.0, 25.0);
            }
            SimulatorControlCommand::SetSensorEnabled {
                astronaut_id,
                sensor_name,
                enabled,
            } => {
                if let Some(state) = self.astronauts.get_mut(&astronaut_id) {
                    if enabled {
                        state.disabled_sensors.remove(&sensor_name);
                    } else {
                        state.disabled_sensors.insert(sensor_name);
                    }
                }
            }
            SimulatorControlCommand::SetSignalInstability {
                packet_loss_pct,
                disconnected,
            } => {
                self.packet_loss_pct = packet_loss_pct.clamp(0.0, 100.0);
                self.link_disconnected = disconnected;
            }
            SimulatorControlCommand::SetManualOverride {
                astronaut_id,
                parameter,
                manual_value,
            } => {
                if let Some(state) = self.astronauts.get_mut(&astronaut_id) {
                    if let Some(val) = manual_value {
                        state.manual_overrides.insert(parameter, val);
                    } else {
                        state.manual_overrides.remove(&parameter);
                    }
                }
            }
        }
    }

    /// Генерує реалістичну морфологію сигналу ЕКГ (відведення II, зубці P, Q, R, S, T) у мВ.
    pub fn synthesize_ecg_waveform(heart_rate_bpm: f64, sample_rate_hz: u32, samples: usize) -> Vec<f64> {
        let mut out = Vec::with_capacity(samples);
        let beat_period_sec = 60.0 / heart_rate_bpm.max(30.0);
        for i in 0..samples {
            let t = (i as f64 / sample_rate_hz as f64) % beat_period_sec;
            let norm_t = t / beat_period_sec;
            // Сума гауссіанів для зубців P, Q, R, S, T
            let p_wave = 0.15 * (-((norm_t - 0.18) / 0.035).powi(2)).exp();
            let q_wave = -0.18 * (-((norm_t - 0.36) / 0.012).powi(2)).exp();
            let r_wave = 1.15 * (-((norm_t - 0.40) / 0.015).powi(2)).exp();
            let s_wave = -0.28 * (-((norm_t - 0.44) / 0.014).powi(2)).exp();
            let t_wave = 0.28 * (-((norm_t - 0.68) / 0.055).powi(2)).exp();
            let mv = p_wave + q_wave + r_wave + s_wave + t_wave;
            out.push((mv * 1000.0).round() / 1000.0);
        }
        out
    }

    fn apply_noise_and_override(
        val: f64,
        param_name: &str,
        error_margin_pct: f64,
        overrides: &HashMap<String, f64>,
    ) -> f64 {
        if let Some(&manual) = overrides.get(param_name) {
            return manual;
        }
        if error_margin_pct <= 0.0001 {
            return (val * 100.0).round() / 100.0;
        }
        let mut rng = rand::thread_rng();
        let max_delta = val.abs() * (error_margin_pct / 100.0);
        let delta = rng.gen_range(-max_delta..=max_delta);
        ((val + delta) * 100.0).round() / 100.0
    }

    /// Генерує черговий підписаний пакет телеметрії для вказаного космонавта.
    /// Повертає `None`, якщо симуляцію зупинено або імітується втрата пакета / розрив лінії.
    pub fn generate_next_envelope(&mut self, astronaut_id: &str) -> Option<TelemetryEnvelope> {
        if !self.running || self.link_disconnected {
            return None;
        }

        if self.packet_loss_pct > 0.0 {
            let mut rng = rand::thread_rng();
            if rng.gen_range(0.0..100.0) < self.packet_loss_pct {
                return None;
            }
        }

        let state = self.astronauts.get_mut(astronaut_id)?;
        state.sequence_nonce += 1;
        state.phase_rad = (state.phase_rad + 0.25) % (2.0 * PI);
        let wave = state.phase_rad.sin();

        // Фізіологічно взаємопов'язані базові показники залежно від активного сценарію
        let (
            base_hr,
            base_sys,
            base_dia,
            base_resp,
            base_tidal_ml,
            base_spo2,
            base_temp,
            base_gluc,
            base_lactate,
            base_stress,
            base_sleep_h,
            base_awakenings,
            base_sleep_qual,
            base_suit_p_kpa,
            base_rad_rate_usv_h,
        ) = match state.scenario {
            SimulationScenario::Nominal => (
                68.0 + 3.0 * wave,
                118.0 + 2.5 * wave,
                75.0 + 1.5 * wave,
                14.0 + 0.8 * wave,
                500.0 + 15.0 * wave,
                98.5 + 0.3 * wave,
                36.68 + 0.08 * wave,
                5.1 + 0.15 * wave,
                1.1 + 0.1 * wave,
                18.0 + 3.0 * wave,
                7.7,
                1,
                89.0,
                30.1 + 0.1 * wave,
                21.5 + 1.5 * wave,
            ),
            SimulationScenario::PhysicalExertion => (
                142.0 + 6.0 * wave,
                148.0 + 5.0 * wave,
                86.0 + 3.0 * wave,
                28.5 + 2.0 * wave,
                920.0 + 40.0 * wave,
                96.2 + 0.4 * wave,
                37.75 + 0.12 * wave,
                6.3 + 0.25 * wave,
                4.4 + 0.3 * wave,
                48.0 + 4.0 * wave,
                7.2,
                1,
                82.0,
                30.4 + 0.2 * wave,
                24.0 + 2.0 * wave,
            ),
            SimulationScenario::HighStress => (
                114.0 + 5.0 * wave,
                141.0 + 4.0 * wave,
                92.0 + 3.0 * wave,
                22.0 + 1.5 * wave,
                610.0 + 25.0 * wave,
                97.4 + 0.3 * wave,
                37.15 + 0.1 * wave,
                6.9 + 0.2 * wave,
                2.1 + 0.2 * wave,
                81.0 + 4.0 * wave,
                5.8,
                3,
                61.0,
                30.0 + 0.15 * wave,
                22.0 + 1.5 * wave,
            ),
            SimulationScenario::SleepDeprivation => (
                82.0 + 3.5 * wave,
                129.0 + 3.0 * wave,
                84.0 + 2.0 * wave,
                16.5 + 1.0 * wave,
                460.0 + 20.0 * wave,
                97.2 + 0.3 * wave,
                36.42 + 0.08 * wave,
                5.9 + 0.2 * wave,
                1.7 + 0.15 * wave,
                68.0 + 3.0 * wave,
                3.6,
                6,
                38.0,
                30.1 + 0.1 * wave,
                21.8 + 1.2 * wave,
            ),
            SimulationScenario::EmergencyHypoxia => (
                168.0 + 5.0 * wave,
                172.0 + 6.0 * wave,
                105.0 + 4.0 * wave,
                35.5 + 2.5 * wave,
                370.0 + 25.0 * wave,
                84.2 + 0.8 * wave,
                38.45 + 0.15 * wave,
                8.4 + 0.3 * wave,
                6.9 + 0.4 * wave,
                94.0 + 2.0 * wave,
                5.1,
                5,
                45.0,
                21.4 + 0.4 * wave, // критичне падіння тиску у скафандрі
                32.0 + 2.5 * wave,
            ),
            SimulationScenario::SolarParticleEvent => (
                96.0 + 4.0 * wave,
                132.0 + 3.5 * wave,
                84.0 + 2.5 * wave,
                19.0 + 1.2 * wave,
                540.0 + 20.0 * wave,
                97.8 + 0.3 * wave,
                36.95 + 0.1 * wave,
                5.7 + 0.2 * wave,
                1.8 + 0.2 * wave,
                74.0 + 3.5 * wave,
                6.5,
                2,
                71.0,
                30.2 + 0.1 * wave,
                620.0 + 35.0 * wave, // радіаційна тривога SPE > 500 µSv/h
            ),
        };

        let err = self.error_margin_pct;
        let ov = &state.manual_overrides;

        let hr_val = Self::apply_noise_and_override(base_hr, "heart_rate_bpm", err, ov);
        let sys_val = Self::apply_noise_and_override(base_sys, "systolic_bp_mmhg", err, ov);
        let dia_val = Self::apply_noise_and_override(base_dia, "diastolic_bp_mmhg", err, ov);
        let resp_val = Self::apply_noise_and_override(base_resp, "respiration_rate_cpm", err, ov);
        let tidal_val = Self::apply_noise_and_override(base_tidal_ml, "tidal_volume_ml", err, ov);
        let spo2_val = Self::apply_noise_and_override(base_spo2, "spo2_pct", err, ov).clamp(50.0, 100.0);
        let temp_val = Self::apply_noise_and_override(base_temp, "core_body_temp_c", err, ov);
        let gluc_val = Self::apply_noise_and_override(base_gluc, "glucose_mmol_l", err, ov);
        let lact_val = Self::apply_noise_and_override(base_lactate, "lactate_mmol_l", err, ov);
        let stress_val = Self::apply_noise_and_override(base_stress, "stress_index_0_100", err, ov).clamp(0.0, 100.0);
        let suit_p_val = Self::apply_noise_and_override(base_suit_p_kpa, "suit_internal_pressure_kpa", err, ov);
        let rad_rate_val = Self::apply_noise_and_override(base_rad_rate_usv_h, "equivalent_dose_rate_usv_h", err, ov).max(0.0);

        let dt_hours = (self.poll_interval_ms as f64) / 3_600_000.0;
        state.cumulative_equivalent_dose_msv += (rad_rate_val * dt_hours) / 1000.0;
        state.cumulative_absorbed_dose_mgy += (rad_rate_val * dt_hours * 0.45) / 1000.0;

        let ecg_waveform = if state.disabled_sensors.contains("ecg") {
            Vec::new()
        } else {
            Self::synthesize_ecg_waveform(hr_val, 250, 25)
        };
        let ecg_instant = ecg_waveform.last().copied().unwrap_or(0.0);

        let height_m = state.profile.height_cm / 100.0;
        let bmi_val = ((state.profile.baseline_mass_kg / (height_m * height_m)) * 100.0).round() / 100.0;

        let opt_metric = |name: &str, val: f64, unit: &str, sensor: &str, hz: f64, bus: &str| {
            if state.disabled_sensors.contains(name) {
                None
            } else {
                Some(MeasuredMetric::new(val, unit, sensor, hz, bus, true))
            }
        };

        let now = Utc::now();

        let cardiovascular = CardiovascularMetrics {
            heart_rate_bpm: opt_metric(
                "heart_rate_bpm",
                hr_val,
                "bpm",
                "Optical PPG + 3-Lead Chest ECG Patch",
                1.0,
                "BIOMED-SPI-BUS-01",
            ),
            ecg_lead_ii_mv: opt_metric(
                "ecg",
                ecg_instant,
                "mV",
                "3-Lead Chest Bio-Potential Amplifier",
                250.0,
                "BIOMED-SPI-BUS-01",
            ),
            ecg_waveform_mv: ecg_waveform,
            ecg_sampling_rate_hz: 250,
            systolic_bp_mmhg: opt_metric(
                "blood_pressure",
                sys_val,
                "mmHg",
                "Optical Pulse Transit Time Cuffless Sensor",
                0.033,
                "BIOMED-I2C-BUS-01",
            ),
            diastolic_bp_mmhg: opt_metric(
                "blood_pressure",
                dia_val,
                "mmHg",
                "Optical Pulse Transit Time Cuffless Sensor",
                0.033,
                "BIOMED-I2C-BUS-01",
            ),
        };

        let respiratory = RespiratoryMetrics {
            respiration_rate_cpm: opt_metric(
                "respiration",
                resp_val,
                "cycles/min",
                "Thoracic Bio-Impedance Pneumography Belt",
                1.0,
                "BIOMED-SPI-BUS-02",
            ),
            tidal_volume_ml: opt_metric(
                "respiration",
                tidal_val,
                "mL",
                "Thoracic Bio-Impedance Pneumography Belt",
                1.0,
                "BIOMED-SPI-BUS-02",
            ),
            spo2_pct: opt_metric(
                "spo2",
                spo2_val,
                "%",
                "Dual-Wavelength Finger/Wrist Pulse Oximeter",
                1.0,
                "BIOMED-I2C-BUS-02",
            ),
        };

        let physiological = PhysiologicalAndSleepMetrics {
            core_body_temp_c: opt_metric(
                "core_temp",
                temp_val,
                "°C",
                "Tympanic IR / Ingestible Core Pill",
                0.2,
                "BIOMED-BLE-CH-01",
            ),
            glucose_mmol_l: opt_metric(
                "glucose",
                gluc_val,
                "mmol/L",
                "Subcutaneous Continuous Glucose Monitor (CGM)",
                0.0033,
                "BIOMED-BLE-CH-02",
            ),
            lactate_mmol_l: opt_metric(
                "lactate",
                lact_val,
                "mmol/L",
                "Electrochemical Interstitial Sweat Biosensor",
                0.016,
                "BIOMED-BLE-CH-03",
            ),
            body_mass_kg: Some(MeasuredMetric::new(
                state.profile.baseline_mass_kg,
                "kg",
                "Microgravity Linear Inertial Mass Rig",
                0.0001,
                "HAB-CAN-BUS-01",
                true,
            )),
            height_cm: Some(MeasuredMetric::new(
                state.profile.height_cm,
                "cm",
                "Laser Anthropometric Scanner",
                0.0001,
                "HAB-CAN-BUS-01",
                true,
            )),
            bmi_kg_m2: Some(MeasuredMetric::new(
                bmi_val,
                "kg/m²",
                "Computed Anthropometric Index",
                0.0001,
                "HAB-CAN-BUS-01",
                true,
            )),
            sleep_duration_hours: Some(MeasuredMetric::new(
                base_sleep_h,
                "h",
                "Wrist Actigraphy + EEG Headband",
                0.001,
                "BIOMED-BLE-CH-04",
                true,
            )),
            sleep_awakenings_count: Some(MeasuredMetric::new(
                base_awakenings,
                "count",
                "Wrist Actigraphy + EEG Headband",
                0.001,
                "BIOMED-BLE-CH-04",
                true,
            )),
            stress_index_0_100: Some(MeasuredMetric::new(
                stress_val,
                "scale_0_100",
                "HRV RMSSD/LF-HF Autonomic Analyzer",
                0.2,
                "BIOMED-DSP-CORE",
                true,
            )),
            sleep_quality_0_100: Some(MeasuredMetric::new(
                base_sleep_qual,
                "scale_0_100",
                "Polysomnography Sleep Architecture Scorer",
                0.001,
                "BIOMED-DSP-CORE",
                true,
            )),
        };

        let hab_co2 = Self::apply_noise_and_override(720.0 + 40.0 * wave, "co2_concentration_ppm", err, ov);
        let habitat = HabitatEnvironmentTelemetry {
            habitat_record_id: Uuid::new_v4(),
            module_id: "HAB-ALPHA-CORE".to_string(),
            measured_at_utc: now,
            atmospheric_pressure_kpa: MeasuredMetric::new(
                101.1 + 0.15 * wave,
                "kPa",
                "MEMS Piezoresistive Barometer",
                0.5,
                "ECLSS-MODBUS-01",
                true,
            ),
            o2_concentration_pct: MeasuredMetric::new(
                20.85 + 0.05 * wave,
                "% vol",
                "Galvanic Zirconia O2 Analyzer",
                0.5,
                "ECLSS-MODBUS-01",
                true,
            ),
            co2_concentration_ppm: MeasuredMetric::new(
                hab_co2,
                "ppm",
                "NDIR Optical CO2 Sensor",
                0.5,
                "ECLSS-MODBUS-01",
                true,
            ),
            nitrogen_pct: MeasuredMetric::new(
                78.1,
                "% vol",
                "Mass Spectrometer Gas Monitor",
                0.1,
                "ECLSS-MODBUS-01",
                true,
            ),
            trace_co_ppm: MeasuredMetric::new(
                0.4,
                "ppm",
                "Electrochemical Trace CO Detector",
                0.5,
                "ECLSS-MODBUS-01",
                true,
            ),
            air_temperature_c: MeasuredMetric::new(
                21.9 + 0.2 * wave,
                "°C",
                "Pt100 RTD Ambient Thermometer",
                0.5,
                "ECLSS-MODBUS-02",
                true,
            ),
            humidity_rh_pct: MeasuredMetric::new(
                45.0 + 1.0 * wave,
                "% RH",
                "Capacitive Polymer Hygrometer",
                0.5,
                "ECLSS-MODBUS-02",
                true,
            ),
            illuminance_lx: Some(MeasuredMetric::new(
                430.0 + 10.0 * wave,
                "lx",
                "Silicon Photodiode Circadian Luxmeter",
                0.2,
                "ECLSS-MODBUS-02",
                true,
            )),
        };

        let radiation = RadiationTelemetry {
            radiation_record_id: Uuid::new_v4(),
            astronaut_id: state.profile.astronaut_id().to_string(),
            module_id: "HAB-ALPHA-CORE".to_string(),
            measured_at_utc: now,
            absorbed_dose_mgy: MeasuredMetric::new(
                (state.cumulative_absorbed_dose_mgy * 1000.0).round() / 1000.0,
                "mGy",
                "Active Silicon PIN Dosimeter",
                0.2,
                "RAD-RS485-BUS",
                true,
            ),
            equivalent_dose_msv: MeasuredMetric::new(
                (state.cumulative_equivalent_dose_msv * 1000.0).round() / 1000.0,
                "mSv",
                "Tissue-Equivalent Proportional Counter (TEPC)",
                0.2,
                "RAD-RS485-BUS",
                true,
            ),
            equivalent_dose_rate_usv_h: MeasuredMetric::new(
                rad_rate_val,
                "µSv/h",
                "TEPC Real-Time Dose Rate Spectrometer",
                0.2,
                "RAD-RS485-BUS",
                true,
            ),
            cosmic_ray_flux_cm2_s: MeasuredMetric::new(
                (rad_rate_val / 6.2 * 100.0).round() / 100.0,
                "particles/(cm²·s)",
                "Charged Particle Telescope Detector",
                0.2,
                "RAD-RS485-BUS",
                true,
            ),
        };

        let spacesuit = SpacesuitTelemetry {
            suit_record_id: Uuid::new_v4(),
            suit_id: state.profile.assigned_suit_id.clone(),
            astronaut_id: state.profile.astronaut_id().to_string(),
            measured_at_utc: now,
            suit_spo2_pct: MeasuredMetric::new(
                spo2_val,
                "%",
                "EVA Helmet Forehead Optical PPG/SpO2",
                1.0,
                "PLSS-CAN-BUS",
                true,
            ),
            suit_heart_rate_bpm: MeasuredMetric::new(
                hr_val,
                "bpm",
                "EVA Helmet Forehead Optical PPG/SpO2",
                1.0,
                "PLSS-CAN-BUS",
                true,
            ),
            suit_internal_pressure_kpa: MeasuredMetric::new(
                suit_p_val,
                "kPa",
                "Redundant Dual-Channel PLSS Pressure Sensor",
                2.0,
                "PLSS-CAN-BUS",
                true,
            ),
            suit_internal_temp_c: MeasuredMetric::new(
                24.2 + 0.4 * wave,
                "°C",
                "LCVG Sublimator Loop Thermistor",
                1.0,
                "PLSS-CAN-BUS",
                true,
            ),
            suit_o2_pct: MeasuredMetric::new(
                if state.scenario == SimulationScenario::EmergencyHypoxia {
                    17.8
                } else {
                    99.2
                },
                "% vol",
                "PLSS Primary Oxygen Loop Sensor",
                1.0,
                "PLSS-CAN-BUS",
                true,
            ),
            suit_humidity_rh_pct: MeasuredMetric::new(
                52.0 + 2.0 * wave,
                "% RH",
                "PLSS Water Separator Humidity Sensor",
                0.5,
                "PLSS-CAN-BUS",
                true,
            ),
        };

        let mut envelope = TelemetryEnvelope {
            record_id: Uuid::new_v4(),
            sequence_nonce: state.sequence_nonce,
            astronaut_id: state.profile.astronaut_id().to_string(),
            measured_at_utc: now,
            received_at_utc: None,
            scenario: state.scenario,
            cardiovascular,
            respiratory,
            physiological,
            habitat,
            radiation,
            spacesuit,
            hmac_sha256_signature: String::new(),
        };

        envelope.hmac_sha256_signature = compute_envelope_hmac(&envelope, &self.hmac_secret);
        Some(envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_core::verify_envelope_hmac;

    #[test]
    fn test_simulator_generates_valid_signed_envelopes_and_scenarios() {
        let secret = b"test_lunar_hmac_secret_key_2026".to_vec();
        let mut sim = SensorSimulatorEngine::new_with_default_crew(secret.clone());

        let env_nom = sim.generate_next_envelope("AST-001").expect("Nominal frame");
        assert!(verify_envelope_hmac(&env_nom, &secret).is_ok());
        assert_eq!(env_nom.astronaut_id, "AST-001");
        assert_eq!(env_nom.ecg_sampling_rate_check(), 250);

        sim.apply_command(SimulatorControlCommand::SetScenario {
            astronaut_id: "AST-001".to_string(),
            scenario: SimulationScenario::EmergencyHypoxia,
        });
        let env_emerg = sim.generate_next_envelope("AST-001").expect("Emergency frame");
        assert!(
            env_emerg.respiratory.spo2_pct.unwrap().value < 88.0,
            "SpO2 має падати під час сценарію гіпоксії"
        );
    }
}

trait EcgCheckExt {
    fn ecg_sampling_rate_check(&self) -> u32;
}

impl EcgCheckExt for TelemetryEnvelope {
    fn ecg_sampling_rate_check(&self) -> u32 {
        self.cardiovascular.ecg_sampling_rate_hz
    }
}
