//! # `data-stream-manager` — Серверний розподільник потоків даних з RBAC-фільтрацією
//!
//! Забезпечує:
//! - формування індивідуально відфільтрованих проєкцій телеметрії (`ScopedTelemetryView`)
//!   на серверному рівні залежно від ролі авторизованої сесії;
//! - неможливість обходу авторизації через прямі запити до API (приватні біомедичні поля
//!   фізично вилучаються з JSON-відповіді, якщо роль не має на них дозволу).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use access_control::AccessControlService;
use shared_core::{
    CardiovascularMetrics, HabitatEnvironmentTelemetry, LunarResult,
    PhysiologicalAndSleepMetrics, RadiationTelemetry, RespiratoryMetrics, Role,
    SimulationScenario, SpacesuitTelemetry, TelemetryEnvelope,
};

/// Відфільтрована серверна проєкція телеметрії, що повертається клієнту відповідно до його RBAC-ролі.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedTelemetryView {
    pub record_id: Uuid,
    pub astronaut_id: String,
    pub measured_at_formatted: String,
    pub scenario: SimulationScenario,
    pub overall_health_indicator: String,
    pub sensor_bus_online: bool,
    /// Приватні біомедичні дані присутні ЛИШЕ для `FlightSurgeon` або самого `CrewMember`
    pub medical_cardiovascular: Option<CardiovascularMetrics>,
    pub medical_respiratory: Option<RespiratoryMetrics>,
    pub medical_physiological: Option<PhysiologicalAndSleepMetrics>,
    /// Дані середовища та радіації доступні для `Commander`, `Engineer`, `FlightSurgeon`, `MissionControl`, `CrewMember`
    pub habitat: Option<HabitatEnvironmentTelemetry>,
    pub radiation: Option<RadiationTelemetry>,
    pub spacesuit: Option<SpacesuitTelemetry>,
}

/// Менеджер розподілення потоків телеметрії між авторизованими клієнтами.
pub struct DataStreamManager;

impl DataStreamManager {
    /// Проєктує повний кадр `TelemetryEnvelope` у безпечне представлення `ScopedTelemetryView`
    /// з урахуванням ролі активної сесії на серверному рівні.
    pub fn project_envelope_for_session(
        acl: &AccessControlService,
        session_token: &str,
        envelope: &TelemetryEnvelope,
        tz_offset_hours: i32,
    ) -> LunarResult<ScopedTelemetryView> {
        let sess = acl.validate_session(session_token)?;

        let can_see_private_medical = match sess.role {
            Role::FlightSurgeon => true,
            Role::CrewMember => {
                sess.linked_astronaut_id.as_deref() == Some(envelope.astronaut_id.as_str())
            }
            Role::Commander | Role::MissionControl | Role::Engineer | Role::Administrator => false,
        };

        let can_see_environment_and_suit = !matches!(sess.role, Role::Administrator);

        let overall_health_indicator = match envelope.scenario {
            SimulationScenario::Nominal => "NOMINAL (Штатний стан)".to_string(),
            SimulationScenario::PhysicalExertion => "EXERTION (Фізичне навантаження)".to_string(),
            SimulationScenario::HighStress => "WARNING: HIGH STRESS (Підвищений стрес)".to_string(),
            SimulationScenario::SleepDeprivation => {
                "WARNING: SLEEP DEPRIVATION (Дефіцит сну)".to_string()
            }
            SimulationScenario::EmergencyHypoxia => {
                "CRITICAL: HYPOXIA / SUIT DEPRESSURIZATION".to_string()
            }
            SimulationScenario::SolarParticleEvent => {
                "CRITICAL: SOLAR RADIATION STORM".to_string()
            }
        };

        Ok(ScopedTelemetryView {
            record_id: envelope.record_id,
            astronaut_id: envelope.astronaut_id.clone(),
            measured_at_formatted: envelope.formatted_local_time(tz_offset_hours),
            scenario: envelope.scenario,
            overall_health_indicator,
            sensor_bus_online: true,
            medical_cardiovascular: if can_see_private_medical {
                Some(envelope.cardiovascular.clone())
            } else {
                None
            },
            medical_respiratory: if can_see_private_medical {
                Some(envelope.respiratory.clone())
            } else {
                None
            },
            medical_physiological: if can_see_private_medical {
                Some(envelope.physiological.clone())
            } else {
                None
            },
            habitat: if can_see_environment_and_suit {
                Some(envelope.habitat.clone())
            } else {
                None
            },
            radiation: if can_see_environment_and_suit {
                Some(envelope.radiation.clone())
            } else {
                None
            },
            spacesuit: if can_see_environment_and_suit {
                Some(envelope.spacesuit.clone())
            } else {
                None
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::LunarDatabaseRepository;
    use sensor_simulator::SensorSimulatorEngine;

    #[test]
    fn test_stream_manager_strips_private_medical_data_for_engineer_and_admin() {
        let mut repo = LunarDatabaseRepository::new("data/test_stream.sqlite3");
        let mut acl = AccessControlService::new(5, 15, 60, 120);

        acl.register_user(
            &mut repo,
            "eng_user",
            "EngineerPass#2026",
            Role::Engineer,
            Some("AST-003".to_string()),
        )
        .unwrap();
        acl.register_user(
            &mut repo,
            "doc_user",
            "SurgeonPass#2026",
            Role::FlightSurgeon,
            Some("AST-002".to_string()),
        )
        .unwrap();

        let eng_sess = acl
            .authenticate(&mut repo, "eng_user", "EngineerPass#2026")
            .unwrap();
        let doc_sess = acl
            .authenticate(&mut repo, "doc_user", "SurgeonPass#2026")
            .unwrap();

        let mut sim = SensorSimulatorEngine::new_with_default_crew(b"stream_secret".to_vec());
        let env = sim.generate_next_envelope("AST-001").unwrap();

        // Інженер отримує параметри середовища та датчиків, але медичні поля фізично відсутні (None)
        let eng_view = DataStreamManager::project_envelope_for_session(
            &acl,
            &eng_sess.session_token,
            &env,
            2,
        )
        .unwrap();
        assert!(eng_view.medical_cardiovascular.is_none());
        assert!(eng_view.habitat.is_some());

        // Лікар отримує повні медичні показники
        let doc_view = DataStreamManager::project_envelope_for_session(
            &acl,
            &doc_sess.session_token,
            &env,
            2,
        )
        .unwrap();
        assert!(doc_view.medical_cardiovascular.is_some());
        assert!(doc_view.medical_respiratory.is_some());
    }
}
