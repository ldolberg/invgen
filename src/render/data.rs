//! The JSON view model handed to the typst template. All numbers are
//! pre-formatted here so the template only lays out strings.
//!
//! This is the public data contract for custom templates, documented in
//! docs/TEMPLATES.md. Bump `CONTRACT_VERSION` on any breaking change
//! (renamed or removed field, changed meaning); adding fields is compatible.

use std::collections::BTreeMap;

use serde::Serialize;

use super::i18n;
use super::locale::Formatter;

use crate::model::{Decimal, Invoice, Party, Status, round_cents, tax_note_preset};

/// Version of the template data contract (docs/TEMPLATES.md).
pub const CONTRACT_VERSION: u32 = 1;

#[derive(Serialize)]
pub(super) struct View {
    contract_version: u32,
    language: String,
    /// Formatting locale of the amounts and dates, e.g. "en" or "de-CH".
    locale: String,
    labels: BTreeMap<String, String>,
    /// Virtual path of the logo image (e.g. "/logo.svg"), or null.
    logo: Option<String>,
    number: String,
    status: &'static str,
    status_label: String,
    date: String,
    due: Option<String>,
    currency: String,
    accent: String,
    from: PartyView,
    to: PartyView,
    payment: PaymentView,
    items: Vec<ItemView>,
    subtotal: String,
    tax_label: String,
    tax_rate: String,
    tax: String,
    /// One entry per distinct tax rate, highest first (empty without items).
    tax_breakdown: Vec<TaxGroupView>,
    /// Legal tax mention (preset translated, or free text), or null.
    tax_note: Option<String>,
    total: String,
    notes: Vec<String>,
    signature: Option<String>,
}

#[derive(Serialize)]
struct PartyView {
    name: String,
    address: Vec<String>,
    tax: Vec<String>,
    email: Option<String>,
    phone: Option<String>,
}

#[derive(Serialize)]
struct PaymentView {
    holder: Option<String>,
    iban: Option<String>,
    bic: Option<String>,
    notes: Vec<String>,
}

#[derive(Serialize)]
struct ItemView {
    description: String,
    detail: Option<String>,
    qty: String,
    unit: String,
    rate: String,
    total: String,
    /// Tax rate that applies to this line, e.g. "21%".
    tax_rate: String,
}

#[derive(Serialize)]
struct TaxGroupView {
    /// e.g. "21%"
    rate: String,
    /// Sum of the line totals at this rate, e.g. "EUR 1,000.00".
    base: String,
    /// Tax at this rate, e.g. "EUR 210.00".
    tax: String,
}

/// Drop empty / whitespace-only strings.
fn clean(v: &[String]) -> Vec<String> {
    v.iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn opt(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 21 -> "21", 7.50 -> "7.5", 0.125 -> "0.13" (at most 2 decimals, rounded
/// half away from zero, no trailing zeros).
pub(super) fn format_qty(q: Decimal) -> String {
    let r = round_cents(q).normalize();
    if r.is_zero() {
        "0".into()
    } else {
        r.to_string()
    }
}

/// Accept "#rgb" / "#rrggbb" (with or without '#'); anything else falls back.
pub(super) fn normalize_accent(s: &str) -> String {
    let h = s.trim().trim_start_matches('#');
    let ok = (h.len() == 3 || h.len() == 6) && h.chars().all(|c| c.is_ascii_hexdigit());
    if ok {
        format!("#{}", h.to_ascii_lowercase())
    } else {
        "#111827".into()
    }
}

impl From<&Party> for PartyView {
    fn from(p: &Party) -> Self {
        PartyView {
            name: p.name.trim().to_string(),
            address: clean(&p.address),
            tax: clean(&p.tax),
            email: opt(&p.email),
            phone: opt(&p.phone),
        }
    }
}

impl View {
    /// `logo` is the virtual path the logo bytes are served at, if any.
    pub(super) fn new(inv: &Invoice, logo: Option<String>) -> Self {
        let cur = inv.currency.trim();
        // An unsupported code (only possible when building an Invoice by hand)
        // renders in English rather than failing in typst.
        let language = i18n::normalize_language(&inv.language).unwrap_or("en");
        let labels = i18n::labels(language, &inv.labels);
        let fmt = Formatter::new(inv);
        let tax_label = match inv.tax_label.trim() {
            "" => labels["tax"].clone(),
            l => l.to_string(),
        };
        let breakdown = inv.tax_breakdown();
        // With a single rate (the common case) `tax_rate` is that rate, even
        // when every line overrides the invoice default.
        let tax_rate = match breakdown.as_slice() {
            [only] => only.rate,
            _ => inv.tax_rate,
        };
        let tax_note = opt(&inv.tax_note).map(|n| match tax_note_preset(&n) {
            Some(p) => labels[&format!("tax_note_{p}")].clone(),
            None => n,
        });
        let status = match inv.status {
            Status::Pending => "pending",
            Status::Paid => "paid",
            Status::Overdue => "overdue",
            Status::Draft => "draft",
        };
        View {
            contract_version: CONTRACT_VERSION,
            language: language.to_string(),
            locale: fmt.locale().to_string(),
            status_label: labels[&format!("status_{status}")].clone(),
            labels,
            logo,
            number: inv.number.trim().to_string(),
            status,
            date: fmt.date(inv.date),
            due: inv.due.map(|d| fmt.date(d)),
            currency: cur.to_string(),
            accent: normalize_accent(&inv.accent),
            from: (&inv.from).into(),
            to: (&inv.to).into(),
            payment: PaymentView {
                holder: opt(&inv.payment.holder),
                iban: opt(&inv.payment.iban),
                bic: opt(&inv.payment.bic),
                notes: clean(&inv.payment.notes),
            },
            items: inv
                .items
                .iter()
                .map(|it| ItemView {
                    description: it.description.trim().to_string(),
                    detail: opt(&it.detail),
                    qty: fmt.qty(it.quantity),
                    unit: it.unit.trim().to_string(),
                    rate: fmt.amount(it.rate),
                    total: fmt.money(cur, it.total()),
                    tax_rate: fmt.percent(inv.item_tax_rate(it)),
                })
                .collect(),
            subtotal: fmt.money(cur, inv.subtotal()),
            tax_label,
            tax_rate: fmt.percent(tax_rate),
            tax: fmt.money(cur, inv.tax()),
            tax_breakdown: breakdown
                .iter()
                .map(|g| TaxGroupView {
                    rate: fmt.percent(g.rate),
                    base: fmt.money(cur, g.base),
                    tax: fmt.money(cur, g.tax),
                })
                .collect(),
            tax_note,
            total: fmt.money(cur, inv.total()),
            notes: clean(&inv.notes),
            signature: opt(&inv.signature),
        }
    }
}
