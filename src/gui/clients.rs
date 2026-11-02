//! Clients window: list, add, edit and delete the clients of `config.yaml`.
//! Writes go through `crate::config_edit`, so comments and order are kept.

use super::forms::{self, Check};
use super::theme::{self, Palette};
use crate::config::{ClientConfig, Config};
use crate::config_edit::{suggest_client_key, valid_client_key};
use crate::model::Party;
use crate::setup;
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, RichText, Sense, vec2};

/// The editable text of one client.
#[derive(Clone, Default, PartialEq)]
pub struct ClientForm {
    pub key: String,
    pub name: String,
    pub address: String,
    pub tax: String,
    pub email: String,
    pub phone: String,
    pub currency: String,
    pub tax_rate: String,
    pub rate: String,
    pub unit: String,
    pub description: String,
}

fn num(n: Option<crate::model::Decimal>) -> String {
    n.map(|n| n.to_string()).unwrap_or_default()
}

impl ClientForm {
    pub fn from_client(key: &str, c: &ClientConfig) -> ClientForm {
        ClientForm {
            key: key.to_string(),
            name: c.party.name.clone(),
            address: c.party.address.join("\n"),
            tax: c.party.tax.join("\n"),
            email: c.party.email.clone().unwrap_or_default(),
            phone: c.party.phone.clone().unwrap_or_default(),
            currency: c.currency.clone().unwrap_or_default(),
            tax_rate: num(c.tax_rate),
            rate: num(c.rate),
            unit: c.unit.clone().unwrap_or_default(),
            description: c.description.clone().unwrap_or_default(),
        }
    }

    /// The client these fields describe (call when `checks` has no error).
    pub fn to_client(&self) -> ClientConfig {
        ClientConfig {
            party: Party {
                name: self.name.trim().to_string(),
                address: setup::lines(&self.address),
                tax: setup::lines(&self.tax),
                email: setup::opt(&self.email),
                phone: setup::opt(&self.phone),
                logo: None,
                ..Party::default()
            },
            currency: setup::opt(&self.currency).map(|c| c.to_uppercase()),
            tax_rate: setup::number(&self.tax_rate).ok().flatten(),
            rate: setup::number(&self.rate).ok().flatten(),
            unit: setup::opt(&self.unit),
            description: setup::opt(&self.description),
            language: None,
            tax_note: None,
            format: Default::default(),
        }
    }
}

/// Inline validation of a client form against the clients already in `cfg`.
pub struct Checks {
    pub key: Check,
    pub name: Check,
    pub tax_rate: Check,
    pub rate: Check,
}

impl Checks {
    pub fn ok(&self) -> bool {
        ![&self.key, &self.name, &self.tax_rate, &self.rate]
            .iter()
            .any(|c| c.is_err())
    }
}

/// `original` is the key being edited (None for a new client).
pub fn check(form: &ClientForm, original: Option<&str>, cfg: &Config) -> Checks {
    let key = form.key.trim();
    let key_check = match valid_client_key(key) {
        Err(e) => Check::Err(e),
        Ok(()) if Some(key) != original && cfg.clients.contains_key(key) => {
            Check::Err(format!("another client already uses `{key}`"))
        }
        Ok(()) if original.is_some_and(|o| o != key) => {
            Check::Warn("invoices that say `client: <old key>` will need the new key".into())
        }
        Ok(()) => Check::Ok,
    };
    Checks {
        key: key_check,
        name: if form.name.trim().is_empty() {
            Check::Err("the client needs a name".into())
        } else {
            Check::Ok
        },
        tax_rate: forms::number_check(&form.tax_rate),
        rate: forms::number_check(&form.rate),
    }
}

/// A key for a new client that is not taken yet.
fn unique_key(name: &str, cfg: &Config) -> String {
    let base = suggest_client_key(name);
    if !cfg.clients.contains_key(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|k| !cfg.clients.contains_key(k))
        .expect("some suffix is free")
}

#[derive(Default)]
pub struct Panel {
    /// Key of the client being edited; None while adding a new one.
    pub original: Option<String>,
    pub form: ClientForm,
    /// What the form held when it was loaded, to know if it changed.
    loaded: ClientForm,
    /// The user typed a key, so stop deriving it from the name.
    key_edited: bool,
    /// A click on another client while this one has unsaved edits.
    switch_to: Option<Option<String>>,
    confirm_delete: bool,
    /// Last save problem: (message, the file uses YAML we cannot edit).
    pub problem: Option<(String, bool)>,
}

pub enum Action {
    Save {
        original: Option<String>,
        key: String,
        client: Box<ClientConfig>,
    },
    Delete(String),
    NewInvoice(String),
    OpenConfig,
}

impl Panel {
    pub fn new(cfg: &Config) -> Panel {
        let mut p = Panel::default();
        match cfg.clients.iter().next() {
            Some((k, c)) => p.select(Some(k), Some(c)),
            None => p.select(None, None),
        }
        p
    }

    fn select(&mut self, key: Option<&str>, client: Option<&ClientConfig>) {
        self.original = key.map(str::to_string);
        self.form = match (key, client) {
            (Some(k), Some(c)) => ClientForm::from_client(k, c),
            _ => ClientForm {
                unit: "days".into(),
                ..ClientForm::default()
            },
        };
        self.loaded = self.form.clone();
        self.key_edited = key.is_some();
        self.switch_to = None;
        self.confirm_delete = false;
        self.problem = None;
    }

    /// Select `key` (None: a new client) as stored in `cfg`.
    pub fn select_in(&mut self, cfg: &Config, key: Option<&str>) {
        let client = key.and_then(|k| cfg.clients.get(k));
        self.select(key.filter(|_| client.is_some()), client);
    }

    pub fn dirty(&self) -> bool {
        self.form != self.loaded
    }

    /// The config was reloaded: keep editing the same client if it still exists.
    pub fn refresh(&mut self, cfg: &Config) {
        if self.dirty() {
            return;
        }
        let key = self.original.clone();
        match key.as_deref().and_then(|k| cfg.clients.get(k)) {
            Some(c) => {
                let k = key.clone().unwrap_or_default();
                self.select(Some(&k), Some(c));
            }
            None if key.is_some() => {
                self.select_in(cfg, cfg.clients.keys().next().map(|k| k.as_str()))
            }
            None => {}
        }
    }

    /// The window opens: show the client being edited, or the first one.
    pub fn on_open(&mut self, cfg: &Config) {
        if self.dirty() {
            return;
        }
        let key = self
            .original
            .clone()
            .filter(|k| cfg.clients.contains_key(k))
            .or_else(|| cfg.clients.keys().next().cloned());
        self.select_in(cfg, key.as_deref());
    }

    /// A new client nobody typed into yet: do not shout "required" at it.
    fn untouched(&self) -> bool {
        self.original.is_none() && !self.dirty()
    }

    fn request_select(&mut self, cfg: &Config, key: Option<String>) {
        if self.dirty() {
            self.switch_to = Some(key);
        } else {
            self.select_in(cfg, key.as_deref());
        }
    }
}

/// Draw the window. Returns what the user asked for; the app performs it.
pub fn show(
    panel: &mut Panel,
    open: &mut bool,
    ctx: &egui::Context,
    p: &Palette,
    cfg: &Config,
    config_path: &str,
) -> Option<Action> {
    let mut action = None;
    egui::Window::new("Clients")
        .open(open)
        .collapsible(false)
        .resizable(true)
        .default_size([660.0, 600.0])
        .min_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Stored in {config_path}"))
                        .size(11.0)
                        .color(p.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .small_button("Open config file")
                        .on_hover_text("Edit config.yaml in your text editor")
                        .clicked()
                    {
                        action = Some(Action::OpenConfig);
                    }
                });
            });
            ui.separator();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(180.0);
                    list(panel, ui, p, cfg);
                });
                ui.separator();
                ui.vertical(|ui| {
                    if let Some(a) = editor(panel, ui, p, cfg) {
                        action = Some(a);
                    }
                });
            });
        });
    action
}

fn list(panel: &mut Panel, ui: &mut egui::Ui, p: &Palette, cfg: &Config) {
    if ui
        .add_sized(vec2(180.0, 28.0), egui::Button::new("+ New client"))
        .clicked()
    {
        panel.request_select(cfg, None);
    }
    ui.add_space(4.0);
    if cfg.clients.is_empty() {
        ui.label(RichText::new("No clients yet").color(p.text_weak));
        return;
    }
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .id_salt("clients-list")
        .auto_shrink([false, true])
        .max_height(460.0)
        .show(ui, |ui| {
            for (key, c) in &cfg.clients {
                let selected = panel.original.as_deref() == Some(key.as_str());
                let (rect, resp) = ui.allocate_exact_size(vec2(176.0, 40.0), Sense::click());
                let fill = if selected {
                    p.accent_soft
                } else if resp.hovered() {
                    p.bg
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
                let inner = rect.shrink2(vec2(10.0, 5.0));
                let painter = ui.painter_at(rect);
                let name = if c.party.name.is_empty() {
                    key.as_str()
                } else {
                    c.party.name.as_str()
                };
                painter.text(
                    inner.left_top(),
                    egui::Align2::LEFT_TOP,
                    name,
                    theme::strong(13.0),
                    p.text_strong,
                );
                painter.text(
                    inner.left_bottom(),
                    egui::Align2::LEFT_BOTTOM,
                    key,
                    egui::FontId::monospace(11.0),
                    p.text_weak,
                );
                if resp
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                    && !selected
                {
                    clicked = Some(key.clone());
                }
            }
        });
    if let Some(k) = clicked {
        panel.request_select(cfg, Some(k));
    }
}

fn editor(panel: &mut Panel, ui: &mut egui::Ui, p: &Palette, cfg: &Config) -> Option<Action> {
    let mut action = None;
    let title = match &panel.original {
        Some(_) => "Edit client",
        None => "New client",
    };
    ui.label(
        RichText::new(title)
            .font(theme::strong(15.0))
            .color(p.text_strong),
    );

    if let Some(target) = panel.switch_to.clone() {
        ui.horizontal(|ui| {
            ui.label(RichText::new("This client has unsaved changes.").color(p.warn));
            if ui.small_button("Discard").clicked() {
                panel.select_in(cfg, target.as_deref());
            }
            if ui.small_button("Keep editing").clicked() {
                panel.switch_to = None;
            }
        });
    }

    let checks = check(&panel.form, panel.original.as_deref(), cfg);
    // Saving stays blocked, but an empty new form shows no errors yet.
    let (name_shown, key_shown) = if panel.untouched() {
        (Check::Ok, Check::Ok)
    } else {
        (checks.name.clone(), checks.key.clone())
    };
    let form = &mut panel.form;
    egui::ScrollArea::vertical()
        .id_salt("client-form")
        .auto_shrink([false, true])
        .max_height(420.0)
        .show(ui, |ui| {
            forms::grid(ui, "client-grid", |ui| {
                let name_changed = forms::text(
                    ui,
                    p,
                    "Name",
                    &mut form.name,
                    "Northwind Labs SL",
                    &name_shown,
                )
                .changed();
                if name_changed && !panel.key_edited {
                    form.key = unique_key(&form.name, cfg);
                }
                if forms::text(ui, p, "Key", &mut form.key, "northwind", &key_shown).changed() {
                    panel.key_edited = true;
                }
                forms::hint_row(ui, p, "Invoices pick this client with `client: <key>`");
                forms::lines(
                    ui,
                    p,
                    "Address",
                    &mut form.address,
                    "Calle Mayor, 12\n28001 Madrid\nSpain",
                );
                forms::lines(ui, p, "Tax lines", &mut form.tax, "Tax ID: B12345678");
                forms::text(
                    ui,
                    p,
                    "Email",
                    &mut form.email,
                    "billing@example.com",
                    &Check::Ok,
                );
                forms::text(ui, p, "Phone", &mut form.phone, "", &Check::Ok);
                forms::text(
                    ui,
                    p,
                    "Currency",
                    &mut form.currency,
                    &format!("{} (config default)", cfg.currency),
                    &Check::Ok,
                );
                forms::text(
                    ui,
                    p,
                    "Tax rate (%)",
                    &mut form.tax_rate,
                    &format!("{} (config default)", cfg.tax_rate),
                    &checks.tax_rate,
                );
                forms::text(ui, p, "Rate", &mut form.rate, "450", &checks.rate);
                forms::text(ui, p, "Unit", &mut form.unit, "days", &Check::Ok);
                forms::text(
                    ui,
                    p,
                    "Description",
                    &mut form.description,
                    "Consulting services",
                    &Check::Ok,
                );
            });
        });

    if let Some((msg, unsupported)) = &panel.problem {
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(p.error_bg)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.label(RichText::new(msg).color(p.error));
                if *unsupported {
                    ui.label(
                        RichText::new("Your config file was not changed. Edit it by hand instead.")
                            .color(p.text),
                    );
                    if ui.button("Open config file").clicked() {
                        action = Some(Action::OpenConfig);
                    }
                }
            });
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let dirty = panel.dirty() || panel.original.is_none();
        let can_save = checks.ok() && dirty;
        let save = egui::Button::new(
            RichText::new(if panel.original.is_some() {
                "Save"
            } else {
                "Add client"
            })
            .font(theme::strong(13.0))
            .color(Color32::WHITE),
        )
        .fill(if can_save {
            p.accent
        } else {
            p.accent.gamma_multiply(0.5)
        });
        if ui.add_enabled(can_save, save).clicked() {
            action = Some(Action::Save {
                original: panel.original.clone(),
                key: panel.form.key.trim().to_string(),
                client: Box::new(panel.form.to_client()),
            });
        }
        if ui
            .add_enabled(panel.dirty(), egui::Button::new("Revert"))
            .clicked()
        {
            panel.form = panel.loaded.clone();
        }
        if let Some(key) = panel.original.clone() {
            if ui
                .add_enabled(!panel.dirty(), egui::Button::new("New invoice"))
                .on_hover_text("New invoice for this client")
                .on_disabled_hover_text("Save the client first")
                .clicked()
            {
                action = Some(Action::NewInvoice(key.clone()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if panel.confirm_delete {
                    let del =
                        egui::Button::new(RichText::new("Delete for good").color(Color32::WHITE))
                            .fill(p.error);
                    if ui.add(del).clicked() {
                        action = Some(Action::Delete(key.clone()));
                    }
                    if ui.button("Cancel").clicked() {
                        panel.confirm_delete = false;
                    }
                } else if ui.button(RichText::new("Delete").color(p.error)).clicked() {
                    panel.confirm_delete = true;
                }
            });
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        crate::config_edit::parse_config(crate::config::DEFAULT_CONFIG).unwrap()
    }

    #[test]
    fn form_round_trips_a_client() {
        let cfg = cfg();
        let c = &cfg.clients["acme"];
        let form = ClientForm::from_client("acme", c);
        assert_eq!(form.address, "Street 1\nCity\n00000\nCountry");
        assert_eq!(form.rate, "450");
        let back = form.to_client();
        assert_eq!(back.party, c.party);
        assert_eq!(back.rate, c.rate);
        assert_eq!(back.unit, c.unit);
        assert_eq!(back.description, c.description);
        assert_eq!(back.currency, None);
    }

    #[test]
    fn checks_flag_bad_input_inline() {
        let cfg = cfg();
        let mut f = ClientForm {
            key: "acme".into(),
            name: "Other".into(),
            ..ClientForm::default()
        };
        // New client with a taken key.
        let c = check(&f, None, &cfg);
        assert!(c.key.is_err());
        assert!(!c.ok());
        // Editing acme itself is fine.
        assert!(check(&f, Some("acme"), &cfg).ok());
        f.key = "bad key".into();
        assert!(check(&f, None, &cfg).key.is_err());
        f.key = "true".into();
        assert!(check(&f, None, &cfg).key.is_err());
        f.key = "contoso".into();
        f.rate = "12x".into();
        let c = check(&f, None, &cfg);
        assert!(c.rate.is_err() && !c.ok());
        f.rate = "12".into();
        f.name = " ".into();
        assert!(check(&f, None, &cfg).name.is_err());
        // Renaming warns about invoices that use the old key.
        f.name = "ACME".into();
        let c = check(&f, Some("acme"), &cfg);
        assert!(matches!(c.key, Check::Warn(_)) && c.ok());
    }

    #[test]
    fn new_keys_are_unique() {
        let cfg = cfg();
        assert_eq!(unique_key("Contoso GmbH", &cfg), "contoso-gmbh");
        assert_eq!(unique_key("Acme", &cfg), "acme-2");
    }

    #[test]
    fn panel_tracks_dirty_and_refresh() {
        let cfg = cfg();
        let mut panel = Panel::new(&cfg);
        assert_eq!(panel.original.as_deref(), Some("acme"));
        assert!(!panel.dirty());
        panel.form.rate = "500".into();
        assert!(panel.dirty());
        // Switching away asks first.
        panel.request_select(&cfg, None);
        assert!(panel.switch_to.is_some());
        assert_eq!(panel.original.as_deref(), Some("acme"));
        panel.select_in(&cfg, None);
        assert!(panel.original.is_none());
        assert_eq!(panel.form.unit, "days");
        // A refresh after the client disappeared falls back to the first one.
        let mut panel = Panel::new(&cfg);
        let empty = Config::default();
        panel.refresh(&empty);
        assert!(panel.original.is_none());
    }
}
