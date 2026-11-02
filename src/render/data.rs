//! The JSON view model handed to the typst template. All numbers are
//! pre-formatted here so the template only lays out strings.

use serde::Serialize;

use crate::model::{
    Decimal, Invoice, Party, Status, format_amount, format_date, format_money, round_cents,
};

#[derive(Serialize)]
pub(super) struct View {
    number: String,
    status: &'static str,
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

impl From<&Invoice> for View {
    fn from(inv: &Invoice) -> Self {
        let cur = inv.currency.trim();
        let tax_label = match inv.tax_label.trim() {
            "" => "VAT".to_string(),
            l => l.to_string(),
        };
        View {
            number: inv.number.trim().to_string(),
            status: match inv.status {
                Status::Pending => "pending",
                Status::Paid => "paid",
                Status::Overdue => "overdue",
                Status::Draft => "draft",
            },
            date: format_date(inv.date),
            due: inv.due.map(format_date),
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
                    qty: format_qty(it.quantity),
                    unit: it.unit.trim().to_string(),
                    rate: format_amount(it.rate),
                    total: format_money(cur, it.total()),
                })
                .collect(),
            subtotal: format_money(cur, inv.subtotal()),
            tax_label,
            tax_rate: format!("{}%", format_qty(inv.tax_rate)),
            tax: format_money(cur, inv.tax()),
            total: format_money(cur, inv.total()),
            notes: clean(&inv.notes),
            signature: opt(&inv.signature),
        }
    }
}
