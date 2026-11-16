//! FPECS Mission Control desktop. All displayed readings are simulated.
mod persistence;
mod db_config;

use std::{sync::mpsc::{self, Receiver}, time::{Duration, Instant}};
use chrono::Utc;
use eframe::egui::{self, Color32, RichText};
use egui_plot::{Line, Plot, PlotPoints};

use desktop_gui::{MissionControlAppState, MissionControlPage, TimeRangePreset};
use persistence::{Persistence, test_postgres};
use db_config::{DatabaseConfig, DatabaseMode, load_password, save_password};
use shared_core::{Role, SimulationScenario, SimulatorControlCommand, TelemetryEnvelope};

// Midnight-blue command deck: luminous telemetry, restrained contrast and spacious cards.
const BG: Color32 = Color32::from_rgb(8, 15, 28);
const SIDEBAR: Color32 = Color32::from_rgb(10, 21, 36);
const PANEL: Color32 = Color32::from_rgb(17, 31, 49);
const PANEL_2: Color32 = Color32::from_rgb(22, 42, 62);
const BORDER: Color32 = Color32::from_rgb(37, 65, 86);
const CYAN: Color32 = Color32::from_rgb(103, 235, 230);
const MUTED: Color32 = Color32::from_rgb(152, 175, 198);
const WHITE: Color32 = Color32::from_rgb(234, 244, 252);
const GREEN: Color32 = Color32::from_rgb(112, 234, 181);
const AMBER: Color32 = Color32::from_rgb(250, 197, 110);
const RED: Color32 = Color32::from_rgb(255, 124, 150);
const BLUE: Color32 = Color32::from_rgb(137, 163, 255);

fn configure_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(WHITE);
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.faint_bg_color = PANEL;
    visuals.extreme_bg_color = SIDEBAR;
    visuals.selection.bg_fill = Color32::from_rgb(39, 106, 125);
    visuals.selection.stroke.color = CYAN;
    visuals.hyperlink_color = CYAN;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.fg_stroke.color = MUTED;
    visuals.widgets.inactive.bg_fill = PANEL_2;
    visuals.widgets.inactive.fg_stroke.color = WHITE;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(29, 83, 101);
    visuals.widgets.hovered.fg_stroke.color = CYAN;
    visuals.widgets.active.bg_fill = Color32::from_rgb(32, 111, 125);
    visuals.widgets.active.fg_stroke.color = WHITE;
    ctx.set_visuals(visuals);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(14.0, 9.0);
    style.spacing.window_margin = egui::Margin::same(16.0);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
    style.visuals.widgets.active.rounding = egui::Rounding::same(8.0);
    ctx.set_style(style);
}

fn panel(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(PANEL)
        .inner_margin(19.0)
        .rounding(egui::Rounding::same(14.0))
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .show(ui, add);
}

fn small_label(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).color(MUTED).size(12.0));
}

fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(CYAN).size(11.0).strong().monospace());
}

fn page_heading(ui: &mut egui::Ui, title: &str, subheading: &str) {
    ui.add_space(2.0);
    eyebrow(ui, "FPECS  /  LUNAR OPERATIONS");
    ui.heading(RichText::new(title).size(29.0).strong().color(WHITE));
    small_label(ui, subheading);
    ui.add_space(12.0);
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).color(WHITE).size(17.0).strong());
    ui.add_space(4.0);
}

fn status_chip(ui: &mut egui::Ui, text: &str, tint: Color32) {
    egui::Frame::none().fill(PANEL_2).rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 6.0)).show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(tint).strong().monospace());
        });
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    egui::Frame::none().fill(PANEL_2).rounding(egui::Rounding::same(11.0))
        .stroke(egui::Stroke::new(0.5_f32, BORDER))
        .inner_margin(14.0).show(ui, |ui| {
            ui.set_min_width(145.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").size(10.0).color(color));
                ui.label(RichText::new(label).size(11.0).color(MUTED).strong());
            });
            ui.add_space(5.0);
            ui.label(RichText::new(value).size(24.0).strong().color(WHITE).monospace());
        });
}

fn role_allows_technical(role: Role) -> bool {
    matches!(role, Role::Engineer | Role::Commander | Role::FlightSurgeon | Role::MissionControl | Role::Administrator)
}

fn latest_for(state: &MissionControlAppState, astronaut_id: &str) -> Option<TelemetryEnvelope> {
    let now = Utc::now();
    state.database.query_telemetry_by_astronaut_and_range(
        astronaut_id,
        now - chrono::Duration::days(3650),
        now + chrono::Duration::minutes(1),
    ).into_iter().max_by_key(|frame| frame.measured_at_utc)
}

fn show_plot(ui: &mut egui::Ui, id: &str, name: &str, values: Vec<[f64; 2]>, color: Color32) {
    small_label(ui, name);
    Plot::new(id).height(195.0).allow_drag(false).allow_scroll(false)
        .allow_zoom(false).allow_boxed_zoom(false)
        .show(ui, |plot_ui| {
            plot_ui.line(Line::new(PlotPoints::from(values)).name(name)
                .color(color).width(2.0));
        });
}

struct DatabaseWizard {
    config: DatabaseConfig,
    password: String,
    status: String,
    tested: bool,
    confirm_switch: bool,
    // DB connection checks run off the GUI thread to keep egui responsive.
    test_receiver: Option<Receiver<Result<String, String>>>,
}

impl DatabaseWizard {
    fn new() -> Self {
        match DatabaseConfig::load() {
            Ok(config) => Self {
                config, password: String::new(), status: String::new(),
                tested: false, confirm_switch: false, test_receiver: None,
            },
            Err(e) => Self {
                config: DatabaseConfig::default(), password: String::new(),
                status: format!("Налаштування не прочитано: {e}"),
                tested: false, confirm_switch: false, test_receiver: None,
            },
        }
    }

    fn check_connection(&mut self) {
        if self.test_receiver.is_some() { return; }
        self.tested = false;
        self.status = "Перевірка з’єднання з PostgreSQL…".into();
        let config = self.config.clone();
        let entered_password = self.password.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let password = if entered_password.is_empty() {
                load_password(&config).ok().flatten().unwrap_or_default()
            } else { entered_password };
            let result = test_postgres(&config, &password);
            let _ = sender.send(result);
        });
        self.test_receiver = Some(receiver);
    }

    fn poll_connection(&mut self, ctx: &egui::Context) {
        let received = self.test_receiver.as_ref().map(|rx| rx.try_recv());
        match received {
            Some(Ok(Ok(message))) => {
                self.tested = true;
                self.status = format!("✓ {message}");
                self.test_receiver = None;
            }
            Some(Ok(Err(message))) => {
                self.tested = false;
                self.status = format!("Помилка підключення: {message}");
                self.test_receiver = None;
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.tested = false;
                self.status = "Не вдалося завершити перевірку з'єднання.".into();
                self.test_receiver = None;
            }
            _ => {}
        }
        if self.test_receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

struct MissionControlWindow {
    state: MissionControlAppState,
    storage: Persistence,
    db_wizard: DatabaseWizard,
    last_step: Instant,
    last_save: Instant,
    dirty: bool,
}

impl MissionControlWindow {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_theme(&cc.egui_ctx);
        let mut state = MissionControlAppState::new_prototype_environment();
        let storage = Persistence::initialize(&mut state);
        Self {
            state, storage, db_wizard: DatabaseWizard::new(),
            last_step: Instant::now(),
            last_save: Instant::now() - Duration::from_secs(10),
            dirty: false,
        }
    }

    fn changed(&mut self) {
        self.dirty = true;
        // Persist administrative actions immediately, not only on the next simulator tick.
        self.last_save = Instant::now() - Duration::from_secs(10);
    }

    fn is_admin(&self) -> bool {
        self.state.active_session.as_ref().is_some_and(|s|
            self.state.access_control.authorize_user_administration(&s.session_token).is_ok())
    }

    fn is_technical(&self) -> bool {
        self.state.active_session.as_ref().is_some_and(|s|
            self.state.access_control.validate_session(&s.session_token).is_ok()
                && role_allows_technical(s.role))
    }

    fn tick(&mut self, ctx: &egui::Context) {
        if self.state.simulator.running && !self.state.simulator.astronauts.is_empty() {
            let interval = Duration::from_millis(self.state.simulator.poll_interval_ms.max(200));
            if self.last_step.elapsed() >= interval {
                self.state.step_simulation_pipeline();
                self.last_step = Instant::now();
                self.dirty = true;
            }
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }

    fn save_when_needed(&mut self) {
        if self.dirty && self.last_save.elapsed() >= Duration::from_secs(2) {
            match self.storage.save(&self.state) {
                Ok(()) => { self.dirty = false; self.last_save = Instant::now(); }
                Err(e) => {
                    self.state.status_banner = format!("Помилка збереження: {e}");
                    self.last_save = Instant::now();
                }
            }
        }
    }

    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("mission_header")
            .frame(egui::Frame::none().fill(SIDEBAR)
                .inner_margin(egui::Margin::symmetric(22.0, 13.0))
                .stroke(egui::Stroke::new(1.0_f32, BORDER)))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("◉  FPECS").size(22.0).strong().color(CYAN));
                        ui.label(RichText::new("FLIGHT PERSONNEL · MISSION CONTROL")
                            .size(10.0).monospace().color(MUTED));
                    });
                    ui.add_space(17.0);
                    status_chip(ui, "MOON / SIMULATED", AMBER);
                    status_chip(ui, if self.state.simulator.running {
                        "● TELEMETRY LIVE" } else { "○ SIMULATION PAUSED" },
                        if self.state.simulator.running {GREEN} else {AMBER});
                    status_chip(ui, if self.storage.is_postgres() { "● POSTGRESQL" }
                        else { "● LOCAL MODE" }, if self.storage.is_postgres() {GREEN} else {CYAN});
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(session) = self.state.active_session.clone() {
                            if ui.button("Вийти  ↗").clicked() {
                                let _ = self.state.access_control.terminate_session(
                                    &mut self.state.database, &session.session_token);
                                self.state.active_session = None;
                                self.changed();
                            }
                            ui.label(RichText::new(format!("◌  {}", session.username))
                                .color(WHITE).strong());
                        } else {
                            ui.label(RichText::new("○  НЕ АВТОРИЗОВАНО").color(AMBER));
                        }
                    });
                });
                if !self.state.status_banner.is_empty() || self.storage.last_error.is_some() {
                    ui.add_space(6.0);
                    small_label(ui, self.state.status_banner.clone());
                    if let Some(error) = &self.storage.last_error {
                        ui.label(RichText::new(error).size(11.0).color(RED));
                    }
                }
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("mission_nav")
            .frame(egui::Frame::none().fill(SIDEBAR).inner_margin(egui::Margin::same(16.0)))
            .resizable(true).default_width(235.0).min_width(185.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                eyebrow(ui, "COMMAND DECK / FPECS");
                ui.label(RichText::new("LUNAR OPERATIONS").size(15.0).strong().color(WHITE));
                ui.add_space(15.0);
                ui.separator();
                ui.add_space(11.0);
                let sections: &[(&str, &[(MissionControlPage, &str)])] = &[
                    ("MISSION OVERVIEW", &[
                        (MissionControlPage::CrewDashboard, "⌂   Головна панель"),
                        (MissionControlPage::AstronautDetail, "♡   Здоров'я екіпажу"),
                        (MissionControlPage::AlertCenter, "⚑   Сповіщення"),
                    ]),
                    ("LIVE SYSTEMS", &[
                        (MissionControlPage::SensorDiagnostics, "◎   Сенсори"),
                        (MissionControlPage::HabitatEnvironment, "▣   Середовище"),
                        (MissionControlPage::RadiationControl, "◉   Радіація"),
                        (MissionControlPage::SimulatorControlPanel, "▷   Симулятор"),
                        (MissionControlPage::EarthSynchronization, "◇   Earth Link"),
                    ]),
                    ("ADMINISTRATION", &[
                        (MissionControlPage::UserAndAstronautManagement, "♙   Екіпаж і акаунти"),
                        (MissionControlPage::SystemSettings, "⚙   Налаштування"),
                        (MissionControlPage::SystemAuditLog, "≡   Журнал аудиту"),
                    ]),
                ];
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (section, pages) in sections {
                        ui.add_space(7.0);
                        ui.label(RichText::new(*section).size(10.0).color(MUTED).monospace().strong());
                        ui.add_space(4.0);
                        for (page, label) in *pages {
                            let active = self.state.current_page == *page;
                            let button = egui::Button::new(RichText::new(*label).size(13.0)
                                    .color(if active {CYAN} else {WHITE}))
                                .fill(if active {PANEL_2} else {SIDEBAR})
                                .stroke(egui::Stroke::new(if active {1.0_f32} else {0.0_f32}, BORDER))
                                .rounding(egui::Rounding::same(9.0));
                            if ui.add_sized([ui.available_width(), 36.0], button).clicked() {
                                self.state.current_page = *page;
                            }
                        }
                        ui.add_space(11.0);
                    }
                });
                ui.separator();
                ui.add_space(7.0);
                small_label(ui, "MISSION STATUS · DEMO / 2026");
                small_label(ui, if self.storage.is_postgres() {"● PostgreSQL backend"}
                    else {"● Offline storage"});
            });
    }

    fn login(&mut self, ui: &mut egui::Ui) {
        if self.state.active_session.is_some() { return; }
        panel(ui, |ui| {
            ui.heading("Авторизація системи");
            ui.horizontal(|ui| {
                ui.label("Логін");
                ui.text_edit_singleline(&mut self.state.login_username_input);
                ui.label("Пароль");
                ui.add(egui::TextEdit::singleline(&mut self.state.login_password_input).password(true));
                if ui.button("Увійти").clicked() {
                    if let Err(e) = self.state.authenticate_user() {
                        self.state.login_password_input.clear();
                        self.state.status_banner = format!("Помилка входу: {e}");
                    }
                    self.changed();
                }
            });
        });
    }

    fn setup(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "Welcome to Mission Control", "Початкова конфігурація · чиста база без демонстраційних акаунтів");
        panel(ui, |ui| {
            eyebrow(ui, "01 / STORAGE");
            ui.label("Оберіть локальний режим або PostgreSQL. Якщо змінюєте режим — перезапустіть застосунок до створення першого адміністратора.");
        });
        self.database_wizard(ui);
        panel(ui, |ui| {
            eyebrow(ui, "02 / SECURITY");
            section_title(ui, "Створення головного адміністратора");
            ui.label("Логін адміністратора");
            ui.text_edit_singleline(&mut self.state.first_admin_username);
            ui.label("Пароль (мінімум 8 символів)");
            ui.add(egui::TextEdit::singleline(&mut self.state.first_admin_password).password(true));
            if ui.button("Створити адміністратора").clicked() {
                match self.state.bootstrap_first_administrator() {
                    Ok(()) => { self.state.current_page = MissionControlPage::UserAndAstronautManagement; self.changed(); }
                    Err(e) => self.state.status_banner = format!("Не вдалося створити акаунт: {e}"),
                }
            }
        });
    }

    fn dashboard(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "Mission Overview", "Командна панель моніторингу здоров’я екіпажу · виключно симульовані показники");
        panel(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    eyebrow(ui, "LUNAR HEALTH MONITORING / DEMONSTRATION");
                    ui.label(RichText::new("Every heartbeat. One mission.").size(23.0).strong().color(WHITE));
                    small_label(ui, "Симуляція телеметрії, історія показників та підтримка ухвалення рішень.");
                });
                status_chip(ui, "● SYSTEM ONLINE", GREEN);
                status_chip(ui, "DATA / SYNTHETIC", AMBER);
            });
        });
        section_title(ui, "Mission at a glance");
        ui.horizontal_wrapped(|ui| {
            metric(ui, "КОСМОНАВТИ", &self.state.database.list_astronauts().len().to_string(), CYAN);
            metric(ui, "ЗАПИСІВ", &self.state.database.total_telemetry_count().to_string(), GREEN);
            metric(ui, "СПОВІЩЕННЯ", &self.state.database.list_alerts().len().to_string(), AMBER);
            metric(ui, "СИМУЛЯТОР", if self.state.simulator.running {"RUNNING"} else {"PAUSED"},
                if self.state.simulator.running {GREEN} else {AMBER});
        });
        self.login(ui);
        // Live dashboard chart is shown only when the current role may see
        // this particular astronaut's detailed medical history.
        if let Some(person) = self.state.database.list_astronauts().into_iter().find(|person| {
            self.state.active_session.as_ref().is_some_and(|session|
                self.state.access_control.authorize_medical_detail_access(
                    &session.session_token, person.astronaut_id()).is_ok())
        }) {
            let now = Utc::now();
            let records = self.state.database.query_telemetry_by_astronaut_and_range(
                person.astronaut_id(), now - chrono::Duration::hours(24), now);
            let points: Vec<[f64; 2]> = records.iter().rev().take(120)
                .collect::<Vec<_>>().into_iter().rev().enumerate()
                .filter_map(|(index, record)| record.cardiovascular.heart_rate_bpm.as_ref()
                    .map(|m| [index as f64, m.value]))
                .collect();
            if !points.is_empty() {
                section_title(ui, "LIVE HEALTH TREND");
                panel(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        eyebrow(ui, "HEART RATE / 24H");
                        status_chip(ui, &format!("CREW / {}", person.callsign), CYAN);
                        status_chip(ui, "SIMULATED", AMBER);
                    });
                    show_plot(ui, "dashboard_hr_preview", "HEART RATE / BPM", points, GREEN);
                });
            }
        }
        if self.is_technical() {
            ui.horizontal(|ui| {
                if ui.button("▶ Одне вимірювання").clicked() {
                    if !self.state.simulator.running { self.state.simulator.running = true; }
                    self.state.step_simulation_pipeline();
                    self.changed();
                }
                if ui.button("Керувати симулятором").clicked() {
                    self.state.current_page = MissionControlPage::SimulatorControlPanel;
                }
            });
        }
        section_title(ui, "CREW STATUS / LIVE FEED");
        let crew = self.state.database.list_astronauts();
        if crew.is_empty() {
            panel(ui, |ui| {
                ui.heading("Екіпаж ще не створено");
                ui.label("Створіть профіль космонавта в розділі 'Екіпаж і акаунти'.");
                if self.is_admin() && ui.button("Додати космонавта").clicked() {
                    self.state.current_page = MissionControlPage::UserAndAstronautManagement;
                }
            });
        }
        for person in crew {
            panel(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.horizontal(|ui| {
                        egui::Frame::none().fill(PANEL_2).rounding(egui::Rounding::same(12.0))
                            .inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                                ui.label(RichText::new("◉").size(20.0).color(CYAN));
                            });
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&person.full_name).size(19.0).strong().color(WHITE));
                            small_label(ui, format!("{}   ·   {}   ·   {}", person.astronaut_id(), person.mission_role, person.callsign));
                            small_label(ui, format!("EVA suit: {}", person.assigned_suit_id));
                        });
                    });
                    if let Some(frame) = latest_for(&self.state, person.astronaut_id()) {
                        if self.state.active_session.as_ref().is_some_and(|sess|
                            self.state.access_control.authorize_medical_detail_access(&sess.session_token, person.astronaut_id()).is_ok()) {
                            if let Some(hr) = frame.cardiovascular.heart_rate_bpm { metric(ui, "HR / BPM", &format!("{:.0}", hr.value), GREEN); }
                            if let Some(spo2) = frame.respiratory.spo2_pct { metric(ui, "SPO2 / %", &format!("{:.1}", spo2.value), CYAN); }
                        } else {
                            ui.label(RichText::new("TELEMETRY / RESTRICTED").color(AMBER));
                        }
                    } else {
                        ui.label(RichText::new("AWAITING TELEMETRY").color(AMBER));
                    }
                    if ui.button("Переглянути показники").clicked() {
                        match self.state.open_astronaut_card(person.astronaut_id()) {
                            Ok(()) => {},
                            Err(e) => self.state.status_banner = format!("Доступ обмежено: {e}"),
                        }
                    }
                });
            });
        }
    }

    fn astronaut_detail(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "Astronaut Telemetry", "Медичні показники, вибір члена екіпажу та історія симульованих вимірювань");
        let crew = self.state.database.list_astronauts();
        egui::ComboBox::from_label("Космонавт")
            .selected_text(if self.state.selected_astronaut_id.is_empty() {"Оберіть космонавта"} else {&self.state.selected_astronaut_id})
            .show_ui(ui, |ui| {
                for person in crew {
                    ui.selectable_value(&mut self.state.selected_astronaut_id, person.astronaut_id().to_owned(),
                        format!("{} — {}", person.astronaut_id(), person.full_name));
                }
            });
        if !self.state.can_view_selected_medical() {
            ui.label("Медична історія доступна лише FlightSurgeon або космонавту для власного профілю.");
            return;
        }
        egui::ComboBox::from_label("Історія")
            .selected_text(self.state.selected_time_range.label())
            .show_ui(ui, |ui| {
                for preset in [TimeRangePreset::Last5Minutes, TimeRangePreset::Last15Minutes,
                    TimeRangePreset::Last1Hour, TimeRangePreset::Last6Hours,
                    TimeRangePreset::Last12Hours, TimeRangePreset::Last24Hours, TimeRangePreset::Last7Days] {
                    ui.selectable_value(&mut self.state.selected_time_range, preset, preset.label());
                }
            });
        let now = Utc::now();
        let records = self.state.database.query_telemetry_by_astronaut_and_range(
            &self.state.selected_astronaut_id,
            now - self.state.selected_time_range.duration(), now + chrono::Duration::seconds(5));
        if records.is_empty() { ui.label("Поки немає вимірювань у вибраному інтервалі."); return; }
        if let Some(current) = records.last() {
            small_label(ui, format!("UTC: {} / Scenario: {}", current.measured_at_utc, current.scenario.as_code()));
            ui.horizontal_wrapped(|ui| {
                if let Some(v) = &current.cardiovascular.heart_rate_bpm {
                    metric(ui, "HEART RATE", &format!("{:.1} bpm", v.value), GREEN);
                }
                if let Some(v) = &current.respiratory.spo2_pct {
                    metric(ui, "SPO2", &format!("{:.1} %", v.value), CYAN);
                }
                if let Some(v) = &current.physiological.core_body_temp_c {
                    metric(ui, "BODY TEMP", &format!("{:.2} °C", v.value), AMBER);
                }
                if let Some(v) = &current.cardiovascular.systolic_bp_mmhg {
                    metric(ui, "SYSTOLIC BP", &format!("{:.0} mmHg", v.value), CYAN);
                }
            });
        }
        let indexed: Vec<&TelemetryEnvelope> = records.iter().rev().take(500).collect::<Vec<_>>()
            .into_iter().rev().collect();
        let hr: Vec<[f64;2]> = indexed.iter().enumerate().filter_map(|(i,r)| r.cardiovascular.heart_rate_bpm.as_ref().map(|m| [i as f64,m.value])).collect();
        let spo2: Vec<[f64;2]> = indexed.iter().enumerate().filter_map(|(i,r)| r.respiratory.spo2_pct.as_ref().map(|m| [i as f64,m.value])).collect();
        let temp: Vec<[f64;2]> = indexed.iter().enumerate().filter_map(|(i,r)| r.physiological.core_body_temp_c.as_ref().map(|m| [i as f64,m.value])).collect();
        let bp: Vec<[f64;2]> = indexed.iter().enumerate().filter_map(|(i,r)| r.cardiovascular.systolic_bp_mmhg.as_ref().map(|m| [i as f64,m.value])).collect();
        ui.columns(2, |cols| {
            panel(&mut cols[0], |ui| show_plot(ui,"hr_plot","HEART RATE / BPM",hr,GREEN));
            panel(&mut cols[1], |ui| show_plot(ui,"spo2_plot","OXYGEN SATURATION / %",spo2,CYAN));
        });
        ui.columns(2, |cols| {
            panel(&mut cols[0], |ui| show_plot(ui,"temp_plot","BODY TEMPERATURE / °C",temp,AMBER));
            panel(&mut cols[1], |ui| show_plot(ui,"bp_plot","SYSTOLIC PRESSURE / mmHg",bp,CYAN));
        });
    }

    fn alert_center(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "Alert Center", "Попередження й аномалії в телеметрії / симульовані сценарії");
        let user = self.state.active_session.as_ref();
        let allowed = user.is_some_and(|s| self.state.access_control.validate_session(&s.session_token).is_ok()
            && matches!(s.role, Role::Commander | Role::FlightSurgeon | Role::MissionControl));
        if !allowed { ui.label("Потрібна роль Commander, FlightSurgeon або MissionControl."); return; }
        let alerts = self.state.database.list_alerts().to_vec();
        if alerts.is_empty() { ui.label("Сповіщень немає. Система не виявила відхилень у поточних даних."); }
        for alert in alerts.into_iter().rev().take(200) {
            panel(ui, |ui| {
                let tint = match alert.severity.as_str() {
                    "CRITICAL" => RED, "WARNING" => AMBER, _ => CYAN,
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(alert.severity.as_str()).strong().color(tint));
                    ui.label(format!("{}  /  {}", alert.astronaut_id.as_deref().unwrap_or("MISSION"), alert.parameter_name));
                    small_label(ui, alert.triggered_at_utc.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                });
                ui.label(alert.message.clone());
                small_label(ui, format!("{:.2} {}  ·  threshold {:.2} {}", alert.measured_value,
                    alert.unit, alert.threshold_limit, alert.unit));
                if alert.acknowledged {
                    ui.label(RichText::new("Підтверджено").color(GREEN));
                } else if ui.button("Підтвердити отримання").clicked() {
                    if let Some(s) = &self.state.active_session {
                        if let Err(e) = self.state.database.acknowledge_alert(alert.alert_id, s.user_id) {
                            self.state.status_banner = e.to_string();
                        } else { self.changed(); }
                    }
                }
            });
        }
    }

    fn technical_overview(&mut self, ui: &mut egui::Ui, page: MissionControlPage) {
        let title = match page {
            MissionControlPage::SensorDiagnostics => "SENSOR DIAGNOSTICS",
            MissionControlPage::HabitatEnvironment => "HABITAT ENVIRONMENT",
            _ => "RADIATION MONITORING",
        };
        page_heading(ui, title, "Технічні метрики та діагностика систем моніторингу / симуляція");
        if !self.is_technical() {
            ui.label("Технічна телеметрія доступна лише авторизованим спеціалістам місії."); return;
        }
        let crew = self.state.database.list_astronauts();
        if crew.is_empty() { ui.label("Спочатку додайте космонавта, потім запустіть симуляцію."); }
        for person in crew {
            let Some(record) = latest_for(&self.state, person.astronaut_id()) else { continue; };
            panel(ui, |ui| {
                ui.heading(format!("{}  /  {}", person.callsign, person.astronaut_id()));
                match page {
                    MissionControlPage::SensorDiagnostics => {
                        for (name, metric) in [
                            ("Heart rate", record.cardiovascular.heart_rate_bpm.as_ref()),
                            ("SpO2", record.respiratory.spo2_pct.as_ref()),
                            ("Atmospheric pressure", Some(&record.habitat.atmospheric_pressure_kpa)),
                            ("Radiation", Some(&record.radiation.equivalent_dose_rate_usv_h)),
                        ] {
                            if let Some(m) = metric {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(name).color(CYAN));
                                    ui.label(format!("{} · {:.0} Hz · {}",
                                        m.sensor_type, m.sampling_rate_hz,
                                        if m.sensor_online {"ONLINE"} else {"OFFLINE"}));
                                });
                            }
                        }
                    }
                    MissionControlPage::HabitatEnvironment => {
                        ui.horizontal_wrapped(|ui| {
                            metric(ui,"PRESSURE",&format!("{:.1} kPa",record.habitat.atmospheric_pressure_kpa.value),CYAN);
                            metric(ui,"OXYGEN",&format!("{:.1} %",record.habitat.o2_concentration_pct.value),GREEN);
                            metric(ui,"CO₂",&format!("{:.0} ppm",record.habitat.co2_concentration_ppm.value),AMBER);
                            metric(ui,"HUMIDITY",&format!("{:.0} %",record.habitat.humidity_rh_pct.value),CYAN);
                            metric(ui,"AIR TEMP",&format!("{:.1} °C",record.habitat.air_temperature_c.value),GREEN);
                        });
                        small_label(ui,"Habitat parameters are separate from the EVA suit system.");
                    }
                    _ => {
                        ui.horizontal_wrapped(|ui| {
                            metric(ui,"ABSORBED DOSE",&format!("{:.2} mGy",record.radiation.absorbed_dose_mgy.value),CYAN);
                            metric(ui,"EQUIVALENT DOSE",&format!("{:.2} mSv",record.radiation.equivalent_dose_msv.value),AMBER);
                            metric(ui,"DOSE RATE",&format!("{:.1} µSv/h",record.radiation.equivalent_dose_rate_usv_h.value),RED);
                        });
                    }
                }
            });
        }
    }

    fn manage_users(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "Crew & Access", "Окремі профілі космонавтів і ролі користувачів · без попередньо створених акаунтів");
        if !self.is_admin() { ui.label("Лише Administrator може додавати профілі та облікові записи."); return; }
        panel(ui, |ui| {
            ui.heading("Новий космонавт");
            ui.horizontal(|ui| { ui.label("ID (AST-XXX)"); ui.text_edit_singleline(&mut self.state.new_astronaut_form.astronaut_id); });
            ui.horizontal(|ui| { ui.label("Повне ім'я"); ui.text_edit_singleline(&mut self.state.new_astronaut_form.full_name); });
            ui.horizontal(|ui| { ui.label("Роль у місії"); ui.text_edit_singleline(&mut self.state.new_astronaut_form.mission_role); });
            ui.horizontal(|ui| { ui.label("Позивний"); ui.text_edit_singleline(&mut self.state.new_astronaut_form.callsign); });
            ui.horizontal(|ui| { ui.label("Номер скафандра"); ui.text_edit_singleline(&mut self.state.new_astronaut_form.assigned_suit_id); });
            ui.horizontal(|ui| {
                ui.label("Зріст, см"); ui.add(egui::DragValue::new(&mut self.state.new_astronaut_form.height_cm).speed(1.0));
                ui.label("Маса, кг"); ui.add(egui::DragValue::new(&mut self.state.new_astronaut_form.baseline_mass_kg).speed(1.0));
            });
            if ui.button("Додати космонавта").clicked() {
                match self.state.submit_new_astronaut_form() {
                    Ok(()) => self.changed(),
                    Err(e) => self.state.status_banner = format!("Не вдалося створити профіль: {e}"),
                }
            }
        });
        panel(ui, |ui| {
            ui.heading("Окремий обліковий запис користувача");
            ui.horizontal(|ui| { ui.label("Логін"); ui.text_edit_singleline(&mut self.state.new_user_form.username); });
            ui.horizontal(|ui| {
                ui.label("Пароль");
                ui.add(egui::TextEdit::singleline(&mut self.state.new_user_form.password).password(true));
            });
            egui::ComboBox::from_label("Роль")
                .selected_text(self.state.new_user_form.role.as_str())
                .show_ui(ui, |ui| {
                    for role in [Role::Commander, Role::FlightSurgeon, Role::Engineer,
                        Role::CrewMember, Role::MissionControl, Role::Administrator] {
                        ui.selectable_value(&mut self.state.new_user_form.role, role, role.as_str());
                    }
                });
            ui.horizontal(|ui| { ui.label("ID космонавта (необов'язково)"); ui.text_edit_singleline(&mut self.state.new_user_form.linked_astronaut_id); });
            if ui.button("Створити користувача").clicked() {
                match self.state.submit_new_user_form() {
                    Ok(()) => self.changed(),
                    Err(e) => self.state.status_banner = format!("Не вдалося створити користувача: {e}"),
                }
            }
        });
        panel(ui, |ui| {
            ui.heading("Зареєстровані записи");
            for p in self.state.database.list_astronauts() {
                ui.label(format!("{} · {} · {}",p.astronaut_id(),p.full_name,p.mission_role));
            }
            ui.separator();
            for a in self.state.access_control.list_users() {
                ui.label(format!("{} · {} · {}",a.username,a.role.as_str(),
                    a.linked_astronaut_id.as_deref().unwrap_or("без профілю")));
            }
        });
    }

    fn simulator(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("SIMULATION CONTROL").size(25.0).color(CYAN));
        if !self.is_technical() { ui.label("Для керування симуляцією увійдіть як спеціаліст місії."); return; }
        panel(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button(if self.state.simulator.running {"⏸ Pause"} else {"▶ Resume"}).clicked() {
                    self.state.simulator.running = !self.state.simulator.running;
                    self.changed();
                }
                if ui.button("Виміряти зараз").clicked() {
                    let running = self.state.simulator.running;
                    self.state.simulator.running = true;
                    self.state.step_simulation_pipeline();
                    self.state.simulator.running = running;
                    self.changed();
                }
                ui.label(format!("Космонавтів: {}",self.state.simulator.astronauts.len()));
            });
            let a = ui.add(egui::Slider::new(&mut self.state.simulator.poll_interval_ms, 250..=5000)
                .text("Інтервал вимірювань, мс")).changed();
            let b = ui.add(egui::Slider::new(&mut self.state.simulator.error_margin_pct, 0.0..=10.0)
                .text("Похибка сенсорів, %")).changed();
            let c = ui.add(egui::Slider::new(&mut self.state.simulator.packet_loss_pct, 0.0..=50.0)
                .text("Втрата пакетів, %")).changed();
            let d = ui.checkbox(&mut self.state.simulator.link_disconnected,
                "Імітувати розрив зв'язку датчиків").changed();
            if a || b || c || d { self.dirty = true; }
        });
        let crew = self.state.database.list_astronauts();
        for astronaut in crew {
            let id = astronaut.astronaut_id().to_string();
            let current = self.state.simulator.astronauts.get(&id).map(|s| s.scenario);
            panel(ui, |ui| {
                ui.label(RichText::new(format!("{} / {}",astronaut.full_name,id)).strong().color(CYAN));
                if let Some(mut scenario) = current {
                    egui::ComboBox::from_id_source(format!("scenario_{id}"))
                        .selected_text(scenario.as_code()).show_ui(ui, |ui| {
                            for candidate in [SimulationScenario::Nominal,
                                SimulationScenario::PhysicalExertion, SimulationScenario::HighStress,
                                SimulationScenario::SleepDeprivation, SimulationScenario::EmergencyHypoxia,
                                SimulationScenario::SolarParticleEvent] {
                                ui.selectable_value(&mut scenario,candidate,candidate.as_code());
                            }
                        });
                    if Some(scenario) != current {
                        self.state.simulator.apply_command(SimulatorControlCommand::SetScenario {
                            astronaut_id:id, scenario,
                        });
                        self.changed();
                    }
                }
            });
        }
        small_label(ui,"Simulation scenarios are artificial test conditions, not clinical assessments.");
    }

    fn earth_sync(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "Earth Link", "Відкладена передача пакетів · виключно локальна симуляція, не NASA DSN");
        let can_manage = self.state.active_session.as_ref().is_some_and(|s|
            self.state.access_control.validate_session(&s.session_token).is_ok()
            && matches!(s.role, Role::Administrator | Role::Commander | Role::MissionControl));
        if !can_manage { ui.label("Для симуляції наземного зв'язку потрібна роль MissionControl, Commander або Administrator."); return; }
        ui.label(RichText::new("Це локальна модель черги передачі, НЕ справжній канал NASA DSN.").color(AMBER));
        panel(ui, |ui| {
            let mut seconds = self.state.distributor.settings.accumulation_window_secs;
            if ui.add(egui::Slider::new(&mut seconds,5..=3600).text("Вікно накопичення, с")).changed() {
                self.state.distributor.set_accumulation_slider_secs(seconds);
                self.dirty = true;
            }
            if ui.checkbox(&mut self.state.distributor.settings.earth_link_online,
                "Імітувати доступний канал зв'язку").changed() { self.dirty = true; }
            ui.horizontal(|ui| {
                metric(ui,"ПАКЕТИ В ЧЕРЗІ",&self.state.distributor.queued_batches_count().to_string(),CYAN);
                metric(ui,"НЕСИНХРОНІЗОВАНІ ЗАПИСИ",&self.state.database.get_unsynced_telemetry(1000000).len().to_string(),AMBER);
            });
            if ui.button("Імітувати передавання черги").clicked() {
                let result = self.state.distributor.flush_buffer_to_priority_queue(&mut self.state.database)
                    .and_then(|_| self.state.distributor.transmit_next_queued_batch(&mut self.state.database).map(|_| ()));
                match result {
                    Ok(()) => { self.state.status_banner = "Симуляцію відправлення виконано (без реального каналу).".into(); self.changed(); },
                    Err(e) => self.state.status_banner = format!("Помилка імітації зв'язку: {e}"),
                }
            }
        });
        for batch in self.state.database.list_sync_batches().into_iter().take(50) {
            ui.label(format!("{} · {} · {} записів · {}",batch.batch_id,batch.sync_status,batch.record_count,batch.created_at_utc));
        }
    }

    fn database_wizard(&mut self, ui: &mut egui::Ui) {
        let using_postgres = self.storage.is_postgres();
        let has_users = !self.state.access_control.list_users().is_empty();
        let wizard = &mut self.db_wizard;
        panel(ui, |ui| {
            eyebrow(ui, "DATABASE / CONNECTION WIZARD");
            section_title(ui, "Storage & Connection");
            small_label(ui, "PostgreSQL є необов'язковим. Пароль не записується до JSON-конфігурації — використовується системне сховище секретів GNOME/Windows.");
            if std::env::var_os("DATABASE_URL").is_some() {
                ui.label(RichText::new("⚠ Змінна DATABASE_URL має пріоритет над майстром. Приберіть її та перезапустіть FPECS, щоб застосувати налаштування інтерфейсу.").color(AMBER));
            }
            ui.add_space(9.0);
            ui.add_enabled_ui(wizard.test_receiver.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.add(egui::Button::new(RichText::new("▣  Local / Offline").color(
                        if wizard.config.mode == DatabaseMode::Local {CYAN} else {WHITE}))
                        .fill(if wizard.config.mode == DatabaseMode::Local {PANEL_2} else {PANEL}))
                        .clicked() {
                        wizard.config.mode = DatabaseMode::Local;
                        wizard.tested = false;
                        wizard.confirm_switch = false;
                    }
                    if ui.add(egui::Button::new(RichText::new("◈  PostgreSQL").color(
                        if wizard.config.mode == DatabaseMode::Postgres {CYAN} else {WHITE}))
                        .fill(if wizard.config.mode == DatabaseMode::Postgres {PANEL_2} else {PANEL}))
                        .clicked() {
                        wizard.config.mode = DatabaseMode::Postgres;
                        wizard.tested = false;
                        wizard.confirm_switch = false;
                    }
                });
                ui.add_space(8.0);
                if wizard.config.mode == DatabaseMode::Local {
                    egui::Frame::none().fill(PANEL_2).rounding(egui::Rounding::same(10.0))
                        .inner_margin(14.0).show(ui, |ui| {
                            status_chip(ui, "● OFFLINE READY", GREEN);
                            small_label(ui, "Дані зберігаються у приватному локальному state.json. Сервер і пароль БД не потрібні.");
                        });
                } else {
                    egui::Frame::none().fill(PANEL_2).rounding(egui::Rounding::same(11.0))
                        .inner_margin(15.0).show(ui, |ui| {
                            eyebrow(ui, "POSTGRESQL CONNECTION PARAMETERS");
                            let mut changed = false;
                            egui::Grid::new("fpecs_db_config_fields").num_columns(2)
                                .spacing([20.0, 11.0]).show(ui, |ui| {
                                ui.label("Host / IP");
                                changed |= ui.add(egui::TextEdit::singleline(&mut wizard.config.host)
                                    .desired_width(245.0).hint_text("localhost")).changed();
                                ui.end_row();
                                ui.label("Port");
                                changed |= ui.add(egui::DragValue::new(&mut wizard.config.port)
                                    .clamp_range(1..=65535)).changed();
                                ui.end_row();
                                ui.label("Database");
                                changed |= ui.add(egui::TextEdit::singleline(&mut wizard.config.database)
                                    .desired_width(245.0).hint_text("lunar_hms")).changed();
                                ui.end_row();
                                ui.label("Username");
                                changed |= ui.add(egui::TextEdit::singleline(&mut wizard.config.username)
                                    .desired_width(245.0).hint_text("fpecs")).changed();
                                ui.end_row();
                                ui.label("Password");
                                changed |= ui.add(egui::TextEdit::singleline(&mut wizard.password)
                                    .desired_width(245.0).password(true)
                                    .hint_text("Введіть пароль або залиште збережений")).changed();
                                ui.end_row();
                                if !wizard.config.is_loopback() {
                                    ui.label("Trusted CA / TLS");
                                    changed |= ui.add(egui::TextEdit::singleline(&mut wizard.config.ca_cert_path)
                                        .desired_width(245.0).hint_text("/path/to/ca.crt")).changed();
                                    ui.end_row();
                                }
                            });
                            if changed {
                                wizard.tested = false;
                                wizard.status.clear();
                            }
                            ui.add_space(6.0);
                            small_label(ui, if wizard.config.is_loopback() {
                                "Localhost: TLS preferred. Remote hosts: TLS VerifyFull and a trusted CA certificate are required."
                            } else {
                                "REMOTE SERVER: hostname and certificate verification required; plaintext fallback disabled."
                            });
                        });
                }
            });
            if wizard.config.mode == DatabaseMode::Postgres {
                ui.add_space(7.0);
                ui.horizontal_wrapped(|ui| {
                    let checking = wizard.test_receiver.is_some();
                    if ui.add_enabled(!checking, egui::Button::new(
                        if checking {"◌ Перевіряємо…"} else {"◎ Перевірити підключення"})).clicked() {
                        wizard.check_connection();
                    }
                    if checking { status_chip(ui, "CONNECTING", AMBER); }
                    else if wizard.tested { status_chip(ui, "● CONNECTED / VERIFIED", GREEN); }
                    else { status_chip(ui, "○ NOT VERIFIED", MUTED); }
                });
                small_label(ui, "Перевірка читає схему й доступ до БД. Таблиці автоматично не створюються.");
            }
            let switching = using_postgres != (wizard.config.mode == DatabaseMode::Postgres);
            if switching && has_users {
                ui.add_space(8.0);
                ui.label(RichText::new("⚠ ПЕРЕМИКАННЯ СХОВИЩА").color(AMBER).strong());
                small_label(ui, "Акаунти й телеметрія не переносяться автоматично. Старе сховище не видаляється. Спочатку зробіть резервну копію.");
                ui.checkbox(&mut wizard.confirm_switch,
                    "Я розумію наслідки та маю резервну копію даних.");
            }
            let can_save = wizard.test_receiver.is_none()
                && (wizard.config.mode == DatabaseMode::Local || wizard.tested)
                && (!switching || !has_users || wizard.confirm_switch);
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(can_save, egui::Button::new(
                    RichText::new("Зберегти конфігурацію  →").strong().color(WHITE))
                    .fill(Color32::from_rgb(27, 101, 112))).clicked() {
                    // Persist to CURRENT storage before changing backends; do not silently migrate.
                    match self.storage.save(&self.state) {
                        Err(e) => wizard.status = format!("Поточні дані не збережено: {e}"),
                        Ok(()) => {
                            let saved = (|| -> Result<(), String> {
                                if wizard.config.mode == DatabaseMode::Postgres && !wizard.password.is_empty() {
                                    save_password(&wizard.config, &wizard.password)?;
                                }
                                wizard.config.save()?;
                                Ok(())
                            })();
                            match saved {
                                Ok(()) => {
                                    wizard.password.clear();
                                    wizard.tested = false;
                                    wizard.status = "Налаштування збережено. Перезапустіть FPECS, щоб увімкнути вибране сховище. Дані автоматично не переносяться.".into();
                                }
                                Err(e) => wizard.status = e,
                            }
                        }
                    }
                }
                status_chip(ui, if using_postgres {"CURRENT: POSTGRESQL"} else {"CURRENT: LOCAL"}, CYAN);
            });
            if !wizard.status.is_empty() {
                ui.add_space(6.0);
                let tint = if wizard.status.starts_with('✓') {GREEN}
                    else if wizard.status.starts_with("Помилка") || wizard.status.starts_with("Поточні") {RED}
                    else {MUTED};
                ui.label(RichText::new(&wizard.status).color(tint));
            }
        });
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "System Settings", "Параметри місії, пороги попереджень і спосіб збереження даних");
        if !self.is_admin() { ui.label("Цей розділ доступний адміністратору."); return; }
        panel(ui, |ui| {
            ui.label("Часовий пояс відображення (дані зберігаються у UTC)");
            ui.add(egui::Slider::new(&mut self.state.timezone_offset_hours, -12..=14).text("UTC offset"));
            ui.separator();
            ui.label("Пороги попереджень аналізатора (демонстрація)");
            ui.add(egui::Slider::new(&mut self.state.analyzer.thresholds.heart_rate.warning_high,100.0..=180.0).text("HR warning high / bpm"));
            ui.add(egui::Slider::new(&mut self.state.analyzer.thresholds.spo2.warning_low,80.0..=98.0).text("SpO2 warning low / %"));
            if ui.button("Зберегти налаштування").clicked() { self.changed(); }
        });
        small_label(ui, "Storage: ");
        small_label(ui, self.storage.description.clone());
        self.database_wizard(ui);
    }

    fn audit(&mut self, ui: &mut egui::Ui) {
        page_heading(ui, "Audit Log", "Події доступу, адміністративні дії та зміни стану системи");
        if !self.is_admin() { ui.label("Журнал безпеки доступний лише Administrator."); return; }
        for entry in self.state.database.list_audit_logs().iter().rev().take(150) {
            panel(ui, |ui| {
                ui.label(format!("{}  ·  {}  ·  {}",entry.occurred_at_utc,entry.actor_username,entry.action_type));
                small_label(ui,format!("{} · {}",entry.status,entry.target_resource));
            });
        }
    }
}

impl eframe::App for MissionControlWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
        self.db_wizard.poll_connection(ctx);
        self.header(ctx);
        self.sidebar(ctx);
        egui::CentralPanel::default().frame(egui::Frame::none().fill(BG)).show(ctx, |ui| {
            // Lightweight mission-control grid, rendered behind ordinary egui controls.
            let rect = ui.max_rect();
            let painter = ui.painter();
            let line_color = Color32::from_rgba_unmultiplied(58, 98, 121, 18);
            let mut x = rect.left();
            while x < rect.right() {
                painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(0.5_f32, line_color));
                x += 48.0;
            }
            let mut y = rect.top();
            while y < rect.bottom() {
                painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    egui::Stroke::new(0.5_f32, line_color));
                y += 48.0;
            }
            egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui, |ui| {
                ui.add_space(18.0);
                ui.add_space(2.0);
                if self.state.access_control.list_users().is_empty() {
                    self.setup(ui);
                    return;
                }
                match self.state.current_page {
                    MissionControlPage::CrewDashboard => self.dashboard(ui),
                    MissionControlPage::AstronautDetail => self.astronaut_detail(ui),
                    MissionControlPage::AlertCenter => self.alert_center(ui),
                    MissionControlPage::SensorDiagnostics | MissionControlPage::HabitatEnvironment
                        | MissionControlPage::RadiationControl => self.technical_overview(ui,self.state.current_page),
                    MissionControlPage::UserAndAstronautManagement => self.manage_users(ui),
                    MissionControlPage::SimulatorControlPanel => self.simulator(ui),
                    MissionControlPage::EarthSynchronization => self.earth_sync(ui),
                    MissionControlPage::SystemSettings => self.settings(ui),
                    MissionControlPage::SystemAuditLog => self.audit(ui),
                }
                ui.add_space(24.0);
            });
        });
        self.save_when_needed();
    }
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();
    let icon = eframe::icon_data::from_png_bytes(
        include_bytes!("../../../packaging/assets/fpecs-icon.png")
    ).unwrap_or_default();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 640.0])
            .with_icon(icon)
            .with_title("FPECS — Lunar Mission Control"),
        ..Default::default()
    };
    eframe::run_native(
        "FPECS — Lunar Health Mission Control",
        native_options,
        Box::new(|cc| Ok(Box::new(MissionControlWindow::new(cc)))),
    )
}
