use crate::compute::{self, CalculationOutput};
use crate::input::{CoordinateFormat, InputState, ObserverMode, TargetFrame, TimeMode};
use chrono::{DateTime, Utc};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

const PANEL_FILL: Color32 = Color32::from_rgb(18, 31, 45);
const PLOT_FILL: Color32 = Color32::from_rgb(10, 24, 38);
const GRID: Color32 = Color32::from_rgb(47, 67, 84);
const BLUE: Color32 = Color32::from_rgb(89, 169, 255);
const GREEN: Color32 = Color32::from_rgb(92, 225, 160);
const TWILIGHT: Color32 = Color32::from_rgb(77, 105, 142);
const NIGHT: Color32 = Color32::from_rgb(35, 55, 82);
const MUTED: Color32 = Color32::from_rgb(162, 178, 192);
const ERROR: Color32 = Color32::from_rgb(255, 120, 120);

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
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(9, 18, 28);
        visuals.window_fill = PANEL_FILL;
        visuals.extreme_bg_color = Color32::from_rgb(8, 20, 31);
        cc.egui_ctx.set_visuals(visuals);

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
            .exact_width(315.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(4.0);
                    section(ui, |ui| self.observer_inputs(ui));
                    ui.add_space(8.0);
                    section(ui, |ui| self.time_inputs(ui));
                    ui.add_space(8.0);
                    section(ui, |ui| self.target_inputs(ui));
                    ui.add_space(8.0);
                    section(ui, |ui| self.component_inputs(ui));
                    ui.add_space(12.0);

                    let running = matches!(self.state, CalculationState::Running);
                    let button = egui::Button::new(if running {
                        "Calculating…"
                    } else {
                        "Calculate"
                    })
                    .min_size(Vec2::new(ui.available_width(), 42.0));
                    if ui.add_enabled(!running, button).clicked() {
                        self.start_calculation(ctx);
                    }
                    ui.add_space(8.0);
                });
            });
    }

    fn observer_inputs(&mut self, ui: &mut egui::Ui) {
        header_with_combo(
            ui,
            "Observer",
            "observer-mode",
            self.input.observer_mode,
            |ui| {
                for mode in ObserverMode::ALL {
                    ui.selectable_value(&mut self.input.observer_mode, mode, mode.to_string());
                }
            },
        );
        ui.add_space(8.0);

        match self.input.observer_mode {
            ObserverMode::Coordinates => {
                input_row(ui, "Longitude", &mut self.input.longitude, "°");
                input_row(ui, "Latitude", &mut self.input.latitude, "°");
                input_row(ui, "Altitude", &mut self.input.altitude_m, "m");
            }
            ObserverMode::Map => {
                self.offline_map(ui);
                ui.add_space(5.0);
                ui.label(
                    egui::RichText::new("Click the map to set longitude/latitude.")
                        .small()
                        .color(MUTED),
                );
                input_row(ui, "Altitude", &mut self.input.altitude_m, "m");
            }
            ObserverMode::Site => {
                egui::ComboBox::from_id_salt("observer-site")
                    .selected_text(&self.input.site_name)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for site in &self.site_names {
                            ui.selectable_value(&mut self.input.site_name, site.clone(), site);
                        }
                    });
                ui.label(
                    egui::RichText::new("Siderust built-in observatory catalogue")
                        .small()
                        .color(MUTED),
                );
            }
        }
    }

    fn offline_map(&mut self, ui: &mut egui::Ui) {
        let size = Vec2::new(ui.available_width(), 145.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, PLOT_FILL);

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
        painter.circle_filled(marker, 5.0, BLUE);
        painter.text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            Align2::LEFT_TOP,
            format!("{lat:.2}°, {lon:.2}°"),
            FontId::proportional(12.0),
            Color32::WHITE,
        );
    }

    fn time_inputs(&mut self, ui: &mut egui::Ui) {
        header_with_combo(ui, "Date & Time", "time-mode", self.input.time_mode, |ui| {
            for mode in TimeMode::ALL {
                ui.selectable_value(&mut self.input.time_mode, mode, mode.to_string());
            }
        });
        ui.add_space(8.0);
        match self.input.time_mode {
            TimeMode::Local => {
                input_row(ui, "Date", &mut self.input.local_date, "");
                input_row(ui, "Time", &mut self.input.local_time, "");
                input_row(ui, "UTC offset", &mut self.input.utc_offset, "");
                ui.label(
                    egui::RichText::new("Local civil time with a fixed UTC offset")
                        .small()
                        .color(MUTED),
                );
            }
            TimeMode::JulianDate => {
                input_row(ui, "JD", &mut self.input.jd_tt, "TT");
                ui.label(
                    egui::RichText::new("Julian Date on the TT timescale")
                        .small()
                        .color(MUTED),
                );
            }
            TimeMode::ModifiedJulianDate => {
                input_row(ui, "MJD", &mut self.input.mjd_tt, "TT");
                ui.label(
                    egui::RichText::new("Modified Julian Date on the TT timescale")
                        .small()
                        .color(MUTED),
                );
            }
        }
    }

    fn target_inputs(&mut self, ui: &mut egui::Ui) {
        header_with_combo(
            ui,
            "Target",
            "target-frame",
            self.input.target_frame,
            |ui| {
                for frame in TargetFrame::ALL {
                    ui.selectable_value(&mut self.input.target_frame, frame, frame.to_string());
                }
            },
        );
        ui.add_space(8.0);

        match (self.input.target_frame, self.input.coordinate_format) {
            (TargetFrame::Icrs, CoordinateFormat::Sexagesimal) => {
                input_row(ui, "RA", &mut self.input.ra_hms, "h m s");
                input_row(ui, "Dec", &mut self.input.dec_dms, "° ′ ″");
            }
            (TargetFrame::Icrs, CoordinateFormat::DecimalDegrees) => {
                input_row(ui, "RA", &mut self.input.ra_deg, "°");
                input_row(ui, "Dec", &mut self.input.dec_deg, "°");
            }
            (TargetFrame::Horizontal, CoordinateFormat::Sexagesimal) => {
                input_row(ui, "Azimuth", &mut self.input.az_dms, "° ′ ″");
                input_row(ui, "Altitude", &mut self.input.alt_dms, "° ′ ″");
            }
            (TargetFrame::Horizontal, CoordinateFormat::DecimalDegrees) => {
                input_row(ui, "Azimuth", &mut self.input.az_deg, "°");
                input_row(ui, "Altitude", &mut self.input.target_alt_deg, "°");
            }
        }

        ui.horizontal(|ui| {
            ui.label("Format");
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
        if self.input.target_frame == TargetFrame::Horizontal {
            ui.label(
                egui::RichText::new(
                    "Horizontal pointing is interpreted at the window start and converted to a fixed J2000 direction.",
                )
                .small()
                .color(MUTED),
            );
        }
    }

    fn component_inputs(&mut self, ui: &mut egui::Ui) {
        ui.heading("Components");
        ui.add_space(4.0);
        ui.checkbox(&mut self.input.moonlight, "Moonlight");
        ui.checkbox(&mut self.input.zodiacal, "Zodiacal light");
        ui.checkbox(&mut self.input.airglow, "Airglow");
        let available = InputState::starlight_available();
        ui.add_enabled(
            available,
            egui::Checkbox::new(&mut self.input.starlight, "Integrated starlight"),
        );
        if !available {
            self.input.starlight = false;
            ui.label(
                egui::RichText::new(
                    "No admitted production starlight map is bundled in this build.",
                )
                .small()
                .color(MUTED),
            );
        }
    }

    fn results(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let results_stale = self
                .last_submitted
                .as_ref()
                .is_some_and(|submitted| submitted != &self.input);
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(4.0);
                let state = &self.state;
                let input = &mut self.input;
                let criteria_open = &mut self.criteria_open;
                match state {
                    CalculationState::Ready(output) => {
                        if results_stale {
                            ui.colored_label(
                                Color32::from_rgb(255, 196, 92),
                                "Inputs changed since this result was calculated. Calculate again to refresh it.",
                            );
                            ui.add_space(5.0);
                        }
                        render_chart(ui, output);
                        ui.add_space(8.0);
                        ui.columns(2, |columns| {
                            render_criteria(
                                &mut columns[0],
                                input,
                                criteria_open,
                                Some(output),
                                results_stale,
                            );
                            render_summary(&mut columns[1], output);
                        });
                        ui.add_space(8.0);
                        render_components(ui, output);
                        ui.add_space(8.0);
                        render_timeline(ui, output);
                    }
                    CalculationState::Running => {
                        render_criteria(ui, input, criteria_open, None, false);
                        ui.add_space(18.0);
                        ui.vertical_centered(|ui| {
                            ui.spinner();
                            ui.heading("Calculating NSB…");
                            ui.label("The scientific model is running on a worker thread.");
                        });
                    }
                    CalculationState::Error(error) => {
                        render_criteria(ui, input, criteria_open, None, false);
                        ui.add_space(18.0);
                        ui.colored_label(ERROR, "Calculation error");
                        ui.label(error);
                    }
                    CalculationState::Idle => {
                        render_criteria(ui, input, criteria_open, None, false);
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.heading("Night Sky Brightness");
                            ui.label(
                                "Configure the observation and calculate to plot real NSB model results.",
                            );
                        });
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

fn section(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(PANEL_FILL)
        .inner_margin(egui::Margin::same(12))
        .show(ui, add_contents);
}

fn header_with_combo<T: Copy + ToString>(
    ui: &mut egui::Ui,
    title: &str,
    id: &'static str,
    selected: T,
    add_options: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt(id)
                .selected_text(selected.to_string())
                .show_ui(ui, add_options);
        });
    });
}

fn input_row(ui: &mut egui::Ui, label: &str, value: &mut String, unit: &str) {
    egui::Grid::new(format!("input-row-{label}"))
        .num_columns(3)
        .spacing([8.0, 5.0])
        .show(ui, |ui| {
            ui.label(label);
            ui.add(egui::TextEdit::singleline(value).desired_width(135.0));
            ui.label(egui::RichText::new(unit).color(MUTED));
            ui.end_row();
        });
}

fn render_criteria(
    ui: &mut egui::Ui,
    input: &mut InputState,
    open: &mut bool,
    applied: Option<&CalculationOutput>,
    inputs_changed: bool,
) {
    section(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Observing criteria");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(if *open { "Done" } else { "Edit" })
                    .clicked()
                {
                    *open = !*open;
                }
            });
        });
        let threshold = applied
            .map(|output| format!("{:.6}", output.threshold))
            .unwrap_or_else(|| input.max_radiance.clone());
        ui.label(format!("Integrated NSB ≤ {threshold} ph cm⁻² ns⁻¹ sr⁻¹"));
        let sun_ceiling = applied
            .map(|output| {
                output
                    .sun_altitude_ceiling_deg
                    .map(|value| format!("{value:.3}"))
            })
            .unwrap_or_else(|| {
                input
                    .use_sun_ceiling
                    .then(|| input.sun_altitude_ceiling_deg.clone())
            });
        if let Some(value) = sun_ceiling {
            ui.label(format!("Sun altitude ≤ {value}°"));
        }
        let target_floor = applied
            .map(|output| {
                output
                    .target_altitude_floor_deg
                    .map(|value| format!("{value:.3}"))
            })
            .unwrap_or_else(|| {
                input
                    .use_target_floor
                    .then(|| input.target_altitude_floor_deg.clone())
            });
        if let Some(value) = target_floor {
            ui.label(format!("Target altitude ≥ {value}°"));
        }
        if let Some(output) = applied {
            ui.label(format!(
                "Search span {} · sample step {} s",
                format_duration((output.end - output.start).num_seconds() as f64),
                output.sample_step_seconds
            ));
        }
        ui.label(
            egui::RichText::new(
                "Matching windows are continuous intervals satisfying every enabled rule.",
            )
            .small()
            .color(MUTED),
        );
        if inputs_changed {
            ui.label(
                egui::RichText::new("Showing the criteria applied to the plotted result.")
                    .small()
                    .color(Color32::from_rgb(255, 196, 92)),
            );
        }

        if *open {
            ui.separator();
            input_row(ui, "Max NSB", &mut input.max_radiance, "ph cm⁻² ns⁻¹ sr⁻¹");
            ui.horizontal(|ui| {
                ui.checkbox(&mut input.use_sun_ceiling, "Sun altitude ceiling");
                ui.add_enabled(
                    input.use_sun_ceiling,
                    egui::TextEdit::singleline(&mut input.sun_altitude_ceiling_deg)
                        .desired_width(60.0),
                );
                ui.label("°");
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut input.use_target_floor, "Target altitude floor");
                ui.add_enabled(
                    input.use_target_floor,
                    egui::TextEdit::singleline(&mut input.target_altitude_floor_deg)
                        .desired_width(60.0),
                );
                ui.label("°");
            });
            input_row(ui, "Duration", &mut input.duration_hours, "h");
            input_row(ui, "Sample step", &mut input.sample_step_seconds, "s");
            if applied.is_some() {
                ui.label(
                    egui::RichText::new("Edits take effect on the next Calculate.")
                        .small()
                        .color(MUTED),
                );
            }
        }
    });
}

fn render_summary(ui: &mut egui::Ui, output: &CalculationOutput) {
    section(ui, |ui| {
        ui.heading("Summary");
        ui.label(egui::RichText::new("Lowest sampled integrated NSB").color(MUTED));
        ui.label(
            egui::RichText::new(format!(
                "{:.5} ph cm⁻² ns⁻¹ sr⁻¹",
                output.reference_radiance
            ))
            .size(22.0)
            .color(GREEN),
        );
        ui.label(format!(
            "at {} UTC",
            output.reference_time.format("%Y-%m-%d %H:%M")
        ));
        ui.add_space(5.0);
        ui.label(format!(
            "B diagnostic: {:.3} mag/arcsec²",
            output.reference_b_mag_arcsec2
        ));
        ui.label(format!(
            "V diagnostic: {:.3} mag/arcsec²",
            output.reference_v_mag_arcsec2
        ));
        ui.label(if output.reference_satisfies_criteria {
            "All active criteria are satisfied at this sampled time."
        } else {
            "Not all active criteria are satisfied at this sampled time."
        });
        ui.separator();
        match output.windows.as_slice() {
            [] => {
                ui.colored_label(MUTED, "No interval satisfies all selected criteria.");
            }
            [window] => {
                ui.label(egui::RichText::new("Observing window").color(MUTED));
                ui.label(
                    egui::RichText::new(format!(
                        "{} — {} UTC",
                        window.start.format("%H:%M"),
                        window.end.format("%H:%M")
                    ))
                    .size(20.0)
                    .color(GREEN),
                );
                ui.label(format_duration(window.duration_seconds()));
            }
            windows => {
                ui.label(
                    egui::RichText::new(format!("{} matching observing windows", windows.len()))
                        .size(18.0)
                        .color(GREEN),
                );
                for window in windows.iter().take(4) {
                    ui.label(format!(
                        "{} — {} UTC ({})",
                        window.start.format("%H:%M"),
                        window.end.format("%H:%M"),
                        format_duration(window.duration_seconds())
                    ));
                }
                if windows.len() > 4 {
                    ui.label(format!("… and {} more", windows.len() - 4));
                }
            }
        }
    });
}

fn render_components(ui: &mut egui::Ui, output: &CalculationOutput) {
    section(ui, |ui| {
        ui.heading("Component radiance at reference time");
        ui.label(
            egui::RichText::new(
                "Percentages are computed from additive integrated photon radiance, not magnitudes.",
            )
            .small()
            .color(MUTED),
        );
        ui.add_space(5.0);
        for component in &output.components {
            ui.horizontal(|ui| {
                ui.label(format!("{:<20}", component.name));
                let text = format!(
                    "{:.1}% · {:.4} ph cm⁻² ns⁻¹ sr⁻¹",
                    component.share * 100.0,
                    component.integrated_radiance
                );
                ui.add(egui::ProgressBar::new(component.share as f32).text(text));
            });
        }
    });
}

fn render_chart(ui: &mut egui::Ui, output: &CalculationOutput) {
    section(ui, |ui| {
        ui.heading("Night Sky Brightness");
        ui.label(
            egui::RichText::new(
                "Integrated photon radiance (300–650 nm) · lower values are darker",
            )
            .small()
            .color(MUTED),
        );
        ui.add_space(5.0);

        let desired = Vec2::new(ui.available_width(), 315.0);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, PLOT_FILL);
        let plot = Rect::from_min_max(
            rect.min + Vec2::new(64.0, 18.0),
            rect.max - Vec2::new(14.0, 42.0),
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
            y_min -= 0.5;
            y_max += 0.5;
        } else {
            let padding = (y_max - y_min) * 0.10;
            y_min = (y_min - padding).max(0.0);
            y_max += padding;
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
                Pos2::new(plot.left() - 8.0, y),
                Align2::RIGHT_CENTER,
                format!("{value:.3}"),
                FontId::proportional(11.0),
                MUTED,
            );
        }
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
                when.format("%H:%M").to_string(),
                FontId::proportional(11.0),
                MUTED,
            );
        }

        for window in &output.windows {
            let x0 = time_x(window.start, output.start, output.end, plot);
            let x1 = time_x(window.end, output.start, output.end, plot);
            let window_rect =
                Rect::from_min_max(Pos2::new(x0, plot.top()), Pos2::new(x1, plot.bottom()));
            painter.rect_filled(
                window_rect,
                0.0,
                Color32::from_rgba_unmultiplied(45, 180, 120, 42),
            );
        }

        let threshold_y = value_y(output.threshold, y_min, y_max, plot);
        dashed_horizontal(&painter, plot, threshold_y, GREEN);
        painter.text(
            Pos2::new(plot.right() - 4.0, threshold_y - 5.0),
            Align2::RIGHT_BOTTOM,
            format!("threshold {:.3}", output.threshold),
            FontId::proportional(11.0),
            GREEN,
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
        painter.line(points, Stroke::new(2.0_f32, BLUE));

        painter.text(
            Pos2::new(rect.left() + 12.0, rect.center().y),
            Align2::CENTER_CENTER,
            "ph cm⁻² ns⁻¹ sr⁻¹",
            FontId::proportional(11.0),
            MUTED,
        );

        if let Some(pointer) = response
            .hover_pos()
            .filter(|position| plot.contains(*position))
        {
            let fraction = ((pointer.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
            let index = ((output.samples.len() - 1) as f32 * fraction).round() as usize;
            if let Some(sample) = output.samples.get(index) {
                let point = Pos2::new(
                    time_x(sample.time, output.start, output.end, plot),
                    value_y(sample.integrated_radiance, y_min, y_max, plot),
                );
                painter.circle_filled(point, 4.0, Color32::WHITE);
                painter.text(
                    point + Vec2::new(8.0, -8.0),
                    Align2::LEFT_BOTTOM,
                    format!(
                        "{} UTC\n{:.5} ph cm⁻² ns⁻¹ sr⁻¹\nB {:.2} · V {:.2} mag/arcsec²",
                        sample.time.format("%H:%M"),
                        sample.integrated_radiance,
                        sample.b_mag_arcsec2,
                        sample.v_mag_arcsec2
                    ),
                    FontId::proportional(11.0),
                    Color32::WHITE,
                );
            }
        }
    });
}

fn render_timeline(ui: &mut egui::Ui, output: &CalculationOutput) {
    section(ui, |ui| {
        ui.heading("Observing windows");
        ui.label(
            egui::RichText::new(
                "Green intervals satisfy the exact same combined criteria used by the threshold search above.",
            )
            .small()
            .color(MUTED),
        );
        ui.add_space(5.0);
        ui.horizontal_wrapped(|ui| {
            timeline_legend(ui, Color32::from_rgb(105, 91, 72), "Day");
            timeline_legend(ui, TWILIGHT, "Twilight (Sun 0° to −18°)");
            timeline_legend(ui, NIGHT, "Astronomical night");
            timeline_legend(ui, GREEN, "All criteria");
        });
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 76.0), Sense::hover());
        let painter = ui.painter_at(rect);
        let label_width = 88.0;
        let solar_bar = Rect::from_min_max(
            Pos2::new(rect.left() + label_width, rect.top() + 5.0),
            Pos2::new(rect.right(), rect.top() + 25.0),
        );
        let criteria_bar = solar_bar.translate(Vec2::new(0.0, 30.0));
        painter.text(
            Pos2::new(rect.left(), solar_bar.center().y),
            Align2::LEFT_CENTER,
            "Solar state",
            FontId::proportional(11.0),
            MUTED,
        );
        painter.text(
            Pos2::new(rect.left(), criteria_bar.center().y),
            Align2::LEFT_CENTER,
            "All criteria",
            FontId::proportional(11.0),
            MUTED,
        );
        painter.rect_filled(solar_bar, 4.0, Color32::from_rgb(105, 91, 72));
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
        painter.rect_filled(criteria_bar, 4.0, Color32::from_rgb(30, 43, 54));
        paint_periods(&painter, criteria_bar, &output.windows, output, GREEN);
        for i in 0..=5 {
            let fraction = i as f32 / 5.0;
            let x = egui::lerp(criteria_bar.left()..=criteria_bar.right(), fraction);
            let when = interpolate_time(output.start, output.end, fraction as f64);
            painter.text(
                Pos2::new(x, criteria_bar.bottom() + 6.0),
                Align2::CENTER_TOP,
                when.format("%H:%M").to_string(),
                FontId::proportional(10.5),
                MUTED,
            );
        }
    });
}

fn timeline_legend(ui: &mut egui::Ui, color: Color32, label: &str) {
    let (swatch, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().rect_filled(swatch, 2.0, color);
    ui.label(egui::RichText::new(label).small().color(MUTED));
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
            3.0,
            color,
        );
    }
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

fn dashed_horizontal(painter: &egui::Painter, rect: Rect, y: f32, color: Color32) {
    let dash = 7.0;
    let gap = 5.0;
    let mut x = rect.left();
    while x < rect.right() {
        painter.line_segment(
            [Pos2::new(x, y), Pos2::new((x + dash).min(rect.right()), y)],
            Stroke::new(1.5_f32, color),
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

    #[test]
    fn duration_format_is_compact() {
        assert_eq!(format_duration(4.0 * 3600.0 + 10.0 * 60.0), "4 h 10 min");
        assert_eq!(format_duration(45.0 * 60.0), "45 min");
    }
}
