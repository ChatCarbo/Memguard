#![windows_subsystem = "windows"]

#[cfg(not(windows))]
compile_error!("memguard ne cible que Windows.");

mod i18n;
mod optimizer;
mod win_api;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use eframe::egui;
use i18n::{t, Lang, K};
use optimizer::{parse_whitelist, Policy};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use win_api::{MemStats, ProcInfo};

const HISTORY_LEN: usize = 120;
const AUTO_COOLDOWN: Duration = Duration::from_secs(60);
const TOP_ROWS: usize = 15;

// ---------------------------------------------------------------------------
// État partagé entre l'interface, l'icône de notification et le thread de fond.
// Rien ne dépend d'une frame egui : la fenêtre cachée peut ne plus être redessinée.
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct Settings {
    policy: Policy,
    auto: bool,
}

static HIDDEN: AtomicBool = AtomicBool::new(false);
static HAS_PRIV: AtomicBool = AtomicBool::new(false);
static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);
static MESSAGE: Mutex<String> = Mutex::new(String::new());

thread_local! {
    // L'icône n'est pas Send : elle reste sur le thread principal.
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
    static MENU_ITEMS: RefCell<Option<[MenuItem; 3]>> = const { RefCell::new(None) };
}

fn current_settings() -> Settings {
    SETTINGS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or(Settings {
            policy: Policy::default(),
            auto: false,
        })
}

fn set_message(msg: String) {
    *MESSAGE.lock().unwrap_or_else(|e| e.into_inner()) = msg;
}

fn set_tooltip(text: String) {
    TRAY.with(|tr| {
        if let Ok(b) = tr.try_borrow() {
            if let Some(tray) = b.as_ref() {
                let _ = tray.set_tooltip(Some(text));
            }
        }
    });
}

/// Met à jour les libellés du menu de l'icône après un changement de langue.
fn apply_tray_language() {
    MENU_ITEMS.with(|m| {
        if let Ok(b) = m.try_borrow() {
            if let Some([show, optim, quit]) = b.as_ref() {
                show.set_text(t(K::Show));
                optim.set_text(t(K::OptimizeNow));
                quit.set_text(t(K::Quit));
            }
        }
    });
}

fn fmt_mb(mb: f64) -> String {
    if mb >= 1024.0 {
        format!("{:.1} {}", mb / 1024.0, i18n::unit_gb())
    } else {
        format!("{:.0} {}", mb, i18n::unit_mb())
    }
}

fn run_optimization(procs: &[ProcInfo], auto: bool) -> String {
    let s = current_settings();
    let r = optimizer::run(&s.policy, procs, HAS_PRIV.load(Ordering::Relaxed));
    i18n::optimization_report(auto, r.processes_lightened, r.standby_purged, r.delta_avail_mb)
}

/// Icône 32x32 générée en mémoire (disque bleu) : aucun fichier à embarquer.
fn make_icon() -> Icon {
    const N: u32 = 32;
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    let c = (N as f32 - 1.0) / 2.0;
    for y in 0..N {
        for x in 0..N {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            if d <= 14.0 {
                rgba.extend_from_slice(&[60, 140, 235, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    Icon::from_rgba(rgba, N, N).expect("icône invalide")
}

// ---------------------------------------------------------------------------
// Actions déclenchées depuis l'icône (exécutées sur le thread principal).
// ---------------------------------------------------------------------------
fn show_window(ctx: &egui::Context) {
    win_api::show_main_window();
    HIDDEN.store(false, Ordering::Relaxed);
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.request_repaint();
}

fn tray_optimize(ctx: &egui::Context) {
    let mut buf = Vec::new();
    let procs = win_api::list_processes(&mut buf);
    set_message(run_optimization(&procs, false));
    ctx.request_repaint();
}

fn quit_app() -> ! {
    // Supprime l'icône avant de quitter (sinon elle reste fantôme dans la zone).
    TRAY.with(|t| {
        if let Ok(mut b) = t.try_borrow_mut() {
            b.take();
        }
    });
    std::process::exit(0);
}

fn install_tray_handlers(ctx: &egui::Context, show: MenuId, optim: MenuId, quit: MenuId) {
    let c1 = ctx.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if ev.id == show {
            show_window(&c1);
        } else if ev.id == optim {
            tray_optimize(&c1);
        } else if ev.id == quit {
            quit_app();
        }
    }));
    let c2 = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        if matches!(ev, TrayIconEvent::DoubleClick { .. }) {
            show_window(&c2);
        }
    }));
}

/// Thread de fond : réveille l'interface quand elle est visible et, quand la
/// fenêtre est cachée, exécute l'auto-optimisation sans passer par egui.
fn background_worker(ctx: egui::Context) {
    let mut buf: Vec<u32> = Vec::new();
    let mut last_auto = Instant::now()
        .checked_sub(AUTO_COOLDOWN)
        .unwrap_or_else(Instant::now);
    loop {
        std::thread::sleep(Duration::from_secs(5));
        if !HIDDEN.load(Ordering::Relaxed) {
            ctx.request_repaint();
            continue;
        }
        let s = current_settings();
        if !s.auto || last_auto.elapsed() < AUTO_COOLDOWN {
            continue;
        }
        let stats = win_api::memory_stats();
        set_tooltip(i18n::tooltip(stats.load_pct, &fmt_mb(stats.avail_mb)));
        if stats.load_pct < s.policy.threshold_pct {
            continue;
        }
        let procs = win_api::list_processes(&mut buf);
        set_message(run_optimization(&procs, true));
        last_auto = Instant::now();
    }
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------
struct App {
    stats: MemStats,
    procs: Vec<ProcInfo>,
    pid_buf: Vec<u32>,
    history: VecDeque<f32>,
    policy: Policy,
    whitelist_text: String,
    auto_mode: bool,
    has_privilege: bool,
    last_poll: Instant,
    last_auto: Instant,
    was_visible: bool,
    message: String,
    lang: Lang,
}

impl App {
    fn new(ctx: &egui::Context, start_hidden: bool) -> Self {
        let has_privilege = win_api::enable_memory_privilege();
        HAS_PRIV.store(has_privilege, Ordering::Relaxed);
        HIDDEN.store(start_hidden, Ordering::Relaxed);

        let show_item = MenuItem::new(t(K::Show), true, None);
        let optim_item = MenuItem::new(t(K::OptimizeNow), true, None);
        let quit_item = MenuItem::new(t(K::Quit), true, None);
        let menu = Menu::new();
        menu.append_items(&[&show_item, &optim_item, &quit_item])
            .expect("menu tray");
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("MemGuard")
            .with_icon(make_icon())
            .build()
            .expect("icône de notification");
        TRAY.with(|t| *t.borrow_mut() = Some(tray));
        install_tray_handlers(
            ctx,
            show_item.id().clone(),
            optim_item.id().clone(),
            quit_item.id().clone(),
        );
        MENU_ITEMS.with(|m| *m.borrow_mut() = Some([show_item, optim_item, quit_item]));

        let mut pid_buf = Vec::new();
        Self {
            stats: win_api::memory_stats(),
            procs: win_api::list_processes(&mut pid_buf),
            pid_buf,
            history: VecDeque::with_capacity(HISTORY_LEN),
            policy: Policy::default(),
            whitelist_text: String::new(),
            auto_mode: false,
            has_privilege,
            last_poll: Instant::now(),
            last_auto: Instant::now()
                .checked_sub(AUTO_COOLDOWN)
                .unwrap_or_else(Instant::now),
            was_visible: true,
            message: String::new(),
            lang: i18n::lang(),
        }
    }

    fn sync_settings(&mut self) {
        self.policy.whitelist = parse_whitelist(&self.whitelist_text);
        *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Some(Settings {
            policy: self.policy.clone(),
            auto: self.auto_mode,
        });
    }

    fn poll(&mut self, visible: bool) {
        self.sync_settings();
        self.stats = win_api::memory_stats();
        if self.history.len() == HISTORY_LEN {
            self.history.pop_front();
        }
        self.history.push_back(self.stats.load_pct as f32);
        if !visible {
            set_tooltip(i18n::tooltip(self.stats.load_pct, &fmt_mb(self.stats.avail_mb)));
        }

        let need_auto = self.auto_mode
            && self.stats.load_pct >= self.policy.threshold_pct
            && self.last_auto.elapsed() >= AUTO_COOLDOWN;

        // La liste des processus (coûteuse) n'est lue que si on l'affiche ou si on agit.
        if visible || need_auto {
            self.procs = win_api::list_processes(&mut self.pid_buf);
        } else if !self.procs.is_empty() {
            self.procs = Vec::new(); // libère la mémoire quand on est réduit
        }
        if need_auto {
            self.optimize_now(true);
        }
        self.last_poll = Instant::now();
    }

    fn optimize_now(&mut self, auto: bool) {
        self.sync_settings();
        self.message = run_optimization(&self.procs, auto);
        self.last_auto = Instant::now();
        self.stats = win_api::memory_stats();
    }

    fn sparkline(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 48.0),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        if self.history.len() < 2 {
            return;
        }
        let step = rect.width() / (HISTORY_LEN - 1) as f32;
        let start = HISTORY_LEN - self.history.len();
        let pts: Vec<egui::Pos2> = self
            .history
            .iter()
            .enumerate()
            .map(|(i, v)| {
                egui::pos2(
                    rect.left() + step * (start + i) as f32,
                    rect.bottom() - rect.height() * (v / 100.0).clamp(0.0, 1.0),
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            pts,
            egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(90, 170, 255)),
        ));
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (minimized, focused) = ctx.input(|i| {
            let v = i.viewport();
            (v.minimized.unwrap_or(false), v.focused.unwrap_or(true))
        });

        // Croix de fermeture = réduction dans la zone de notification.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            HIDDEN.store(true, Ordering::Relaxed);
            self.sync_settings();
            set_tooltip(i18n::tooltip(self.stats.load_pct, &fmt_mb(self.stats.avail_mb)));
        }

        // Résultat d'une optimisation lancée depuis l'icône ou le thread de fond.
        if let Ok(mut m) = MESSAGE.lock() {
            if !m.is_empty() {
                self.message = std::mem::take(&mut *m);
                self.last_auto = Instant::now();
            }
        }

        let visible = !minimized && !HIDDEN.load(Ordering::Relaxed);

        // Polling adaptatif : 2 s actif, 5 s en arrière-plan, 10 s réduit.
        let interval = if !visible {
            Duration::from_secs(10)
        } else if focused {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(5)
        };

        if self.last_poll.elapsed() >= interval || (visible && !self.was_visible) {
            self.poll(visible);
        }
        // À la réduction, on rend la mémoire de memguard à Windows.
        if self.was_visible && !visible {
            win_api::trim_self();
        }
        self.was_visible = visible;

        let mut to_lighten: Option<u32> = None;
        let mut optimize = false;
        let mut quit = false;

        if visible {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("MemGuard");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        quit = ui.button(t(K::Quit)).clicked();
                        let before = self.lang;
                        ui.selectable_value(&mut self.lang, Lang::En, "EN");
                        ui.selectable_value(&mut self.lang, Lang::Fr, "FR");
                        if self.lang != before {
                            i18n::set_lang(self.lang);
                            apply_tray_language();
                            self.message.clear();
                        }
                    });
                });
                let used = self.stats.total_mb - self.stats.avail_mb;
                ui.add(
                    egui::ProgressBar::new(self.stats.load_pct as f32 / 100.0).text(format!(
                        "{} / {}  ({} %)",
                        fmt_mb(used),
                        fmt_mb(self.stats.total_mb),
                        self.stats.load_pct
                    )),
                );
                self.sparkline(ui);
                ui.label(i18n::stats_line(
                    &fmt_mb(self.stats.standby_mb),
                    &fmt_mb(self.stats.modified_mb),
                    &fmt_mb(self.stats.commit_used_mb),
                    &fmt_mb(self.stats.commit_limit_mb),
                ));
                ui.separator();

                ui.horizontal(|ui| {
                    optimize = ui.button(t(K::OptimizeNow)).clicked();
                    ui.checkbox(&mut self.auto_mode, t(K::Auto));
                    ui.add(
                        egui::Slider::new(&mut self.policy.threshold_pct, 50..=95)
                            .suffix(" %")
                            .text(t(K::Threshold)),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut self.policy.min_ws_mb, 20.0..=500.0)
                            .suffix(format!(" {}", i18n::unit_mb()))
                            .text(t(K::MinPerProcess)),
                    );
                    ui.add_enabled(
                        self.has_privilege,
                        egui::Checkbox::new(&mut self.policy.purge_standby, t(K::PurgeStandby)),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(t(K::Whitelist));
                    ui.text_edit_singleline(&mut self.whitelist_text)
                        .on_hover_text(t(K::WhitelistHint));
                });
                if !self.message.is_empty() {
                    ui.label(&self.message);
                }
                ui.separator();

                egui::Grid::new("procs").striped(true).show(ui, |ui| {
                    for p in self.procs.iter().take(TOP_ROWS) {
                        ui.label(&p.name);
                        ui.label(fmt_mb(p.ws_mb as f64));
                        if optimizer::is_protected(&p.name) {
                            ui.label(t(K::Protected));
                        } else if ui.small_button(t(K::Lighten)).clicked() {
                            to_lighten = Some(p.pid);
                        }
                        ui.end_row();
                    }
                });
            });
            self.sync_settings();
        }

        if quit {
            quit_app();
        }
        if let Some(pid) = to_lighten {
            win_api::lighten_process(pid, true);
            self.poll(true);
        }
        if optimize {
            self.poll(true);
            self.optimize_now(false);
        }

        ctx.request_repaint_after(interval);
    }
}

fn main() -> eframe::Result<()> {
    // `memguard.exe --tray` : démarre directement dans la zone de notification.
    // `--lang=fr` / `--lang=en` force la langue (sinon : langue de Windows).
    let start_hidden = std::env::args().any(|a| a == "--tray");
    i18n::set_lang(i18n::from_args_or_detect());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MemGuard")
            .with_inner_size([520.0, 560.0])
            .with_min_inner_size([420.0, 360.0])
            .with_visible(!start_hidden),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "MemGuard",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let worker_ctx = ctx.clone();
            std::thread::Builder::new()
                .stack_size(256 * 1024)
                .spawn(move || background_worker(worker_ctx))
                .ok();
            Ok(Box::new(App::new(&ctx, start_hidden)))
        }),
    )
}
