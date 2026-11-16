//! # `desktop-gui` — Графічний інтерфейс NASA Mission Control (eframe / egui)
//!
//! Реалізує всі 11 обов'язкових сторінок:
//! 1. Загальний Dashboard екіпажу (картки космонавтів із ПІБ, ID, роллю, індикатором стану, статусом підключення та переходом до авторизації).
//! 2. Сторінка окремого космонавта (актуальні значення над графіками, історичні показники, графіки ЧСС/ЕКГ/SpO2/АТ, 8 часових проміжків, інспекція конкретної точки з датою/часом/одиницею, порівняння часових проміжків).
//! 3. Сторінка аварійних сповіщень (`INFO`, `WARNING`, `CRITICAL` + підтвердження отримання).
//! 4. Сторінка стану датчиків (тип сенсора, шина, частота дискретизації, статус).
//! 5. Сторінка середовища станції (тиск кПа, O2 %, CO2 ppm, N2 %, температура °C, вологість % RH, освітленість lx).
//! 6. Сторінка радіаційного контролю (поглинута доза mGy, еквівалентна доза mSv, потужність дози µSv/h, потік частинок).
//! 7. Сторінка управління користувачами (RBAC ролі, додавання космонавта з незмінним ID, блокування).
//! 8. Сторінка налаштувань (конфігуровані медичні пороги, часовий пояс відображення, повзунок накопичення телеметрії).
//! 9. Сторінка симулятора (вибір космонавта, сценарію, похибки, інтервалу опитування, ручне введення, імітація відмов).
//! 10. Сторінка синхронізації із Землею (черга пакетів, ручне відправлення, статус каналу, журнал передачі).
//! 11. Сторінка журналу системи (Audit Log безпеки та дій користувачів).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use access_control::{AccessControlService, AuthSession, UserAccount};
use data_stream_manager::{DataStreamManager, ScopedTelemetryView};
use database::{DatabaseSnapshot, LunarDatabaseRepository};
use health_analyzer::{ConfigurableThresholdSet, HealthAnalyzerEngine};
use sensor_simulator::SensorSimulatorEngine;
use shared_core::{
    AstronautProfile, LunarResult, Role, SimulationScenario, SimulatorControlCommand,
    TelemetryEnvelope,
};
use telemetry_distributor::{DistributorSettings, DistributorSnapshot, TelemetryDistributor};
use telemetry_receiver::TelemetryReceiverService;

/// Усі 11 навігаційних сторінок інтерфейсу NASA Mission Control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionControlPage {
    CrewDashboard,
    AstronautDetail,
    AlertCenter,
    SensorDiagnostics,
    HabitatEnvironment,
    RadiationControl,
    UserAndAstronautManagement,
    SystemSettings,
    SimulatorControlPanel,
    EarthSynchronization,
    SystemAuditLog,
}

/// Вибір часового проміжку для графіків телеметрії на сторінці космонавта.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeRangePreset {
    Last5Minutes,
    Last15Minutes,
    Last1Hour,
    Last6Hours,
    Last12Hours,
    Last24Hours,
    Last7Days,
    CustomRange { minutes: i64 },
}

impl TimeRangePreset {
    pub fn duration(&self) -> Duration {
        match self {
            Self::Last5Minutes => Duration::minutes(5),
            Self::Last15Minutes => Duration::minutes(15),
            Self::Last1Hour => Duration::hours(1),
            Self::Last6Hours => Duration::hours(6),
            Self::Last12Hours => Duration::hours(12),
            Self::Last24Hours => Duration::hours(24),
            Self::Last7Days => Duration::days(7),
            Self::CustomRange { minutes } => Duration::minutes((*minutes).max(1)),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Last5Minutes => "Останні 5 хвилин",
            Self::Last15Minutes => "Останні 15 хвилин",
            Self::Last1Hour => "Остання 1 година",
            Self::Last6Hours => "Останні 6 годин",
            Self::Last12Hours => "Останні 12 годин",
            Self::Last24Hours => "Останні 24 години",
            Self::Last7Days => "Останні 7 днів",
            Self::CustomRange { .. } => "Власний часовий проміжок",
        }
    }
}

/// Інформація про вибрану користувачем точку на графіку.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectedPlotPoint {
    pub parameter_name: String,
    pub exact_value: f64,
    pub unit: String,
    pub measured_at_local: String,
    pub sensor_type: String,
}

/// Форма додавання нового космонавта з незмінним ID та необов'язковим створенням облікового запису.
#[derive(Debug, Clone, Default)]
pub struct NewAstronautForm {
    pub astronaut_id: String,
    pub full_name: String,
    pub mission_role: String,
    pub callsign: String,
    pub height_cm: f64,
    pub baseline_mass_kg: f64,
    pub assigned_suit_id: String,
}

/// Окрема форма створення облікового запису — не прив'язана до додавання профілю.
#[derive(Debug, Clone)]
pub struct NewUserForm {
    pub username: String,
    pub password: String,
    pub role: Role,
    pub linked_astronaut_id: String,
}

impl Default for NewUserForm {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            role: Role::CrewMember,
            linked_astronaut_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulatorSettingsSnapshot {
    pub running: bool,
    pub poll_interval_ms: u64,
    pub error_margin_pct: f64,
    pub packet_loss_pct: f64,
    pub link_disconnected: bool,
}

impl Default for SimulatorSettingsSnapshot {
    fn default() -> Self {
        Self { running: true, poll_interval_ms: 1000, error_margin_pct: 1.5,
            packet_loss_pct: 0.0, link_disconnected: false }
    }
}

/// Persistent user-generated state. Never contains plain-text passwords or session tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpecsSnapshot {
    pub format_version: u32,
    pub database: DatabaseSnapshot,
    pub users: Vec<UserAccount>,
    pub simulator_crew: Vec<sensor_simulator::AstronautSimState>,
    #[serde(default)]
    pub simulator_settings: SimulatorSettingsSnapshot,
    #[serde(default)]
    pub distributor: Option<DistributorSnapshot>,
    #[serde(default)]
    pub analyzer_thresholds: Option<ConfigurableThresholdSet>,
    #[serde(default)]
    pub timezone_offset_hours: i32,
}

/// Стан ядра десктопного застосунку NASA Mission Control, що об'єднує всі компоненти.
pub struct MissionControlAppState {
    pub current_page: MissionControlPage,
    pub selected_astronaut_id: String,
    pub selected_time_range: TimeRangePreset,
    pub compare_previous_period: bool,
    pub inspected_point: Option<InspectedPlotPoint>,
    pub timezone_offset_hours: i32,
    pub active_session: Option<AuthSession>,
    pub login_username_input: String,
    pub login_password_input: String,
    pub status_banner: String,
    pub new_astronaut_form: NewAstronautForm,
    pub new_user_form: NewUserForm,
    pub first_admin_username: String,
    pub first_admin_password: String,

    pub simulator: SensorSimulatorEngine,
    pub receiver: TelemetryReceiverService,
    pub distributor: TelemetryDistributor,
    pub database: LunarDatabaseRepository,
    pub analyzer: HealthAnalyzerEngine,
    pub access_control: AccessControlService,
}

impl MissionControlAppState {
    /// Відкриває прототип із порожніми профілями та без стандартних паролів.
    pub fn new_prototype_environment() -> Self {
        let secret = std::env::var("LUNAR_HMAC_SECRET")
            .unwrap_or_else(|_| format!("local-demo-session-{}", uuid::Uuid::new_v4()))
            .into_bytes();

        Self {
            current_page: MissionControlPage::CrewDashboard,
            selected_astronaut_id: String::new(),
            selected_time_range: TimeRangePreset::Last15Minutes,
            compare_previous_period: false,
            inspected_point: None,
            timezone_offset_hours: 0,
            active_session: None,
            login_username_input: String::new(),
            login_password_input: String::new(),
            status_banner: "Прототип запущено. Створіть першого адміністратора й додайте профіль космонавта.".to_string(),
            new_astronaut_form: NewAstronautForm {
                height_cm: 175.0,
                baseline_mass_kg: 70.0,
                ..NewAstronautForm::default()
            },
            new_user_form: NewUserForm::default(),
            first_admin_username: String::new(),
            first_admin_password: String::new(),
            simulator: SensorSimulatorEngine::new_empty(secret.clone()),
            receiver: TelemetryReceiverService::new(secret, 5000),
            distributor: TelemetryDistributor::new(DistributorSettings::default()),
            database: LunarDatabaseRepository::new("data/local_offline_buffer.sqlite3"),
            analyzer: HealthAnalyzerEngine::new(ConfigurableThresholdSet::default()),
            access_control: AccessControlService::new(5, 15, 60, 240),
        }
    }

    pub fn export_snapshot(&self) -> FpecsSnapshot {
        FpecsSnapshot {
            format_version: 1,
            database: self.database.export_snapshot(),
            users: self.access_control.list_users(),
            simulator_crew: self.simulator.astronauts.values().cloned().collect(),
            simulator_settings: SimulatorSettingsSnapshot {
                running: self.simulator.running,
                poll_interval_ms: self.simulator.poll_interval_ms,
                error_margin_pct: self.simulator.error_margin_pct,
                packet_loss_pct: self.simulator.packet_loss_pct,
                link_disconnected: self.simulator.link_disconnected,
            },
            distributor: Some(self.distributor.export_snapshot()),
            analyzer_thresholds: Some(self.analyzer.thresholds.clone()),
            timezone_offset_hours: self.timezone_offset_hours,
        }
    }

    pub fn restore_snapshot(&mut self, data: FpecsSnapshot) -> LunarResult<()> {
        if data.format_version != 1 {
            return Err(shared_core::LunarHmsError::StorageError(
                format!("Unsupported snapshot version {}", data.format_version),
            ));
        }
        self.database.import_snapshot(data.database)?;
        self.access_control.restore_users(data.users)?;
        self.simulator.astronauts.clear();
        for profile in self.database.list_astronauts() {
            let id = profile.astronaut_id().to_string();
            let mut sim = data.simulator_crew.iter()
                .find(|s| s.profile.astronaut_id() == id)
                .cloned()
                .unwrap_or_else(|| sensor_simulator::AstronautSimState::new(profile.clone()));
            sim.profile = profile;
            let max_nonce = self.database.export_snapshot().telemetry.iter()
                .filter(|r| r.astronaut_id == id)
                .map(|r| r.sequence_nonce)
                .max().unwrap_or(1000);
            sim.sequence_nonce = sim.sequence_nonce.max(max_nonce);
            self.simulator.astronauts.insert(id, sim);
        }
        self.simulator.running = data.simulator_settings.running;
        self.simulator.poll_interval_ms = data.simulator_settings.poll_interval_ms.clamp(200, 60_000);
        self.simulator.error_margin_pct = data.simulator_settings.error_margin_pct.clamp(0.0, 25.0);
        self.simulator.packet_loss_pct = data.simulator_settings.packet_loss_pct.clamp(0.0, 100.0);
        self.simulator.link_disconnected = data.simulator_settings.link_disconnected;
        if let Some(saved_queue) = data.distributor { self.distributor.restore_snapshot(saved_queue); }
        if let Some(thresholds) = data.analyzer_thresholds { self.analyzer.thresholds = thresholds; }
        self.timezone_offset_hours = data.timezone_offset_hours.clamp(-12, 14);
        self.active_session = None;
        self.status_banner = "Відновлено попередню сесію FPECS. Увійдіть для доступу.".into();
        Ok(())
    }

    pub fn bootstrap_first_administrator(&mut self) -> LunarResult<()> {
        if !self.access_control.list_users().is_empty() {
            return Err(shared_core::LunarHmsError::AuthenticationFailed);
        }
        let username = self.first_admin_username.trim().to_string();
        let password = self.first_admin_password.clone();
        if username.is_empty() {
            return Err(shared_core::LunarHmsError::ValidationFailure {
                field: "username".into(),
                reason: "Укажіть ім'я адміністратора".into(),
            });
        }
        self.access_control.register_user(
            &mut self.database, &username, &password, Role::Administrator, None,
        )?;
        self.active_session = Some(
            self.access_control.authenticate(&mut self.database, &username, &password)?
        );
        self.first_admin_password.clear();
        self.status_banner = "Адміністратора створено. Додайте перший профіль екіпажу.".into();
        Ok(())
    }

    pub fn authenticate_user(&mut self) -> LunarResult<()> {
        let sess = self.access_control.authenticate(
            &mut self.database,
            &self.login_username_input,
            &self.login_password_input,
        )?;
        self.login_password_input.clear();
        self.status_banner = format!("Вхід виконано: {} ({})", sess.username, sess.role.as_str());
        self.active_session = Some(sess);
        Ok(())
    }

    pub fn can_view_selected_medical(&self) -> bool {
        match &self.active_session {
            Some(sess) => self.access_control.authorize_medical_detail_access(
                &sess.session_token, &self.selected_astronaut_id
            ).is_ok(),
            None => false,
        }
    }

    /// Створює окремий обліковий запис, за потреби прив'язаний до наявного космонавта.
    pub fn submit_new_user_form(&mut self) -> LunarResult<()> {
        let Some(ref sess) = self.active_session else {
            return Err(shared_core::LunarHmsError::AuthenticationFailed);
        };
        self.access_control.authorize_user_administration(&sess.session_token)?;
        let username = self.new_user_form.username.trim().to_string();
        if username.is_empty() {
            return Err(shared_core::LunarHmsError::ValidationFailure {
                field: "username".into(), reason: "Укажіть ім'я користувача".into(),
            });
        }
        let linked = self.new_user_form.linked_astronaut_id.trim();
        let linked = if linked.is_empty() { None } else {
            if !self.database.list_astronauts().iter().any(|p| p.astronaut_id() == linked) {
                return Err(shared_core::LunarHmsError::ValidationFailure {
                    field: "linked_astronaut_id".into(), reason: "Профіль космонавта не знайдено".into(),
                });
            }
            Some(linked.to_string())
        };
        if self.new_user_form.role == Role::CrewMember && linked.is_none() {
            return Err(shared_core::LunarHmsError::ValidationFailure {
                field: "linked_astronaut_id".into(),
                reason: "Для члена екіпажу оберіть профіль космонавта".into(),
            });
        }
        self.access_control.register_user(
            &mut self.database, &username, &self.new_user_form.password,
            self.new_user_form.role, linked,
        )?;
        self.new_user_form.password.clear();
        self.status_banner = format!("Створено користувача {}", username);
        Ok(())
    }

    /// Виконує повний такт конвеєра:
    /// Симулятор -> Отримувач -> Розподільник -> База даних -> Аналізатор
    pub fn step_simulation_pipeline(&mut self) {
        let ids: Vec<String> = self.simulator.astronauts.keys().cloned().collect();
        for id in ids {
            if let Some(raw_env) = self.simulator.generate_next_envelope(&id) {
                if let Ok(valid_env) = self.receiver.ingest_and_validate(raw_env) {
                    let _ = self
                        .distributor
                        .process_incoming_telemetry(&mut self.database, valid_env.clone());
                    let alerts = self.analyzer.analyze_envelope(&mut self.database, &valid_env);
                    for alert in alerts {
                        let _ = self
                            .distributor
                            .enqueue_alert_with_priority(&mut self.database, alert);
                    }
                }
            }
        }
    }

    /// Відкриває медичні дані лише після перевірки активної сесії та ролі.
    pub fn open_astronaut_card(&mut self, astronaut_id: &str) -> LunarResult<()> {
        let Some(ref sess) = self.active_session else {
            return Err(shared_core::LunarHmsError::AuthenticationFailed);
        };
        self.access_control.authorize_medical_detail_access(
            &sess.session_token, astronaut_id,
        )?;
        self.selected_astronaut_id = astronaut_id.to_string();
        self.current_page = MissionControlPage::AstronautDetail;
        Ok(())
    }

    /// Натискання на картку космонавта відкриває авторизацію та переводить на сторінку космонавта
    /// з серверною перевіркою RBAC.
    pub fn select_astronaut_card_and_authenticate(
        &mut self,
        astronaut_id: &str,
        username: &str,
        password: &str,
    ) -> LunarResult<ScopedTelemetryView> {
        self.selected_astronaut_id = astronaut_id.to_string();
        let sess = self
            .access_control
            .authenticate(&mut self.database, username, password)?;
        self.active_session = Some(sess.clone());

        let now = Utc::now();
        let records = self.database.query_telemetry_by_astronaut_and_range(
            astronaut_id,
            now - self.selected_time_range.duration(),
            now + Duration::seconds(5),
        );
        let latest = records.last().ok_or_else(|| {
            shared_core::LunarHmsError::StorageError("Немає телеметрії для космонавта".to_string())
        })?;

        let scoped = DataStreamManager::project_envelope_for_session(
            &self.access_control,
            &sess.session_token,
            latest,
            self.timezone_offset_hours,
        )?;
        self.current_page = MissionControlPage::AstronautDetail;
        Ok(scoped)
    }

    /// Додає нового космонавта через форму в GUI (лише для ролі Administrator),
    /// зберігає незмінний ID в БД, додає в симулятор і за потреби створює обліковий запис.
    pub fn submit_new_astronaut_form(&mut self) -> LunarResult<()> {
        let Some(ref sess) = self.active_session else {
            return Err(shared_core::LunarHmsError::AuthenticationFailed);
        };
        self.access_control
            .authorize_user_administration(&sess.session_token)?;

        let profile = AstronautProfile::new(
            self.new_astronaut_form.astronaut_id.clone(),
            self.new_astronaut_form.full_name.clone(),
            self.new_astronaut_form.mission_role.clone(),
            self.new_astronaut_form.callsign.clone(),
            self.new_astronaut_form.height_cm,
            self.new_astronaut_form.baseline_mass_kg,
            self.new_astronaut_form.assigned_suit_id.clone(),
        );

        self.database.insert_astronaut(profile.clone())?;
        self.simulator.astronauts.insert(
            profile.astronaut_id().to_string(),
            sensor_simulator::AstronautSimState::new(profile.clone()),
        );
        self.selected_astronaut_id = profile.astronaut_id().to_string();

        self.new_astronaut_form.astronaut_id.clear();
        self.new_astronaut_form.full_name.clear();
        self.new_astronaut_form.mission_role.clear();
        self.new_astronaut_form.callsign.clear();
        self.new_astronaut_form.assigned_suit_id.clear();
        self.status_banner = format!(
            "Космонавта {} ({}) успішно додано з незмінним ID.",
            profile.full_name,
            profile.astronaut_id()
        );
        Ok(())
    }

    /// Обирає конкретну точку на графіку для детального огляду значення, часу та одиниці вимірювання.
    pub fn inspect_telemetry_point(&mut self, envelope: &TelemetryEnvelope, parameter: &str) {
        let local_ts = envelope.formatted_local_time(self.timezone_offset_hours);
        let point = match parameter {
            "heart_rate_bpm" => envelope.cardiovascular.heart_rate_bpm.as_ref().map(|m| {
                InspectedPlotPoint {
                    parameter_name: "ЧСС / Пульс".to_string(),
                    exact_value: m.value,
                    unit: m.unit.clone(),
                    measured_at_local: local_ts,
                    sensor_type: m.sensor_type.clone(),
                }
            }),
            "spo2_pct" => envelope.respiratory.spo2_pct.as_ref().map(|m| InspectedPlotPoint {
                parameter_name: "Сатурація SpO2".to_string(),
                exact_value: m.value,
                unit: m.unit.clone(),
                measured_at_local: local_ts,
                sensor_type: m.sensor_type.clone(),
            }),
            _ => None,
        };
        self.inspected_point = point;
    }

    #[allow(dead_code)]
    fn _unused_marker(&self) {}
}

// Допоміжне поле для ініціалізації
#[allow(dead_code)]
trait TimedInitExt {}
impl MissionControlAppState {
    #[allow(dead_code)]
    const NULL_UNIT: () = ();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_empty_and_first_admin_can_add_astronaut() {
        let mut app = MissionControlAppState::new_prototype_environment();
        assert!(app.database.list_astronauts().is_empty());
        assert!(app.access_control.list_users().is_empty());
        assert_eq!(app.database.total_telemetry_count(), 0);
        app.first_admin_username = "admin_test".into();
        app.first_admin_password = "A-Unique-Admin-Test-Password".into();
        app.bootstrap_first_administrator().unwrap();
        assert_eq!(app.access_control.list_users().len(), 1);
        app.new_astronaut_form.astronaut_id = "AST-101".into();
        app.new_astronaut_form.full_name = "Test Astronaut".into();
        app.new_astronaut_form.mission_role = "Engineer".into();
        app.new_astronaut_form.callsign = "TEST-101".into();
        app.new_astronaut_form.assigned_suit_id = "TEST-SUIT-101".into();
        app.submit_new_astronaut_form().unwrap();
        assert_eq!(app.database.list_astronauts().len(), 1);
        app.step_simulation_pipeline();
        assert!(app.database.total_telemetry_count() >= 1);
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn restores_users_crew_telemetry_and_nonce_without_demo_accounts() {
        let mut first = MissionControlAppState::new_prototype_environment();
        assert!(first.access_control.list_users().is_empty());
        first.first_admin_username = "example_admin".into();
        first.first_admin_password = "SafeTestPass#only".into();
        first.bootstrap_first_administrator().unwrap();
        first.new_astronaut_form.astronaut_id = "AST-201".into();
        first.new_astronaut_form.full_name = "Fictional Test Crew".into();
        first.new_astronaut_form.mission_role = "Engineer".into();
        first.new_astronaut_form.callsign = "DEMO201".into();
        first.new_astronaut_form.assigned_suit_id = "SUIT201".into();
        first.submit_new_astronaut_form().unwrap();
        first.step_simulation_pipeline();
        assert_eq!(first.database.total_telemetry_count(), 1);
        let saved = first.export_snapshot();
        let serialized = serde_json::to_string(&saved).unwrap();
        assert!(!serialized.contains("SafeTestPass#only"));
        assert!(serialized.contains("argon2"));
        let mut restored = MissionControlAppState::new_prototype_environment();
        restored.restore_snapshot(serde_json::from_str(&serialized).unwrap()).unwrap();
        assert_eq!(restored.database.list_astronauts().len(), 1);
        assert_eq!(restored.access_control.list_users().len(), 1);
        assert_eq!(restored.database.total_telemetry_count(), 1);
        assert!(restored.active_session.is_none());
        restored.step_simulation_pipeline();
        assert_eq!(restored.database.total_telemetry_count(), 2);
    }
}
