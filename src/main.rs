// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ssh;
mod ui;

use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Stroke};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use ui::{icon, theme::*};

const POLL_EVERY: Duration = Duration::from_secs(10);
const DEFAULT_SCRIPT: &str = "export DEBIAN_FRONTEND=noninteractive\napt-get update\napt-get install -y htop curl\n";

#[derive(Clone, Serialize, Deserialize)]
struct Pi {
    id: u64,
    name: String,
    host: String,
    port: u16,
    user: String,
    /// SHA256 fingerprint of the Pi's SSH host key, stored on first contact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host_key: Option<String>,
}

#[derive(Clone, Default, PartialEq)]
enum State {
    #[default]
    Checking,
    SettingUp,
    Online,
    Offline,
    SetupFailed,
}

#[derive(Clone, Default)]
struct Status {
    state: State,
    last_seen: Option<u64>,
    error: Option<String>,
    stats: HashMap<String, String>,
    log: String,
}

type Kv = HashMap<String, String>;

/// Results of the diagnostics window for one Pi.
#[derive(Clone, Default)]
struct Diag {
    checks_running: bool,
    checks: Option<Result<Kv, String>>,
    speed_running: bool,
    speed: Option<Result<Kv, String>>,
}

#[derive(Default)]
struct Data {
    pis: Vec<Pi>,
    status: HashMap<u64, Status>,
    diags: HashMap<u64, Diag>,
}

/// Runs a diagnostics job for `pi` in the background; `slot` picks which
/// running-flag / result field of `Diag` it fills.
fn run_diag(
    shared: &Arc<Shared>,
    ctx: &egui::Context,
    pi: &Pi,
    speed: bool,
) {
    {
        let mut d = shared.data.lock().unwrap();
        let g = d.diags.entry(pi.id).or_default();
        let running = if speed { &mut g.speed_running } else { &mut g.checks_running };
        if *running {
            return;
        }
        *running = true;
    }
    let (shared, ctx, pi) = (shared.clone(), ctx.clone(), pi.clone());
    thread::spawn(move || {
        let result = if speed {
            ssh::speedtest(&pi.host, pi.port, &pi.user, &shared.key, pi.host_key.as_deref())
        } else {
            ssh::diagnostics(&pi.host, pi.port, &pi.user, &shared.key, pi.host_key.as_deref())
        };
        {
            let mut d = shared.data.lock().unwrap();
            let g = d.diags.entry(pi.id).or_default();
            if speed {
                g.speed_running = false;
                g.speed = Some(result);
            } else {
                g.checks_running = false;
                g.checks = Some(result);
            }
        }
        ctx.request_repaint();
    });
}

/// Everything the UI and the background threads share.
struct Shared {
    data: Mutex<Data>,
    dir: PathBuf,
    key: PathBuf,
    pubkey: String,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

impl Shared {
    fn save(&self) {
        let json = serde_json::to_string_pretty(&self.data.lock().unwrap().pis).unwrap();
        let _ = std::fs::write(self.dir.join("pis.json"), json);
    }

    fn poll(&self, pi: &Pi) {
        let result = ssh::poll(&pi.host, pi.port, &pi.user, &self.key, pi.host_key.as_deref());
        let mut learned_key = false;
        {
            let mut d = self.data.lock().unwrap();
            let Some(stored) = d.pis.iter_mut().find(|p| p.id == pi.id) else {
                return; // removed while we were polling
            };
            // Pis added before host-key checking existed: trust the key we see now.
            if let Ok((_, fp)) = &result {
                if stored.host_key.is_none() && !fp.is_empty() {
                    stored.host_key = Some(fp.clone());
                    learned_key = true;
                }
            }
            let s = d.status.entry(pi.id).or_default();
            match result {
                Ok((stats, _)) => {
                    s.state = State::Online;
                    s.last_seen = Some(now());
                    s.error = None;
                    s.stats = stats;
                }
                Err(err) => {
                    s.state = State::Offline;
                    s.error = Some(err);
                }
            }
        }
        if learned_key {
            self.save();
        }
    }

    fn poll_all(self: &Arc<Self>, ctx: &egui::Context) {
        let pis: Vec<Pi> = {
            let d = self.data.lock().unwrap();
            d.pis
                .iter()
                .filter(|p| {
                    let st = d.status.get(&p.id).map(|s| &s.state);
                    !matches!(st, Some(State::SettingUp | State::SetupFailed))
                })
                .cloned()
                .collect()
        };
        // Every Pi in parallel; repaint as soon as each one answers.
        let jobs: Vec<_> = pis
            .into_iter()
            .map(|pi| {
                let (me, ctx) = (self.clone(), ctx.clone());
                thread::spawn(move || {
                    me.poll(&pi);
                    ctx.request_repaint();
                })
            })
            .collect();
        for job in jobs {
            let _ = job.join();
        }
    }
}

/// Creates the app's own SSH key pair on first run.
fn ensure_key(dir: &Path) -> Result<(PathBuf, String), String> {
    let key = dir.join("id_ed25519");
    if !key.exists() {
        let mut cmd = std::process::Command::new("ssh-keygen");
        cmd.args(["-q", "-t", "ed25519", "-N", "", "-C", "homelab", "-f"])
            .arg(&key);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let ok = cmd.status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            let hint = if cfg!(windows) {
                "Install the Windows OpenSSH client (Settings → System → Optional features)"
            } else {
                "Install the OpenSSH client (e.g. `sudo apt install openssh-client`)"
            };
            return Err(format!("Couldn't run ssh-keygen. {hint} and restart the app."));
        }
    }
    let pubkey = std::fs::read_to_string(key.with_extension("pub")).map_err(|e| e.to_string())?;
    Ok((key, pubkey))
}

/// Per-user settings folder: %APPDATA% on Windows, ~/Library/Application Support
/// on macOS, $XDG_CONFIG_HOME or ~/.config on Linux.
fn config_base() -> PathBuf {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = || var("HOME").unwrap_or_else(|| PathBuf::from("."));
    if cfg!(windows) {
        var("APPDATA").unwrap_or_else(|| PathBuf::from("."))
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else {
        var("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
    }
}

fn start(ctx: &egui::Context) -> Result<Arc<Shared>, String> {
    let base = config_base();
    let dir = base.join("homelab");
    // One-time move from the app's old name, keeping your Pis and SSH key.
    let old = base.join("pi-dashboard");
    if old.exists() && !dir.exists() {
        let _ = std::fs::rename(&old, &dir);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let (key, pubkey) = ensure_key(&dir)?;
    let pis: Vec<Pi> = std::fs::read_to_string(dir.join("pis.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let shared = Arc::new(Shared {
        data: Mutex::new(Data { pis, ..Default::default() }),
        dir,
        key,
        pubkey,
    });

    let (poller, ctx) = (shared.clone(), ctx.clone());
    thread::spawn(move || loop {
        poller.poll_all(&ctx);
        thread::sleep(POLL_EVERY);
    });
    Ok(shared)
}

struct Form {
    name: String,
    host: String,
    port: u16,
    user: String,
    password: String,
    script: String,
    error: Option<String>,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: 22,
            user: "pi".into(),
            password: String::new(),
            script: DEFAULT_SCRIPT.into(),
            error: None,
        }
    }
}

struct App {
    shared: Result<Arc<Shared>, String>,
    form: Form,
    show_form: bool,
    confirm_remove: Option<u64>,
    open_logs: HashSet<u64>,
    diag_open: Option<u64>,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        ui::install(&cc.egui_ctx);
        Self {
            shared: start(&cc.egui_ctx),
            form: Form::default(),
            show_form: false,
            confirm_remove: None,
            open_logs: HashSet::new(),
            diag_open: None,
        }
    }

    fn open_diag(&mut self, shared: &Arc<Shared>, ctx: &egui::Context, pi: &Pi) {
        self.diag_open = Some(pi.id);
        let needs_run = shared.data.lock().unwrap().diags.get(&pi.id).map_or(true, |g| g.checks.is_none());
        if needs_run {
            run_diag(shared, ctx, pi, false);
        }
    }

    // ───────────────────────── diagnostics dialog ─────────────────────────

    fn diag_ui(&mut self, ui: &mut egui::Ui, shared: &Arc<Shared>, pi: &Pi, g: &Diag) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon::STETHOSCOPE).size(18.0).color(ACCENT));
            ui.label(RichText::new("Diagnostics").size(19.0).strong().color(TEXT));
            ui.label(RichText::new(format!("·  {}", pi.name)).size(15.0).color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::icon_button(ui, icon::XMARK, "Close").clicked() {
                    self.diag_open = None;
                }
            });
        });
        ui.add_space(14.0);

        let max_h = (ui.ctx().screen_rect().height() - 150.0).max(200.0);
        egui::ScrollArea::vertical().id_salt("diag_scroll").max_height(max_h).show(ui, |ui| {
            if ui.available_width() >= 720.0 {
                ui.columns(2, |c| {
                    c[0].spacing_mut().item_spacing.y = 8.0;
                    self.diag_left(&mut c[0], shared, pi, g);
                    self.diag_right(&mut c[1], shared, pi, g);
                });
            } else {
                self.diag_left(ui, shared, pi, g);
                ui.add_space(16.0);
                self.diag_right(ui, shared, pi, g);
            }
        });
    }

    /// Speedtest + network checks.
    fn diag_left(&mut self, ui: &mut egui::Ui, shared: &Arc<Shared>, pi: &Pi, g: &Diag) {
            ui::section_title(ui, "Speedtest");
            ui::inset(ui, |ui| {
                match (&g.speed, g.speed_running) {
                    (_, true) => ui::notice(ui, None, "Measuring LAN and internet speed… (up to ~40 s)", ACCENT),
                    (None, false) => {
                        ui.label(RichText::new("Measures PC ↔ Pi over your network, and the Pi's own internet connection.").color(MUTED));
                    }
                    (Some(Err(err)), false) => ui::notice(ui, Some(icon::WARN), err, RED),
                    (Some(Ok(m)), false) => speed_results(ui, m),
                }
                ui.add_space(8.0);
                ui.add_enabled_ui(!g.speed_running, |ui| {
                    let text = if g.speed.is_some() { "Run again" } else { "Run speedtest" };
                    if ui::primary_button(ui, &format!("{}  {text}", icon::PLAY)).clicked() {
                        run_diag(shared, ui.ctx(), pi, true);
                    }
                });
            });
            ui.add_space(16.0);
            match &g.checks {
                Some(Ok(m)) => network_results(ui, m),
                Some(Err(err)) => ui::notice(ui, Some(icon::WARN), err, RED),
                None => ui::notice(ui, None, "Running checks…", MUTED),
            }
    }

    /// Health checks + re-run button.
    fn diag_right(&mut self, ui: &mut egui::Ui, shared: &Arc<Shared>, pi: &Pi, g: &Diag) {
        if let Some(Ok(m)) = &g.checks {
            health_results(ui, m);
        } else if g.checks.is_none() {
            ui::section_title(ui, "Health");
            ui::notice(ui, None, "Running checks…", MUTED);
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!g.checks_running, |ui| {
                if ui::secondary_button(ui, &format!("{}  Re-run checks", icon::REFRESH)).clicked() {
                    run_diag(shared, ui.ctx(), pi, false);
                }
            });
            if g.checks_running && g.checks.is_some() {
                ui.add(egui::Spinner::new().size(14.0).color(MUTED));
            }
        });
    }

    fn add_pi(&mut self, shared: &Arc<Shared>, ctx: &egui::Context) {
        let f = &mut self.form;
        if f.host.trim().is_empty() || f.user.trim().is_empty() || f.password.is_empty() {
            f.error = Some("Host, user and password are required.".into());
            return;
        }
        let pi = Pi {
            id: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64,
            name: if f.name.trim().is_empty() { f.host.trim().into() } else { f.name.trim().into() },
            host: f.host.trim().into(),
            port: f.port,
            user: f.user.trim().into(),
            host_key: None,
        };
        let (password, script) = (std::mem::take(&mut f.password), f.script.clone());
        *f = Form::default();
        self.show_form = false;
        self.open_logs.insert(pi.id);

        {
            let mut d = shared.data.lock().unwrap();
            d.status.insert(
                pi.id,
                Status { state: State::SettingUp, log: "connecting…\n".into(), ..Default::default() },
            );
            d.pis.push(pi.clone());
        }
        shared.save();

        let (shared, ctx) = (shared.clone(), ctx.clone());
        thread::spawn(move || {
            let mut pi = pi;
            let result = ssh::setup(&pi.host, pi.port, &pi.user, &password, &shared.pubkey, &shared.key, &script);
            {
                let mut d = shared.data.lock().unwrap();
                if let Ok((_, fp)) = &result {
                    pi.host_key = Some(fp.clone());
                    if let Some(stored) = d.pis.iter_mut().find(|p| p.id == pi.id) {
                        stored.host_key = Some(fp.clone());
                    }
                }
                let s = d.status.entry(pi.id).or_default();
                match result {
                    Ok((log, _)) => {
                        s.state = State::Checking;
                        s.log = log;
                    }
                    Err(log) => {
                        s.state = State::SetupFailed;
                        s.error = Some(log.lines().last().unwrap_or("setup failed").to_string());
                        s.log = log;
                    }
                }
            }
            shared.save();
            ctx.request_repaint();
            shared.poll(&pi);
            ctx.request_repaint();
        });
    }

    // ───────────────────────── add-Pi dialog ─────────────────────────

    fn form_ui(&mut self, ui: &mut egui::Ui, shared: &Arc<Shared>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon::SERVER).size(18.0).color(ACCENT));
            ui.label(RichText::new("Add a Pi").size(19.0).strong().color(TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui::icon_button(ui, icon::XMARK, "Close").clicked() {
                    self.show_form = false;
                }
            });
        });
        ui.add_space(4.0);
        ui.label(RichText::new("The app logs in once with your password, installs its own SSH key and runs your script.").color(MUTED));
        ui.add_space(14.0);

        ui::field_label(ui, icon::TAG, "Name");
        ui.add(ui::text_input(&mut self.form.name).hint_text("living-room-pi"));
        ui.add_space(10.0);

        ui.columns(2, |c| {
            ui::field_label(&mut c[0], icon::NETWORK, "Host / IP");
            c[0].add(ui::text_input(&mut self.form.host).hint_text("192.168.1.50"));
            ui::field_label(&mut c[1], icon::PLUG, "Port");
            c[1].add_sized(
                [c[1].available_width(), 34.0],
                egui::DragValue::new(&mut self.form.port).range(1..=65535),
            );
        });
        ui.add_space(10.0);

        ui.columns(2, |c| {
            ui::field_label(&mut c[0], icon::USER, "User");
            c[0].add(ui::text_input(&mut self.form.user));
            ui::field_label(&mut c[1], icon::LOCK, "Password");
            c[1].add(ui::text_input(&mut self.form.password).password(true));
        });
        ui.add_space(10.0);

        ui::field_label(ui, icon::TERMINAL, "Install script · runs once as root");
        ui.add(
            egui::TextEdit::multiline(&mut self.form.script)
                .code_editor()
                .desired_rows(5)
                .desired_width(f32::INFINITY)
                .margin(egui::vec2(10.0, 8.0)),
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("{}  Your password is only used for setup and is never saved.", icon::SHIELD))
                .small()
                .color(MUTED),
        );

        if let Some(err) = &self.form.error {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("{}  {err}", icon::WARN)).color(RED));
        }

        ui.add_space(16.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui::primary_button(ui, &format!("{}  Connect & install", icon::BOLT)).clicked() {
                let ctx = ui.ctx().clone();
                self.add_pi(shared, &ctx);
            }
            if ui::secondary_button(ui, "Cancel").clicked() {
                self.show_form = false;
            }
        });
    }

    // ───────────────────────── Pi card ─────────────────────────

    fn card(&mut self, ui: &mut egui::Ui, shared: &Arc<Shared>, pi: &Pi, s: &Status, width: f32) {
        let (state_text, state_color) = match s.state {
            State::Online => ("Online", GREEN),
            State::Offline => ("Offline", RED),
            State::SettingUp => ("Setting up", AMBER),
            State::SetupFailed => ("Setup failed", RED),
            State::Checking => ("Checking", MUTED),
        };

        Frame::none()
            .fill(CARD)
            .stroke(Stroke::new(1.0_f32, LINE))
            .rounding(14.0)
            .inner_margin(Margin::same(CARD_PAD))
            .show(ui, |ui| {
                ui.set_width(width - CARD_PAD * 2.0);
                // The grid row is horizontal; the card's own content stacks top-down.
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;

                // Header: icon · name/host · status pill
                ui.horizontal(|ui| {
                    ui::icon_tile(ui, icon::SERVER, state_color);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new(&pi.name).size(16.0).strong().color(TEXT));
                        let port = if pi.port == 22 { String::new() } else { format!(":{}", pi.port) };
                        ui.label(RichText::new(format!("{}@{}{port}", pi.user, pi.host)).small().color(MUTED));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui::pill(ui, state_text, state_color, s.state == State::SettingUp || s.state == State::Checking);
                    });
                });
                ui.add_space(12.0);

                // Body
                match s.state {
                    State::Online => metrics(ui, &s.stats),
                    State::SettingUp => {
                        ui::notice(ui, None, "Connecting and running the install script…", AMBER);
                    }
                    State::Checking => {
                        ui::notice(ui, None, "Checking…", MUTED);
                    }
                    State::Offline => {
                        let msg = match s.last_seen {
                            Some(t) => format!("Unreachable · last seen {} ago", fmt_duration(now().saturating_sub(t))),
                            None => "Unreachable".into(),
                        };
                        ui::notice(ui, Some(icon::PLUG), &msg, RED);
                    }
                    State::SetupFailed => {
                        ui::notice(ui, Some(icon::WARN), "Setup failed. Check the log, remove and try again.", RED);
                    }
                }
                if let Some(err) = &s.error {
                    if s.state != State::Online {
                        ui.label(RichText::new(err).small().color(MUTED));
                    }
                }

                // Log
                let log_open = self.open_logs.contains(&pi.id);
                if log_open && !s.log.is_empty() {
                    ui.add_space(4.0);
                    Frame::none().fill(BG).rounding(8.0).inner_margin(Margin::same(10.0)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical()
                            .id_salt(("log", pi.id))
                            .max_height(170.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                ui.label(RichText::new(&s.log).monospace().size(11.5).color(MUTED));
                            });
                    });
                }

                // Footer
                ui.add_space(8.0);
                ui::divider(ui);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let up = s.stats.get("uptime").and_then(|u| u.parse::<f64>().ok());
                    if let (State::Online, Some(up)) = (&s.state, up) {
                        ui.label(RichText::new(format!("{}  up {}", icon::CLOCK, fmt_duration(up as u64))).small().color(MUTED));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.confirm_remove == Some(pi.id) {
                            if ui::danger_button(ui, "Remove").clicked() {
                                self.confirm_remove = None;
                                {
                                    let mut d = shared.data.lock().unwrap();
                                    d.pis.retain(|p| p.id != pi.id);
                                    d.status.remove(&pi.id);
                                }
                                shared.save();
                            }
                            if ui::secondary_button(ui, "Cancel").clicked() {
                                self.confirm_remove = None;
                            }
                        } else {
                            if ui::icon_button(ui, icon::TRASH, "Remove").clicked() {
                                self.confirm_remove = Some(pi.id);
                            }
                            if s.state != State::SettingUp
                                && s.state != State::SetupFailed
                                && ui::icon_button(ui, icon::STETHOSCOPE, "Diagnostics & speedtest").clicked()
                            {
                                let ctx = ui.ctx().clone();
                                self.open_diag(shared, &ctx, pi);
                            }
                            if !s.log.is_empty() {
                                let tip = if log_open { "Hide setup log" } else { "Show setup log" };
                                if ui::icon_button(ui, icon::TERMINAL, tip).clicked() {
                                    if log_open {
                                        self.open_logs.remove(&pi.id);
                                    } else {
                                        self.open_logs.insert(pi.id);
                                    }
                                }
                            }
                        }
                    });
                });
                });
            });
    }
}

fn metrics(ui: &mut egui::Ui, st: &HashMap<String, String>) {
    let num = |k: &str| st.get(k).and_then(|v| v.parse::<f64>().ok());

    let temp = num("temp").map(|t| t / 1000.0);
    let cpus = num("cpus").unwrap_or(1.0).max(1.0);
    let load: f64 = st.get("load").and_then(|l| l.split(' ').next()?.parse().ok()).unwrap_or(0.0);
    let mem = match (num("mem_total"), num("mem_avail")) {
        (Some(t), Some(a)) => Some((t * 1024.0, (t - a) * 1024.0)),
        _ => None,
    };
    let disk: Vec<f64> = st.get("disk").map(|d| d.split(' ').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();

    ui.columns(2, |c| {
        match temp {
            Some(t) => ui::metric(&mut c[0], icon::TEMP, "CPU temp", &format!("{t:.1} °C"), t / 85.0, level(t, 60.0, 75.0)),
            None => ui::metric(&mut c[0], icon::TEMP, "CPU temp", "—", 0.0, MUTED),
        }
        let lf = load / cpus;
        ui::metric(&mut c[1], icon::GAUGE, "Load", &format!("{load:.2}  ·  {cpus} cores"), lf, level(lf * 100.0, 60.0, 85.0));
    });
    ui.add_space(10.0);
    ui.columns(2, |c| {
        if let Some((t, u)) = mem {
            let f = u / t;
            ui::metric(&mut c[0], icon::MEMORY, "Memory", &format!("{} / {}", fmt_bytes(u), fmt_bytes(t)), f, level(f * 100.0, 60.0, 85.0));
        }
        if let [t, u] = disk[..] {
            let f = u / t;
            ui::metric(&mut c[1], icon::DISK, "Disk", &format!("{} / {}", fmt_bytes(u), fmt_bytes(t)), f, level(f * 100.0, 70.0, 90.0));
        }
    });

    let info: Vec<&str> = ["model", "os"].iter().filter_map(|k| st.get(*k)).map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    if !info.is_empty() {
        ui.add_space(10.0);
        ui.label(RichText::new(format!("{}  {}", icon::CHIP, info.join("  ·  "))).small().color(MUTED));
    }
}

fn speed_results(ui: &mut egui::Ui, m: &Kv) {
    let get = |k: &str| m.get(k).and_then(|v| v.parse::<f64>().ok());
    // curl reports bytes per second
    let inet = |k: &str| get(k).filter(|v| *v > 0.0).map(|v| format!("{:.1}", v * 8.0 / 1e6)).unwrap_or("—".into());
    let lan = |k: &str| m.get(k).cloned().unwrap_or("—".into());

    ui.label(RichText::new(format!("{}  PC ↔ Pi (your network)", icon::DESKTOP)).color(TEXT));
    ui.add_space(4.0);
    ui.columns(2, |c| {
        ui::speed_tile(&mut c[0], icon::DOWNLOAD, "Pi → PC", &lan("lan_down"), "Mbps");
        ui::speed_tile(&mut c[1], icon::UPLOAD, "PC → Pi", &lan("lan_up"), "Mbps");
    });
    ui.add_space(12.0);
    ui::divider(ui);
    ui.add_space(10.0);
    ui.label(RichText::new(format!("{}  Pi ↔ internet", icon::GLOBE)).color(TEXT));
    ui.add_space(4.0);
    if let Some(err) = m.get("inet_error") {
        ui::notice(ui, Some(icon::WARN), err, AMBER);
    } else {
        ui.columns(3, |c| {
            ui::speed_tile(&mut c[0], icon::DOWNLOAD, "Download", &inet("inet_down"), "Mbps");
            ui::speed_tile(&mut c[1], icon::UPLOAD, "Upload", &inet("inet_up"), "Mbps");
            let ping = get("inet_ping").map(|p| format!("{p:.0}")).unwrap_or("—".into());
            ui::speed_tile(&mut c[2], icon::STOPWATCH, "Ping", &ping, "ms");
        });
    }
}

fn network_results(ui: &mut egui::Ui, m: &Kv) {
    let s = |k: &str| m.get(k).map(|v| v.trim()).unwrap_or("");
    let f = |k: &str| s(k).parse::<f64>().ok();
    let ms = |v: Option<f64>, good: f64, ok: f64, missing: &str| match v {
        Some(v) => (format!("{v:.1} ms"), level(v, good, ok)),
        None => (missing.to_string(), RED),
    };

    ui::section_title(ui, "Network");
    ui::inset(ui, |ui| {
        let (v, c) = ms(f("ssh_ms"), 120.0, 300.0, "—");
        ui::diag_row(ui, icon::DESKTOP, "SSH response from this PC", &v, c);
        let gw = s("gateway");
        let (v, c) = if gw.is_empty() { ("no default route".into(), RED) } else { ms(f("gw_ping"), 10.0, 50.0, "no reply") };
        ui::diag_row(ui, icon::ROUTER, &format!("Router ping {}", if gw.is_empty() { "" } else { gw }), &v, c);
        let (v, c) = ms(f("net_ping"), 40.0, 100.0, "no internet");
        ui::diag_row(ui, icon::GLOBE, "Internet ping (1.1.1.1)", &v, c);
        let (v, c) = match s("dns_ms") {
            "fail" | "" => ("failed".to_string(), RED),
            d => ms(d.parse().ok(), 100.0, 500.0, "failed"),
        };
        ui::diag_row(ui, icon::SEARCH, "DNS lookup", &v, c);
        let (v, c) = match f("wifi") {
            Some(dbm) => (
                format!("{dbm:.0} dBm"),
                if dbm >= -60.0 { GREEN } else if dbm >= -70.0 { AMBER } else { RED },
            ),
            None => ("wired / not in use".into(), MUTED),
        };
        ui::diag_row(ui, icon::WIFI, "Wi-Fi signal", &v, c);
        ui::diag_row(ui, icon::NETWORK, "IP address", s("ip"), MUTED);
    });
}

fn health_results(ui: &mut egui::Ui, m: &Kv) {
    let s = |k: &str| m.get(k).map(|v| v.trim()).unwrap_or("");
    let f = |k: &str| s(k).parse::<f64>().ok();

    ui::section_title(ui, "Health");
    ui::inset(ui, |ui| {
        let (v, c) = throttled(s("throttled"));
        ui::diag_row(ui, icon::BOLT, "Power & throttling", &v, c);
        let failed = s("failed");
        let (v, c) = if failed.is_empty() { ("none".into(), GREEN) } else { (failed.replace(',', ", "), RED) };
        ui::diag_row(ui, icon::LIST_CHECK, "Failed services", &v, c);
        let (v, c) = match f("updates") {
            Some(0.0) => ("up to date".into(), GREEN),
            Some(n) => (format!("{n:.0} pending"), AMBER),
            None => ("—".into(), MUTED),
        };
        ui::diag_row(ui, icon::BOX, "Package updates", &v, c);
        let (v, c) = match f("errors") {
            Some(0.0) => ("none".into(), GREEN),
            Some(n) => (format!("{n:.0}"), if n < 20.0 { AMBER } else { RED }),
            None => ("—".into(), MUTED),
        };
        ui::diag_row(ui, icon::FILE, "Errors in log since boot", &v, c);
        let (v, c): (String, _) = match s("ntp") {
            "yes" => ("synced".into(), GREEN),
            "no" => ("not synced".into(), AMBER),
            _ => ("—".into(), MUTED),
        };
        ui::diag_row(ui, icon::CLOCK, "Time sync", &v, c);
        ui::diag_row(ui, icon::SWAP, "Swap used", s("swap"), MUTED);
        ui::diag_row(ui, icon::CHIP, "Kernel", s("kernel"), MUTED);
        ui::diag_row(ui, icon::CHART, "Top processes", s("top"), MUTED);
    });
}

/// Decodes `vcgencmd get_throttled` (Raspberry Pi firmware flags).
fn throttled(raw: &str) -> (String, egui::Color32) {
    let Ok(bits) = u32::from_str_radix(raw.trim_start_matches("0x"), 16) else {
        return ("n/a".into(), MUTED);
    };
    if bits & 0x1 != 0 {
        ("under-voltage now!".into(), RED)
    } else if bits & 0x4 != 0 {
        ("throttled now".into(), RED)
    } else if bits & 0x8 != 0 {
        ("temperature limit".into(), AMBER)
    } else if bits & 0x1_0000 != 0 {
        ("under-voltage since boot".into(), AMBER)
    } else if bits & 0x4_0000 != 0 {
        ("throttled since boot".into(), AMBER)
    } else if bits == 0 {
        ("OK".into(), GREEN)
    } else {
        (format!("flags {raw}"), AMBER)
    }
}

fn level(v: f64, warn: f64, hot: f64) -> egui::Color32 {
    if v >= hot {
        RED
    } else if v >= warn {
        AMBER
    } else {
        GREEN
    }
}

fn fmt_bytes(mut b: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0;
    while b >= 1024.0 && i < units.len() - 1 {
        b /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b:.0} B") } else { format!("{b:.1} {}", units[i]) }
}

fn fmt_duration(s: u64) -> String {
    let (d, h, m) = (s / 86400, s % 86400 / 3600, s % 3600 / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{s}s")
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Only tick once a second for "last seen" text; real updates repaint instantly.
        ctx.request_repaint_after(Duration::from_secs(1));

        let shared = match &self.shared {
            Ok(s) => s.clone(),
            Err(err) => {
                let err = err.clone();
                egui::CentralPanel::default().frame(ui::page_frame()).show(ctx, |ui| {
                    ui::notice(ui, Some(icon::WARN), &err, RED);
                });
                return;
            }
        };
        let (pis, status) = {
            let d = shared.data.lock().unwrap();
            (d.pis.clone(), d.status.clone())
        };

        // ── Top bar ──
        egui::TopBottomPanel::top("top")
            .frame(Frame::none().fill(BG).inner_margin(Margin::symmetric(28.0, 18.0)))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui::icon_tile(ui, icon::HOUSE, ACCENT);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(RichText::new("Home Lab").size(21.0).strong().color(TEXT));
                        ui.label(RichText::new(format!("{} device{}", pis.len(), if pis.len() == 1 { "" } else { "s" })).small().color(MUTED));
                    });
                    ui.add_space(14.0);
                    let online = status.values().filter(|s| s.state == State::Online).count();
                    let down = status.values().filter(|s| matches!(s.state, State::Offline | State::SetupFailed)).count();
                    if online > 0 {
                        ui::pill(ui, &format!("{online} online"), GREEN, false);
                    }
                    if down > 0 {
                        ui::pill(ui, &format!("{down} offline"), RED, false);
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui::primary_button(ui, &format!("{}  Add Pi", icon::PLUS)).clicked() {
                            self.show_form = true;
                        }
                        if !pis.is_empty() && ui::icon_button(ui, icon::REFRESH, "Refresh now").clicked() {
                            let (s, c) = (shared.clone(), ctx.clone());
                            thread::spawn(move || s.poll_all(&c));
                        }
                    });
                });
            });

        // ── Add dialog ──
        if self.show_form {
            ui::dim_background(ctx);
            egui::Window::new("add-pi")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .min_width(460.0).max_width(460.0)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| self.form_ui(ui, &shared));
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.show_form = false;
            }
        }

        // ── Diagnostics dialog ──
        if let Some(id) = self.diag_open {
            match pis.iter().find(|p| p.id == id) {
                None => self.diag_open = None,
                Some(pi) => {
                    let g = shared.data.lock().unwrap().diags.get(&id).cloned().unwrap_or_default();
                    ui::dim_background(ctx);
                    let diag_w = (ctx.screen_rect().width() - 48.0).clamp(320.0, 900.0);
                    egui::Window::new("diag")
                        .title_bar(false)
                        .collapsible(false)
                        .resizable(false)
                        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                        .min_width(diag_w).max_width(diag_w)
                        .order(egui::Order::Foreground)
                        .show(ctx, |ui| self.diag_ui(ui, &shared, pi, &g));
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.diag_open = None;
                    }
                }
            }
        }

        // ── Cards ──
        egui::CentralPanel::default().frame(ui::page_frame()).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if pis.is_empty() {
                    ui.add_space(90.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(icon::SERVER).size(46.0).color(LINE_STRONG));
                        ui.add_space(14.0);
                        ui.label(RichText::new("No Pis yet").size(18.0).strong().color(TEXT));
                        ui.add_space(2.0);
                        ui.label(RichText::new("Add one with its IP, user and password. The app does the rest.").color(MUTED));
                        ui.add_space(18.0);
                        if ui::primary_button(ui, &format!("{}  Add your first Pi", icon::PLUS)).clicked() {
                            self.show_form = true;
                        }
                    });
                    return;
                }

                // Responsive grid: as many ≥340px columns as fit, all equal width.
                const GAP: f32 = 18.0;
                let avail = ui.available_width();
                let cols = ((avail + GAP) / (340.0 + GAP)).floor().max(1.0) as usize;
                let width = (avail - GAP * (cols as f32 - 1.0)) / cols as f32;
                for row in pis.chunks(cols) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for pi in row {
                            let s = status.get(&pi.id).cloned().unwrap_or_default();
                            self.card(ui, &shared, pi, &s, width);
                        }
                    });
                    ui.add_space(GAP);
                }
            });
        });
    }
}

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Home Lab")
        .with_inner_size([1120.0, 760.0])
        .with_min_inner_size([420.0, 360.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        // Skip MSAA/depth/stencil buffers we don't need: faster window creation.
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native("Home Lab", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
