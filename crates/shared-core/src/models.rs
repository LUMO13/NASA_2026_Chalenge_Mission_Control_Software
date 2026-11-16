//! Доменні моделі телеметрії, космонавтів, середовища станції, радіації, скафандра та ролей RBAC.
//!
//! Важлива архітектурна вимога:
//! 1. Кожне вимірювання супроводжується явно визначеною фізичною одиницею вимірювання,
//!    типом фізичного сенсора та частотою опитування.
//! 2. Показники середовища житлового модуля станції (`HabitatTelemetry`) та показники
//!    індивідуального скафандра (`SpacesuitTelemetry`) строго розділені у різні структури.
//! 3. Часові мітки зберігаються виключно в UTC (`DateTime<Utc>`), а конвертація у вибраний
//!    часовий пояс виконується лише на рівні відображення в GUI.

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Обгортка над вимірюваною фізичною величиною з явною одиницею вимірювання,
/// типом сенсора, частотою дискретизації та джерелом сигналу.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeasuredMetric<T> {
    pub value: T,
    pub unit: String,
    pub sensor_type: String,
    pub sampling_rate_hz: f64,
    pub source_bus: String,
    pub sensor_online: bool,
}

impl<T> MeasuredMetric<T> {
    pub fn new(
        value: T,
        unit: &str,
        sensor_type: &str,
        sampling_rate_hz: f64,
        source_bus: &str,
        sensor_online: bool,
    ) -> Self {
        Self {
            value,
            unit: unit.to_string(),
            sensor_type: sensor_type.to_string(),
            sampling_rate_hz,
            source_bus: source_bus.to_string(),
            sensor_online,
        }
    }
}

/// Сценарій симуляції фізіологічного стану та зовнішніх умов.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SimulationScenario {
    /// Нормальний стан спокою або штатної роботи на станції
    Nominal,
    /// Інтенсивне фізичне навантаження (велоергометр / робота на поверхні Місяця)
    PhysicalExertion,
    /// Психоемоційний стрес та когнітивне перевантаження
    HighStress,
    /// Депривація та порушення архітектури сну
    SleepDeprivation,
    /// Аварійна ситуація: гіпоксія та розгерметизація контуру скафандра
    EmergencyHypoxia,
    /// Аварійна ситуація: сонячний протонно-радіаційний спалах (SPE)
    SolarParticleEvent,
}

impl SimulationScenario {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::Nominal => "NOMINAL",
            Self::PhysicalExertion => "PHYSICAL_EXERTION",
            Self::HighStress => "HIGH_STRESS",
            Self::SleepDeprivation => "SLEEP_DEPRIVATION",
            Self::EmergencyHypoxia => "EMERGENCY_HYPOXIA",
            Self::SolarParticleEvent => "SOLAR_PARTICLE_EVENT",
        }
    }
}

/// Профіль космонавта. Поле `astronaut_id` є незмінним (immutable) після реєстрації.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstronautProfile {
    pub(crate) astronaut_id: String,
    pub full_name: String,
    pub mission_role: String,
    pub callsign: String,
    pub height_cm: f64,
    pub baseline_mass_kg: f64,
    pub assigned_suit_id: String,
    pub is_active: bool,
    pub created_at_utc: DateTime<Utc>,
}

impl AstronautProfile {
    pub fn new(
        astronaut_id: String,
        full_name: String,
        mission_role: String,
        callsign: String,
        height_cm: f64,
        baseline_mass_kg: f64,
        assigned_suit_id: String,
    ) -> Self {
        Self {
            astronaut_id,
            full_name,
            mission_role,
            callsign,
            height_cm,
            baseline_mass_kg,
            assigned_suit_id,
            is_active: true,
            created_at_utc: Utc::now(),
        }
    }

    /// Повертає незмінний унікальний ідентифікатор космонавта (`AST-XXX`).
    pub fn astronaut_id(&self) -> &str {
        &self.astronaut_id
    }
}

/// Показники серцево-судинної системи космонавта.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CardiovascularMetrics {
    /// ЧСС / пульс — уд/хв (bpm), оптичний ФПГ + 3-канальний ЕКГ-патч
    pub heart_rate_bpm: Option<MeasuredMetric<f64>>,
    /// Миттєва амплітуда ЕКГ (відведення II) — мВ (mV)
    pub ecg_lead_ii_mv: Option<MeasuredMetric<f64>>,
    /// Масив відліків форми сигналу ЕКГ за вікно опитування (мВ) із частотою дискретизації (напр. 250 Гц)
    pub ecg_waveform_mv: Vec<f64>,
    pub ecg_sampling_rate_hz: u32,
    /// Систолічний артеріальний тиск — мм рт. ст. (mmHg)
    pub systolic_bp_mmhg: Option<MeasuredMetric<f64>>,
    /// Діастолічний артеріальний тиск — мм рт. ст. (mmHg)
    pub diastolic_bp_mmhg: Option<MeasuredMetric<f64>>,
}

/// Показники дихальної системи космонавта.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RespiratoryMetrics {
    /// Частота дихання — циклів/хв (cycles/min), імпедансна пневмографія
    pub respiration_rate_cpm: Option<MeasuredMetric<f64>>,
    /// Глибина дихання (дихальний об'єм) — мл (mL)
    pub tidal_volume_ml: Option<MeasuredMetric<f64>>,
    /// Сатурація периферичної крові киснем SpO2 — % (за наявності пульсоксиметричного сенсора)
    pub spo2_pct: Option<MeasuredMetric<f64>>,
}

/// Показники терморегуляції, біохімії, антропометрії та сну.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhysiologicalAndSleepMetrics {
    /// Температура ядра тіла — °C
    pub core_body_temp_c: Option<MeasuredMetric<f64>>,
    /// Глюкоза інтерстиціальної рідини — ммоль/л (mmol/L)
    pub glucose_mmol_l: Option<MeasuredMetric<f64>>,
    /// Лактат — ммоль/л (mmol/L)
    pub lactate_mmol_l: Option<MeasuredMetric<f64>>,
    /// Маса тіла в умовах мікрогравітації/місячної гравітації — кг (kg)
    pub body_mass_kg: Option<MeasuredMetric<f64>>,
    /// Зріст — см (cm)
    pub height_cm: Option<MeasuredMetric<f64>>,
    /// Індекс маси тіла — кг/м² (kg/m²)
    pub bmi_kg_m2: Option<MeasuredMetric<f64>>,
    /// Тривалість останнього циклу сну — год (h)
    pub sleep_duration_hours: Option<MeasuredMetric<f64>>,
    /// Частота пробуджень за цикл сну — разів (count)
    pub sleep_awakenings_count: Option<MeasuredMetric<u32>>,
    /// Рівень вегетативного стресу (на основі ВСР RMSSD/LF-HF) — умовна шкала 0–100
    pub stress_index_0_100: Option<MeasuredMetric<f64>>,
    /// Інтегральна якість сну — умовна шкала 0–100
    pub sleep_quality_0_100: Option<MeasuredMetric<f64>>,
}

/// Параметри середовища проживання місячної бази (Habitat Module).
/// СТРОГО ВІДОКРЕМЛЕНО від параметрів внутрішнього контуру скафандра.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HabitatEnvironmentTelemetry {
    pub habitat_record_id: Uuid,
    pub module_id: String,
    pub measured_at_utc: DateTime<Utc>,
    /// Атмосферний тиск у модулі — кПа (kPa)
    pub atmospheric_pressure_kpa: MeasuredMetric<f64>,
    /// Концентрація кисню — % об'єму (% vol)
    pub o2_concentration_pct: MeasuredMetric<f64>,
    /// Концентрація вуглекислого газу CO2 — ppm
    pub co2_concentration_ppm: MeasuredMetric<f64>,
    /// Концентрація азоту N2 — % об'єму (% vol)
    pub nitrogen_pct: MeasuredMetric<f64>,
    /// Домішки чадного газу CO — ppm
    pub trace_co_ppm: MeasuredMetric<f64>,
    /// Температура повітря в модулі — °C
    pub air_temperature_c: MeasuredMetric<f64>,
    /// Відносна вологість повітря — % RH
    pub humidity_rh_pct: MeasuredMetric<f64>,
    /// Рівень освітленості — люкс (lx), за наявності люксметра
    pub illuminance_lx: Option<MeasuredMetric<f64>>,
}

/// Показники радіаційного контролю космонавта та відсіку.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RadiationTelemetry {
    pub radiation_record_id: Uuid,
    pub astronaut_id: String,
    pub module_id: String,
    pub measured_at_utc: DateTime<Utc>,
    /// Накопичена поглинута доза — мГр (mGy)
    pub absorbed_dose_mgy: MeasuredMetric<f64>,
    /// Накопичена еквівалентна доза — мЗв (mSv)
    pub equivalent_dose_msv: MeasuredMetric<f64>,
    /// Потужність еквівалентної дози — мкЗв/год (µSv/h)
    pub equivalent_dose_rate_usv_h: MeasuredMetric<f64>,
    /// Потік галактичного та сонячного космічного випромінювання — частинок/(см²·с)
    pub cosmic_ray_flux_cm2_s: MeasuredMetric<f64>,
}

/// Показники індивідуального скафандра (EVA Spacesuit PLSS).
/// СТРОГО ВІДОКРЕМЛЕНО від показників житлового модуля станції.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpacesuitTelemetry {
    pub suit_record_id: Uuid,
    pub suit_id: String,
    pub astronaut_id: String,
    pub measured_at_utc: DateTime<Utc>,
    /// Сатурація SpO2 за датчиком шолома скафандра — %
    pub suit_spo2_pct: MeasuredMetric<f64>,
    /// Пульс за датчиком скафандра — уд/хв (bpm)
    pub suit_heart_rate_bpm: MeasuredMetric<f64>,
    /// Тиск усередині скафандра — кПа (kPa) (номінал 29.6–30.3 кПа чистий O2)
    pub suit_internal_pressure_kpa: MeasuredMetric<f64>,
    /// Температура всередині скафандра — °C
    pub suit_internal_temp_c: MeasuredMetric<f64>,
    /// Концентрація кисню у контурі скафандра — % об'єму (% vol)
    pub suit_o2_pct: MeasuredMetric<f64>,
    /// Вологість у контурі скафандра — % RH
    pub suit_humidity_rh_pct: MeasuredMetric<f64>,
}

/// Повний кадр телеметрії космонавта, що передається від симулятора/фізичних датчиків.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryEnvelope {
    pub record_id: Uuid,
    pub sequence_nonce: u64,
    pub astronaut_id: String,
    pub measured_at_utc: DateTime<Utc>,
    pub received_at_utc: Option<DateTime<Utc>>,
    pub scenario: SimulationScenario,
    pub cardiovascular: CardiovascularMetrics,
    pub respiratory: RespiratoryMetrics,
    pub physiological: PhysiologicalAndSleepMetrics,
    pub habitat: HabitatEnvironmentTelemetry,
    pub radiation: RadiationTelemetry,
    pub spacesuit: SpacesuitTelemetry,
    pub hmac_sha256_signature: String,
}

impl TelemetryEnvelope {
    /// Форматує часову мітку вимірювання (що зберігається в UTC) у вибраний користувачем
    /// часовий пояс для відображення в інтерфейсі (наприклад, UTC+02:00 або Mission Elapsed Time).
    pub fn formatted_local_time(&self, tz_offset_hours: i32) -> String {
        let offset = FixedOffset::east_opt(tz_offset_hours * 3600)
            .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC offset 0 is always valid"));
        self.measured_at_utc
            .with_timezone(&offset)
            .format("%Y-%m-%d %H:%M:%S%.3f %:z")
            .to_string()
    }
}

/// Рівень критичності сповіщення підсистеми аналізу здоров'я.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlertSeverity {
    /// 1. INFO — інформаційне повідомлення або незначне відхилення від базової лінії
    Info,
    /// 2. WARNING — потенційно небезпечне відхилення, що вимагає уваги лікаря або інженера
    Warning,
    /// 3. CRITICAL — критичний стан, що загрожує життю та потребує негайного втручання
    Critical,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }
}

/// Подія сповіщення, згенерована аналізатором на основі реального порівняння з порогами та трендами.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertEvent {
    pub alert_id: Uuid,
    pub astronaut_id: Option<String>,
    pub source_subsystem: String,
    pub parameter_name: String,
    pub severity: AlertSeverity,
    pub measured_value: f64,
    pub unit: String,
    pub threshold_limit: f64,
    pub trend_slope_per_min: f64,
    pub message: String,
    pub triggered_at_utc: DateTime<Utc>,
    pub acknowledged: bool,
    pub acknowledged_by_user: Option<Uuid>,
    pub acknowledged_at_utc: Option<DateTime<Utc>>,
}

/// Ролі користувачів у системі RBAC.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Role {
    Commander,
    FlightSurgeon,
    Engineer,
    CrewMember,
    MissionControl,
    Administrator,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Commander => "Commander",
            Self::FlightSurgeon => "FlightSurgeon",
            Self::Engineer => "Engineer",
            Self::CrewMember => "CrewMember",
            Self::MissionControl => "MissionControl",
            Self::Administrator => "Administrator",
        }
    }
}

/// Пріоритет пакета для синхронізації із Землею.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncPriority {
    /// Звичайні архівні пакети
    ArchiveBulk = 1,
    /// Штатна періодична телеметрія
    NormalTelemetry = 2,
    /// Попереджувальні сповіщення WARNING
    HighAlert = 3,
    /// Критичні аварійні сповіщення CRITICAL — відправляються позачергово першими
    CriticalImmediate = 4,
}

impl SyncPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ArchiveBulk => "ARCHIVE_BULK",
            Self::NormalTelemetry => "NORMAL_TELEMETRY",
            Self::HighAlert => "HIGH_ALERT",
            Self::CriticalImmediate => "CRITICAL_IMMEDIATE",
        }
    }
}
