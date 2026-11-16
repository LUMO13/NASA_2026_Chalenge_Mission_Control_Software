//! Завантаження конфігурації та порогів тривог із TOML та змінних середовища.

use serde::{Deserialize, Serialize};

/// Порогові межі для одного фізіологічного чи інженерного параметра.
/// Є повністю конфігурованими у демонстраційному прототипі.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterThreshold {
    pub parameter_name: String,
    pub unit: String,
    pub sensor_type: String,
    pub sampling_rate_hz: f64,
    pub info_low: f64,
    pub info_high: f64,
    pub warning_low: f64,
    pub warning_high: f64,
    pub critical_low: f64,
    pub critical_high: f64,
}

impl ParameterThreshold {
    pub fn default_heart_rate() -> Self {
        Self {
            parameter_name: "heart_rate_bpm".to_string(),
            unit: "bpm".to_string(),
            sensor_type: "Optical PPG + 3-Lead Chest ECG Patch".to_string(),
            sampling_rate_hz: 1.0,
            info_low: 55.0,
            info_high: 105.0,
            warning_low: 48.0,
            warning_high: 135.0,
            critical_low: 40.0,
            critical_high: 165.0,
        }
    }

    pub fn default_spo2() -> Self {
        Self {
            parameter_name: "spo2_pct".to_string(),
            unit: "%".to_string(),
            sensor_type: "Dual-Wavelength Pulse Oximeter".to_string(),
            sampling_rate_hz: 1.0,
            info_low: 95.0,
            info_high: 100.0,
            warning_low: 91.0,
            warning_high: 100.0,
            critical_low: 86.0,
            critical_high: 100.0,
        }
    }

    pub fn default_core_temp() -> Self {
        Self {
            parameter_name: "core_body_temp_c".to_string(),
            unit: "°C".to_string(),
            sensor_type: "Tympanic IR / Core Telemetry Pill".to_string(),
            sampling_rate_hz: 0.2,
            info_low: 36.2,
            info_high: 37.4,
            warning_low: 35.5,
            warning_high: 38.2,
            critical_low: 34.8,
            critical_high: 39.2,
        }
    }

    pub fn default_co2_ppm() -> Self {
        Self {
            parameter_name: "co2_concentration_ppm".to_string(),
            unit: "ppm".to_string(),
            sensor_type: "NDIR Optical CO2 Analyzer".to_string(),
            sampling_rate_hz: 0.5,
            info_low: 350.0,
            info_high: 1200.0,
            warning_low: 250.0,
            warning_high: 3500.0,
            critical_low: 150.0,
            critical_high: 5500.0,
        }
    }

    pub fn default_radiation_rate() -> Self {
        Self {
            parameter_name: "equivalent_dose_rate_usv_h".to_string(),
            unit: "µSv/h".to_string(),
            sensor_type: "Silicon PIN Dosimeter + TEPC".to_string(),
            sampling_rate_hz: 0.2,
            info_low: 0.0,
            info_high: 35.0,
            warning_low: 0.0,
            warning_high: 120.0,
            critical_low: 0.0,
            critical_high: 500.0,
        }
    }

    pub fn default_suit_pressure() -> Self {
        Self {
            parameter_name: "suit_internal_pressure_kpa".to_string(),
            unit: "kPa".to_string(),
            sensor_type: "Dual-Channel EVA Suit Pressure Transducer".to_string(),
            sampling_rate_hz: 2.0,
            info_low: 28.5,
            info_high: 31.5,
            warning_low: 26.0,
            warning_high: 34.0,
            critical_low: 22.0,
            critical_high: 38.0,
        }
    }
}
