//! First-run welcome form: shown when there is no config file yet. It writes
//! a commented config (see `crate::setup`) and opens the first invoice.

use super::forms::{self, Check};
use super::theme::{self, Palette};
use crate::config::ClientConfig;
use crate::config_edit::suggest_client_key;
use crate::model::{Party, Payment};
use crate::setup::{self, Setup};
use eframe::egui::{self, Color32, CornerRadius, Frame, Id, Margin, RichText, Stroke};

pub struct Form {
    name: String,
    address: String,
    tax: String,
    email: String,
    holder: String,
    iban: String,
    bic: String,
    client_name: String,
    client_address: String,
    client_tax: String,
    rate: String,
    unit: String,
    description: String,
    prefix: String,
    currency: String,
    tax_rate: String,
    invoices_dir: String,
    /// Show "required" errors only after the first attempt.
    tried: bool,
    /// Writing the config failed.
    pub error: Option<String>,
}

impl Default for Form {
    fn default() -> Self {
        Form {
            name: String::new(),
            address: String::new(),
            tax: String::new(),
            email: String::new(),
            holder: String::new(),
            iban: String::new(),
            bic: String::new(),
            client_name: String::new(),
            client_address: String::new(),
            client_tax: String::new(),
            rate: String::new(),
            unit: "days".into(),
            description: String::new(),
            prefix: "INV".into(),
            currency: "EUR".into(),
            tax_rate: "0".into(),
            invoices_dir: "~/Documents/Invoices".into(),
            tried: false,
            error: None,
        }
    }
}

pub enum Outcome {
    /// Write this config and open the first invoice for `client_key`.
    Create(Box<Setup>),
    /// Write the commented template as is and open it in a text editor.
    Skip,
}

/// Per-field checks: `Err` blocks, `Warn` only informs.
struct Checks {
    name: Check,
    client_name: Check,
    rate: Check,
    tax_rate: Check,
    prefix: Check,
    iban: Check,
    invoices_dir: Check,
}

impl Form {
    fn checks(&self) -> Checks {
        let required = |v: &str, what: &str| {
            if v.trim().is_empty() && self.tried {
                Check::Err(format!("{what} is required"))
            } else {
                Check::Ok
            }
        };
        Checks {
            name: required(&self.name, "Your name"),
            client_name: required(&self.client_name, "The client's name"),
            rate: forms::number_check(&self.rate),
            tax_rate: forms::number_check(&self.tax_rate),
            prefix: match setup::check_prefix(&self.prefix) {
                Ok(()) => Check::Ok,
                Err(e) => Check::Err(e),
            },
            iban: if self.iban.trim().is_empty() {
                Check::Ok
            } else {
                match setup::check_iban(&self.iban) {
                    Ok(()) => Check::Ok,
                    Err(e) => Check::Warn(e),
                }
            },
            invoices_dir: if self.invoices_dir.trim().is_empty() {
                Check::Err("choose a folder for the invoices".into())
            } else {
                Check::Ok
            },
        }
    }

    fn valid(&self) -> bool {
        let c = self.checks();
        !self.name.trim().is_empty()
            && !self.client_name.trim().is_empty()
            && [
                &c.rate,
                &c.tax_rate,
                &c.prefix,
                &c.invoices_dir,
                &c.name,
                &c.client_name,
            ]
            .iter()
            .all(|c| !c.is_err())
    }

    fn setup(&self) -> Setup {
        let client_key = suggest_client_key(&self.client_name);
        Setup {
            from: Party {
                name: self.name.trim().to_string(),
                address: setup::lines(&self.address),
                tax: setup::lines(&self.tax),
                email: setup::opt(&self.email),
                phone: None,
                logo: None,
            },
            payment: Payment {
                holder: setup::opt(&self.holder),
                iban: setup::opt(&self.iban).map(|s| s.to_uppercase()),
                bic: setup::opt(&self.bic).map(|s| s.to_uppercase()),
                notes: Vec::new(),
            },
            client_key,
            client: ClientConfig {
                party: Party {
                    name: self.client_name.trim().to_string(),
                    address: setup::lines(&self.client_address),
                    tax: setup::lines(&self.client_tax),
                    ..Party::default()
                },
                rate: setup::number(&self.rate).ok().flatten(),
                unit: setup::opt(&self.unit),
                description: setup::opt(&self.description),
                ..ClientConfig::default()
            },
            number_prefix: self.prefix.trim().to_string(),
            currency: self.currency.trim().to_uppercase(),
            tax_rate: setup::number(&self.tax_rate)
                .ok()
                .flatten()
                .unwrap_or_default(),
            invoices_dir: self.invoices_dir.trim().to_string(),
        }
    }
}

/// Draw the welcome form. `config` is the path the config will be written to.
pub fn show(form: &mut Form, ctx: &egui::Context, p: &Palette, config: &str) -> Option<Outcome> {
    let mut out = None;
    let screen = ctx.content_rect();
    egui::Modal::new(Id::unique("onboarding"))
        .backdrop_color(Color32::from_black_alpha(140))
        .frame(
            Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(22))
                .shadow(ctx.global_style().visuals.window_shadow),
        )
        .show(ctx, |ui| {
            ui.set_width(560.0_f32.min(screen.width() - 80.0));
            ui.label(
                RichText::new("Welcome to invgen")
                    .font(theme::strong(19.0))
                    .color(p.text_strong),
            );
            ui.add_space(2.0);
            ui.label(
                RichText::new(
                    "A few details once, and every invoice after that only needs what changes. \
                     You can change all of this later in the config file.",
                )
                .color(p.text_weak),
            );
            ui.add_space(10.0);

            let checks = form.checks();
            egui::ScrollArea::vertical()
                .max_height((screen.height() - 230.0).max(200.0))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    forms::section(ui, p, "YOU");
                    forms::grid(ui, "ob-you", |ui| {
                        forms::text(ui, p, "Name", &mut form.name, "Alex Morgan", &checks.name);
                        forms::lines(
                            ui,
                            p,
                            "Address",
                            &mut form.address,
                            "Rue de la Gare 1\n1616 Luxembourg\nLuxembourg",
                        );
                        forms::lines(ui, p, "Tax lines", &mut form.tax, "VAT ID: LU12345678");
                        forms::text(
                            ui,
                            p,
                            "Email",
                            &mut form.email,
                            "alex@example.com",
                            &Check::Ok,
                        );
                    });

                    forms::section(ui, p, "BANK DETAILS");
                    forms::grid(ui, "ob-bank", |ui| {
                        forms::text(
                            ui,
                            p,
                            "Account holder",
                            &mut form.holder,
                            "ALEX MORGAN",
                            &Check::Ok,
                        );
                        forms::text(
                            ui,
                            p,
                            "IBAN",
                            &mut form.iban,
                            "DE89 3704 0044 0532 0130 00",
                            &checks.iban,
                        );
                        forms::text(ui, p, "BIC", &mut form.bic, "COBADEFFXXX", &Check::Ok);
                    });

                    forms::section(ui, p, "FIRST CLIENT");
                    forms::grid(ui, "ob-client", |ui| {
                        forms::text(
                            ui,
                            p,
                            "Name",
                            &mut form.client_name,
                            "Northwind Labs SL",
                            &checks.client_name,
                        );
                        forms::lines(
                            ui,
                            p,
                            "Address",
                            &mut form.client_address,
                            "Calle Mayor, 12\n28001 Madrid\nSpain",
                        );
                        forms::lines(
                            ui,
                            p,
                            "Tax lines",
                            &mut form.client_tax,
                            "Tax ID: B12345678",
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

                    forms::section(ui, p, "INVOICES");
                    forms::grid(ui, "ob-invoices", |ui| {
                        forms::text(
                            ui,
                            p,
                            "Number prefix",
                            &mut form.prefix,
                            "INV",
                            &checks.prefix,
                        );
                        forms::hint_row(
                            ui,
                            p,
                            &format!(
                                "Numbers look like {}-{}-01",
                                form.prefix.trim(),
                                chrono::Local::now().format("%Y")
                            ),
                        );
                        forms::text(ui, p, "Currency", &mut form.currency, "EUR", &Check::Ok);
                        forms::text(
                            ui,
                            p,
                            "Tax rate (%)",
                            &mut form.tax_rate,
                            "0",
                            &checks.tax_rate,
                        );
                        forms::text(
                            ui,
                            p,
                            "Invoices folder",
                            &mut form.invoices_dir,
                            "~/Documents/Invoices",
                            &checks.invoices_dir,
                        );
                    });
                });

            if let Some(e) = &form.error {
                ui.add_space(8.0);
                ui.label(RichText::new(e).color(p.error));
            }
            ui.add_space(12.0);
            ui.label(
                RichText::new(format!("Saved to {config}"))
                    .size(11.0)
                    .color(p.text_weak),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Skip, I'll edit YAML")
                    .on_hover_text(
                        "Write the commented example config and open it in your text editor",
                    )
                    .clicked()
                {
                    out = Some(Outcome::Skip);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let create = egui::Button::new(
                        RichText::new("Create my first invoice")
                            .font(theme::strong(13.0))
                            .color(Color32::WHITE),
                    )
                    .fill(p.accent);
                    if ui.add(create).clicked() {
                        form.tried = true;
                        if form.valid() {
                            out = Some(Outcome::Create(Box::new(form.setup())));
                        }
                    }
                });
            });
        });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Decimal;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn form_maps_to_setup() {
        let mut f = Form {
            name: " Alex Morgan ".into(),
            address: "Rue de la Gare 1\n\n1616 Luxembourg\n".into(),
            iban: "de89 3704 0044 0532 0130 00".into(),
            client_name: "Northwind Labs SL".into(),
            rate: "450,50".into(),
            ..Form::default()
        };
        assert!(f.valid());
        let s = f.setup();
        assert_eq!(s.from.name, "Alex Morgan");
        assert_eq!(s.from.address, vec!["Rue de la Gare 1", "1616 Luxembourg"]);
        assert_eq!(s.from.email, None);
        assert_eq!(
            s.payment.iban.as_deref(),
            Some("DE89 3704 0044 0532 0130 00")
        );
        assert_eq!(s.client_key, "northwind-labs-sl");
        assert_eq!(s.client.rate, Some(d("450.5")));
        assert_eq!(s.number_prefix, "INV");
        assert!(s.render().is_ok());

        f.rate = "lots".into();
        assert!(!f.valid());
        f.rate.clear();
        f.client_name.clear();
        assert!(!f.valid());
    }

    #[test]
    fn iban_is_a_warning_not_an_error() {
        let f = Form {
            name: "A".into(),
            client_name: "B".into(),
            iban: "DE00 1234".into(),
            ..Form::default()
        };
        assert!(matches!(f.checks().iban, Check::Warn(_)));
        assert!(f.valid());
    }
}
