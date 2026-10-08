use crate::compute::{self, CalculationOutput};
use crate::input::{CoordinateFormat, InputState, ObserverMode, TargetFrame, TimeMode};
use chrono::{DateTime, Utc};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

const APP_BG: Color32 = Color32::from_rgb(7, 15, 24);
const SIDEBAR_BG: Color32 = Color32::from_rgb(10, 22, 34);
const CARD_BG: Color32 = Color32::from_rgb(15, 31, 45);
const CARD_ALT: Color32 = Color32::from_rgb(18, 38, 54);
const INPUT_BG: Color32 = Color32::from_rgb(8, 22, 34);
const PLOT_BG: Color32 = Color32::from_rgb(8, 20, 31);
const BORDER: Color32 = Color32::from_rgb(35, 61, 78);
const GRID: Color32 = Color32::from_rgb(37, 62, 78);
const TEXT: Color32 = Color32::from_rgb(231, 240, 246);
const MUTED: Color32 = Color32::from_rgb(144, 164, 178);
const ACCENT: Color32 = Color32::from_rgb(91, 195, 255);
const SUCCESS: Color32 = Color32::from_rgb(83, 222, 164);
const WARNING: Color32 = Color32::from_rgb(242, 193, 92);
const ERROR: Color32 = Color32::from_rgb(255, 119, 119);
const TWILIGHT: Color32 = Color32::from_rgb(89, 111, 147);
const NIGHT: Color32 = Color32::from_rgb(31, 50, 74);
const DAY: Color32 = Color32::from_rgb(93, 83, 67);

pub struct NsbApp {
    input: InputState,
    site_names: Vec<String>,
    criteria_open: bool,
    last_submitted: Option<InputState>,
    state: CalculationState,
    sender: Sender<Result<CalculationOutput, String>>,
    receiver: Receiver<Result<CalculationOutput, String>>,
}

enum CalculationState {
    Idle,
    Running,
    Ready(Box<CalculationOutput>),
    Error(String),
}

impl NsbApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);

        let (sender, receiver) = mpsc::channel();
        Self {
            input: InputState::default(),
            site_names: InputState::site_names(),
            criteria_open: false,
            last_submitted: None,
            state: CalculationState::Idle,
            sender,
            receiver,
        }
    }

    fn poll_result(&mut self) {
        while let Ok(result) = self.receiver.try_recv() {
            self.state = match result {
                Ok(output) => CalculationState::Ready(Box::new(output)),
                Err(error) => CalculationState::Error(error),
            };
        }
    }

    fn start_calculation(&mut self, ctx: &egui::Context) {
        let request = match self.input.resolve() {
            Ok(request) => request,
            Err(error) => {
                self.state = CalculationState::Error(error);
                return;
            }
        };

        self.last_submitted = Some(self.input.clone());
        self.state = CalculationState::Running;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = compute::calculate(&request);
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }

    fn input_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("input-sidebar")
            .resizable(false)
            .exact_width(350.0)
            .frame(
                egui::Frame::new()
                    .fill(SIDEBAR_BG)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                sidebar_header(ui);
                ui.add_space(14.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        sidebar_card(ui, |ui| self.observer_inputs(ui));
                        ui.add_space(10.0);
                        sidebar_card(ui, |ui| self.time_inputs(ui));
                        ui.add_space(10.0);
                        sidebar_card(ui, |ui| self.target_inputs(ui));
                        ui.add_space(10.0);
                        sidebar_card(ui, |ui| self.component_inputs(ui));
                        ui.add_space(14.0);

                        let running = matches!(self.state, CalculationState::Running);
                        let label = if running {
                            "Calculating…"
                        } else {
                            "Calculate night sky"
                        };
                        let button = egui::Button::new(
                            egui::RichText::new(label)
                                .strong()
                                .size(15.0)
                                .color(Color32::from_rgb(5, 24, 31)),
                        )
                        .fill(ACCENT)
                        .stroke(Stroke::new(0.0_f32, Color32::TRANSPARENT))
                        .corner_radius(8)
                        .min_size(Vec2::new(ui.available_width(), 44.0));

                        if ui.add_enabled(!running, button).clicked() {
                            self.start_calculation(ctx);
                        }

                        ui.add_space(8.0);
                        match &self.state {
                            CalculationState::Running => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(
                                        egui::RichText::new("Model evaluation in progress")
                                            .size(12.0)
                                            .color(MUTED),
                                    );
                                });
                            }
                            CalculationState::Error(error) => {
                                ui.label(egui::RichText::new(error).size(12.0).color(ERROR));
                            }
                            CalculationState::Ready(_) => {
                                ui.label(
                                    egui::RichText::new("Result available")
                                        .size(12.0)
                                        .color(SUCCESS),
                                );
                            }
                            CalculationState::Idle => {
                                ui.label(
                                    egui::RichText::new(
                                        "Runs the NSB scientific library directly.",
                                    )
                                    .size(12.0)
                                    .color(MUTED),
                                );
                            }
                        }
                        ui.add_space(8.0);
                    });
            });
    }

    fn observer_inputs(&mut self, ui: &mut egui::Ui) {
        section_header_with_combo(
            ui,
            "Observer",
            "Where the observation is made",
            "observer-mode",
            self.input.observer_mode,
            |ui| {
                for mode in ObserverMode::ALL {
                    ui.selectable_value(&mut self.input.observer_mode, mode, mode.to_string());
                }
            },
        );
        ui.add_space(12.0);

        match self.input.observer_mode {
            ObserverMode::Coordinates => {
                ui.columns(2, |columns| {
                    field(&mut columns[0], "Longitude", &mut self.input.longitude, "°");
                    field(&mut columns[1], "Latitude", &mut self.input.latitude, "°");
                });
                ui.add_space(8.0);
                field(ui, "Altitude", &mut self.input.altitude_m, "m");
            }
            ObserverMode::Map => {
                self.offline_map(ui);
                ui.add_space(8.0);
                field(ui, "Altitude", &mut self.input.altitude_m, "m");
                help_text(ui, "Click the native map to set longitude and latitude.");
            }
            ObserverMode::Site => {
                field_label(ui, "Observatory");
                egui::ComboBox::from_id_salt("observer-site")
                    .selected_text(&self.input.site_name)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for site in &self.site_names {
                            ui.selectable_value(&mut self.input.site_name, site.clone(), site);
                        }
                    });
                help_text(ui, "Siderust built-in observatory catalogue.");
            }
        }
    }

    fn offline_map(&mut self, ui: &mut egui::Ui) {
        let size = Vec2::new(ui.available_width(), 152.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, PLOT_BG);

        for lon in [-120.0_f32, -60.0, 0.0, 60.0, 120.0] {
            let x = rect.left() + (lon + 180.0) / 360.0 * rect.width();
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0_f32, GRID),
            );
        }
        for lat in [-60.0_f32, -30.0, 0.0, 30.0, 60.0] {
            let y = rect.top() + (90.0 - lat) / 180.0 * rect.height();
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0_f32, GRID),
            );
        }

        if response.clicked() {
            if let Some(position) = response.interact_pointer_pos() {
                let lon = ((position.x - rect.left()) / rect.width() * 360.0 - 180.0)
                    .clamp(-180.0, 180.0);
                let lat =
                    (90.0 - (position.y - rect.top()) / rect.height() * 180.0).clamp(-90.0, 90.0);
                self.input.longitude = format!("{lon:.5}");
                self.input.latitude = format!("{lat:.5}");
            }
        }

        let lon = self
            .input
            .longitude
            .parse::<f32>()
            .unwrap_or(0.0)
            .clamp(-180.0, 180.0);
        let lat = self
            .input
            .latitude
            .parse::<f32>()
            .unwrap_or(0.0)
            .clamp(-90.0, 90.0);
        let marker = Pos2::new(
            rect.left() + (lon + 180.0) / 360.0 * rect.width(),
            rect.top() + (90.0 - lat) / 180.0 * rect.height(),
        );
        painter.circle_filled(marker, 6.0, ACCENT);
        painter.circle_stroke(marker, 9.0, Stroke::new(1.0_f32, Color32::WHITE));
        painter.text(
            rect.left_top() + Vec2::new(10.0, 9.0),
            Align2::LEFT_TOP,
            format!("{lat:.2}°  {lon:.2}°"),
            FontId::proportional(12.0),
            TEXT,
        );
    }

    fn time_inputs(&mut self, ui: &mut egui::Ui) {
        section_header_with_combo(
            ui,
            "Date & time",
            "Reference time and search start",
            "time-mode",
            self.input.time_mode,
            |ui| {
                for mode in TimeMode::ALL {
                    ui.selectable_value(&mut self.input.time_mode, mode, mode.to_string());
                }
            },
        );
        ui.add_space(12.0);

        match self.input.time_mode {
            TimeMode::Local => {
                ui.columns(2, |columns| {
                    field(&mut columns[0], "Date", &mut self.input.local_date, "");
                    field(&mut columns[1], "Time", &mut self.input.local_time, "");
                });
                ui.add_space(8.0);
                field(ui, "UTC offset", &mut self.input.utc_offset, "");
                help_text(ui, "Local civil time with an explicit fixed UTC offset.");
            }
            TimeMode::JulianDate => {
                field(ui, "Julian Date", &mut self.input.jd_tt, "TT");
                help_text(ui, "Julian Date on the TT timescale.");
            }
            TimeMode::ModifiedJulianDate => {
                field(ui, "Modified Julian Date", &mut self.input.mjd_tt, "TT");
                help_text(ui, "Modified Julian Date on the TT timescale.");
            }
        }
    }

    fn target_inputs(&mut self, ui: &mut egui::Ui) {
        section_header_with_combo(
            ui,
            "Target",
            "Coordinate system and representation",
            "target-frame",
            self.input.target_frame,
            |ui| {
                for frame in TargetFrame::ALL {
                    ui.selectable_value(&mut self.input.target_frame, frame, frame.to_string());
                }
            },
        );

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Representation")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("coordinate-format")
                    .selected_text(self.input.coordinate_format.to_string())
                    .show_ui(ui, |ui| {
                        for format in CoordinateFormat::ALL {
                            ui.selectable_value(
                                &mut self.input.coordinate_format,
                                format,
                                format.to_string(),
                            );
                        }
                    });
            });
        });
        ui.add_space(10.0);

        match (self.input.target_frame, self.input.coordinate_format) {
            (TargetFrame::Icrs, CoordinateFormat::Sexagesimal) => {
                field(ui, "Right ascension", &mut self.input.ra_hms, "h m s");
                ui.add_space(8.0);
                field(ui, "Declination", &mut self.input.dec_dms, "° ′ ″");
            }
            (TargetFrame::Icrs, CoordinateFormat::DecimalDegrees) => {
                field(ui, "Right ascension", &mut self.input.ra_deg, "°");
                ui.add_space(8.0);
                field(ui, "Declination", &mut self.input.dec_deg, "°");
            }
            (TargetFrame::Horizontal, CoordinateFormat::Sexagesimal) => {
                field(ui, "Azimuth", &mut self.input.az_dms, "° ′ ″");
                ui.add_space(8.0);
                field(ui, "Altitude", &mut self.input.alt_dms, "° ′ ″");
            }
            (TargetFrame::Horizontal, CoordinateFormat::DecimalDegrees) => {
                field(ui, "Azimuth", &mut self.input.az_deg, "°");
                ui.add_space(8.0);
                field(ui, "Altitude", &mut self.input.target_alt_deg, "°");
            }
        }

        if self.input.target_frame == TargetFrame::Horizontal {
            help_text(
                ui,
                "Horizontal pointing is resolved at the search start and converted to a fixed J2000 direction.",
            );
        }
    }

    fn component_inputs(&mut self, ui: &mut egui::Ui) {
        section_header(
            ui,
            "Model components",
            "Contributors included in the NSB model",
        );
        ui.add_space(10.0);

        ui.columns(2, |columns| {
            columns[0].checkbox(&mut self.input.moonlight, "Moonlight");
            columns[1].checkbox(&mut self.input.zodiacal, "Zodiacal light");
        });
        ui.columns(2, |columns| {
            columns[0].checkbox(&mut self.input.airglow, "Airglow");
            let available = InputState::starlight_available();
            columns[1].add_enabled(
                available,
                egui::Checkbox::new(&mut self.input.starlight, "Starlight"),
            );
            if !available {
                self.input.starlight = false;
            }
        });

        if !InputState::starlight_available() {
            help_text(
                ui,
                "Starlight is unavailable because this build has no admitted production map.",
            );
        }
    }

    fn results(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(APP_BG)
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                let results_stale = self
                    .last_submitted
                    .as_ref()
                    .is_some_and(|submitted| submitted != &self.input);

                page_header(ui, &self.state);
                ui.add_space(14.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let state = &self.state;
                        let input = &mut self.input;
                        let criteria_open = &mut self.criteria_open;

                        match state {
                            CalculationState::Ready(output) => {
                                if results_stale {
                                    stale_banner(ui);
                                    ui.add_space(10.0);
                                }

                                render_metric_strip(ui, output);
                                ui.add_space(10.0);
                                render_chart(ui, output);
                                ui.add_space(10.0);

                                ui.columns(2, |columns| {
                                    render_criteria(
                                        &mut columns[0],
                                        input,
                                        criteria_open,
                                        Some(output),
                                        results_stale,
                                    );
                                    render_windows(&mut columns[1], output);
                                });

                                ui.add_space(10.0);
                                render_components(ui, output);
                                ui.add_space(10.0);
                                render_timeline(ui, output);
                                ui.add_space(12.0);
                            }
                            CalculationState::Running => {
                                render_criteria(ui, input, criteria_open, None, false);
                                ui.add_space(10.0);
                                card(ui, |ui| {
                                    ui.set_min_height(210.0);
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(52.0);
                                        ui.spinner();
                                        ui.add_space(10.0);
                                        ui.label(
                                            egui::RichText::new("Calculating night-sky brightness")
                                                .size(18.0)
                                                .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(
                                                "NSB is running on a background worker; the window remains responsive.",
                                            )
                                            .size(13.0)
                                            .color(MUTED),
                                        );
                                    });
                                });
                            }
                            CalculationState::Error(error) => {
                                render_criteria(ui, input, criteria_open, None, false);
                                ui.add_space(10.0);
                                error_card(ui, error);
                            }
                            CalculationState::Idle => {
                                render_criteria(ui, input, criteria_open, None, false);
                                ui.add_space(10.0);
                                empty_state(ui);
                            }
                        }
                    });
            });
    }
}

impl eframe::App for NsbApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_result();
        self.input_sidebar(ctx);
        self.results(ctx);
    }
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 8.0);
    style.spacing.interact_size = Vec2::new(40.0, 34.0);
    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(20.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(12.0));

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = APP_BG;
    visuals.window_fill = CARD_BG;
    visuals.extreme_bg_color = INPUT_BG;
    visuals.override_text_color = Some(TEXT);
    style.visuals = visuals;
    ctx.set_style(style);
}

fn sidebar_header(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("NSB").size(25.0).strong().color(TEXT));
    ui.label(
        egui::RichText::new("Observation planner")
            .size(14.0)
            .color(ACCENT),
    );
    ui.add_space(3.0);
    ui.label(
        egui::RichText::new("Native scientific desktop interface")
            .size(12.0)
            .color(MUTED),
    );
}

fn page_header(ui: &mut egui::Ui, state: &CalculationState) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Night sky planner")
                    .size(25.0)
                    .strong()
                    .color(TEXT),
            );
            ui.label(
                egui::RichText::new(
                    "Night-sky background, model contributions, and observing windows",
                )
                .size(13.0)
                .color(MUTED),
            );
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (label, color) = match state {
                CalculationState::Idle => ("Ready", MUTED),
                CalculationState::Running => ("Calculating", ACCENT),
                CalculationState::Ready(_) => ("Calculated", SUCCESS),
                CalculationState::Error(_) => ("Needs attention", ERROR),
            };
            status_pill(ui, label, color);
        });
    });
}

fn status_pill(ui: &mut egui::Ui, label: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.13))
        .corner_radius(99)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(12.0).strong().color(color));
        });
}

fn sidebar_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(14))
        .show(ui, add_contents);
}

fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(16))
        .show(ui, add_contents);
}

fn section_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(egui::RichText::new(title).size(17.0).strong().color(TEXT));
    ui.label(egui::RichText::new(subtitle).size(11.5).color(MUTED));
}

fn section_header_with_combo<T: Copy + ToString>(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    id: &'static str,
    selected: T,
    add_options: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(title).size(17.0).strong().color(TEXT));
            ui.label(egui::RichText::new(subtitle).size(11.5).color(MUTED));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt(id)
                .selected_text(selected.to_string())
                .width(118.0)
                .show_ui(ui, add_options);
        });
    });
}

fn field_label(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).size(12.0).color(MUTED));
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String, unit: &str) {
    field_label(ui, label);
    ui.horizontal(|ui| {
        let unit_width = if unit.is_empty() { 0.0 } else { 48.0 };
        let width = (ui.available_width() - unit_width - 6.0).max(72.0);
        ui.add_sized(
            [width, 34.0],
            egui::TextEdit::singleline(value)
                .desired_width(width)
                .margin(egui::Margin::symmetric(9, 6)),
        );
        if !unit.is_empty() {
            ui.label(egui::RichText::new(unit).size(12.0).color(MUTED));
        }
    });
}

fn help_text(ui: &mut egui::Ui, text: &str) {
    ui.add_space(5.0);
    ui.label(egui::RichText::new(text).size(11.5).color(MUTED));
}

fn stale_banner(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(WARNING.gamma_multiply(0.11))
        .stroke(Stroke::new(1.0_f32, WARNING.gamma_multiply(0.55)))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "Inputs changed after this result was calculated. The dashboard still shows the submitted result; calculate again to refresh it.",
                )
                .size(12.5)
                .color(WARNING),
            );
        });
}

fn error_card(ui: &mut egui::Ui, error: &str) {
    egui::Frame::new()
        .fill(ERROR.gamma_multiply(0.08))
        .stroke(Stroke::new(1.0_f32, ERROR.gamma_multiply(0.55)))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Calculation could not start")
                    .size(18.0)
                    .strong()
                    .color(ERROR),
            );
            ui.add_space(4.0);
            ui.label(egui::RichText::new(error).size(13.0).color(TEXT));
        });
}

fn empty_state(ui: &mut egui::Ui) {
    card(ui, |ui| {
        ui.set_min_height(235.0);
        ui.vertical_centered(|ui| {
            ui.add_space(54.0);
            ui.label(
                egui::RichText::new("Ready for an observation plan")
                    .size(21.0)
                    .strong()
                    .color(TEXT),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Choose the observer, time, target, and model components on the left, then calculate.",
                )
                .size(13.0)
                .color(MUTED),
            );
            ui.add_space(7.0);
            ui.label(
                egui::RichText::new(
                    "Results are generated from NSB library calculations; no mock data is used.",
                )
                .size(12.0)
                .color(ACCENT),
            );
        });
    });
}

fn render_metric_strip(ui: &mut egui::Ui, output: &CalculationOutput) {
    ui.columns(3, |columns| {
        metric_card(
            &mut columns[0],
            "Darkest sampled NSB",
            &format!("{:.5}", output.reference_radiance),
            "ph cm⁻² ns⁻¹ sr⁻¹",
            &format!("{} UTC", output.reference_time.format("%Y-%m-%d %H:%M")),
            ACCENT,
        );

        let total_duration: f64 = output
            .windows
            .iter()
            .map(compute::ObservingWindow::duration_seconds)
            .sum();
        metric_card(
            &mut columns[1],
            "Matching windows",
            &output.windows.len().to_string(),
            if output.windows.len() == 1 {
                "continuous interval"
            } else {
                "continuous intervals"
            },
            &format!("{} total", format_duration(total_duration)),
            SUCCESS,
        );

        let status = if output.reference_satisfies_criteria {
            "Satisfied"
        } else {
            "Not satisfied"
        };
        metric_card(
            &mut columns[2],
            "Criteria at darkest sample",
            status,
            "",
            &format!(
                "B {:.3} · V {:.3} mag/arcsec²",
                output.reference_b_mag_arcsec2, output.reference_v_mag_arcsec2
            ),
            if output.reference_satisfies_criteria {
                SUCCESS
            } else {
                WARNING
            },
        );
    });
}

fn metric_card(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    unit: &str,
    footer: &str,
    accent: Color32,
) {
    card(ui, |ui| {
        ui.set_min_height(104.0);
        ui.label(egui::RichText::new(label).size(11.5).color(MUTED));
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(value).size(22.0).strong().color(accent));
            if !unit.is_empty() {
                ui.label(egui::RichText::new(unit).size(11.5).color(MUTED));
            }
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new(footer).size(11.5).color(MUTED));
    });
}

fn render_criteria(
    ui: &mut egui::Ui,
    input: &mut InputState,
    open: &mut bool,
    applied: Option<&CalculationOutput>,
    inputs_changed: bool,
) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Observing criteria")
                        .size(17.0)
                        .strong()
                        .color(TEXT),
                );
                ui.label(
                    egui::RichText::new("Every enabled rule must be satisfied")
                        .size(11.5)
                        .color(MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(if *open { "Done" } else { "Edit" })
                            .fill(CARD_ALT)
                            .corner_radius(7),
                    )
                    .clicked()
                {
                    *open = !*open;
                }
            });
        });
        ui.add_space(11.0);

        let threshold = applied
            .map(|output| format!("{:.6}", output.threshold))
            .unwrap_or_else(|| input.max_radiance.clone());
        criteria_row(
            ui,
            "Integrated NSB",
            &format!("≤ {threshold} ph cm⁻² ns⁻¹ sr⁻¹"),
        );

        let sun_ceiling = applied
            .map(|output| output.sun_altitude_ceiling_deg)
            .unwrap_or_else(|| {
                input
                    .use_sun_ceiling
                    .then(|| input.sun_altitude_ceiling_deg.parse::<f64>().ok())
                    .flatten()
            });
        criteria_row(
            ui,
            "Sun altitude",
            &sun_ceiling
                .map(|value| format!("≤ {value:.1}°"))
                .unwrap_or_else(|| "disabled".into()),
        );

        let target_floor = applied
            .map(|output| output.target_altitude_floor_deg)
            .unwrap_or_else(|| {
                input
                    .use_target_floor
                    .then(|| input.target_altitude_floor_deg.parse::<f64>().ok())
                    .flatten()
            });
        criteria_row(
            ui,
            "Target altitude",
            &target_floor
                .map(|value| format!("≥ {value:.1}°"))
                .unwrap_or_else(|| "disabled".into()),
        );

        if let Some(output) = applied {
            criteria_row(
                ui,
                "Search",
                &format!(
                    "{} · {} s samples",
                    format_duration((output.end - output.start).num_seconds() as f64),
                    output.sample_step_seconds
                ),
            );
        }

        if inputs_changed {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("These are the criteria applied to the plotted result.")
                    .size(11.5)
                    .color(WARNING),
            );
        }

        if *open {
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            field(
                ui,
                "Maximum integrated NSB",
                &mut input.max_radiance,
                "ph cm⁻² ns⁻¹ sr⁻¹",
            );
            ui.add_space(8.0);
            criteria_toggle_field(
                ui,
                &mut input.use_sun_ceiling,
                "Sun altitude ceiling",
                &mut input.sun_altitude_ceiling_deg,
                "°",
            );
            ui.add_space(8.0);
            criteria_toggle_field(
                ui,
                &mut input.use_target_floor,
                "Target altitude floor",
                &mut input.target_altitude_floor_deg,
                "°",
            );
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                field(
                    &mut columns[0],
                    "Search duration",
                    &mut input.duration_hours,
                    "h",
                );
                field(
                    &mut columns[1],
                    "Sample step",
                    &mut input.sample_step_seconds,
                    "s",
                );
            });
            if applied.is_some() {
                help_text(ui, "Edits apply on the next calculation.");
            }
        }
    });
}

fn criteria_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(12.5).color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value).size(12.5).color(TEXT));
        });
    });
}

fn criteria_toggle_field(
    ui: &mut egui::Ui,
    enabled: &mut bool,
    label: &str,
    value: &mut String,
    unit: &str,
) {
    ui.horizontal(|ui| {
        ui.checkbox(enabled, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(unit).size(12.0).color(MUTED));
            ui.add_enabled(
                *enabled,
                egui::TextEdit::singleline(value).desired_width(70.0),
            );
        });
    });
}

fn render_windows(ui: &mut egui::Ui, output: &CalculationOutput) {
    card(ui, |ui| {
        ui.label(
            egui::RichText::new("Observing windows")
                .size(17.0)
                .strong()
                .color(TEXT),
        );
        ui.label(
            egui::RichText::new("Continuous intervals satisfying all active criteria")
                .size(11.5)
                .color(MUTED),
        );
        ui.add_space(10.0);

        if output.windows.is_empty() {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("No matching interval in this search span.")
                    .size(14.0)
                    .color(MUTED),
            );
            ui.add_space(10.0);
            return;
        }

        let multi_day = output.start.date_naive() != output.end.date_naive();
        for (index, window) in output.windows.iter().take(5).enumerate() {
            egui::Frame::new()
                .fill(CARD_ALT)
                .corner_radius(7)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:02}", index + 1))
                                .size(11.0)
                                .strong()
                                .color(SUCCESS),
                        );
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(format_window(window, multi_day))
                                    .size(12.5)
                                    .color(TEXT),
                            );
                            ui.label(
                                egui::RichText::new(format_duration(window.duration_seconds()))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        });
                    });
                });
            ui.add_space(6.0);
        }

        if output.windows.len() > 5 {
            ui.label(
                egui::RichText::new(format!("{} additional windows", output.windows.len() - 5))
                    .size(11.5)
                    .color(MUTED),
            );
        }
    });
}

fn render_components(ui: &mut egui::Ui, output: &CalculationOutput) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Component contribution")
                        .size(17.0)
                        .strong()
                        .color(TEXT),
                );
                ui.label(
                    egui::RichText::new(
                        "Shares are derived from additive integrated photon radiance, never from magnitudes.",
                    )
                    .size(11.5)
                    .color(MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} UTC",
                        output.reference_time.format("%H:%M")
                    ))
                    .size(11.5)
                    .color(MUTED),
                );
            });
        });
        ui.add_space(12.0);

        for component in &output.components {
            component_row(ui, component);
            ui.add_space(7.0);
        }
    });
}

fn component_row(ui: &mut egui::Ui, component: &compute::ComponentContribution) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [150.0, 24.0],
            egui::Label::new(egui::RichText::new(&component.name).size(12.5).color(TEXT)),
        );

        let value_width = 205.0;
        let bar_width = (ui.available_width() - value_width - 8.0).max(90.0);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(bar_width, 9.0), Sense::hover());
        let painter = ui.painter_at(bar);
        painter.rect_filled(bar, 99.0, INPUT_BG);
        let filled = Rect::from_min_max(
            bar.min,
            Pos2::new(
                bar.left() + bar.width() * component.share as f32,
                bar.bottom(),
            ),
        );
        painter.rect_filled(filled, 99.0, component_color(&component.name));

        ui.add_sized(
            [value_width, 24.0],
            egui::Label::new(
                egui::RichText::new(format!(
                    "{:>5.1}%   {:.5} ph cm⁻² ns⁻¹ sr⁻¹",
                    component.share * 100.0,
                    component.integrated_radiance
                ))
                .size(11.5)
                .color(MUTED),
            ),
        );
    });
}

fn component_color(name: &str) -> Color32 {
    match name {
        "Moonlight" => Color32::from_rgb(238, 198, 104),
        "Zodiacal light" => Color32::from_rgb(91, 195, 255),
        "Airglow" => Color32::from_rgb(83, 222, 164),
        "Integrated starlight" => Color32::from_rgb(181, 139, 255),
        _ => ACCENT,
    }
}

fn render_chart(ui: &mut egui::Ui, output: &CalculationOutput) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Night sky brightness")
                        .size(18.0)
                        .strong()
                        .color(TEXT),
                );
                ui.label(
                    egui::RichText::new(
                        "Integrated photon radiance, 300–650 nm · ph cm⁻² ns⁻¹ sr⁻¹ · lower is darker",
                    )
                    .size(11.5)
                    .color(MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                chart_legend(ui, SUCCESS, "all criteria");
                chart_legend(ui, ACCENT, "NSB model");
            });
        });
        ui.add_space(10.0);

        let desired = Vec2::new(ui.available_width(), 310.0);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, PLOT_BG);

        let plot = Rect::from_min_max(
            rect.min + Vec2::new(58.0, 18.0),
            rect.max - Vec2::new(14.0, 36.0),
        );
        if output.samples.len() < 2 {
            return;
        }

        let mut y_min = output
            .samples
            .iter()
            .map(|sample| sample.integrated_radiance)
            .fold(f64::INFINITY, f64::min)
            .min(output.threshold);
        let mut y_max = output
            .samples
            .iter()
            .map(|sample| sample.integrated_radiance)
            .fold(f64::NEG_INFINITY, f64::max)
            .max(output.threshold);

        if (y_max - y_min).abs() < f64::EPSILON {
            y_min = (y_min - 0.5).max(0.0);
            y_max += 0.5;
        } else {
            let padding = (y_max - y_min) * 0.10;
            y_min = (y_min - padding).max(0.0);
            y_max += padding;
        }

        for period in &output.astronomical_night {
            let x0 = time_x(period.start, output.start, output.end, plot);
            let x1 = time_x(period.end, output.start, output.end, plot);
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(x0, plot.top()), Pos2::new(x1, plot.bottom())),
                0.0,
                Color32::from_rgba_unmultiplied(48, 75, 111, 28),
            );
        }

        for window in &output.windows {
            let x0 = time_x(window.start, output.start, output.end, plot);
            let x1 = time_x(window.end, output.start, output.end, plot);
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(x0, plot.top()), Pos2::new(x1, plot.bottom())),
                0.0,
                Color32::from_rgba_unmultiplied(83, 222, 164, 34),
            );
        }

        for i in 0..=4 {
            let fraction = i as f32 / 4.0;
            let y = egui::lerp(plot.bottom()..=plot.top(), fraction);
            painter.line_segment(
                [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
                Stroke::new(1.0_f32, GRID),
            );
            let value = y_min + (y_max - y_min) * fraction as f64;
            painter.text(
                Pos2::new(plot.left() - 9.0, y),
                Align2::RIGHT_CENTER,
                format!("{value:.3}"),
                FontId::proportional(11.5),
                MUTED,
            );
        }

        let multi_day = output.start.date_naive() != output.end.date_naive();
        for i in 0..=5 {
            let fraction = i as f32 / 5.0;
            let x = egui::lerp(plot.left()..=plot.right(), fraction);
            painter.line_segment(
                [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
                Stroke::new(1.0_f32, GRID),
            );
            let when = interpolate_time(output.start, output.end, fraction as f64);
            painter.text(
                Pos2::new(x, plot.bottom() + 9.0),
                Align2::CENTER_TOP,
                format_axis_time(when, multi_day),
                FontId::proportional(11.0),
                MUTED,
            );
        }

        let threshold_y = value_y(output.threshold, y_min, y_max, plot);
        dashed_horizontal(&painter, plot, threshold_y, SUCCESS);
        painter.text(
            Pos2::new(plot.right() - 5.0, threshold_y - 5.0),
            Align2::RIGHT_BOTTOM,
            format!("threshold {:.3}", output.threshold),
            FontId::proportional(11.0),
            SUCCESS,
        );

        let points: Vec<Pos2> = output
            .samples
            .iter()
            .map(|sample| {
                Pos2::new(
                    time_x(sample.time, output.start, output.end, plot),
                    value_y(sample.integrated_radiance, y_min, y_max, plot),
                )
            })
            .collect();
        painter.line(points, Stroke::new(2.2_f32, ACCENT));

        if let Some(pointer) = response
            .hover_pos()
            .filter(|position| plot.contains(*position))
        {
            let fraction = ((pointer.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
            let pointer_time = interpolate_time(output.start, output.end, fraction as f64);
            if let Some(sample) = nearest_sample(&output.samples, pointer_time) {
                let point = Pos2::new(
                    time_x(sample.time, output.start, output.end, plot),
                    value_y(sample.integrated_radiance, y_min, y_max, plot),
                );
                painter.line_segment(
                    [
                        Pos2::new(point.x, plot.top()),
                        Pos2::new(point.x, plot.bottom()),
                    ],
                    Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.45)),
                );
                painter.circle_filled(point, 4.5, TEXT);

                let tooltip_size = Vec2::new(228.0, 68.0);
                let tooltip_x = if point.x + tooltip_size.x + 12.0 <= plot.right() {
                    point.x + 10.0
                } else {
                    point.x - tooltip_size.x - 10.0
                };
                let tooltip_y = (point.y - tooltip_size.y - 10.0)
                    .clamp(plot.top() + 4.0, plot.bottom() - tooltip_size.y - 4.0);
                let tooltip = Rect::from_min_size(Pos2::new(tooltip_x, tooltip_y), tooltip_size);
                painter.rect_filled(tooltip, 7.0, Color32::from_rgb(20, 39, 54));
                painter.text(
                    tooltip.left_top() + Vec2::new(10.0, 9.0),
                    Align2::LEFT_TOP,
                    format!(
                        "{} UTC\n{:.5} ph cm⁻² ns⁻¹ sr⁻¹\nB {:.2} · V {:.2} mag/arcsec²",
                        sample.time.format("%Y-%m-%d %H:%M"),
                        sample.integrated_radiance,
                        sample.b_mag_arcsec2,
                        sample.v_mag_arcsec2
                    ),
                    FontId::proportional(11.5),
                    TEXT,
                );
            }
        }
    });
}

fn chart_legend(ui: &mut egui::Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (swatch, _) = ui.allocate_exact_size(Vec2::new(14.0, 4.0), Sense::hover());
        ui.painter().rect_filled(swatch, 99.0, color);
        ui.label(egui::RichText::new(label).size(11.0).color(MUTED));
    });
}

fn render_timeline(ui: &mut egui::Ui, output: &CalculationOutput) {
    card(ui, |ui| {
        ui.label(
            egui::RichText::new("Observing timeline")
                .size(17.0)
                .strong()
                .color(TEXT),
        );
        ui.label(
            egui::RichText::new(
                "The criteria row uses exactly the same intervals highlighted in the main chart.",
            )
            .size(11.5)
            .color(MUTED),
        );
        ui.add_space(9.0);
        ui.horizontal_wrapped(|ui| {
            timeline_legend(ui, DAY, "Day");
            timeline_legend(ui, TWILIGHT, "Twilight");
            timeline_legend(ui, NIGHT, "Astronomical night");
            timeline_legend(ui, SUCCESS, "All criteria");
        });
        ui.add_space(6.0);

        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 92.0), Sense::hover());
        let painter = ui.painter_at(rect);
        let label_width = 118.0;
        let solar_bar = Rect::from_min_max(
            Pos2::new(rect.left() + label_width, rect.top() + 6.0),
            Pos2::new(rect.right(), rect.top() + 24.0),
        );
        let criteria_bar = solar_bar.translate(Vec2::new(0.0, 32.0));

        painter.text(
            Pos2::new(rect.left(), solar_bar.center().y),
            Align2::LEFT_CENTER,
            "Solar state",
            FontId::proportional(11.5),
            MUTED,
        );
        painter.text(
            Pos2::new(rect.left(), criteria_bar.center().y),
            Align2::LEFT_CENTER,
            "All criteria",
            FontId::proportional(11.5),
            MUTED,
        );

        painter.rect_filled(solar_bar, 5.0, DAY);
        paint_periods(
            &painter,
            solar_bar,
            &output.sun_below_horizon,
            output,
            TWILIGHT,
        );
        paint_periods(
            &painter,
            solar_bar,
            &output.astronomical_night,
            output,
            NIGHT,
        );

        painter.rect_filled(criteria_bar, 5.0, INPUT_BG);
        paint_periods(&painter, criteria_bar, &output.windows, output, SUCCESS);

        let multi_day = output.start.date_naive() != output.end.date_naive();
        for i in 0..=5 {
            let fraction = i as f32 / 5.0;
            let x = egui::lerp(criteria_bar.left()..=criteria_bar.right(), fraction);
            let when = interpolate_time(output.start, output.end, fraction as f64);
            painter.text(
                Pos2::new(x, criteria_bar.bottom() + 8.0),
                Align2::CENTER_TOP,
                format_axis_time(when, multi_day),
                FontId::proportional(10.5),
                MUTED,
            );
        }
    });
}

fn timeline_legend(ui: &mut egui::Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (swatch, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
        ui.painter().rect_filled(swatch, 2.0, color);
        ui.label(egui::RichText::new(label).size(11.0).color(MUTED));
    });
}

fn paint_periods(
    painter: &egui::Painter,
    bar: Rect,
    periods: &[compute::ObservingWindow],
    output: &CalculationOutput,
    color: Color32,
) {
    for period in periods {
        let x0 = time_x(period.start, output.start, output.end, bar);
        let x1 = time_x(period.end, output.start, output.end, bar);
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(x0, bar.top()), Pos2::new(x1, bar.bottom())),
            4.0,
            color,
        );
    }
}

fn nearest_sample(samples: &[compute::Sample], target: DateTime<Utc>) -> Option<&compute::Sample> {
    samples
        .iter()
        .min_by_key(|sample| (sample.time - target).num_milliseconds().abs())
}

fn time_x(time: DateTime<Utc>, start: DateTime<Utc>, end: DateTime<Utc>, rect: Rect) -> f32 {
    let total = (end - start).num_milliseconds().max(1) as f64;
    let elapsed = (time - start).num_milliseconds() as f64;
    let fraction = (elapsed / total).clamp(0.0, 1.0) as f32;
    egui::lerp(rect.left()..=rect.right(), fraction)
}

fn value_y(value: f64, min: f64, max: f64, rect: Rect) -> f32 {
    let fraction = ((value - min) / (max - min)).clamp(0.0, 1.0) as f32;
    egui::lerp(rect.bottom()..=rect.top(), fraction)
}

fn interpolate_time(start: DateTime<Utc>, end: DateTime<Utc>, fraction: f64) -> DateTime<Utc> {
    let millis = (end - start).num_milliseconds();
    start + chrono::Duration::milliseconds((millis as f64 * fraction).round() as i64)
}

fn format_axis_time(time: DateTime<Utc>, multi_day: bool) -> String {
    if multi_day {
        time.format("%d %H:%M").to_string()
    } else {
        time.format("%H:%M").to_string()
    }
}

fn format_window(window: &compute::ObservingWindow, multi_day: bool) -> String {
    if multi_day || window.start.date_naive() != window.end.date_naive() {
        format!(
            "{} → {} UTC",
            window.start.format("%b %d · %H:%M"),
            window.end.format("%b %d · %H:%M")
        )
    } else {
        format!(
            "{} — {} UTC",
            window.start.format("%H:%M"),
            window.end.format("%H:%M")
        )
    }
}

fn dashed_horizontal(painter: &egui::Painter, rect: Rect, y: f32, color: Color32) {
    let dash = 7.0;
    let gap = 5.0;
    let mut x = rect.left();
    while x < rect.right() {
        painter.line_segment(
            [Pos2::new(x, y), Pos2::new((x + dash).min(rect.right()), y)],
            Stroke::new(1.4_f32, color),
        );
        x += dash + gap;
    }
}

fn format_duration(seconds: f64) -> String {
    let total_minutes = (seconds / 60.0).round() as i64;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours > 0 {
        format!("{hours} h {minutes:02} min")
    } else {
        format!("{minutes} min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn duration_format_is_compact() {
        assert_eq!(format_duration(4.0 * 3600.0 + 10.0 * 60.0), "4 h 10 min");
        assert_eq!(format_duration(45.0 * 60.0), "45 min");
    }

    #[test]
    fn hover_selection_uses_sample_timestamps() {
        let start = DateTime::parse_from_rfc3339("2026-10-07T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let samples = vec![
            compute::Sample {
                time: start,
                integrated_radiance: 1.0,
                b_mag_arcsec2: 1.0,
                v_mag_arcsec2: 1.0,
            },
            compute::Sample {
                time: start + Duration::minutes(60),
                integrated_radiance: 2.0,
                b_mag_arcsec2: 2.0,
                v_mag_arcsec2: 2.0,
            },
            compute::Sample {
                time: start + Duration::minutes(61),
                integrated_radiance: 3.0,
                b_mag_arcsec2: 3.0,
                v_mag_arcsec2: 3.0,
            },
        ];

        let selected = nearest_sample(&samples, start + Duration::minutes(31)).unwrap();
        assert_eq!(selected.time, start + Duration::minutes(60));
    }

    #[test]
    fn multi_day_windows_include_calendar_dates() {
        let start = DateTime::parse_from_rfc3339("2026-10-07T23:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let window = compute::ObservingWindow {
            start,
            end: start + Duration::hours(2),
        };
        let label = format_window(&window, true);
        assert!(label.contains("Oct 07"));
        assert!(label.contains("Oct 08"));
    }
}
