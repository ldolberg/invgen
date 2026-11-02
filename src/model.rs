//! Shared invoice data model. This is the contract between the parser,
//! the renderer, the GUI and the CLI. Change it only deliberately.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// A postal party (sender or client).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Party {
    pub name: String,
    /// Address lines, rendered one per line (street, city, postcode, country...).
    pub address: Vec<String>,
    /// Free-form tax lines, e.g. "VAT ID: LU12345678".
    pub tax: Vec<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
}

/// How the client should pay.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Payment {
    /// Account holder as shown on the invoice.
    pub holder: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    /// Extra free-form lines (bank name, reference instructions...).
    pub notes: Vec<String>,
}

/// One billable line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineItem {
    pub description: String,
    /// Optional secondary line under the description (e.g. "October 2026 period").
    pub detail: Option<String>,
    pub quantity: f64,
    /// Unit label, e.g. "days", "hours". Empty means none.
    pub unit: String,
    /// Price per unit, in `Invoice::currency`.
    pub rate: f64,
}

impl LineItem {
    pub fn total(&self) -> f64 {
        round2(self.quantity * self.rate)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Pending,
    Paid,
    Overdue,
    Draft,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Paid => "paid",
            Status::Overdue => "overdue",
            Status::Draft => "draft",
        }
    }
}

/// A fully resolved invoice, ready to render.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invoice {
    /// e.g. "EINV-2026-23"
    pub number: String,
    pub date: NaiveDate,
    pub due: Option<NaiveDate>,
    pub status: Status,
    /// ISO code, e.g. "EUR".
    pub currency: String,
    pub from: Party,
    pub to: Party,
    pub payment: Payment,
    pub items: Vec<LineItem>,
    /// VAT/tax rate in percent (0.0 for reverse charge / exempt).
    pub tax_rate: f64,
    /// Label for the tax row, default "VAT".
    pub tax_label: String,
    /// Free markdown-derived notes shown under the table (plain paragraphs).
    pub notes: Vec<String>,
    /// Text rendered as the signature (defaults to `from.name`). None hides it.
    pub signature: Option<String>,
    /// Accent color as hex "#rrggbb" (default "#111827").
    pub accent: String,
}

impl Invoice {
    pub fn subtotal(&self) -> f64 {
        round2(self.items.iter().map(LineItem::total).sum())
    }
    pub fn tax(&self) -> f64 {
        round2(self.subtotal() * self.tax_rate / 100.0)
    }
    pub fn total(&self) -> f64 {
        round2(self.subtotal() + self.tax())
    }
}

/// Round to cents, half away from zero, as a person would on paper.
///
/// Plain `(x * 100.0).round()` gets decimal halves wrong because they are not
/// exact in binary: `1.005` is stored as `1.00499999...` and would round down.
/// Snapping away the binary noise first makes `1.005 -> 1.01`.
pub fn round2(x: f64) -> f64 {
    let cents = x * 100.0;
    let snapped = (cents * 1e6).round() / 1e6;
    snapped.round() / 100.0
}

/// A safe file stem for an invoice number: path separators and NUL become
/// `_`, and an empty number becomes `invoice`.
pub fn file_stem(number: &str) -> String {
    let s: String = number
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | '\0') {
                '_'
            } else {
                c
            }
        })
        .collect();
    if s.is_empty() || s == "." || s == ".." {
        "invoice".into()
    } else {
        s
    }
}

/// Format an amount with thousands separators and 2 decimals: 9450.0 -> "9,450.00".
pub fn format_amount(x: f64) -> String {
    let cents = (x.abs() * 100.0).round() as u64;
    // No "-0.00" for amounts that round to zero.
    let neg = x < 0.0 && cents > 0;
    let int = (cents / 100).to_string();
    let frac = cents % 100;
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}{}.{:02}", if neg { "-" } else { "" }, out, frac)
}

/// "EUR 9,450.00"
pub fn format_money(currency: &str, x: f64) -> String {
    format!("{} {}", currency, format_amount(x))
}

/// "August 1, 2026"
pub fn format_date(d: NaiveDate) -> String {
    d.format("%B %-d, %Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn amounts() {
        assert_eq!(format_amount(9450.0), "9,450.00");
        assert_eq!(format_amount(0.0), "0.00");
        assert_eq!(format_amount(1234567.891), "1,234,567.89");
        assert_eq!(format_amount(-12.5), "-12.50");
        assert_eq!(format_amount(-0.001), "0.00");
    }

    #[test]
    fn rounding_is_half_away_from_zero() {
        assert_eq!(round2(1.005), 1.01);
        assert_eq!(round2(2.675), 2.68);
        assert_eq!(round2(-1.005), -1.01);
        assert_eq!(round2(1.004), 1.0);
        let item = LineItem {
            description: "x".into(),
            detail: None,
            quantity: 0.5,
            unit: String::new(),
            rate: 2.01,
        };
        assert_eq!(item.total(), 1.01);
    }

    #[test]
    fn file_stems() {
        assert_eq!(file_stem("EINV-2026-01"), "EINV-2026-01");
        assert_eq!(file_stem("A/B\\C"), "A_B_C");
        assert_eq!(file_stem("  "), "invoice");
        assert_eq!(file_stem(".."), "invoice");
    }
}
