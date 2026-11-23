//! The window: a zoomable live view in the middle, controls and results on the right.
//!
//! Layout follows what worked in the reviewed tools: one image stage, a top bar of the few
//! actions (connect, live, measure), and an inspector with the settings and the results table.
//! Nothing here measures anything or decides anything: it builds a `Job`, the camera thread runs
//! it, `metrology_core::verdict` decides whether the reading can be believed and what the verdict
//! is, and this file only shows that. A reading that cannot be believed shows no verdict and no
//! number (unless the user asks to see it, and then it is marked as not trusted).

use crate::settings::Settings;
use crate::worker::{
    self, Cmd, Event, Frame8, Job, LiveSlot, Measured, Mode, Outcome, ThreadOutcome,
};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, Vec2};
use metrology_core::diameter::{Diameter, DiameterWindow};
use metrology_core::field::FieldMap;
use metrology_core::thread_standard::{
    fit as fit_range, unified_catalog, Fit, ThreadLimits, MM_PER_IN,
};
use metrology_core::thread_teeth::{Defect, Side};
use metrology_core::tolerance::{Spec, Verdict};
use metrology_core::verdict::{assess_diameter, assess_scan, thread_verdict, ThreadVerdict};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

type Px = (f64, f64);

const GREEN: Color32 = Color32::from_rgb(40, 160, 70);
const AMBER: Color32 = Color32::from_rgb(220, 160, 20);
const RED: Color32 = Color32::from_rgb(210, 50, 50);
const GREY: Color32 = Color32::from_rgb(110, 110, 120);

/// The editable limits of the chosen thread, in inches (kept in inches so editing never
/// converts back and forth).
#[derive(Clone, Copy)]
struct LimitsIn {
    major_max: f64,
    major_min: f64,
    minor_max: f64,
    minor_min: f64,
    pd_max: f64,
    pd_min: f64,
}

impl LimitsIn {
    fn of(l: &ThreadLimits) -> Self {
        LimitsIn {
            major_max: l.major_max_mm / MM_PER_IN,
            major_min: l.major_min_mm / MM_PER_IN,
            minor_max: l.minor_max_mm / MM_PER_IN,
            minor_min: l.minor_min_mm / MM_PER_IN,
            pd_max: l.pd_max_mm / MM_PER_IN,
            pd_min: l.pd_min_mm / MM_PER_IN,
        }
    }
}

pub struct App {
    cmds: Sender<Cmd>,
    events: Receiver<Event>,
    slot: LiveSlot,
    status: String,
    connected: bool,
    live: bool,
    mode: Mode,
    cal_path: String,
    calibration: Option<Arc<FieldMap>>,
    cal_note: String,
    rows: (usize, usize),
    cols: (usize, usize),
    texture: Option<TextureHandle>,
    image_size: (usize, usize),
    zoom: f32,
    pan: Vec2,
    fit_view: bool,
    apply_tolerance: bool,
    nominal_in: f64,
    plus_in: f64,
    minus_in: f64,
    guard_in: f64,
    catalog: Vec<ThreadLimits>,
    chosen: usize,
    limits_in: LimitsIn,
    show_untrusted: bool,
    /// After a measurement the measured image stays on screen until Live is switched on again.
    hold_image: bool,
    roi_anchor: Option<Vec2>,
    last: Option<Box<Measured>>,
    busy: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let (ev_tx, ev_rx) = channel();
        let slot: LiveSlot = Arc::new(Mutex::new(None));
        let ctx = cc.egui_ctx.clone();
        worker::spawn(cmd_rx, ev_tx, slot.clone(), move || ctx.request_repaint());
        let saved = cc
            .storage
            .and_then(|s| s.get_string("settings"))
            .map(|t| Settings::from_text(&t))
            .unwrap_or_default();
        let catalog = unified_catalog();
        let chosen = catalog
            .iter()
            .position(|t| t.name == saved.thread_size)
            .unwrap_or(0);
        let mut app = App {
            cmds: cmd_tx,
            events: ev_rx,
            slot,
            status: "starting".into(),
            connected: false,
            live: false,
            mode: if saved.thread_mode {
                Mode::Thread
            } else {
                Mode::Diameter
            },
            cal_path: saved.cal_path,
            calibration: None,
            cal_note: String::new(),
            rows: saved.rows,
            cols: saved.cols,
            texture: None,
            image_size: (2448, 2048),
            zoom: 0.3,
            pan: Vec2::ZERO,
            fit_view: true,
            apply_tolerance: saved.apply_tolerance,
            nominal_in: saved.nominal_in,
            plus_in: saved.plus_in,
            minus_in: saved.minus_in,
            guard_in: saved.guard_in,
            limits_in: LimitsIn::of(&catalog[chosen]),
            catalog,
            chosen,
            show_untrusted: false,
            hold_image: false,
            roi_anchor: None,
            last: None,
            busy: false,
        };
        app.load_calibration();
        let _ = app.cmds.send(Cmd::Connect);
        app
    }

    fn settings(&self) -> Settings {
        Settings {
            cal_path: self.cal_path.clone(),
            rows: self.rows,
            cols: self.cols,
            nominal_in: self.nominal_in,
            plus_in: self.plus_in,
            minus_in: self.minus_in,
            guard_in: self.guard_in,
            thread_size: self.catalog[self.chosen].name.clone(),
            thread_mode: self.mode == Mode::Thread,
            apply_tolerance: self.apply_tolerance,
        }
    }

    fn tpi(&self) -> f64 {
        self.catalog[self.chosen].tpi()
    }

    /// The limits as they stand in the editor, in millimetres.
    fn limits_mm(&self) -> ThreadLimits {
        let mut l = self.catalog[self.chosen].clone();
        let l_in = self.limits_in;
        l.major_max_mm = l_in.major_max * MM_PER_IN;
        l.major_min_mm = l_in.major_min * MM_PER_IN;
        l.minor_max_mm = l_in.minor_max * MM_PER_IN;
        l.minor_min_mm = l_in.minor_min * MM_PER_IN;
        l.pd_max_mm = l_in.pd_max * MM_PER_IN;
        l.pd_min_mm = l_in.pd_min * MM_PER_IN;
        l
    }

    fn window(&self) -> DiameterWindow {
        DiameterWindow {
            rows: self.rows,
            cols: self.cols,
        }
    }

    /// True when `m` was measured with the mode, window, thread and calibration now selected.
    fn is_current(&self, m: &Measured) -> bool {
        m.job.mode == self.mode
            && m.job.window == self.window()
            && (m.job.tpi - self.tpi()).abs() < 1e-9
            && self
                .calibration
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, &m.job.calibration))
    }

    fn load_calibration(&mut self) {
        match std::fs::read_to_string(&self.cal_path)
            .map_err(|e| e.to_string())
            .and_then(|t| FieldMap::from_text(&t))
        {
            Ok(map) => {
                let (x0, x1, y0, y1) = map.region();
                self.cal_note = format!(
                    "degree {}, region x {x0:.0} to {x1:.0}, y {y0:.0} to {y1:.0} px",
                    map.degree()
                );
                self.calibration = Some(Arc::new(map));
            }
            Err(e) => {
                self.calibration = None;
                self.cal_note = format!("not loaded: {e}");
            }
        }
    }

    fn start_measure(&mut self) {
        let Some(cal) = self.calibration.clone() else {
            self.status = "load a calibration first".into();
            return;
        };
        if !self.connected || self.busy {
            return;
        }
        self.busy = true;
        self.status = "measuring".into();
        let job = Job {
            mode: self.mode,
            window: self.window(),
            tpi: self.tpi(),
            calibration: cal,
        };
        let _ = self.cmds.send(Cmd::Measure(job));
    }

    fn set_frame(&mut self, ctx: &egui::Context, f: &Frame8) {
        let img = egui::ColorImage::from_gray([f.width, f.height], &f.pixels);
        match &mut self.texture {
            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
            None => {
                self.texture = Some(ctx.load_texture("view", img, egui::TextureOptions::LINEAR));
            }
        }
        // coordinates everywhere are full-image pixels; a half-size live frame is drawn stretched
        self.image_size = (f.width * f.scale, f.height * f.scale);
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        loop {
            let ev = match self.events.try_recv() {
                Ok(ev) => ev,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.status = "the camera thread stopped: restart the app".into();
                    (self.connected, self.live, self.busy) = (false, false, false);
                    break;
                }
            };
            match ev {
                Event::Status(s) => self.status = s,
                Event::Connected(c) => {
                    self.connected = c;
                    if !c {
                        self.live = false;
                        self.busy = false;
                    }
                }
                Event::LiveStopped => self.live = false,
                Event::FrameReady => {
                    let frame = self.slot.lock().ok().and_then(|mut s| s.take());
                    if let Some(f) = frame.filter(|_| !self.hold_image) {
                        self.set_frame(ctx, &f);
                    }
                }
                Event::Measured(m, f) => {
                    self.set_frame(ctx, &f);
                    self.status = format!(
                        "measured in {:.2} s (grab {:.2} s, compute {:.2} s)",
                        (m.grab + m.compute).as_secs_f64(),
                        m.grab.as_secs_f64(),
                        m.compute.as_secs_f64()
                    );
                    // the measured image stays on screen: stop the live stream so the button
                    // does not claim a stream that is being discarded
                    self.hold_image = true;
                    if self.live {
                        self.live = false;
                        let _ = self.cmds.send(Cmd::Live(false));
                    }
                    self.log_measurement(&m);
                    self.last = Some(m);
                    self.busy = false;
                }
                Event::MeasureFailed(why) => {
                    self.status = format!("measurement failed: {why}");
                    self.busy = false;
                }
            }
        }
    }

    /// One line per measurement in `measurements.csv`, with the verdict as it stood.
    fn log_measurement(&mut self, m: &Measured) {
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let (mode, value, verdict, notes) = match &m.outcome {
            Outcome::Diameter(Ok(d)) => {
                let trouble = assess_diameter(d);
                let inches = d.diameter_mm / MM_PER_IN;
                let spec = self
                    .apply_tolerance
                    .then(|| Spec::new(self.nominal_in, self.plus_in, self.minus_in));
                let verdict = diameter_log_verdict(trouble.is_empty(), spec, inches, self.guard_in);
                let notes = trouble
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ");
                ("diameter", format!("{inches:.5} in"), verdict, notes)
            }
            Outcome::Diameter(Err(e)) => (
                "diameter",
                String::new(),
                "NO MEASUREMENT".into(),
                e.clone(),
            ),
            Outcome::Thread(Ok(t)) => {
                let pd = t
                    .fit
                    .as_ref()
                    .map(|f| (f.pd_mm, f.pitch_mm))
                    .map_err(String::as_str);
                let teeth = t.teeth.as_ref().map_err(String::as_str);
                let (verdict, why) = match thread_verdict(&self.limits_mm(), &t.scan, pd, teeth) {
                    ThreadVerdict::Pass => ("PASS", Vec::new()),
                    ThreadVerdict::Fail(w) => ("FAIL", w),
                    ThreadVerdict::NoVerdict(w) => ("NO VERDICT", w),
                };
                let value = t.fit.as_ref().map_or(String::new(), |f| {
                    format!("pitch diameter {:.5} in", f.pd_mm / MM_PER_IN)
                });
                ("thread", value, verdict.to_string(), why.join("; "))
            }
            Outcome::Thread(Err(e)) => {
                ("thread", String::new(), "NO MEASUREMENT".into(), e.clone())
            }
        };
        let line = crate::log::row(unix, mode, &value, &verdict, &notes);
        if let Err(e) = crate::log::append("measurements.csv", &line) {
            self.status = format!("could not write measurements.csv: {e}");
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Thread Vision");
            ui.separator();
            if self.connected {
                if ui.button("Disconnect").clicked() {
                    let _ = self.cmds.send(Cmd::Disconnect);
                }
            } else if ui.button("Connect camera").clicked() {
                let _ = self.cmds.send(Cmd::Connect);
            }
            let live_label = if self.live { "Live: on" } else { "Live: off" };
            if ui
                .add_enabled(self.connected, egui::Button::new(live_label))
                .clicked()
            {
                self.live = !self.live;
                if self.live {
                    self.hold_image = false;
                }
                let _ = self.cmds.send(Cmd::Live(self.live));
            }
            let measure = egui::Button::new(RichText::new("Measure (M)").strong().size(18.0))
                .min_size(Vec2::new(150.0, 30.0));
            if ui
                .add_enabled(self.connected && !self.busy, measure)
                .clicked()
            {
                self.start_measure();
            }
            ui.separator();
            if self.calibration.is_none() {
                ui.colored_label(RED, "no calibration loaded");
                ui.separator();
            }
            ui.label(&self.status);
        });
    }

    fn stage(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(18));
        let (w, h) = (self.image_size.0 as f32, self.image_size.1 as f32);
        if self.fit_view || resp.double_clicked() {
            self.zoom = (rect.width() / w).min(rect.height() / h).max(0.01);
            self.pan = Vec2::ZERO;
            self.fit_view = false;
        }
        if resp.dragged_by(egui::PointerButton::Secondary)
            || resp.dragged_by(egui::PointerButton::Middle)
        {
            self.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let old = self.zoom;
                self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(0.02, 40.0);
                if let Some(p) = resp.hover_pos() {
                    // keep the image point under the cursor fixed
                    let origin = rect.center() + self.pan - Vec2::new(w, h) * 0.5 * old;
                    let img_pt = (p - origin) / old;
                    let new_origin = p - img_pt * self.zoom;
                    self.pan = new_origin + Vec2::new(w, h) * 0.5 * self.zoom - rect.center();
                }
            }
        }
        let origin = rect.center() + self.pan - Vec2::new(w, h) * 0.5 * self.zoom;
        // left-drag draws the scan window
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(p) = resp.interact_pointer_pos() {
                let img = ((p - origin) / self.zoom).clamp(Vec2::ZERO, Vec2::new(w, h));
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    self.roi_anchor = Some(img);
                }
                if let Some(a) = self.roi_anchor {
                    let (x0, x1, y0, y1) = (
                        a.x.min(img.x),
                        a.x.max(img.x),
                        a.y.min(img.y),
                        a.y.max(img.y),
                    );
                    if x1 - x0 >= 20.0 && y1 - y0 >= 20.0 {
                        self.cols = (x0 as usize, x1 as usize);
                        self.rows = (y0 as usize, y1 as usize);
                    }
                }
            }
        }
        if resp.drag_stopped() {
            self.roi_anchor = None;
        }
        painter.text(
            rect.left_bottom() + Vec2::new(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            "drag: scan window    right-drag: pan    wheel: zoom    double-click: fit",
            egui::FontId::proportional(12.0),
            Color32::from_gray(150),
        );
        let to_screen = |x: f64, y: f64| origin + Vec2::new(x as f32, y as f32) * self.zoom;
        if let Some(tex) = &self.texture {
            let image_rect = Rect::from_min_size(origin, Vec2::new(w, h) * self.zoom);
            painter.image(
                tex.id(),
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "no image yet: connect the camera and press Live or Measure",
                egui::FontId::proportional(16.0),
                Color32::GRAY,
            );
        }
        // the scan window
        let roi = Rect::from_two_pos(
            to_screen(self.cols.0 as f64, self.rows.0 as f64),
            to_screen(self.cols.1 as f64, self.rows.1 as f64),
        );
        painter.rect_stroke(
            roi,
            0.0,
            Stroke::new(1.0, Color32::from_rgb(230, 200, 40)),
            egui::StrokeKind::Inside,
        );
        // the measured edges, only while they belong to the window drawn
        if let Some(m) = self.last.as_ref().filter(|m| self.is_current(m)) {
            let (left, right): (&[Px], &[Px]) = match &m.outcome {
                Outcome::Diameter(Ok(d)) => (&d.left_px, &d.right_px),
                Outcome::Thread(Ok(t)) => (&t.scan.left_px, &t.scan.right_px),
                _ => (&[], &[]),
            };
            let step = (left.len() / 120).max(1);
            for (l, r) in left.iter().step_by(step).zip(right.iter().step_by(step)) {
                painter.circle_filled(to_screen(l.0, l.1), 1.8, Color32::from_rgb(60, 220, 255));
                painter.circle_filled(to_screen(r.0, r.1), 1.8, Color32::from_rgb(255, 150, 60));
            }
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Mode");
            ui.selectable_value(&mut self.mode, Mode::Diameter, "Diameter");
            ui.selectable_value(&mut self.mode, Mode::Thread, "Thread");
        });
        ui.separator();
        ui.collapsing("Calibration and scan window", |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.cal_path);
                if ui.button("Load").clicked() {
                    self.load_calibration();
                }
            });
            ui.label(&self.cal_note);
            let (iw, ih) = self.image_size;
            ui.horizontal(|ui| {
                ui.label("rows");
                ui.add(egui::DragValue::new(&mut self.rows.0).range(0..=ih));
                ui.add(egui::DragValue::new(&mut self.rows.1).range(0..=ih));
                ui.label("columns");
                ui.add(egui::DragValue::new(&mut self.cols.0).range(0..=iw));
                ui.add(egui::DragValue::new(&mut self.cols.1).range(0..=iw));
            });
        });
        ui.separator();
        match self.mode {
            Mode::Diameter => self.diameter_panel(ui),
            Mode::Thread => self.thread_panel(ui),
        }
    }

    /// The measurement to show, or why there is none.
    fn current_result(&self, ui: &mut egui::Ui, hint: &str) -> bool {
        match &self.last {
            None => {
                ui.label(hint);
                false
            }
            Some(m) if !self.is_current(m) => {
                ui.colored_label(
                    AMBER,
                    "The mode, scan window, thread or calibration changed since the last measurement. Press Measure.",
                );
                false
            }
            Some(_) => true,
        }
    }

    fn diameter_panel(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.apply_tolerance, "Apply tolerance (PASS / FAIL)");
        ui.add_enabled_ui(self.apply_tolerance, |ui| self.tolerance_fields(ui));
        ui.separator();
        if !self.current_result(ui, "Put the part in the light and press Measure.") {
            return;
        }
        let Some(m) = &self.last else { return };
        match &m.outcome {
            Outcome::Diameter(Ok(d)) => {
                let spec = self
                    .apply_tolerance
                    .then(|| Spec::new(self.nominal_in, self.plus_in, self.minus_in));
                let secs = (m.grab + m.compute).as_secs_f64();
                let mut show_anyway = self.show_untrusted;
                show_diameter(ui, d, spec, self.guard_in, secs, &mut show_anyway);
                self.show_untrusted = show_anyway;
            }
            Outcome::Diameter(Err(e)) => {
                big_chip(ui, "NO MEASUREMENT", GREY);
                ui.label(e);
                ui.label("The scan window must hold the part with background on both sides.");
            }
            Outcome::Thread(_) => {}
        }
    }

    fn tolerance_fields(&mut self, ui: &mut egui::Ui) {
        ui.strong("Tolerance (inches)");
        egui::Grid::new("spec").show(ui, |ui| {
            let field = |ui: &mut egui::Ui, label: &str, v: &mut f64| {
                ui.label(label);
                ui.add(egui::DragValue::new(v).speed(0.0001).fixed_decimals(4));
                ui.end_row();
            };
            field(ui, "nominal", &mut self.nominal_in);
            field(ui, "+ upper", &mut self.plus_in);
            field(ui, "- lower", &mut self.minus_in);
            field(ui, "guard band", &mut self.guard_in);
        });
    }

    fn thread_panel(&mut self, ui: &mut egui::Ui) {
        ui.strong("Thread (class 2A limits, editable)");
        let current = self.catalog[self.chosen].name.clone();
        egui::ComboBox::from_label("size")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for i in 0..self.catalog.len() {
                    let name = self.catalog[i].name.clone();
                    if ui.selectable_value(&mut self.chosen, i, name).clicked() {
                        self.limits_in = LimitsIn::of(&self.catalog[i]);
                    }
                }
            });
        egui::Grid::new("limits").show(ui, |ui| {
            let field = |ui: &mut egui::Ui, label: &str, v: &mut f64| {
                ui.label(label);
                ui.add(egui::DragValue::new(v).speed(0.0001).fixed_decimals(4));
                ui.end_row();
            };
            field(ui, "major max (in)", &mut self.limits_in.major_max);
            field(ui, "major min (in)", &mut self.limits_in.major_min);
            field(ui, "minor max (in)", &mut self.limits_in.minor_max);
            field(ui, "minor min (in)", &mut self.limits_in.minor_min);
            field(ui, "pitch dia max (in)", &mut self.limits_in.pd_max);
            field(ui, "pitch dia min (in)", &mut self.limits_in.pd_min);
        });
        ui.separator();
        if !self.current_result(
            ui,
            "Put the threaded section in the light and press Measure.",
        ) {
            return;
        }
        let limits = self.limits_mm();
        let Some(m) = &self.last else { return };
        match &m.outcome {
            Outcome::Thread(Ok(t)) => {
                let mut show_anyway = self.show_untrusted;
                show_thread(
                    ui,
                    t,
                    &limits,
                    (m.grab + m.compute).as_secs_f64(),
                    &mut show_anyway,
                );
                self.show_untrusted = show_anyway;
            }
            Outcome::Thread(Err(e)) => {
                big_chip(ui, "NO MEASUREMENT", GREY);
                ui.label(e);
            }
            Outcome::Diameter(_) => {}
        }
    }
}

/// A verdict as a block that fills the panel width, readable from arm's length.
fn big_chip(ui: &mut egui::Ui, text: &str, colour: Color32) {
    egui::Frame::new()
        .fill(colour)
        .corner_radius(6.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(text)
                        .size(30.0)
                        .strong()
                        .color(Color32::WHITE),
                );
            });
        });
}

fn verdict_chip(ui: &mut egui::Ui, v: Verdict) {
    match v {
        Verdict::Pass => big_chip(ui, "PASS", GREEN),
        Verdict::Warn => big_chip(ui, "WARN", AMBER),
        Verdict::Fail => big_chip(ui, "FAIL", RED),
    }
}

/// The verdict written to `measurements.csv` for a diameter. `spec` is None when the
/// tolerance is switched off, Some(None) when it is on but not valid.
fn diameter_log_verdict(
    trusted: bool,
    spec: Option<Option<Spec>>,
    inches: f64,
    guard_in: f64,
) -> String {
    match (trusted, spec) {
        (false, _) => "NO VERDICT".into(),
        (true, None) => "NO TOLERANCE".into(),
        (true, Some(None)) => "NO VERDICT".into(),
        (true, Some(Some(s))) => format!("{:?}", s.verdict(inches, guard_in)).to_uppercase(),
    }
}

fn show_diameter(
    ui: &mut egui::Ui,
    d: &Diameter,
    // None: tolerance off (reading only); Some(None): tolerance on but not valid
    spec: Option<Option<Spec>>,
    guard_in: f64,
    secs: f64,
    show_anyway: &mut bool,
) {
    let trouble = assess_diameter(d);
    let inches = d.diameter_mm / MM_PER_IN;
    if trouble.is_empty() {
        ui.label(RichText::new(format!("{inches:.5} in")).size(40.0).strong());
        ui.label(format!("{:.4} mm", d.diameter_mm));
        match spec {
            None => {}
            Some(Some(spec)) => {
                verdict_chip(ui, spec.verdict(inches, guard_in));
                let dev = spec.deviation(inches);
                let arrow = if dev >= 0.0 { "▲" } else { "▼" };
                let (lo, hi) = spec.limits();
                ui.label(format!(
                    "{arrow} {:+.1} µm from nominal   (limits {lo:.4} to {hi:.4} in)",
                    dev * MM_PER_IN * 1000.0
                ));
            }
            Some(None) => big_chip(ui, "TOLERANCE NOT VALID", GREY),
        }
    } else {
        big_chip(ui, "NO VERDICT", GREY);
        for t in &trouble {
            ui.colored_label(AMBER, t.to_string());
        }
        ui.checkbox(show_anyway, "show the reading anyway (not trusted)");
        if *show_anyway {
            ui.label(
                RichText::new(format!("{inches:.5} in  (NOT TRUSTED)"))
                    .size(24.0)
                    .color(GREY),
            );
        }
    }
    ui.separator();
    egui::Grid::new("diameter details").show(ui, |ui| {
        let row = |ui: &mut egui::Ui, k: &str, v: String| {
            ui.label(k);
            ui.label(v);
            ui.end_row();
        };
        row(
            ui,
            "rows measured",
            format!("{} ({} skipped)", d.rows_used, d.rows_skipped),
        );
        row(
            ui,
            "spread between rows",
            format!("{:.2} µm", d.row_sd_mm * 1000.0),
        );
        row(ui, "taper", format!("{:+.4} mm per mm", d.taper));
        row(ui, "tilt", format!("{:+.2}°", d.tilt_deg));
        row(
            ui,
            "light balance",
            d.illumination_imbalance
                .map_or("not judged".to_string(), |i| format!("{:+.1} %", i * 200.0)),
        );
        row(ui, "time", format!("{secs:.2} s"));
    });
}

fn defect_text(d: &Defect) -> String {
    match d {
        Defect::Missing => "broken / missing".into(),
        Defect::LowCrest { short_mm } => format!("crest low {short_mm:.2} mm"),
        Defect::ShallowRoot { high_mm } => format!("root shallow {high_mm:.2} mm"),
        Defect::Shifted { offset_mm } => format!("shifted {offset_mm:+.2} mm"),
        Defect::Narrow { width_mm } => format!("narrow {width_mm:.2} mm"),
        Defect::Wide { width_mm: Some(w) } => format!("wide {w:.2} mm"),
        Defect::Wide { width_mm: None } => "wide".into(),
        Defect::BadFlank {
            falling,
            half_angle_deg,
        } => format!(
            "{} flank {half_angle_deg:.0}°",
            if *falling { "lower" } else { "upper" }
        ),
        Defect::NoFlank { falling } => format!(
            "{} flank not measurable",
            if *falling { "lower" } else { "upper" }
        ),
    }
}

fn fit_label(ui: &mut egui::Ui, what: &str, d_mm: f64, lo: f64, hi: f64) {
    let (text, colour) = match fit_range(d_mm, lo, hi) {
        Fit::Within => ("within", GREEN),
        Fit::Below => ("BELOW MIN", RED),
        Fit::Above => ("ABOVE MAX", RED),
        Fit::Invalid => ("LIMITS NOT VALID", GREY),
    };
    ui.horizontal(|ui| {
        ui.label(format!(
            "{what}: {:.4} in  ({:.4} to {:.4})",
            d_mm / MM_PER_IN,
            lo / MM_PER_IN,
            hi / MM_PER_IN
        ));
        ui.colored_label(colour, text);
    });
}

fn show_thread(
    ui: &mut egui::Ui,
    t: &ThreadOutcome,
    lim: &ThreadLimits,
    secs: f64,
    show_anyway: &mut bool,
) {
    let pd = t
        .fit
        .as_ref()
        .map(|f| (f.pd_mm, f.pitch_mm))
        .map_err(String::as_str);
    let teeth = t.teeth.as_ref().map_err(String::as_str);
    match thread_verdict(lim, &t.scan, pd, teeth) {
        ThreadVerdict::Pass => big_chip(ui, "PASS", GREEN),
        ThreadVerdict::Fail(why) => {
            big_chip(ui, "FAIL", RED);
            for w in why {
                ui.colored_label(RED, w);
            }
        }
        ThreadVerdict::NoVerdict(why) => {
            big_chip(ui, "NO VERDICT", GREY);
            for w in why {
                ui.colored_label(AMBER, w);
            }
        }
    }
    ui.separator();
    if !assess_scan(&t.scan).is_empty() {
        ui.checkbox(show_anyway, "show the numbers anyway (not trusted)");
        if !*show_anyway {
            return;
        }
        ui.colored_label(
            GREY,
            "NOT TRUSTED: the readings below were taken in poor conditions",
        );
    }
    if let Ok(rep) = &t.teeth {
        ui.strong("Diameters against the limits");
        fit_label(
            ui,
            "major (crest)",
            2.0 * rep.reference_crest_r_mm,
            lim.major_min_mm,
            lim.major_max_mm,
        );
        fit_label(
            ui,
            "minor (root)",
            2.0 * rep.reference_root_r_mm,
            lim.minor_min_mm,
            lim.minor_max_mm,
        );
        ui.separator();
        ui.strong("Teeth (side, number from the top)");
        egui::Grid::new("teeth").striped(true).show(ui, |ui| {
            for h in ["", "crest Ø in", "upper °", "lower °", "verdict"] {
                ui.strong(h);
            }
            ui.end_row();
            for tooth in &rep.teeth {
                let side = if tooth.side == Side::Left { "L" } else { "R" };
                ui.label(format!("{side}{}", tooth.index));
                ui.label(format!("{:.4}", 2.0 * tooth.crest_r_mm / MM_PER_IN));
                let ang = |a: Option<f64>| a.map_or("-".to_string(), |a| format!("{a:.0}"));
                ui.label(ang(tooth.rise_half_angle_deg));
                ui.label(ang(tooth.fall_half_angle_deg));
                if tooth.defects.is_empty() {
                    ui.colored_label(GREEN, "ok");
                } else {
                    let text = tooth
                        .defects
                        .iter()
                        .map(defect_text)
                        .collect::<Vec<_>>()
                        .join(", ");
                    ui.colored_label(RED, text);
                }
                ui.end_row();
            }
        });
        ui.separator();
    }
    ui.strong("Whole-thread fit");
    match &t.fit {
        Ok(f) => {
            ui.label(format!(
                "pitch diameter {:.5} in   major {:.5} in   minor {:.5} in",
                f.pd_mm / MM_PER_IN,
                f.major_mm / MM_PER_IN,
                f.minor_mm / MM_PER_IN
            ));
            fit_label(ui, "pitch diameter", f.pd_mm, lim.pd_min_mm, lim.pd_max_mm);
            ui.label(format!("pitch {:.4} mm = {:.3} TPI", f.pitch_mm, f.tpi));
            ui.label(format!(
                "flank half angles: left {:.1}° / {:.1}°, right {:.1}° / {:.1}°",
                f.left_flank.rise_half_angle_deg,
                f.left_flank.fall_half_angle_deg,
                f.right_flank.rise_half_angle_deg,
                f.right_flank.fall_half_angle_deg
            ));
        }
        Err(e) => {
            ui.colored_label(
                AMBER,
                format!("The thread is too damaged or too short to fit as a whole: {e}"),
            );
        }
    }
    ui.label(format!("time {secs:.2} s"));
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_events(&ctx);
        if ctx.memory(|m| m.focused().is_none()) && ctx.input(|i| i.key_pressed(egui::Key::M)) {
            self.start_measure();
        }
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::right("inspector")
            .default_size(440.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.inspector(ui));
            });
        egui::CentralPanel::default().show(ui, |ui| self.stage(ui));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("settings", self.settings().to_text());
    }

    fn on_exit(&mut self) {
        let _ = self.cmds.send(Cmd::Quit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerance_off_never_logs_pass_or_fail() {
        let spec = Spec::new(0.5, 0.0005, 0.0005);
        // a 6 mm pin against the 0.5 in default would FAIL if the tolerance were applied
        assert_eq!(
            diameter_log_verdict(true, None, 0.2363, 0.0),
            "NO TOLERANCE"
        );
        assert_eq!(diameter_log_verdict(true, Some(spec), 0.2363, 0.0), "FAIL");
        assert_eq!(diameter_log_verdict(true, Some(spec), 0.5001, 0.0), "PASS");
    }

    #[test]
    fn untrusted_or_invalid_tolerance_is_no_verdict() {
        let spec = Spec::new(0.5, 0.0005, 0.0005);
        assert_eq!(diameter_log_verdict(false, None, 0.5, 0.0), "NO VERDICT");
        assert_eq!(
            diameter_log_verdict(false, Some(spec), 0.5, 0.0),
            "NO VERDICT"
        );
        assert_eq!(
            diameter_log_verdict(true, Some(None), 0.5, 0.0),
            "NO VERDICT"
        );
    }
}
