-- =============================================================================
-- Lunar Crew Health Monitoring System (NASA Space Apps Challenge 2026)
-- Міграція 001: Повна структура бази даних PostgreSQL
-- Усі часові мітки зберігаються строго в UTC (TIMESTAMPTZ).
-- Усі числові показники супроводжуються явно визначеними фізичними одиницями
-- та метаданими типу сенсора і частоти вимірювання.
-- Показники житлового модуля (habitat_telemetry) та скафандра (spacesuit_telemetry)
-- строго розділені на рівні таблиць та обмежень цілісності.
-- =============================================================================

BEGIN;

-- 1. Таблиця ролей і дозволів (RBAC)
CREATE TABLE IF NOT EXISTS roles_permissions (
    role_code VARCHAR(32) PRIMARY KEY,
    display_name VARCHAR(64) NOT NULL,
    description TEXT NOT NULL,
    can_view_all_crew_medical BOOLEAN NOT NULL DEFAULT FALSE,
    can_view_own_medical BOOLEAN NOT NULL DEFAULT FALSE,
    can_view_crew_summary BOOLEAN NOT NULL DEFAULT FALSE,
    can_view_sensor_diagnostics BOOLEAN NOT NULL DEFAULT FALSE,
    can_view_habitat_and_radiation BOOLEAN NOT NULL DEFAULT FALSE,
    can_acknowledge_alerts BOOLEAN NOT NULL DEFAULT FALSE,
    can_configure_thresholds BOOLEAN NOT NULL DEFAULT FALSE,
    can_manage_users_and_roles BOOLEAN NOT NULL DEFAULT FALSE,
    can_manage_earth_sync BOOLEAN NOT NULL DEFAULT FALSE,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 2. Таблиця космонавтів (ID космонавта є незмінним — тригер блокує UPDATE astronaut_id)
CREATE TABLE IF NOT EXISTS astronauts (
    astronaut_id VARCHAR(32) PRIMARY KEY CHECK (astronaut_id ~ '^AST-[0-9]{3,6}$'),
    full_name VARCHAR(128) NOT NULL,
    mission_role VARCHAR(64) NOT NULL,
    callsign VARCHAR(32) NOT NULL,
    height_cm DOUBLE PRECISION NOT NULL CHECK (height_cm > 100.0 AND height_cm < 250.0),
    baseline_mass_kg DOUBLE PRECISION NOT NULL CHECK (baseline_mass_kg > 30.0 AND baseline_mass_kg < 200.0),
    assigned_suit_id VARCHAR(32) NOT NULL UNIQUE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Захист незмінності astronaut_id на рівні СУБД
CREATE OR REPLACE FUNCTION prevent_astronaut_id_change()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.astronaut_id <> OLD.astronaut_id THEN
        RAISE EXCEPTION 'astronaut_id є незмінним унікальним ідентифікатором і не може бути модифікований';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_immutable_astronaut_id ON astronauts;
CREATE TRIGGER trg_immutable_astronaut_id
BEFORE UPDATE ON astronauts
FOR EACH ROW EXECUTE FUNCTION prevent_astronaut_id_change();

-- 3. Таблиця користувачів (RBAC + Argon2id + захист від перебору паролів)
CREATE TABLE IF NOT EXISTS users (
    user_id UUID PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    password_hash_argon2id TEXT NOT NULL,
    role_code VARCHAR(32) NOT NULL REFERENCES roles_permissions(role_code) ON UPDATE CASCADE,
    linked_astronaut_id VARCHAR(32) REFERENCES astronauts(astronaut_id) ON DELETE SET NULL,
    is_locked BOOLEAN NOT NULL DEFAULT FALSE,
    failed_login_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until_utc TIMESTAMPTZ,
    last_login_utc TIMESTAMPTZ,
    password_changed_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role_code);

-- 4. Головна таблиця біомедичної телеметрії космонавтів
-- Кожне вимірювання містить значення, явну одиницю вимірювання, частоту та метадані сенсорів.
CREATE TABLE IF NOT EXISTS telemetry (
    record_id UUID PRIMARY KEY,
    sequence_nonce BIGINT NOT NULL,
    astronaut_id VARCHAR(32) NOT NULL REFERENCES astronauts(astronaut_id) ON DELETE RESTRICT,
    measured_at_utc TIMESTAMPTZ NOT NULL,
    received_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    simulation_scenario VARCHAR(32) NOT NULL DEFAULT 'NOMINAL',

    -- Серцево-судинна система
    heart_rate_bpm DOUBLE PRECISION,
    heart_rate_unit VARCHAR(16) NOT NULL DEFAULT 'bpm',
    heart_rate_sensor VARCHAR(64) NOT NULL DEFAULT 'Optical PPG + 3-Lead ECG',

    ecg_lead_ii_mv DOUBLE PRECISION,
    ecg_waveform_samples_mv JSONB,
    ecg_sampling_rate_hz INTEGER NOT NULL DEFAULT 250,
    ecg_unit VARCHAR(16) NOT NULL DEFAULT 'mV',
    ecg_sensor VARCHAR(64) NOT NULL DEFAULT '3-Lead Chest Bio-Potential Patch',

    bp_systolic_mmhg DOUBLE PRECISION,
    bp_diastolic_mmhg DOUBLE PRECISION,
    bp_unit VARCHAR(16) NOT NULL DEFAULT 'mmHg',
    bp_sensor VARCHAR(64) NOT NULL DEFAULT 'Optical Pulse Transit Time Cuffless Sensor',

    -- Дихальна система
    respiration_rate_cpm DOUBLE PRECISION,
    respiration_rate_unit VARCHAR(16) NOT NULL DEFAULT 'cycles/min',
    tidal_volume_ml DOUBLE PRECISION,
    tidal_volume_unit VARCHAR(16) NOT NULL DEFAULT 'mL',
    respiration_sensor VARCHAR(64) NOT NULL DEFAULT 'Thoracic Bio-Impedance Belt',

    spo2_pct DOUBLE PRECISION,
    spo2_unit VARCHAR(16) NOT NULL DEFAULT '%',
    spo2_sensor VARCHAR(64) NOT NULL DEFAULT 'Dual-Wavelength Finger/Wrist Pulse Oximeter',

    -- Терморегуляція
    core_body_temp_c DOUBLE PRECISION,
    core_body_temp_unit VARCHAR(16) NOT NULL DEFAULT '°C',
    temp_sensor VARCHAR(64) NOT NULL DEFAULT 'Tympanic IR / Ingestible Core Pill',

    -- Біохімічні показники
    glucose_mmol_l DOUBLE PRECISION,
    glucose_unit VARCHAR(16) NOT NULL DEFAULT 'mmol/L',
    glucose_sensor VARCHAR(64) NOT NULL DEFAULT 'Subcutaneous Interstitial CGM',

    lactate_mmol_l DOUBLE PRECISION,
    lactate_unit VARCHAR(16) NOT NULL DEFAULT 'mmol/L',
    lactate_sensor VARCHAR(64) NOT NULL DEFAULT 'Electrochemical Sweat Biosensor',

    -- Антропометрія
    body_mass_kg DOUBLE PRECISION,
    body_mass_unit VARCHAR(16) NOT NULL DEFAULT 'kg',
    height_cm DOUBLE PRECISION,
    height_unit VARCHAR(16) NOT NULL DEFAULT 'cm',
    bmi_kg_m2 DOUBLE PRECISION,
    bmi_unit VARCHAR(16) NOT NULL DEFAULT 'kg/m²',
    anthropometry_source VARCHAR(64) NOT NULL DEFAULT 'Microgravity Inertial Mass Measurement Device',

    -- Сон та психоемоційний стан
    sleep_duration_hours DOUBLE PRECISION,
    sleep_duration_unit VARCHAR(16) NOT NULL DEFAULT 'h',
    sleep_awakenings_count INTEGER,
    sleep_awakenings_unit VARCHAR(16) NOT NULL DEFAULT 'count',
    stress_index_0_100 DOUBLE PRECISION CHECK (stress_index_0_100 IS NULL OR (stress_index_0_100 >= 0 AND stress_index_0_100 <= 100)),
    stress_unit VARCHAR(16) NOT NULL DEFAULT 'scale_0_100',
    sleep_quality_0_100 DOUBLE PRECISION CHECK (sleep_quality_0_100 IS NULL OR (sleep_quality_0_100 >= 0 AND sleep_quality_0_100 <= 100)),
    sleep_quality_unit VARCHAR(16) NOT NULL DEFAULT 'scale_0_100',
    neuro_sleep_sensor VARCHAR(64) NOT NULL DEFAULT 'Actigraphy + HRV Autonomic Stress Estimator',

    -- Цілісність та синхронізація
    hmac_sha256_signature VARCHAR(128) NOT NULL,
    synced_to_earth BOOLEAN NOT NULL DEFAULT FALSE,
    earth_sync_batch_id UUID,
    archived BOOLEAN NOT NULL DEFAULT FALSE,

    CONSTRAINT uq_telemetry_astronaut_nonce UNIQUE (astronaut_id, sequence_nonce)
);

-- Обов'язкові індекси за ID космонавта та часом
CREATE INDEX IF NOT EXISTS idx_telemetry_astronaut_time ON telemetry(astronaut_id, measured_at_utc DESC);
CREATE INDEX IF NOT EXISTS idx_telemetry_measured_at ON telemetry(measured_at_utc DESC);
CREATE INDEX IF NOT EXISTS idx_telemetry_unsynced ON telemetry(synced_to_earth, measured_at_utc ASC) WHERE synced_to_earth = FALSE;

-- 5. Окрема таблиця параметрів середовища проживання (Habitat Module — НЕ змішується зі скафандром)
CREATE TABLE IF NOT EXISTS habitat_telemetry (
    habitat_record_id UUID PRIMARY KEY,
    module_id VARCHAR(32) NOT NULL DEFAULT 'HAB-ALPHA-CORE',
    measured_at_utc TIMESTAMPTZ NOT NULL,
    atmospheric_pressure_kpa DOUBLE PRECISION NOT NULL,
    pressure_unit VARCHAR(16) NOT NULL DEFAULT 'kPa',
    pressure_sensor VARCHAR(64) NOT NULL DEFAULT 'MEMS Barometric Transducer',

    o2_concentration_pct DOUBLE PRECISION NOT NULL,
    o2_unit VARCHAR(16) NOT NULL DEFAULT '% vol',
    o2_sensor VARCHAR(64) NOT NULL DEFAULT 'Galvanic Zirconia O2 Cell',

    co2_concentration_ppm DOUBLE PRECISION NOT NULL,
    co2_unit VARCHAR(16) NOT NULL DEFAULT 'ppm',
    co2_sensor VARCHAR(64) NOT NULL DEFAULT 'NDIR Optical CO2 Analyzer',

    nitrogen_pct DOUBLE PRECISION NOT NULL DEFAULT 78.0,
    trace_co_ppm DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    other_gases_unit VARCHAR(16) NOT NULL DEFAULT '% vol / ppm',

    air_temperature_c DOUBLE PRECISION NOT NULL,
    temperature_unit VARCHAR(16) NOT NULL DEFAULT '°C',
    humidity_rh_pct DOUBLE PRECISION NOT NULL,
    humidity_unit VARCHAR(16) NOT NULL DEFAULT '% RH',
    thermo_hygro_sensor VARCHAR(64) NOT NULL DEFAULT 'Capacitive Polymer Hygrometer + RTD Pt100',

    illuminance_lx DOUBLE PRECISION,
    illuminance_unit VARCHAR(16) NOT NULL DEFAULT 'lx',
    illuminance_sensor VARCHAR(64) NOT NULL DEFAULT 'Silicon Photodiode Luxmeter',

    synced_to_earth BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_habitat_measured_at ON habitat_telemetry(module_id, measured_at_utc DESC);

-- 6. Окрема таблиця радіаційного контролю
CREATE TABLE IF NOT EXISTS radiation_telemetry (
    radiation_record_id UUID PRIMARY KEY,
    astronaut_id VARCHAR(32) REFERENCES astronauts(astronaut_id) ON DELETE SET NULL,
    module_id VARCHAR(32) NOT NULL DEFAULT 'HAB-ALPHA-CORE',
    measured_at_utc TIMESTAMPTZ NOT NULL,

    absorbed_dose_mgy DOUBLE PRECISION NOT NULL,
    absorbed_dose_unit VARCHAR(16) NOT NULL DEFAULT 'mGy',

    equivalent_dose_msv DOUBLE PRECISION NOT NULL,
    equivalent_dose_unit VARCHAR(16) NOT NULL DEFAULT 'mSv',

    equivalent_dose_rate_usv_h DOUBLE PRECISION NOT NULL,
    dose_rate_unit VARCHAR(16) NOT NULL DEFAULT 'µSv/h',

    gcr_particle_flux_cm2_s DOUBLE PRECISION NOT NULL,
    cosmic_radiation_unit VARCHAR(32) NOT NULL DEFAULT 'particles/(cm²·s)',
    detector_type VARCHAR(64) NOT NULL DEFAULT 'Silicon PIN Dosimeter + TEPC Spectrometer',

    synced_to_earth BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_radiation_astronaut_time ON radiation_telemetry(astronaut_id, measured_at_utc DESC);

-- 7. Окрема таблиця показників скафандра (EVA Spacesuit — строго відокремлена від середовища станції)
CREATE TABLE IF NOT EXISTS spacesuit_telemetry (
    suit_record_id UUID PRIMARY KEY,
    suit_id VARCHAR(32) NOT NULL,
    astronaut_id VARCHAR(32) NOT NULL REFERENCES astronauts(astronaut_id) ON DELETE RESTRICT,
    measured_at_utc TIMESTAMPTZ NOT NULL,

    suit_spo2_pct DOUBLE PRECISION NOT NULL,
    suit_spo2_unit VARCHAR(16) NOT NULL DEFAULT '%',
    suit_heart_rate_bpm DOUBLE PRECISION NOT NULL,
    suit_heart_rate_unit VARCHAR(16) NOT NULL DEFAULT 'bpm',
    suit_biomed_sensor VARCHAR(64) NOT NULL DEFAULT 'EVA Helmet Optical Forehead PPG/SpO2',

    suit_internal_pressure_kpa DOUBLE PRECISION NOT NULL,
    suit_pressure_unit VARCHAR(16) NOT NULL DEFAULT 'kPa',
    suit_pressure_sensor VARCHAR(64) NOT NULL DEFAULT 'Dual-Channel EVA PLSS Pressure Transducer',

    suit_internal_temp_c DOUBLE PRECISION NOT NULL,
    suit_temp_unit VARCHAR(16) NOT NULL DEFAULT '°C',
    suit_o2_pct DOUBLE PRECISION NOT NULL,
    suit_o2_unit VARCHAR(16) NOT NULL DEFAULT '% vol',
    suit_humidity_rh_pct DOUBLE PRECISION NOT NULL,
    suit_humidity_unit VARCHAR(16) NOT NULL DEFAULT '% RH',
    suit_env_sensor VARCHAR(64) NOT NULL DEFAULT 'PLSS Sublimator & Gas Loop Sensor Array',

    synced_to_earth BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_spacesuit_astronaut_time ON spacesuit_telemetry(astronaut_id, measured_at_utc DESC);

-- 8. Таблиця аварійних подій та сповіщень (INFO, WARNING, CRITICAL)
CREATE TABLE IF NOT EXISTS alert_events (
    alert_id UUID PRIMARY KEY,
    astronaut_id VARCHAR(32) REFERENCES astronauts(astronaut_id) ON DELETE RESTRICT,
    source_subsystem VARCHAR(32) NOT NULL,
    parameter_name VARCHAR(64) NOT NULL,
    severity VARCHAR(16) NOT NULL CHECK (severity IN ('INFO', 'WARNING', 'CRITICAL')),
    measured_value DOUBLE PRECISION NOT NULL,
    unit VARCHAR(16) NOT NULL,
    threshold_limit DOUBLE PRECISION NOT NULL,
    trend_slope_per_min DOUBLE PRECISION,
    message TEXT NOT NULL,
    triggered_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    acknowledged BOOLEAN NOT NULL DEFAULT FALSE,
    acknowledged_by_user UUID REFERENCES users(user_id) ON DELETE SET NULL,
    acknowledged_at_utc TIMESTAMPTZ,
    synced_to_earth BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_alerts_severity_time ON alert_events(severity, triggered_at_utc DESC);
CREATE INDEX IF NOT EXISTS idx_alerts_astronaut_time ON alert_events(astronaut_id, triggered_at_utc DESC);

-- 9. Таблиця журналу дій та аудиту безпеки (Audit Log)
CREATE TABLE IF NOT EXISTS audit_logs (
    log_id UUID PRIMARY KEY,
    occurred_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    actor_user_id UUID REFERENCES users(user_id) ON DELETE SET NULL,
    actor_username VARCHAR(64) NOT NULL,
    actor_role VARCHAR(32) NOT NULL,
    action_type VARCHAR(64) NOT NULL,
    target_resource VARCHAR(128) NOT NULL,
    status VARCHAR(32) NOT NULL,
    client_addr VARCHAR(64),
    details JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_time ON audit_logs(occurred_at_utc DESC);

-- 10. Таблиця синхронізації із Землею (Earth Uplink Sync Queue & Verification)
CREATE TABLE IF NOT EXISTS earth_sync_batches (
    batch_id UUID PRIMARY KEY,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    transmitted_at_utc TIMESTAMPTZ,
    verified_at_utc TIMESTAMPTZ,
    priority VARCHAR(16) NOT NULL CHECK (priority IN ('CRITICAL_IMMEDIATE', 'HIGH_ALERT', 'NORMAL_TELEMETRY', 'ARCHIVE_BULK')),
    record_count INTEGER NOT NULL CHECK (record_count > 0),
    compressed_size_bytes INTEGER NOT NULL,
    sha256_checksum VARCHAR(128) NOT NULL,
    sync_status VARCHAR(32) NOT NULL CHECK (sync_status IN ('QUEUED', 'IN_TRANSIT', 'VERIFIED_ACK', 'FAILED_RETRY')),
    retry_count INTEGER NOT NULL DEFAULT 0,
    comm_window_id VARCHAR(64),
    error_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_earth_sync_status_priority ON earth_sync_batches(sync_status, priority, created_at_utc ASC);

COMMIT;
