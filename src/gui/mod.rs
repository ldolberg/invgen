//! Native editor with live PDF preview. OWNER: gui agent.

mod clients;
mod forms;
mod highlight;
mod library;
mod onboarding;
mod theme;
mod view;
mod worker;

use crate::config::{Config, DEFAULT_CONFIG};
use crate::config_edit::{self, EditError};
use crate::model::Invoice;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, TextureHandle, Vec2};
use library::{Entry, FsWatch, Scanner};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use worker::{Job, Outcome, Worker};

pub const APP_ID: &str = "io.github.ldolberg.invgen";
const DEBOUNCE: Duration = Duration::from_millis(150);
const STORAGE_KEY: &str = "invgen-prefs";

/// Window icon, rasterized from packaging/io.github.ldolberg.invgen.svg.
const ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

fn window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

/// Load the config from `path` (from `--config` or `INVGEN_CONFIG`), or the default location.
fn load_config(path: Option<&Path>) -> anyhow::Result<Config> {
    match path {
        Some(p) => Config::load_from(p),
        None => Config::load(),
    }
}

/// Open the editor window, optionally on a file. `config` overrides the
/// config file location (the CLI passes `--config` or `INVGEN_CONFIG` here).
pub fn run(file: Option<PathBuf>, config: Option<PathBuf>) -> anyhow::Result<()> {
    let file = file.map(|f| std::fs::canonicalize(&f).unwrap_or(f));
    let config = config.or_else(|| std::env::var_os("INVGEN_CONFIG").map(PathBuf::from));
    let mut viewport = egui::ViewportBuilder::default()
        .with_app_id(APP_ID)
        .with_title("invgen");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport: viewport
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([720.0, 460.0]),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native(
        APP_ID,
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, file, config.as_deref())))),
    )
    .map_err(|e| anyhow::anyhow!("gui: {e}"))
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Fraction of the work area given to the editor.
    pub split: f32,
    pub zoom: f32,
    pub fit_width: bool,
    pub theme: ThemeChoice,
    pub last_file: Option<PathBuf>,
    pub sidebar: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            split: 0.46,
            zoom: 1.0,
            fit_width: true,
            theme: ThemeChoice::Dark,
            last_file: None,
            sidebar: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub born: Instant,
    pub open: Option<PathBuf>,
}

/// An action that would discard unsaved changes.
#[derive(Clone)]
pub enum Pending {
    /// A new untitled invoice, for this client key (None: the first client).
    New(Option<String>),
    Open(PathBuf),
    Close,
}

enum Dialog {
    Open(Option<PathBuf>),
    SaveAs(Option<PathBuf>, Option<Pending>),
    ExportAs(Option<PathBuf>),
}

pub struct Page {
    pub tex: TextureHandle,
    /// Size in PDF points.
    pub size_pt: Vec2,
}

pub struct App {
    cfg: Arc<Config>,
    /// Where the config lives (`--config`, `INVGEN_CONFIG` or the default).
    config_path: PathBuf,
    /// Reload the config at this time (it changed on disk).
    config_due: Option<Instant>,
    /// First-run form, while there is no config file.
    onboarding: Option<onboarding::Form>,
    clients: clients::Panel,
    clients_open: bool,
    pub prefs: Prefs,

    path: Option<PathBuf>,
    text: String,
    saved_text: String,

    worker: Worker,
    generation: u64,
    preview_due: Option<Instant>,
    in_flight: usize,
    pages: Vec<Page>,
    pages_generation: u64,
    rendered_ppt: f32,
    invoice: Option<Invoice>,
    /// Error of the *current* text: (1-based line or 0, message).
    error: Option<(usize, String)>,
    /// Live check of the last checked text (errors and warnings).
    diagnostics: Vec<crate::lint::Diagnostic>,
    /// The problems list under the editor is expanded.
    problems_open: bool,
    /// The completion popup, while it is open.
    popup: Option<view::Popup>,
    /// Units, descriptions and numbers from the library, for completions.
    vocabulary: library::Vocabulary,

    scanner: Scanner,
    library: Vec<Entry>,
    library_due: Option<Instant>,
    watch: FsWatch,

    toasts: Vec<Toast>,
    confirm: Option<Pending>,
    allow_close: bool,
    dialog_tx: Sender<Dialog>,
    dialog_rx: Receiver<Dialog>,
    dialog_open: bool,
    last_export: Option<PathBuf>,
    /// Issued lock of the current document (see `crate::lock`).
    pub lock: crate::lock::State,
    title: String,
    /// Scroll the editor to this 1-based line on the next frame.
    goto_line: Option<usize>,
    /// Zoom anchor in preview viewport coordinates for this frame.
    zoom_anchor: Option<(egui::Pos2, f32)>,
    preview_offset: Vec2,
    preview_size: Vec2,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>, config: Option<&Path>) -> App {
        let ctx = cc.egui_ctx.clone();
        theme::install_fonts(&ctx);
        theme::install_style(&ctx);

        let prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, STORAGE_KEY))
            .unwrap_or_default();

        let mut toasts = Vec::new();
        let cfg = match std::panic::catch_unwind(|| load_config(config)) {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                toasts.push(Toast {
                    text: format!("Config error: {e:#}"),
                    kind: ToastKind::Error,
                    born: Instant::now(),
                    open: None,
                });
                Config::default()
            }
            Err(_) => Config::default(),
        };
        let config_path = config.map(Path::to_path_buf).unwrap_or_else(Config::path);
        // Absolute, so file watcher events can be matched against it.
        let config_path = std::path::absolute(&config_path).unwrap_or(config_path);
        let onboarding = (!config_path.exists()).then(onboarding::Form::default);
        let cfg = Arc::new(cfg);
        let (dialog_tx, dialog_rx) = channel();

        let mut app = App {
            worker: Worker::spawn(cfg.clone(), ctx.clone()),
            scanner: Scanner::spawn(ctx.clone()),
            watch: FsWatch::new(ctx.clone()),
            clients: clients::Panel::new(&cfg),
            clients_open: false,
            cfg,
            config_path,
            config_due: None,
            onboarding,
            prefs,
            path: None,
            text: String::new(),
            saved_text: String::new(),
            generation: 0,
            preview_due: None,
            in_flight: 0,
            pages: Vec::new(),
            pages_generation: 0,
            rendered_ppt: 0.0,
            invoice: None,
            error: None,
            diagnostics: Vec::new(),
            problems_open: false,
            popup: None,
            vocabulary: library::Vocabulary::default(),
            library: Vec::new(),
            library_due: Some(Instant::now()),
            toasts,
            confirm: None,
            allow_close: false,
            dialog_tx,
            dialog_rx,
            dialog_open: false,
            last_export: None,
            lock: crate::lock::State::Open,
            title: String::new(),
            goto_line: None,
            zoom_anchor: None,
            preview_offset: Vec2::ZERO,
            preview_size: Vec2::ZERO,
        };
        app.apply_theme(&ctx);

        let start = file.or_else(|| app.prefs.last_file.clone().filter(|p| p.is_file()));
        match start {
            Some(p) if p.is_file() => app.load_file(&p),
            Some(p) => {
                // A path that does not exist yet: start a new invoice that saves there.
                app.new_document(None);
                app.path = Some(p);
            }
            None => app.new_document(None),
        }
        app.update_watches();
        app
    }

    // ---------------------------------------------------------------- state

    pub fn dirty(&self) -> bool {
        self.text != self.saved_text
    }

    pub fn display_name(&self) -> String {
        match &self.path {
            Some(p) => p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string()),
            None => "Untitled".into(),
        }
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            kind,
            born: Instant::now(),
            open: None,
        });
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let pref = match self.prefs.theme {
            ThemeChoice::Dark => egui::ThemePreference::Dark,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::System => egui::ThemePreference::System,
        };
        ctx.set_theme(pref);
    }

    pub fn cycle_theme(&mut self, ctx: &egui::Context) {
        self.prefs.theme = match self.prefs.theme {
            ThemeChoice::Dark => ThemeChoice::Light,
            ThemeChoice::Light => ThemeChoice::System,
            ThemeChoice::System => ThemeChoice::Dark,
        };
        self.apply_theme(ctx);
    }

    /// The text changed (edit, load, new): schedule a debounced preview.
    pub fn text_changed(&mut self, immediate: bool) {
        self.generation += 1;
        let delay = if immediate { Duration::ZERO } else { DEBOUNCE };
        self.preview_due = Some(Instant::now() + delay);
    }

    fn set_document(&mut self, path: Option<PathBuf>, text: String, saved: String) {
        self.path = path;
        self.text = text;
        self.saved_text = saved;
        self.error = None;
        self.diagnostics.clear();
        self.popup = None;
        // The parsed invoice belongs to the previous document; keeping it would
        // name saves and exports after the wrong invoice number.
        self.invoice = None;
        self.lock = crate::lock::State::Open;
        self.goto_line = None;
        self.text_changed(true);
        self.update_watches();
        if let Some(p) = &self.path {
            self.prefs.last_file = Some(p.clone());
        }
    }

    fn load_file(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
                self.set_document(Some(path), text.clone(), text);
            }
            Err(e) => self.toast(
                ToastKind::Error,
                format!("Cannot open {}: {e}", path.display()),
            ),
        }
    }

    fn new_document(&mut self, client: Option<&str>) {
        let client = client.filter(|k| self.cfg.clients.contains_key(*k));
        let text = std::panic::catch_unwind(|| self.cfg.new_invoice_markdown(client))
            .unwrap_or_else(|_| FALLBACK_TEMPLATE.to_string());
        // Untitled and dirty, so closing asks before throwing it away.
        self.set_document(None, text, String::new());
    }

    fn update_watches(&mut self) {
        // The library recursively; the open file's folder too when the
        // recursive watch does not already cover it.
        let inv_dir = self.cfg.invoices_dir.clone();
        let canon = std::fs::canonicalize(&inv_dir).unwrap_or_else(|_| inv_dir.clone());
        let mut dirs = vec![(inv_dir, true)];
        if let Some(parent) = self.path.as_ref().and_then(|p| p.parent())
            && !parent.starts_with(&canon)
        {
            dirs.push((parent.to_path_buf(), false));
        }
        // The config's folder, to pick up edits made in a text editor.
        if let Some(parent) = self.config_path.parent() {
            dirs.push((parent.to_path_buf(), false));
        }
        self.watch.set_dirs(&dirs);
    }

    // ---------------------------------------------------------------- config

    /// Read the config file again and hand it to the preview and the library.
    fn reload_config(&mut self) {
        if !self.config_path.exists() {
            return;
        }
        let cfg = match Config::load_from(&self.config_path) {
            Ok(c) => c,
            Err(e) => {
                self.toast(ToastKind::Error, format!("Config error: {e:#}"));
                return;
            }
        };
        let cfg = Arc::new(cfg);
        self.cfg = cfg.clone();
        self.worker.send(Job::Config(cfg));
        self.clients.refresh(&self.cfg);
        self.library_due = Some(Instant::now());
        self.update_watches();
        self.text_changed(true);
    }

    /// Apply `change` to the current config text and write it back (atomic,
    /// with a `.bak`), then reload. The file is read fresh every time, so
    /// edits made meanwhile in a text editor are not lost.
    fn edit_config(
        &mut self,
        change: impl FnOnce(&str) -> Result<String, EditError>,
    ) -> Result<(), EditError> {
        let text = match std::fs::read_to_string(&self.config_path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let new = change(&text)?;
        if new != text {
            config_edit::write_atomic(&self.config_path, &new)?;
        }
        self.reload_config();
        Ok(())
    }

    pub fn open_config_file(&mut self) {
        if !self.config_path.exists()
            && let Err(e) = config_edit::write_atomic(&self.config_path, DEFAULT_CONFIG)
        {
            self.toast(ToastKind::Error, format!("Cannot write the config: {e}"));
            return;
        }
        open_external(&self.config_path);
        self.toast(
            ToastKind::Info,
            format!(
                "Opened {} (changes apply when you save it)",
                tilde(&self.config_path)
            ),
        );
    }

    pub fn open_clients(&mut self) {
        if !self.clients_open {
            self.clients.on_open(&self.cfg);
        }
        self.clients_open = true;
    }

    fn handle_clients(&mut self, action: clients::Action) {
        match action {
            clients::Action::Save {
                original,
                key,
                client,
            } => {
                let res = self.edit_config(|text| {
                    config_edit::upsert_client(text, original.as_deref(), &key, &client)
                });
                match res {
                    Ok(()) => {
                        let cfg = self.cfg.clone();
                        self.clients.select_in(&cfg, Some(&key));
                        self.toast(ToastKind::Success, format!("Saved client {key}"));
                    }
                    Err(e) => self.clients_problem(e),
                }
            }
            clients::Action::Delete(key) => {
                match self.edit_config(|text| config_edit::delete_client(text, &key)) {
                    Ok(()) => {
                        let cfg = self.cfg.clone();
                        let first = cfg.clients.keys().next().map(String::as_str);
                        self.clients.select_in(&cfg, first);
                        self.toast(ToastKind::Success, format!("Deleted client {key}"));
                    }
                    Err(e) => self.clients_problem(e),
                }
            }
            clients::Action::NewInvoice(key) => self.request(Pending::New(Some(key))),
            clients::Action::OpenConfig => self.open_config_file(),
        }
    }

    fn clients_problem(&mut self, e: EditError) {
        let unsupported = matches!(e, EditError::Unsupported(_));
        let msg = if unsupported {
            format!("invgen cannot safely edit this config: {e}.")
        } else {
            format!("Not saved: {e}")
        };
        self.clients.problem = Some((msg, unsupported));
    }

    fn handle_onboarding(&mut self, outcome: onboarding::Outcome) {
        match outcome {
            onboarding::Outcome::Skip => {
                if let Err(e) = config_edit::write_atomic(&self.config_path, DEFAULT_CONFIG) {
                    if let Some(f) = self.onboarding.as_mut() {
                        f.error = Some(format!("Cannot write {}: {e}", tilde(&self.config_path)));
                    }
                    return;
                }
                self.onboarding = None;
                self.reload_config();
                open_external(&self.config_path);
                self.toast(
                    ToastKind::Info,
                    format!(
                        "Wrote {}. Fill it in; the editor picks up your changes when you save.",
                        tilde(&self.config_path)
                    ),
                );
            }
            onboarding::Outcome::Create(setup) => {
                let written = setup.render().and_then(|text| {
                    config_edit::write_atomic(&self.config_path, &text).map_err(EditError::from)
                });
                if let Err(e) = written {
                    if let Some(f) = self.onboarding.as_mut() {
                        f.error = Some(format!("Could not write the config: {e}"));
                    }
                    return;
                }
                self.onboarding = None;
                self.reload_config();
                // Keep a file the user opened explicitly; otherwise start the first invoice.
                if self.path.is_some() {
                    self.toast(ToastKind::Success, "Config saved");
                    return;
                }
                match self.cfg.write_new_invoice(Some(&setup.client_key)) {
                    Ok(path) => {
                        self.load_file(&path);
                        self.library_due = Some(Instant::now());
                        self.toast(
                            ToastKind::Success,
                            format!(
                                "All set. Created {}: check the line items, then Export PDF.",
                                file_name(&path)
                            ),
                        );
                    }
                    Err(e) => {
                        self.new_document(Some(&setup.client_key));
                        self.toast(ToastKind::Error, format!("Config saved, but {e:#}"));
                    }
                }
            }
        }
    }

    /// Run `action` now, or ask first when it would drop unsaved changes.
    pub fn request(&mut self, action: Pending) {
        if self.dirty() {
            self.confirm = Some(action);
        } else {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: Pending) {
        match action {
            Pending::New(client) => self.new_document(client.as_deref()),
            Pending::Open(p) => self.load_file(&p),
            Pending::Close => {
                self.allow_close = true;
            }
        }
    }

    fn start_dialog(&mut self, f: impl FnOnce() -> Dialog + Send + 'static) {
        if self.dialog_open {
            return;
        }
        self.dialog_open = true;
        let tx = self.dialog_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
    }

    fn start_dir(&self) -> PathBuf {
        self.path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| self.cfg.invoices_dir.clone())
    }

    pub fn open_dialog(&mut self) {
        let dir = self.start_dir();
        self.start_dialog(move || {
            Dialog::Open(
                rfd::FileDialog::new()
                    .set_title("Open invoice")
                    .add_filter("Markdown invoice", &["md", "markdown"])
                    .set_directory(dir)
                    .pick_file(),
            )
        });
    }

    fn save_as_dialog(&mut self, then: Option<Pending>) {
        let dir = self.start_dir();
        let name = format!("{}.md", self.suggested_stem());
        self.start_dialog(move || {
            Dialog::SaveAs(
                rfd::FileDialog::new()
                    .set_title("Save invoice")
                    .add_filter("Markdown invoice", &["md"])
                    .set_directory(dir)
                    .set_file_name(name)
                    .save_file(),
                then,
            )
        });
    }

    pub fn export_as_dialog(&mut self) {
        let target = self.export_path();
        let dir = target.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.start_dialog(move || {
            Dialog::ExportAs(
                rfd::FileDialog::new()
                    .set_title("Export PDF")
                    .add_filter("PDF", &["pdf"])
                    .set_directory(dir)
                    .set_file_name(name)
                    .save_file(),
            )
        });
    }

    fn suggested_stem(&self) -> String {
        self.invoice
            .as_ref()
            .map(|i| i.number.trim())
            .filter(|n| !n.is_empty())
            .map(crate::model::file_stem)
            .or_else(|| {
                self.path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "invoice".into())
    }

    /// `<number>.pdf` next to the markdown file, or in the invoices dir.
    pub fn export_path(&self) -> PathBuf {
        self.export_dir()
            .join(format!("{}.pdf", self.suggested_stem()))
    }

    /// Save; returns true when the text is on disk afterwards.
    pub fn save(&mut self, then: Option<Pending>) -> bool {
        let path = match &self.path {
            Some(p) => p.clone(),
            None => {
                // Untitled: drop it into the invoices dir as <number>.md when free.
                let p = self
                    .cfg
                    .invoices_dir
                    .join(format!("{}.md", self.suggested_stem()));
                if p.exists() || self.invoice.is_none() {
                    self.save_as_dialog(then);
                    return false;
                }
                p
            }
        };
        self.write_to(path, then)
    }

    fn write_to(&mut self, path: PathBuf, then: Option<Pending>) -> bool {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(&path, &self.text) {
            Ok(()) => {
                let was_untitled = self.path.is_none();
                self.saved_text = self.text.clone();
                self.path = Some(std::fs::canonicalize(&path).unwrap_or(path.clone()));
                self.prefs.last_file = self.path.clone();
                self.update_watches();
                self.library_due = Some(Instant::now());
                let msg = if was_untitled {
                    format!(
                        "Saved {} to {}",
                        file_name(&path),
                        path.parent().map(tilde).unwrap_or_default()
                    )
                } else {
                    format!("Saved {}", self.display_name())
                };
                self.toast(ToastKind::Success, msg);
                if let Some(a) = then {
                    self.perform(a);
                }
                true
            }
            Err(e) => {
                self.toast(ToastKind::Error, format!("Save failed: {e}"));
                false
            }
        }
    }

    /// Directory for the default export (`<number>.pdf`).
    fn export_dir(&self) -> PathBuf {
        match self.path.as_ref().and_then(|p| p.parent()) {
            Some(dir) => dir.to_path_buf(),
            None => self.cfg.invoices_dir.clone(),
        }
    }

    /// Export the current text. Without an explicit path the worker names the
    /// file after the number it parses from that same text, so a number edited
    /// a moment ago (preview not caught up yet) cannot overwrite another PDF.
    pub fn export(&mut self, path: Option<PathBuf>) {
        self.worker.send(Job::Export {
            text: self.text.clone(),
            path,
            dir: self.export_dir(),
        });
        self.in_flight += 1;
    }

    /// Remove `issued_hash` so the invoice can be edited without warnings.
    pub fn unlock(&mut self) {
        match crate::lock::unlock(&self.text) {
            Ok(text) if text != self.text => {
                self.text = text;
                self.lock = crate::lock::State::Open;
                self.text_changed(true);
                self.toast(
                    ToastKind::Info,
                    "Unlocked. Save to keep it; the next export locks it again.",
                );
            }
            Ok(_) => self.lock = crate::lock::State::Open,
            Err(e) => self.toast(ToastKind::Error, format!("Cannot unlock: {e:#}")),
        }
    }

    /// After exporting `inv`, record the issued lock in the editor text and
    /// in the file on disk (keeping unsaved edits unsaved).
    fn record_lock(&mut self, inv: &Invoice) {
        let Ok(Some(text)) = crate::lock::record(&self.text, inv) else {
            if let crate::lock::State::Changed { .. } = crate::lock::state(&self.text, inv) {
                self.toast(
                    ToastKind::Error,
                    "This issued invoice changed since it was sent. Prefer a credit note.",
                );
            }
            return;
        };
        let was_clean = !self.dirty();
        self.text = text;
        self.lock = crate::lock::State::Locked;
        self.text_changed(true);
        if was_clean {
            self.saved_text = self.text.clone();
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        let written = std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|disk| {
                let locked = crate::lock::record(&disk, inv)?.unwrap_or(disk);
                std::fs::write(&path, &locked)?;
                Ok(locked)
            });
        match written {
            Ok(locked) => self.saved_text = locked,
            Err(e) => self.toast(ToastKind::Error, format!("Cannot record the lock: {e:#}")),
        }
    }

    pub fn open_pdf(&mut self) {
        let target = self
            .last_export
            .clone()
            .filter(|p| p.is_file())
            .unwrap_or_else(|| self.export_path());
        if target.is_file() {
            open_external(&target);
        } else {
            self.toast(ToastKind::Info, "Not exported yet. Press Ctrl+E first.");
        }
    }

    // ---------------------------------------------------------------- zoom

    pub fn set_zoom(&mut self, z: f32, anchor: Option<egui::Pos2>) {
        let old = self.prefs.zoom;
        let z = z.clamp(0.25, 5.0);
        self.prefs.fit_width = false;
        self.prefs.zoom = z;
        if let Some(a) = anchor {
            self.zoom_anchor = Some((a, z / old));
        }
    }

    pub fn zoom_step(&mut self, factor: f32) {
        let z = self.prefs.zoom * factor;
        // Snap to a few round values near 100%.
        let z = if (z - 1.0).abs() < 0.04 { 1.0 } else { z };
        // Keyboard zoom keeps the middle of the preview in place.
        let center = (self.preview_size / 2.0).to_pos2();
        self.set_zoom(z, Some(center));
    }

    /// Preview density wanted for the current zoom.
    pub fn wanted_ppt(&self, ctx: &egui::Context) -> f32 {
        (self.prefs.zoom * ctx.pixels_per_point()).clamp(0.5, 8.0)
    }

    // ---------------------------------------------------------------- per frame

    fn poll(&mut self, ctx: &egui::Context) {
        for out in self.worker.poll() {
            match out {
                Outcome::Preview {
                    generation,
                    invoice,
                    pixels_per_pt,
                    pages,
                    diagnostics,
                } => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    if generation == self.generation {
                        self.error = None;
                        self.diagnostics = diagnostics;
                    }
                    if generation == self.generation {
                        self.lock = crate::lock::state(&self.text, &invoice);
                    }
                    if generation >= self.pages_generation {
                        self.pages_generation = generation;
                        self.invoice = Some(*invoice);
                        self.rendered_ppt = pixels_per_pt;
                        self.pages = pages
                            .into_iter()
                            .enumerate()
                            .map(|(i, p)| {
                                let img = egui::ColorImage::from_rgba_unmultiplied(
                                    [p.width as usize, p.height as usize],
                                    &p.rgba,
                                );
                                Page {
                                    tex: ctx.load_texture(
                                        format!("page-{i}"),
                                        img,
                                        egui::TextureOptions::LINEAR,
                                    ),
                                    size_pt: egui::vec2(p.width as f32, p.height as f32)
                                        / pixels_per_pt,
                                }
                            })
                            .collect();
                    }
                }
                Outcome::Failed {
                    generation,
                    line,
                    message,
                    diagnostics,
                } => {
                    if generation != 0 {
                        self.in_flight = self.in_flight.saturating_sub(1);
                    }
                    if generation == self.generation || generation == 0 {
                        self.error = Some((line, message));
                    }
                    if generation == self.generation {
                        self.diagnostics = diagnostics;
                    }
                }
                Outcome::Skipped => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                }
                Outcome::Exported(res) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    match res {
                        Ok((p, inv)) => {
                            self.toasts.push(Toast {
                                text: format!("Exported {}", file_name(&p)),
                                kind: ToastKind::Success,
                                born: Instant::now(),
                                open: Some(p.clone()),
                            });
                            self.record_lock(&inv);
                            self.last_export = Some(p);
                            self.library_due = Some(Instant::now());
                            // Non-blocking: the PDF is written either way.
                            if let Some(w) =
                                self.invoice.as_ref().and_then(Invoice::tax_note_warning)
                            {
                                self.toast(ToastKind::Info, format!("Note: {w}"));
                            }
                        }
                        Err(e) => {
                            self.toast(ToastKind::Error, format!("Export failed: {e}"));
                        }
                    }
                }
            }
        }

        if let Some(entries) = self.scanner.poll() {
            self.vocabulary = library::Vocabulary::from_entries(&entries);
            self.library = entries;
        }

        while let Ok(d) = self.dialog_rx.try_recv() {
            self.dialog_open = false;
            match d {
                Dialog::Open(Some(p)) => self.request(Pending::Open(p)),
                Dialog::SaveAs(Some(p), then) => {
                    let p = if p.extension().is_none() {
                        p.with_extension("md")
                    } else {
                        p
                    };
                    self.write_to(p, then);
                }
                Dialog::ExportAs(Some(p)) => self.export(Some(p)),
                _ => {}
            }
        }

        // External file changes.
        let changed = self.watch.poll();
        if changed.iter().any(|p| {
            p.file_name() == self.config_path.file_name() && p.parent() == self.config_path.parent()
        }) {
            self.config_due = Some(Instant::now() + Duration::from_millis(300));
        }
        if !changed.is_empty() {
            let inv_dir = std::fs::canonicalize(&self.cfg.invoices_dir)
                .unwrap_or_else(|_| self.cfg.invoices_dir.clone());
            for raw in &changed {
                // A deleted file cannot be canonicalized, so also try the raw
                // event path against the configured dir.
                let p = std::fs::canonicalize(raw).unwrap_or_else(|_| raw.clone());
                if crate::library::affects(&inv_dir, &p)
                    || crate::library::affects(&self.cfg.invoices_dir, raw)
                {
                    self.library_due = Some(Instant::now() + Duration::from_millis(250));
                }
            }
            if let Some(cur) = self.path.clone() {
                let touched = changed
                    .iter()
                    .any(|p| p == &cur || p.file_name() == cur.file_name());
                if touched
                    && let Ok(disk) = std::fs::read_to_string(&cur)
                    && disk != self.saved_text
                    && disk != self.text
                {
                    if self.dirty() {
                        self.toast(
                            ToastKind::Info,
                            format!("{} changed on disk (kept your edits)", self.display_name()),
                        );
                        self.saved_text = disk;
                    } else {
                        self.text = disk.clone();
                        self.saved_text = disk;
                        self.text_changed(true);
                        self.toast(ToastKind::Info, "Reloaded from disk");
                    }
                }
            }
        }

        let now = Instant::now();
        if let Some(due) = self.config_due {
            if now >= due {
                self.config_due = None;
                self.reload_config();
            } else {
                ctx.request_repaint_after(due - now);
            }
        }
        if let Some(due) = self.library_due {
            if now >= due {
                self.library_due = None;
                self.scanner
                    .request(self.cfg.invoices_dir.clone(), self.cfg.clone());
            } else {
                ctx.request_repaint_after(due - now);
            }
        }

        // Re-render when the zoom moved the needed density far enough.
        if self.preview_due.is_none() && !self.pages.is_empty() && self.in_flight == 0 {
            let want = self.wanted_ppt(ctx);
            let ratio = want / self.rendered_ppt.max(0.01);
            if !(0.85..=1.15).contains(&ratio) {
                self.preview_due = Some(now + Duration::from_millis(120));
            }
        }
        if let Some(due) = self.preview_due {
            if now >= due {
                self.preview_due = None;
                self.worker.send(Job::Preview {
                    generation: self.generation,
                    text: self.text.clone(),
                    pixels_per_pt: self.wanted_ppt(ctx),
                });
                self.in_flight += 1;
            } else {
                ctx.request_repaint_after(due - now);
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let sc = |m: Modifiers, k: Key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)))
        };
        if sc(cmd_shift, Key::S) {
            self.save_as_dialog(None);
        }
        if sc(cmd, Key::S) {
            self.save(None);
        }
        if sc(cmd_shift, Key::E) {
            self.export_as_dialog();
        }
        if sc(cmd, Key::E) {
            self.export(None);
        }
        if sc(cmd, Key::N) {
            self.request(Pending::New(None));
        }
        if sc(cmd, Key::O) {
            self.open_dialog();
        }
        if sc(cmd, Key::B) {
            self.prefs.sidebar = !self.prefs.sidebar;
        }
        if sc(cmd, Key::Equals) || sc(cmd, Key::Plus) || sc(cmd_shift, Key::Equals) {
            self.zoom_step(1.1);
        }
        if sc(cmd, Key::Minus) {
            self.zoom_step(1.0 / 1.1);
        }
        if sc(cmd, Key::Num0) {
            self.prefs.fit_width = true;
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if let Some(p) = dropped.into_iter().find(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("md" | "markdown" | "txt")
            )
        }) {
            self.request(Pending::Open(p));
        }
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Pending::Close);
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = format!(
            "{}{} - invgen",
            if self.dirty() { "\u{2022} " } else { "" },
            self.display_name()
        );
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close(&ctx);
        self.shortcuts(&ctx);
        self.handle_drops(&ctx);
        self.poll(&ctx);
        view::draw(self, ui);
        self.update_title(&ctx);
        if self.in_flight > 0 {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, STORAGE_KEY, &self.prefs);
    }
}

/// Replace the home directory with `~` for display.
pub fn tilde(p: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(rest) = p.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    p.display().to_string()
}

/// Open a file with the desktop's default application, without waiting for it.
pub fn open_external(p: &Path) {
    let _ = opener(p)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// The platform's "open with the default application" command for `p`:
/// `open` on macOS, `explorer.exe` on Windows (it hands the path to
/// ShellExecute and, unlike `cmd /C start`, needs no shell quoting, so names
/// with `&` or `^` still open), `xdg-open` elsewhere.
fn opener(p: &Path) -> std::process::Command {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer.exe"
    } else {
        "xdg-open"
    };
    let mut cmd = std::process::Command::new(program);
    cmd.arg(p);
    cmd
}

const FALLBACK_TEMPLATE: &str = "---\nstatus: draft\n---\n\n# Services\n\n| Description | Qty | Unit | Rate |\n|-------------|-----|------|------|\n| Consulting  | 1   | days | 0.00 |\n";

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opener_uses_the_platform_command() {
        let cmd = opener(Path::new("out.pdf"));
        let want = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(windows) {
            "explorer.exe"
        } else {
            "xdg-open"
        };
        assert_eq!(cmd.get_program(), want);
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["out.pdf"]);
    }
}
