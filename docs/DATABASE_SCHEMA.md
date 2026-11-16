> **Submission note (2026-11-15):** The current desktop application uses an **in-memory repository**, not connected PostgreSQL or SQLite. Remote Earth transmission and some GUI pages remain simulations/placeholders. This document describes the intended architecture or schema; see [CURRENT_STATUS.md](CURRENT_STATUS.md) for implemented functionality.

# Опис структури бази даних та матриця сенсорів

## 1. Матриця фізичних параметрів, типів сенсорів, одиниць та частот опитування

Жоден параметр у системі не змішується в абстрактний «єдиний датчик». Кожне вимірювання супроводжується явно визначеними фізичними одиницями та специфікацією сенсора:

### 1.1. Біологічні показники космонавта (`telemetry`)
| Параметр | Поле в БД | Одиниця вимірювання | Тип необхідного сенсора | Орієнтовна частота | Джерело даних |
| :--- | :--- | :--- | :--- | :--- | :--- |
| ЧСС / пульс | `heart_rate_bpm` | `bpm` (уд/хв) | Optical PPG + 3-Lead Chest ECG Patch | 1.0 Гц | `BIOMED-SPI-BUS-01` |
| ЕКГ (відведення II + форма хвилі) | `ecg_lead_ii_mv`, `ecg_waveform_samples_mv` | `mV` (мВ) | 3-Lead Chest Bio-Potential Amplifier | 250 Гц | `BIOMED-SPI-BUS-01` |
| Артеріальний тиск (систолічний / діастолічний) | `bp_systolic_mmhg`, `bp_diastolic_mmhg` | `mmHg` (мм рт. ст.) | Optical Pulse Transit Time (PTT) Cuffless Sensor | 0.033 Гц (1 раз / 30 с) | `BIOMED-I2C-BUS-01` |
| Частота дихання | `respiration_rate_cpm` | `cycles/min` (циклів/хв) | Thoracic Bio-Impedance Pneumography Belt | 1.0 Гц | `BIOMED-SPI-BUS-02` |
| Глибина дихання (дихальний об'єм) | `tidal_volume_ml` | `mL` (мл) | Thoracic Bio-Impedance Pneumography Belt | 1.0 Гц | `BIOMED-SPI-BUS-02` |
| Сатурація SpO2 | `spo2_pct` | `%` | Dual-Wavelength Finger/Wrist Pulse Oximeter (660/940 нм) | 1.0 Гц | `BIOMED-I2C-BUS-02` |
| Температура тіла | `core_body_temp_c` | `°C` | Tympanic IR Thermistor / Ingestible Core Pill | 0.2 Гц (1 раз / 5 с) | `BIOMED-BLE-CH-01` |
| Глюкоза | `glucose_mmol_l` | `mmol/L` (ммоль/л) | Subcutaneous Continuous Interstitial Glucose Monitor (CGM) | 0.0033 Гц (1 раз / 5 хв) | `BIOMED-BLE-CH-02` |
| Лактат | `lactate_mmol_l` | `mmol/L` (ммоль/л) | Electrochemical Interstitial Sweat Biosensor Patch | 0.016 Гц (1 раз / хв) | `BIOMED-BLE-CH-03` |
| Маса тіла, зріст, ІМТ | `body_mass_kg`, `height_cm`, `bmi_kg_m2` | `kg`, `cm`, `kg/m²` | Microgravity Linear Inertial Mass Measurement Device | Періодичний (щоденний) | `HAB-CAN-BUS-01` |
| Тривалість сну, пробудження, стрес, якість сну | `sleep_duration_hours`, `sleep_awakenings_count`, `stress_index_0_100`, `sleep_quality_0_100` | `h`, `count`, `scale_0_100` | Wrist Actigraphy + EEG Headband + HRV RMSSD Analyzer | 0.2 Гц (стрес) / за цикл сну | `BIOMED-DSP-CORE` |

### 1.2. Параметри середовища проживання станції (`habitat_telemetry` — окремо від скафандра)
| Параметр | Поле в БД | Одиниця | Тип сенсора | Частота | Джерело |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Атмосферний тиск | `atmospheric_pressure_kpa` | `kPa` (кПа) | MEMS Piezoresistive Barometric Transducer | 0.5 Гц | `ECLSS-MODBUS-01` |
| Концентрація кисню O2 | `o2_concentration_pct` | `% vol` (% об'єму) | Galvanic Zirconia O2 Cell | 0.5 Гц | `ECLSS-MODBUS-01` |
| Концентрація CO2 | `co2_concentration_ppm` | `ppm` | NDIR (Non-Dispersive Infrared) Gas Analyzer | 0.5 Гц | `ECLSS-MODBUS-01` |
| Інші гази (N2, CO) | `nitrogen_pct`, `trace_co_ppm` | `% vol`, `ppm` | Mass Spectrometer + Electrochemical Trace CO Cell | 0.1–0.5 Гц | `ECLSS-MODBUS-01` |
| Температура повітря та вологість | `air_temperature_c`, `humidity_rh_pct` | `°C`, `% RH` | Pt100 RTD Thermometer + Capacitive Polymer Hygrometer | 0.5 Гц | `ECLSS-MODBUS-02` |
| Рівень освітленості | `illuminance_lx` | `lx` (лк) | Silicon Photodiode Circadian Luxmeter | 0.2 Гц | `ECLSS-MODBUS-02` |

### 1.3. Радіаційний контроль (`radiation_telemetry`)
| Параметр | Поле в БД | Одиниця | Тип детектора | Частота | Джерело |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Накопичена поглинута доза | `absorbed_dose_mgy` | `mGy` (мГр) | Active Silicon PIN Diode Dosimeter | 0.2 Гц | `RAD-RS485-BUS` |
| Еквівалентна доза | `equivalent_dose_msv` | `mSv` (мЗв) | Tissue-Equivalent Proportional Counter (TEPC) | 0.2 Гц | `RAD-RS485-BUS` |
| Потужність еквівалентної дози | `equivalent_dose_rate_usv_h` | `µSv/h` (мкЗв/год) | TEPC Real-Time Dose Rate Spectrometer | 0.2 Гц | `RAD-RS485-BUS` |
| Космічне випромінювання (GCR/SPE) | `gcr_particle_flux_cm2_s` | `particles/(cm²·s)` | Charged Particle Telescope Detector | 0.2 Гц | `RAD-RS485-BUS` |

### 1.4. Показники скафандра (`spacesuit_telemetry` — строго відокремлено від середовища станції)
| Параметр | Поле в БД | Одиниця | Тип сенсора | Частота | Джерело |
| :--- | :--- | :--- | :--- | :--- | :--- |
| SpO2 та пульс у скафандрі | `suit_spo2_pct`, `suit_heart_rate_bpm` | `%`, `bpm` | EVA Helmet Forehead Optical PPG/SpO2 Sensor | 1.0 Гц | `PLSS-CAN-BUS` |
| Тиск усередині скафандра | `suit_internal_pressure_kpa` | `kPa` (кПа) | Redundant Dual-Channel EVA PLSS Pressure Transducer | 2.0 Гц | `PLSS-CAN-BUS` |
| Температура всередині скафандра | `suit_internal_temp_c` | `°C` | LCVG Sublimator Cooling Loop Thermistor | 1.0 Гц | `PLSS-CAN-BUS` |
| Концентрація кисню та вологість | `suit_o2_pct`, `suit_humidity_rh_pct` | `% vol`, `% RH` | PLSS Primary Oxygen Loop & Water Separator Sensors | 0.5–1.0 Гц | `PLSS-CAN-BUS` |

## 2. Таблиці та індекси PostgreSQL

У файлі `migrations/001_initial_schema.sql` реалізовано:
1. `roles_permissions` — матриця прав доступу RBAC.
2. `astronauts` — картки космонавтів із тригером `trg_immutable_astronaut_id`, що блокує будь-яку спробу змінити `astronaut_id`.
3. `users` — облікові записи з хешами `Argon2id`, лічильником невдалих спроб та часом блокування.
4. `telemetry` — біомедичні вимірювання з індексами `idx_telemetry_astronaut_time (astronaut_id, measured_at_utc DESC)` та унікальним обмеженням `(astronaut_id, sequence_nonce)`.
5. `habitat_telemetry` — параметри житлового модуля станції.
6. `radiation_telemetry` — радіаційне навантаження.
7. `spacesuit_telemetry` — параметри скафандра EVA.
8. `alert_events` — історія аварійних подій (`INFO`, `WARNING`, `CRITICAL`) із фіксацією підтвердження (`acknowledged_by_user`).
9. `audit_logs` — незмінний журнал аудиту дій користувачів.
10. `earth_sync_batches` — черга та журнал верифікованої синхронізації із Землею.
