#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Rect, Response, RichText,
    Sense, Stroke,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sheshnag_core::kavach::{self, Kavach, VaultMeta};
use sheshnag_core::{Engine, Outcome, Target};

// Peacock-inspired palettes: deep teal, emerald, sapphire and iris, with a gold eyespot accent.
#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    bg: Color32,
    card: Color32,
    card_hi: Color32,
    line: Color32,
    text: Color32,
    muted: Color32,
    good: Color32,
    blue: Color32,
    iris: Color32,
    teal: Color32,
    warn: Color32,
    on_accent: Color32,
    term_bg: Color32,
    ghost: Color32,
    shadow: u8,
}

const DARK: Palette = Palette {
    dark: true,
    bg: Color32::from_rgb(6, 26, 33),
    card: Color32::from_rgb(13, 42, 53),
    card_hi: Color32::from_rgb(20, 58, 72),
    line: Color32::from_rgb(28, 76, 90),
    text: Color32::from_rgb(236, 246, 244),
    muted: Color32::from_rgb(130, 165, 172),
    good: Color32::from_rgb(46, 204, 160),
    blue: Color32::from_rgb(74, 140, 255),
    iris: Color32::from_rgb(158, 128, 236),
    teal: Color32::from_rgb(32, 190, 200),
    warn: Color32::from_rgb(242, 184, 64),
    on_accent: Color32::from_rgb(6, 26, 33),
    term_bg: Color32::from_rgb(4, 17, 22),
    ghost: Color32::from_rgba_premultiplied(24, 24, 24, 24),
    shadow: 90,
};

const LIGHT: Palette = Palette {
    dark: false,
    bg: Color32::from_rgb(247, 244, 236),
    card: Color32::from_rgb(255, 253, 248),
    card_hi: Color32::from_rgb(236, 232, 220),
    line: Color32::from_rgb(222, 216, 200),
    text: Color32::from_rgb(11, 43, 51),
    muted: Color32::from_rgb(92, 114, 120),
    good: Color32::from_rgb(8, 134, 100),
    blue: Color32::from_rgb(28, 88, 200),
    iris: Color32::from_rgb(104, 76, 178),
    teal: Color32::from_rgb(0, 128, 140),
    warn: Color32::from_rgb(176, 110, 8),
    on_accent: Color32::from_rgb(255, 253, 248),
    term_bg: Color32::from_rgb(238, 233, 220),
    ghost: Color32::from_rgba_premultiplied(0, 0, 0, 16),
    shadow: 28,
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

fn pal() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

#[derive(Clone, Copy, PartialEq)]
enum ThemeMode {
    System,
    Dark,
    Light,
}

impl ThemeMode {
    fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }
    fn parse(s: &str) -> Self {
        match s.trim() {
            "dark" => ThemeMode::Dark,
            "light" => ThemeMode::Light,
            _ => ThemeMode::System,
        }
    }
}

const PILL: CornerRadius = CornerRadius::same(255);

static ROUNDED_FONT: AtomicBool = AtomicBool::new(false);

fn main() -> eframe::Result<()> {
    // The scheduled monitor runs this same program as `sheshnag-ui dekho quiet`, with no window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|a| sheshnag_core::cli::is_command(a)).unwrap_or(false) {
        std::process::exit(sheshnag_core::cli::run(&args));
    }

    // Without an explicit icon eframe shows its own default "e" logo in the Dock.
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../packaging/assets/icon-512.png"))
        .expect("embedded app icon is a valid PNG");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(icon)
            .with_inner_size([1180.0, 860.0])
            .with_min_inner_size([900.0, 680.0])
            .with_title("Sheshnaag"),
        ..Default::default()
    };

    eframe::run_native(
        "Sheshnaag",
        options,
        Box::new(|cc| Ok(Box::new(SheshnagApp::new(&cc.egui_ctx)))),
    )
}

type EngineFactory = Arc<dyn Fn() -> Engine + Send + Sync>;

struct ActionResult {
    action: String,
    output: String,
    error: Option<String>,
    candidates: Option<Vec<Target>>,
}

struct SheshnagApp {
    home: Option<PathBuf>,
    factory: EngineFactory,
    baseline_exists: bool,
    monitor_installed: bool,
    findings: Vec<String>,
    log_lines: Vec<String>,
    read_error: Option<String>,
    action_output: Option<String>,
    busy: bool,
    /// What Agni could act on, from the latest scan.
    candidates: Vec<Target>,
    vault: Vec<VaultMeta>,
    /// Targets waiting for the person's confirmation in the Agni dialog.
    confirm: Option<Vec<Target>>,
    quarantine_label: String,
    accepted_path: String,
    tx: Sender<ActionResult>,
    rx: Receiver<ActionResult>,
    last_refresh: Instant,
    toast: Option<(String, Instant)>,
    theme_mode: ThemeMode,
    applied_dark: Option<bool>,
}

fn native_engine() -> Engine {
    Engine::new(sheshnag_core::platform::native())
}

impl SheshnagApp {
    fn new(ctx: &egui::Context) -> Self {
        Self::with_factory(ctx, Arc::new(native_engine))
    }

    fn with_factory(ctx: &egui::Context, factory: EngineFactory) -> Self {
        configure_style(ctx);
        let (tx, rx) = mpsc::channel();
        let home = Some(factory().platform.home().to_path_buf());
        let mut app = Self {
            home,
            factory,
            baseline_exists: false,
            monitor_installed: false,
            findings: Vec::new(),
            log_lines: Vec::new(),
            read_error: None,
            action_output: None,
            busy: false,
            candidates: Vec::new(),
            vault: Vec::new(),
            confirm: None,
            quarantine_label: String::new(),
            accepted_path: String::new(),
            tx,
            rx,
            last_refresh: Instant::now(),
            toast: None,
            theme_mode: ThemeMode::System,
            applied_dark: None,
        };
        if let Some(h) = &app.home {
            if let Ok(t) = fs::read_to_string(h.join(".sheshnag/theme")) {
                app.theme_mode = ThemeMode::parse(&t);
            }
        }
        app.refresh_monitor();
        app.refresh_data();
        app
    }

    /// Asking the platform whether the monitor exists can start a program, so it is not polled.
    fn refresh_monitor(&mut self) {
        self.monitor_installed = (self.factory)().platform.monitor_installed();
    }

    fn refresh_data(&mut self) {
        let Some(home) = self.home.clone() else {
            self.read_error = Some("The home directory is not available.".to_owned());
            return;
        };
        let state_dir = home.join(".sheshnag");

        self.baseline_exists = state_dir.join("baseline/agents").is_file();
        self.read_error = None;

        match read_optional_lines(&state_dir.join("findings.now")) {
            Ok(lines) => self.findings = lines.into_iter().filter(|line| !line.trim().is_empty()).collect(),
            Err(error) => self.read_error = Some(format!("Could not read findings: {error}")),
        }

        match read_optional_lines(&state_dir.join("log")) {
            Ok(mut lines) => {
                if lines.len() > 120 {
                    lines.drain(..lines.len() - 120);
                }
                self.log_lines = lines;
            }
            Err(error) => self.read_error = Some(format!("Could not read activity log: {error}")),
        }
        self.vault = Kavach::new(&(self.factory)()).vault();
        self.last_refresh = Instant::now();
    }

    fn run_action(&mut self, action: &str, args: Vec<String>) {
        if self.busy {
            return;
        }
        let factory = self.factory.clone();
        let tx = self.tx.clone();
        let known = self.candidates.clone();
        let name = action.to_owned();
        self.busy = true;
        self.action_output = None;

        std::thread::spawn(move || {
            let engine = factory();
            let mut candidates: Option<Vec<Target>> = None;
            let out: Outcome = match name.as_str() {
                "dekho" => {
                    let (o, t) = engine.dekho_with_targets(false);
                    candidates = Some(t);
                    o
                }
                "baseline" => engine.baseline(),
                "install" => match std::env::current_exe() {
                    Ok(exe) => engine.install(&exe),
                    Err(e) => Outcome { code: 1, output: format!("Could not find the app's own path: {e}") },
                },
                "uninstall" => engine.uninstall(),
                "jaago" => engine.jaago(args.first().map(String::as_str).unwrap_or("")),
                "theek" => engine.theek(args.first().map(String::as_str).unwrap_or("")),
                "agni" => {
                    let targets: Vec<Target> =
                        args.iter().filter_map(|k| known.iter().find(|t| &t.key() == k).cloned()).collect();
                    match kavach::destroy(&engine, targets, "user-ui") {
                        Ok(report) => Outcome { code: if report.ok() { 0 } else { 1 }, output: report.text() },
                        Err(refusal) => Outcome { code: 1, output: refusal.to_string() },
                    }
                }
                "restore" => match Kavach::new(&engine).restore(args.first().map(String::as_str).unwrap_or("")) {
                    Ok(msg) => Outcome { code: 0, output: msg },
                    Err(e) => Outcome { code: 1, output: e },
                },
                other => Outcome { code: 1, output: format!("Unknown action {other}") },
            };
            if candidates.is_none() && name != "uninstall" && name != "install" {
                candidates = Some(engine.candidates());
            }
            let (output, error) = match (name.as_str(), out.code) {
                ("dekho", 0) => (format!("Scan finished. No findings reported.\n\n{}", out.output), None),
                ("dekho", 1) => (format!("Scan finished. Findings need your review.\n\n{}", out.output), None),
                (_, 0) => (out.output, None),
                (_, code) => (out.output, Some(format!("Finished with status {code}."))),
            };
            let _ = tx.send(ActionResult { action: name, output, error, candidates });
        });
    }

    fn receive_action(&mut self) {
        while let Ok(result) = self.rx.try_recv() {
            self.busy = false;
            if let Some(c) = result.candidates {
                self.candidates = c;
            }
            self.refresh_monitor();
            self.refresh_data();
            let mut text = result.output;
            if let Some(error) = result.error {
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(&error);
                self.toast = Some((format!("{} needs attention", result.action), Instant::now()));
            } else {
                self.toast = Some((format!("{} completed", result.action), Instant::now()));
            }
            self.action_output = (!text.trim().is_empty()).then_some(text);
        }
    }
}

type Pending = Option<(&'static str, Vec<String>)>;

enum Mood {
    Setup,
    Clean,
    Sus(usize),
}

impl eframe::App for SheshnagApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl SheshnagApp {
    fn ui(&mut self, ctx: &egui::Context) {
        self.receive_action();
        if self.last_refresh.elapsed() >= Duration::from_secs(3) {
            self.refresh_data();
        }
        ctx.request_repaint_after(Duration::from_millis(40));
        let sys_dark = ctx.system_theme().map(|t| t == egui::Theme::Dark).unwrap_or(true);
        let dark = match self.theme_mode {
            ThemeMode::System => sys_dark,
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
        };
        IS_DARK.store(dark, Ordering::Relaxed);
        if self.applied_dark != Some(dark) {
            apply_visuals(ctx);
            self.applied_dark = Some(dark);
        }
        let time = ctx.input(|i| i.time) as f32;

        let mut pending: Pending = None;
        let mut refresh = false;
        let busy = self.busy;
        let mood = if !self.findings.is_empty() {
            Mood::Sus(self.findings.len())
        } else if !self.baseline_exists {
            Mood::Setup
        } else {
            Mood::Clean
        };
        let side = ((ctx.screen_rect().width() - 1120.0) / 2.0).clamp(28.0, 127.0) as i8;

        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::new().fill(pal().bg).inner_margin(Margin::symmetric(side, 18)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    serpent_mark(ui, time);
                    ui.add_space(4.0);
                    ui.label(display("sheshnaag", 26.0, pal().text));
                    chip(ui, "v1.0", pal().blue);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme_switch(ui, &mut self.theme_mode) {
                            if let Some(h) = &self.home {
                                let _ = fs::create_dir_all(h.join(".sheshnag"));
                                let _ = fs::write(h.join(".sheshnag/theme"), self.theme_mode.as_str());
                            }
                        }
                        ui.add_space(6.0);
                        if busy {
                            chip(ui, "working on it", pal().good);
                            ui.add(egui::Spinner::new().color(pal().good).size(16.0));
                        } else {
                            chip(ui, "on demand only", pal().muted);
                        }
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal().bg).inner_margin(Margin::symmetric(side, 4)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(14.0, 14.0);
                        ui.add_space(6.0);

                        self.hero(ui, time, &mood, &mut pending);

                        // ---- the three layers ----
                        ui.add_space(8.0);
                        section_title(ui, "the sheshnag protocol", "three layers: watch, destroy, shield");
                        ui.columns(3, |c| {
                            protocol_card(&mut c[0], "01", "Sakshi", "the witness · outer eyes",
                                "Detects, protects and warns. It only watches and speaks, and never changes anything on its own.",
                                "live", pal().good);
                            protocol_card(&mut c[1], "02", "Agni", "the fire · destroy, nothing else",
                                "Runs only when you confirm a specific finding, and does one thing: destroy it. No modes, no second guessing.",
                                "you confirm", pal().warn);
                            protocol_card(&mut c[2], "03", "Kavach", "the shield · inner eyes",
                                "Checks every target first, seals a backup, then verifies the result. Restore anything from the vault.",
                                "live", pal().teal);
                        });

                        // ---- status tiles with their own controls ----
                        ui.add_space(8.0);
                        section_title(ui, "the setup", "baseline + background watch");
                        let baseline_exists = self.baseline_exists;
                        let monitor_installed = self.monitor_installed;
                        ui.columns(2, |c| {
                            card(&mut c[0], pal().blue, 150.0, |ui| {
                                let (title, desc, tint) = if baseline_exists {
                                    ("baseline locked in", "Your current startup setup can be compared against it.", pal().good)
                                } else {
                                    ("no baseline yet", "Snapshot a clean state first, so we know what normal looks like.", pal().warn)
                                };
                                chip(ui, "trusted baseline", tint);
                                ui.label(display(title, 24.0, pal().text));
                                ui.add(egui::Label::new(RichText::new(desc).color(pal().muted)).wrap());
                                ui.add_space(4.0);
                                let label = if baseline_exists { "Re-record baseline" } else { "Lock in baseline" };
                                if pill_button(ui, label, pal().blue, Color32::WHITE, !busy).clicked() {
                                    pending = Some(("baseline", vec![]));
                                }
                            });
                            card(&mut c[1], pal().iris, 150.0, |ui| {
                                let (title, desc, tint) = if monitor_installed {
                                    ("auto-watch is on", "The LaunchAgent checks in every 10 minutes.", pal().good)
                                } else {
                                    ("auto-watch is off", "Background checks only start if you turn them on.", pal().muted)
                                };
                                chip(ui, "scheduled monitor", tint);
                                ui.label(display(title, 24.0, pal().text));
                                ui.add(egui::Label::new(RichText::new(desc).color(pal().muted)).wrap());
                                ui.add_space(4.0);
                                if monitor_installed {
                                    if pill_button(ui, "Turn off auto-watch", pal().ghost, pal().text, !busy).clicked() {
                                        pending = Some(("uninstall", vec![]));
                                    }
                                } else if pill_button(ui, "Turn on auto-watch", pal().iris, Color32::WHITE, !busy).clicked() {
                                    pending = Some(("install", vec![]));
                                }
                            });
                        });

                        // ---- findings ----
                        ui.add_space(8.0);
                        section_title(
                            ui,
                            "needs your eyes",
                            &format!("{} finding(s) from the latest check", self.findings.len()),
                        );
                        let findings = &self.findings;
                        card(ui, if findings.is_empty() { pal().good } else { pal().warn }, 0.0, |ui| {
                            if findings.is_empty() {
                                ui.horizontal(|ui| {
                                    {
                                        let (r, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::hover());
                                        draw_check(ui.painter(), r.center(), 30.0, pal().good);
                                    }
                                    ui.vertical(|ui| {
                                        ui.label(display("nothing sus rn", 20.0, pal().text));
                                        ui.label(
                                            RichText::new("Run a check to refresh this. A quiet result is not a guarantee of safety.")
                                                .size(12.0)
                                                .color(pal().muted),
                                        );
                                    });
                                });
                            } else {
                                for finding in findings.iter().rev().take(8) {
                                    finding_row(ui, finding);
                                }
                                if findings.len() > 8 {
                                    ui.label(
                                        RichText::new(format!("+{} more in the activity log", findings.len() - 8))
                                            .size(12.0)
                                            .color(pal().muted),
                                    );
                                }
                            }
                        });

                        // ---- agni + kavach ----
                        ui.add_space(8.0);
                        section_title(ui, "agni and kavach", "destroy what is flagged, with a backup sealed first");
                        let mut ask: Option<Vec<Target>> = None;
                        let mut restore: Option<String> = None;
                        let candidates = &self.candidates;
                        let vault = &self.vault;
                        ui.columns(2, |c| {
                            card(&mut c[0], pal().warn, 150.0, |ui| {
                                ui.label(display("agni · the fire", 18.0, pal().text));
                                if candidates.is_empty() {
                                    ui.add(egui::Label::new(
                                        RichText::new("Nothing to destroy. Run a vibe check, and anything flagged that Agni can act on shows up here.")
                                            .size(12.5)
                                            .color(pal().muted),
                                    ).wrap());
                                }
                                for t in candidates.iter().take(6) {
                                    egui::Frame::new()
                                        .fill(pal().card_hi)
                                        .corner_radius(CornerRadius::same(14))
                                        .inner_margin(Margin::symmetric(12, 10))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.add(egui::Label::new(RichText::new(t.describe()).monospace().size(11.5).color(pal().text)).wrap());
                                            if pill_button(ui, "Review and destroy", pal().warn, pal().on_accent, !busy).clicked() {
                                                ask = Some(vec![t.clone()]);
                                            }
                                        });
                                }
                                if candidates.len() > 6 {
                                    ui.label(RichText::new(format!("+{} more", candidates.len() - 6)).size(12.0).color(pal().muted));
                                }
                            });
                            card(&mut c[1], pal().teal, 150.0, |ui| {
                                ui.label(display("kavach · the vault", 18.0, pal().text));
                                if vault.is_empty() {
                                    ui.add(egui::Label::new(
                                        RichText::new("The vault is empty. Before Agni destroys anything, Kavach seals a copy here so you can restore it.")
                                            .size(12.5)
                                            .color(pal().muted),
                                    ).wrap());
                                }
                                for m in vault.iter().rev().take(6) {
                                    egui::Frame::new()
                                        .fill(pal().card_hi)
                                        .corner_radius(CornerRadius::same(14))
                                        .inner_margin(Margin::symmetric(12, 10))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.add(egui::Label::new(RichText::new(&m.id).monospace().size(11.5).color(pal().text)).wrap());
                                            ui.label(RichText::new(format!("sealed {}", m.sealed_at)).size(11.0).color(pal().muted));
                                            if m.kind == "persistence" && pill_button(ui, "Restore", pal().teal, pal().on_accent, !busy).clicked() {
                                                restore = Some(m.vault_id.clone());
                                            }
                                        });
                                }
                            });
                        });
                        if let Some(t) = ask {
                            self.confirm = Some(t);
                        }
                        if let Some(id) = restore {
                            pending = Some(("restore", vec![id]));
                        }

                        // ---- manual tools ----
                        ui.add_space(8.0);
                        section_title(ui, "your call", "manual tools: nothing happens until you press the button");
                        let quarantine_label = &mut self.quarantine_label;
                        let accepted_path = &mut self.accepted_path;
                        ui.columns(2, |c| {
                            card(&mut c[0], pal().warn, 0.0, |ui| {
                                ui.label(display("quarantine a launch agent", 18.0, pal().text));
                                ui.label(RichText::new("Unloads a matching agent and moves its plist to quarantine.").size(12.0).color(pal().muted));
                                if input_row(ui, quarantine_label, "com.example.agent", "Quarantine it", pal().warn, !busy) {
                                    pending = Some(("jaago", vec![quarantine_label.trim().to_owned()]));
                                }
                            });
                            card(&mut c[1], pal().teal, 0.0, |ui| {
                                ui.label(display("mark a file as chill", 18.0, pal().text));
                                ui.label(RichText::new("Stops flagging this file until its contents change.").size(12.0).color(pal().muted));
                                if input_row(ui, accepted_path, "/path/to/file.plist", "Trust it", pal().teal, !busy) {
                                    pending = Some(("theek", vec![accepted_path.trim().to_owned()]));
                                }
                            });
                        });

                        if let Some(error) = &self.read_error {
                            ui.label(RichText::new(error).color(Color32::from_rgb(214, 80, 90)));
                        }

                        if let Some(output) = &self.action_output {
                            ui.add_space(8.0);
                            section_title(ui, "last action", "what the script just said");
                            terminal(ui, |ui| {
                                ui.add(egui::Label::new(RichText::new(output).monospace().size(12.0).color(pal().text)).wrap());
                            });
                        }

                        // ---- activity ----
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            section_title(ui, "activity", "latest entries from ~/.sheshnag/log");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if pill_button(ui, "Refresh", pal().ghost, pal().text, true).clicked() {
                                    refresh = true;
                                }
                            });
                        });
                        let log_lines = &self.log_lines;
                        terminal(ui, |ui| {
                            if log_lines.is_empty() {
                                ui.label(RichText::new("no activity yet. start with a vibe check or lock in a baseline.").monospace().size(12.0).color(pal().muted));
                            }
                            for line in log_lines.iter().rev().take(12) {
                                log_line(ui, line);
                            }
                        });

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("sheshnaag observes · you decide").size(11.0).color(pal().muted));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("updated {}s ago", self.last_refresh.elapsed().as_secs()))
                                        .size(11.0)
                                        .color(pal().muted),
                                );
                            });
                        });
                        ui.add_space(16.0);
                    });
            });

        if let Some(targets) = self.confirm.clone() {
            let mut decision: Option<bool> = None;
            let screen = ctx.screen_rect();
            egui::Area::new(egui::Id::new("dim"))
                .order(egui::Order::Middle)
                .fixed_pos(screen.min)
                .show(ctx, |ui| {
                    let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
                    ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(165));
                });
            egui::Window::new("agni_confirm")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .fixed_size([520.0, 0.0])
                .frame(
                    egui::Frame::new()
                        .fill(pal().card)
                        .stroke(Stroke::new(1.0_f32, pal().warn.gamma_multiply(0.6)))
                        .corner_radius(CornerRadius::same(26))
                        .inner_margin(Margin::same(26)),
                )
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    ui.label(display("Agni needs your word", 24.0, pal().text));
                    ui.add(egui::Label::new(RichText::new("You are about to destroy:").color(pal().muted)).wrap());
                    for t in &targets {
                        egui::Frame::new()
                            .fill(pal().card_hi)
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::symmetric(12, 9))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(egui::Label::new(RichText::new(t.describe()).monospace().size(12.0).color(pal().text)).wrap());
                            });
                    }
                    ui.add(egui::Label::new(
                        RichText::new("Kavach checks each target and seals a backup first, so a startup entry can be restored from the vault. A running process cannot be brought back.")
                            .size(12.5)
                            .color(pal().muted),
                    ).wrap());
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if pill_button(ui, "Cancel", pal().ghost, pal().text, true).clicked() {
                            decision = Some(false);
                        }
                        let label = format!("Destroy {} target{}", targets.len(), if targets.len() == 1 { "" } else { "s" });
                        if pill_button(ui, &label, Color32::from_rgb(214, 70, 80), Color32::WHITE, !busy).clicked() {
                            decision = Some(true);
                        }
                    });
                });
            match decision {
                Some(true) => {
                    let keys: Vec<String> = targets.iter().map(|t| t.key()).collect();
                    self.confirm = None;
                    pending = Some(("agni", keys));
                }
                Some(false) => self.confirm = None,
                None => {}
            }
        }

        if refresh {
            self.refresh_data();
        }
        if let Some((action, args)) = pending {
            self.run_action(action, args);
        }

        if let Some((message, when)) = &self.toast {
            let age = when.elapsed().as_secs_f32();
            if age < 3.0 {
                let fade = (age / 0.25).min(1.0).min((3.0 - age) / 0.4);
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, [-28.0, -24.0 + (1.0 - fade) * 12.0])
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(pal().card.gamma_multiply(fade))
                            .stroke(Stroke::new(1.0_f32, pal().good.gamma_multiply(0.6 * fade)))
                            .corner_radius(PILL)
                            .inner_margin(Margin::symmetric(18, 11))
                            .shadow(egui::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha((pal().shadow as f32 * fade) as u8) })
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("●").size(10.0).color(pal().good.gamma_multiply(fade)));
                                    ui.label(display(message, 14.0, pal().text.gamma_multiply(fade)));
                                });
                            });
                    });
            } else {
                self.toast = None;
            }
        }
    }
}

impl SheshnagApp {
    fn hero(&self, ui: &mut egui::Ui, time: f32, mood: &Mood, pending: &mut Pending) {
        let (c1, c2) = match (self.busy, mood) {
            (true, _) => (Color32::from_rgb(18, 62, 160), Color32::from_rgb(0, 124, 150)),
            (_, Mood::Clean) => (Color32::from_rgb(14, 58, 150), Color32::from_rgb(0, 128, 122)),
            (_, Mood::Sus(_)) => (Color32::from_rgb(120, 38, 34), Color32::from_rgb(186, 104, 18)),
            (_, Mood::Setup) => (Color32::from_rgb(80, 56, 164), Color32::from_rgb(22, 88, 184)),
        };
        let (headline, sub) = match (self.busy, mood) {
            (true, _) => ("on it…".to_owned(), "sheshnaag is looking around your mac. hang tight.".to_owned()),
            (_, Mood::Clean) => ("all quiet. we vibin.".to_owned(), "No findings from the latest check. Run another whenever you want.".to_owned()),
            (_, Mood::Sus(n)) => (
                format!("{n} sus thing{} found", if *n == 1 { "" } else { "s" }),
                "Scroll down, review each one, then decide. Nothing was changed.".to_owned(),
            ),
            (_, Mood::Setup) => ("let's set a baseline".to_owned(), "Lock in a clean snapshot first so checks know what normal looks like.".to_owned()),
        };

        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 232.0), Sense::hover());
        rounded_gradient(ui.painter(), rect, 34.0, c1, c2);

        // pulsing orb
        let p = ui.painter().with_clip_rect(rect);
        let center = Pos2::new(rect.right() - 140.0, rect.center().y);
        for i in 0..4 {
            let k = i as f32;
            let pulse = ((time * 1.8 - k * 0.6).sin() * 0.5 + 0.5) * 7.0;
            p.circle_stroke(center, 54.0 + k * 24.0 + pulse, Stroke::new(1.5_f32, Color32::WHITE.gamma_multiply(0.34 - k * 0.07)));
        }
        p.circle_filled(center, 52.0, Color32::from_white_alpha(40));
        p.circle_filled(center, 40.0, Color32::from_rgba_unmultiplied(4, 20, 26, 205));
        let gold = Color32::from_rgb(242, 193, 78);
        let mint = Color32::from_rgb(88, 232, 196);
        match (self.busy, mood) {
            (true, _) => {
                for i in 0..3 {
                    let k = i as f32;
                    let bounce = ((time * 5.0 - k * 0.8).sin() * 0.5 + 0.5) * 5.0;
                    p.circle_filled(Pos2::new(center.x + (k - 1.0) * 16.0, center.y - bounce + 2.0), 5.0, mint);
                }
            }
            (_, Mood::Clean) => draw_check(&p, center, 44.0, mint),
            (_, Mood::Sus(n)) => {
                p.text(center, egui::Align2::CENTER_CENTER, n.to_string(), FontId::new(34.0, display_family()), gold);
            }
            (_, Mood::Setup) => {
                p.text(center, egui::Align2::CENTER_CENTER, "?", FontId::new(34.0, display_family()), gold);
            }
        }

        let inner = Rect::from_min_size(
            rect.min + egui::vec2(38.0, 30.0),
            egui::vec2((rect.width() - 340.0).max(320.0), rect.height() - 60.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        {
            let ui = &mut child;
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
            chip(ui, "vibe check", Color32::WHITE);
            ui.add(egui::Label::new(display(&headline, 42.0, Color32::WHITE)).wrap());
            ui.add(egui::Label::new(RichText::new(sub).size(14.0).color(Color32::WHITE.gamma_multiply(0.85))).wrap());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if pill_button(ui, "Run vibe check", Color32::from_rgb(242, 193, 78), Color32::from_rgb(26, 20, 5), !self.busy).clicked() {
                    *pending = Some(("dekho", vec![]));
                }
                ui.label(RichText::new("one scan, right now").size(12.0).color(Color32::WHITE.gamma_multiply(0.75)));
            });
        }
    }
}

// ---------- style ----------

fn configure_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = fs::read("/System/Library/Fonts/Supplemental/Arial Rounded Bold.ttf") {
        fonts.font_data.insert("display".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        let mut family = vec!["display".to_owned()];
        family.extend(fonts.families[&FontFamily::Proportional].clone());
        fonts.families.insert(FontFamily::Name("display".into()), family);
        ROUNDED_FONT.store(true, Ordering::Relaxed);
    }
    ctx.set_fonts(fonts);
}

/// Applies the active palette to egui's visuals. Called whenever the resolved theme changes.
fn apply_visuals(ctx: &egui::Context) {
    let p = pal();
    ctx.set_theme(if p.dark { egui::Theme::Dark } else { egui::Theme::Light });
    let mut style = (*ctx.style()).clone();
    style.visuals = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let v = &mut style.visuals;
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.extreme_bg_color = p.card_hi;
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = p.blue.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0_f32, p.blue);
    v.text_cursor.stroke = Stroke::new(2.0_f32, p.good);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.corner_radius = CornerRadius::same(14);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.line);
    v.widgets.inactive.bg_fill = p.card_hi;
    v.widgets.inactive.weak_bg_fill = p.card_hi;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.line);
    v.widgets.hovered.bg_fill = p.card_hi.lerp_to_gamma(p.blue, 0.18);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.blue);
    v.widgets.active.bg_fill = p.blue.gamma_multiply(0.5);
    v.window_corner_radius = CornerRadius::same(20);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.scroll.bar_width = 6.0;
    ctx.set_style(style);
}

fn draw_check(p: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new(s * 0.14, color);
    let a = c + egui::vec2(-0.46 * s, 0.02 * s);
    let b = c + egui::vec2(-0.12 * s, 0.36 * s);
    let d = c + egui::vec2(0.48 * s, -0.34 * s);
    p.line_segment([a, b], stroke);
    p.line_segment([b, d], stroke);
    for pt in [a, b, d] {
        p.circle_filled(pt, s * 0.07, color);
    }
}

/// Three-way segmented control. Returns true when the selection changed.
fn theme_switch(ui: &mut egui::Ui, mode: &mut ThemeMode) -> bool {
    let items = [(ThemeMode::System, "System"), (ThemeMode::Dark, "Dark"), (ThemeMode::Light, "Light")];
    let galleys: Vec<_> = items
        .iter()
        .map(|(_, l)| ui.painter().layout_no_wrap((*l).to_owned(), FontId::new(12.5, display_family()), pal().text))
        .collect();
    let total_w: f32 = galleys.iter().map(|g| g.size().x + 26.0).sum::<f32>() + 8.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, 36.0), Sense::hover());
    ui.painter().rect_filled(rect, PILL, pal().card);
    ui.painter().rect_stroke(rect, PILL, Stroke::new(1.0_f32, pal().line), egui::StrokeKind::Inside);
    let mut x = rect.left() + 4.0;
    let mut changed = false;
    for (i, ((m, _), galley)) in items.iter().zip(galleys).enumerate() {
        let w = galley.size().x + 26.0;
        let seg = Rect::from_min_size(Pos2::new(x, rect.top() + 4.0), egui::vec2(w, 28.0));
        let resp = ui.interact(seg, ui.id().with(("theme_seg", i)), Sense::click());
        let selected = *mode == *m;
        if selected {
            ui.painter().rect_filled(seg, PILL, pal().blue);
        } else if resp.hovered() {
            ui.painter().rect_filled(seg, PILL, pal().ghost);
        }
        let col = if selected { Color32::WHITE } else { pal().muted };
        let g = ui.painter().layout_no_wrap(items[i].1.to_owned(), FontId::new(12.5, display_family()), col);
        ui.painter().galley(seg.center() - g.size() / 2.0, g, col);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() && !selected {
            *mode = *m;
            changed = true;
        }
        x += w;
    }
    changed
}

fn display_family() -> FontFamily {
    if ROUNDED_FONT.load(Ordering::Relaxed) {
        FontFamily::Name("display".into())
    } else {
        FontFamily::Proportional
    }
}

fn display(text: &str, size: f32, color: Color32) -> RichText {
    RichText::new(text).font(FontId::new(size, display_family())).color(color)
}

// ---------- widgets ----------

/// Rounded rectangle filled with a diagonal two-colour gradient.
fn rounded_gradient(painter: &egui::Painter, rect: Rect, radius: f32, c1: Color32, c2: Color32) {
    let corners = [
        (rect.right() - radius, rect.top() + radius, -90.0_f32),
        (rect.right() - radius, rect.bottom() - radius, 0.0),
        (rect.left() + radius, rect.bottom() - radius, 90.0),
        (rect.left() + radius, rect.top() + radius, 180.0),
    ];
    let mut mesh = egui::epaint::Mesh::default();
    let color_at = |p: Pos2| {
        let t = ((p.x - rect.left()) / rect.width() * 0.65 + (p.y - rect.top()) / rect.height() * 0.35).clamp(0.0, 1.0);
        c1.lerp_to_gamma(c2, t)
    };
    mesh.colored_vertex(rect.center(), color_at(rect.center()));
    for (cx, cy, start) in corners {
        for s in 0..=10 {
            let a = (start + 9.0 * s as f32).to_radians();
            let p = Pos2::new(cx + radius * a.cos(), cy + radius * a.sin());
            mesh.colored_vertex(p, color_at(p));
        }
    }
    let n = (mesh.vertices.len() - 1) as u32;
    for i in 1..=n {
        let next = if i == n { 1 } else { i + 1 };
        mesh.add_triangle(0, i, next);
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn pill_button(ui: &mut egui::Ui, label: &str, fill: Color32, fg: Color32, enabled: bool) -> Response {
    let fg = if enabled { fg } else { fg.gamma_multiply(0.5) };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::new(15.0, display_family()), fg);
    let size = galley.size() + egui::vec2(40.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let t = ui.ctx().animate_bool_with_time(response.id, enabled && response.hovered(), 0.12);
    let rect = rect.translate(egui::vec2(0.0, -2.0 * t));
    let painter = ui.painter();
    if t > 0.0 {
        painter.rect_filled(rect.expand(4.0 * t), PILL, fill.gamma_multiply(0.28 * t));
    }
    let body = if enabled { fill.lerp_to_gamma(Color32::WHITE, 0.14 * t) } else { fill.gamma_multiply(0.4) };
    painter.rect_filled(rect, PILL, body);
    painter.galley(rect.center() - galley.size() / 2.0, galley, fg);
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_uppercase(), FontId::new(10.5, display_family()), color);
    let size = galley.size() + egui::vec2(34.0, 12.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, PILL, color.gamma_multiply(0.16));
    painter.rect_stroke(rect, PILL, Stroke::new(1.0_f32, color.gamma_multiply(0.45)), egui::StrokeKind::Inside);
    painter.circle_filled(Pos2::new(rect.left() + 13.0, rect.center().y), 3.0, color);
    painter.galley(Pos2::new(rect.left() + 23.0, rect.center().y - galley.size().y / 2.0), galley, color);
}

fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(display(title, 22.0, pal().text));
        ui.label(RichText::new(subtitle).size(12.0).color(pal().muted));
    });
}

/// Bento card with a colour accent bar. `min_height` of 0 means fit to content.
fn card<R>(ui: &mut egui::Ui, tint: Color32, min_height: f32, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(pal().card)
        .stroke(Stroke::new(1.0_f32, pal().line))
        .corner_radius(CornerRadius::same(24))
        .inner_margin(Margin::same(20))
        .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(pal().shadow) })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(min_height);
            ui.spacing_mut().item_spacing.y = 8.0;
            let (bar, _) = ui.allocate_exact_size(egui::vec2(34.0, 5.0), Sense::hover());
            ui.painter().rect_filled(bar, PILL, tint);
            add_contents(ui)
        })
        .inner
}

fn protocol_card(ui: &mut egui::Ui, num: &str, name: &str, tagline: &str, desc: &str, status: &str, tint: Color32) {
    card(ui, tint, 214.0, |ui| {
        ui.label(display(num, 40.0, tint.gamma_multiply(0.9)));
        ui.label(display(name, 20.0, pal().text));
        ui.label(RichText::new(tagline).size(12.0).color(tint));
        ui.add(egui::Label::new(RichText::new(desc).size(12.5).color(pal().muted)).wrap());
        ui.add_space(2.0);
        chip(ui, status, tint);
    });
}

fn finding_row(ui: &mut egui::Ui, finding: &str) {
    let clean = finding.trim_start_matches("!!").trim();
    let (head, tail) = clean.split_once(": ").unwrap_or((clean, ""));
    egui::Frame::new()
        .fill(pal().warn.gamma_multiply(0.09))
        .stroke(Stroke::new(1.0_f32, pal().warn.gamma_multiply(0.35)))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                chip(ui, "sus", pal().warn);
                ui.add(egui::Label::new(display(head, 15.0, pal().text)).wrap());
            });
            if !tail.is_empty() {
                ui.add(egui::Label::new(RichText::new(tail).monospace().size(12.0).color(pal().muted)).wrap());
            }
        });
}

/// Width a pill button will take, so rows can share space exactly.
fn pill_width(ui: &egui::Ui, label: &str) -> f32 {
    ui.painter().layout_no_wrap(label.to_owned(), FontId::new(15.0, display_family()), Color32::WHITE).size().x + 40.0
}

/// Text field + pill button on one row. Returns true when the button is pressed.
fn input_row(ui: &mut egui::Ui, value: &mut String, hint: &str, button: &str, tint: Color32, enabled: bool) -> bool {
    let mut clicked = false;
    let pad = 14.0;
    let btn_w = pill_width(ui, button);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        // desired_width is the text area only, egui adds the margin on both sides.
        // A few points of slack: the field draws a stroke outside its measured size.
        let edit_w = (ui.available_width() - btn_w - ui.spacing().item_spacing.x - 2.0 * pad - 6.0).max(80.0);
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text(hint)
                .margin(Margin::symmetric(pad as i8, 11))
                .desired_width(edit_w),
        );
        let ok = enabled && !value.trim().is_empty();
        clicked = pill_button(ui, button, tint, pal().on_accent, ok).clicked();
    });
    clicked
}

fn terminal(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(pal().term_bg)
        .stroke(Stroke::new(1.0_f32, pal().line))
        .corner_radius(CornerRadius::same(22))
        .inner_margin(Margin::symmetric(20, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                for c in [pal().iris, pal().warn, pal().good] {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 5.0, c.gamma_multiply(0.85));
                }
            });
            ui.add_space(4.0);
            add_contents(ui);
        });
}

fn log_line(ui: &mut egui::Ui, line: &str) {
    let sus = line.contains("!!");
    let has_stamp = line.get(4..5) == Some("-") && line.get(10..11) == Some(" ") && line.len() > 21;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let msg = if has_stamp {
            ui.label(RichText::new(&line[11..19]).monospace().size(11.5).color(pal().blue));
            line[19..].trim()
        } else {
            line
        };
        ui.add(egui::Label::new(
            RichText::new(msg).monospace().size(11.5).color(if sus { pal().warn } else { pal().muted }),
        ).wrap());
    });
}

fn serpent_mark(ui: &mut egui::Ui, time: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), Sense::hover());
    let painter = ui.painter();
    let glow = 0.5 + 0.5 * (time * 1.6).sin();
    for (r, a) in [(30.0, 0.05), (26.0, 0.08), (23.0, 0.12)] {
        painter.circle_filled(rect.center(), r + glow * 2.0, pal().good.gamma_multiply(a));
    }
    painter.circle_filled(rect.center(), 21.0, pal().card);
    painter.circle_stroke(rect.center(), 20.0, Stroke::new(1.5_f32, pal().good.gamma_multiply(0.7)));

    let point = |x: f32, y: f32| Pos2::new(rect.left() + x, rect.top() + y);
    let ash = pal().text;
    for (y, half_width) in [(12.0, 12.5), (22.0, 6.5), (32.0, 12.5)] {
        if y == 22.0 {
            painter.line_segment([point(5.0, y), point(22.0 - half_width, y)], Stroke::new(2.2_f32, ash));
            painter.line_segment([point(22.0 + half_width, y), point(39.0, y)], Stroke::new(2.2_f32, ash));
        } else {
            painter.line_segment([point(7.0, y), point(37.0, y)], Stroke::new(2.2_f32, ash));
        }
    }
    painter.add(egui::Shape::convex_polygon(
        vec![
            point(22.0, 14.5), point(18.0, 19.0), point(17.0, 22.0), point(19.0, 26.0),
            point(22.0, 29.0), point(25.0, 26.0), point(27.0, 22.0), point(26.0, 19.0),
        ],
        pal().card,
        Stroke::new(1.8_f32, pal().good),
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![point(22.0, 17.0), point(19.0, 22.0), point(22.0, 26.0), point(25.0, 22.0)],
        pal().text,
        Stroke::NONE,
    ));
    painter.circle_filled(point(22.0, 22.0), 1.5, Color32::from_rgb(242, 200, 121));
}

fn read_optional_lines(path: &Path) -> Result<Vec<String>, String> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents.lines().map(str::to_owned).collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Vec2;
    use sheshnag_core::platform::linux::Linux;
    use sheshnag_core::runner::{CmdOut, Runner};

    /// Runs nothing and reports success, so UI tests never start a real program.
    struct NullRunner;
    impl Runner for NullRunner {
        fn run(&self, _: &str, _: &[&str]) -> CmdOut {
            CmdOut { code: 0, stdout: String::new(), stderr: String::new() }
        }
        fn run_input(&self, p: &str, a: &[&str], _: &str) -> CmdOut {
            self.run(p, a)
        }
        fn spawn(&self, _: &str, _: &[&str]) {}
    }

    struct Fixture {
        _tmp: tempfile::TempDir,
        home: PathBuf,
        ctx: egui::Context,
        app: SheshnagApp,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let (h, root) = (home.clone(), tmp.path().join("root"));
        let factory: EngineFactory = Arc::new(move || {
            Engine::new(Box::new(Linux::with_parts(h.clone(), root.clone(), Arc::new(NullRunner))))
        });
        let ctx = egui::Context::default();
        let app = SheshnagApp::with_factory(&ctx, factory);
        Fixture { _tmp: tmp, home, ctx, app }
    }

    /// Run one frame at `size` and return every filled rectangle the UI painted.
    fn frame_rects(f: &mut Fixture, size: Vec2) -> Vec<Rect> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        };
        let out = f.ctx.run(input, |ctx| f.app.ui(ctx));
        let mut rects = Vec::new();
        fn walk(shape: &egui::Shape, rects: &mut Vec<Rect>) {
            match shape {
                egui::Shape::Rect(r) => rects.push(r.rect),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, rects)),
                egui::Shape::Mesh(m) => rects.push(m.calc_bounds()),
                _ => {}
            }
        }
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut rects);
        }
        rects
    }

    /// Everything below the header must stay inside the content column. A single widget that is
    /// too wide widens the whole scroll area, so full-width panels end up wider than the cards.
    #[test]
    fn nothing_overshoots_the_content_column_at_any_window_width() {
        // egui snaps odd-width columns to half points, which drifts by up to about 2pt. The bug this
        // test guards against was 22pt, so a small tolerance still catches it.
        const TOLERANCE: f32 = 2.5;
        for width in [900.0_f32, 1000.0, 1180.0, 1400.0, 1920.0] {
            let side = ((width - 1120.0) / 2.0).clamp(28.0, 127.0);
            let right_edge = width - side;
            for with_output in [false, true] {
                let mut f = fixture();
                if with_output {
                    f.app.action_output = Some("Scan finished. No findings reported.".to_owned());
                    f.app.candidates = vec![Target::Process { pid: 9, command: "x".into() }];
                    f.app.findings = vec!["!! launch agent NEW: /a/very/long/path/that/keeps/going/and/going/file.plist".into()];
                }
                // Tall enough that every section is on screen, since offscreen shapes are not emitted.
                let size = Vec2::new(width, 3600.0);
                frame_rects(&mut f, size);
                let rects = frame_rects(&mut f, size);
                assert!(rects.iter().any(|r| r.top() > 1500.0 && r.width() > 300.0), "lower sections were not painted");
                for r in rects
                    .iter()
                    .filter(|r| r.width() > 150.0 && r.height() > 40.0 && r.left() > 1.0 && r.top() >= 80.0)
                {
                    assert!(
                        r.right() <= right_edge + TOLERANCE,
                        "{r:?} ends past the content edge {right_edge} in a {width}px window (output: {with_output})"
                    );
                }
            }
        }
    }

    #[test]
    fn mood_follows_findings_and_baseline() {
        let mut f = fixture();
        assert!(!f.app.baseline_exists);
        fs::create_dir_all(f.home.join(".sheshnag/baseline")).unwrap();
        fs::write(f.home.join(".sheshnag/baseline/agents"), "").unwrap();
        fs::write(f.home.join(".sheshnag/findings.now"), "!! startup unit NEW: /x\n").unwrap();
        f.app.refresh_data();
        assert!(f.app.baseline_exists);
        assert_eq!(f.app.findings.len(), 1);
    }

    #[test]
    fn the_agni_dialog_names_exactly_what_will_be_destroyed() {
        let mut f = fixture();
        f.app.confirm = Some(vec![Target::Process { pid: 42, command: "xmrig --x".into() }]);
        let size = Vec2::new(1180.0, 860.0);
        frame_rects(&mut f, size);
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), ..Default::default() };
        let out = f.ctx.run(input, |ctx| f.app.ui(ctx));
        let mut text = String::new();
        fn collect(s: &egui::Shape, t: &mut String) {
            match s {
                egui::Shape::Text(tx) => t.push_str(&tx.galley.job.text),
                egui::Shape::Vec(v) => v.iter().for_each(|x| collect(x, t)),
                _ => {}
            }
            t.push('\n');
        }
        out.shapes.iter().for_each(|c| collect(&c.shape, &mut text));
        assert!(text.contains("Agni needs your word"), "dialog missing");
        assert!(text.contains("process 42"), "target not named");
        assert!(f.app.confirm.is_some(), "the dialog must stay open until the person decides");
    }

    #[test]
    fn theme_choice_is_parsed_and_defaults_to_system() {
        assert!(matches!(ThemeMode::parse("dark"), ThemeMode::Dark));
        assert!(matches!(ThemeMode::parse("light"), ThemeMode::Light));
        assert!(matches!(ThemeMode::parse("anything else"), ThemeMode::System));
    }

    #[test]
    fn log_lines_with_and_without_a_timestamp_are_both_handled() {
        let mut f = fixture();
        fs::create_dir_all(f.home.join(".sheshnag")).unwrap();
        fs::write(
            f.home.join(".sheshnag/log"),
            "2026-10-16 08:07:08  !! launch agent CHANGED: /x\nplain line without a stamp\nshort\n",
        )
        .unwrap();
        f.app.refresh_data();
        frame_rects(&mut f, Vec2::new(1180.0, 860.0)); // must not panic on odd lines
        assert_eq!(f.app.log_lines.len(), 3);
    }
}
