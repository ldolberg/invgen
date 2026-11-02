//! Shared invoice data model. This is the contract between the parser,
//! the renderer, the GUI and the CLI. Change it only deliberately.

use chrono::NaiveDate;
pub use rust_decimal::Decimal;
use rust_decimal::RoundingStrategy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

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
    /// Logo image (SVG, PNG, JPEG, GIF or WebP) shown in the invoice header.
    /// Only used for the sender (`from`). After parsing it is an absolute path
    /// (`~` expanded, relative paths resolved against the config directory).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<PathBuf>,
    /// E-invoicing only (not printed): VAT identifier with its country
    /// prefix, e.g. `DE123456789`. When unset it is read from a `tax:` line
    /// that looks like a VAT ID (see docs/E-INVOICING.md).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vat_id: Option<String>,
    /// E-invoicing only: company registration number (EN 16931 BT-30/BT-47).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legal_id: Option<String>,
    /// E-invoicing only: ISO 3166-1 alpha-2 country code, e.g. `DE`. When
    /// unset it is guessed from the last address line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// E-invoicing only: electronic address as `<scheme>:<id>` (Peppol EAS
    /// code), e.g. `0208:0123456789`. Defaults to the email address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// E-invoicing only: the reference the buyer asked to quote (BT-10),
    /// e.g. a purchase order or a German Leitweg-ID. Set it on a client;
    /// the invoice front matter `buyer_reference:` overrides it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyer_reference: Option<String>,
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
    /// Print an EPC (SEPA credit transfer) QR code next to the payment
    /// details. Only rendered for EUR invoices with a valid IBAN and a
    /// positive total; see `render::epc`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub qr: bool,
}

/// One billable line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineItem {
    pub description: String,
    /// Optional secondary line under the description (e.g. "October 2026 period").
    pub detail: Option<String>,
    #[serde(with = "num")]
    pub quantity: Decimal,
    /// Unit label, e.g. "days", "hours". Empty means none.
    pub unit: String,
    /// Price per unit, in `Invoice::currency`.
    #[serde(with = "num")]
    pub rate: Decimal,
    /// Tax rate in percent for this line. `None` uses `Invoice::tax_rate`.
    #[serde(default, with = "num::option", skip_serializing_if = "Option::is_none")]
    pub tax_rate: Option<Decimal>,
}

impl LineItem {
    /// `quantity * rate`, rounded to cents (see [`round_cents`]).
    pub fn total(&self) -> Decimal {
        round_cents(self.quantity.saturating_mul(self.rate))
    }

    /// Like [`LineItem::total`], but `None` when the product overflows.
    pub fn checked_total(&self) -> Option<Decimal> {
        self.quantity.checked_mul(self.rate).map(round_cents)
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

impl std::str::FromStr for Status {
    type Err = String;

    /// `pending`, `paid`, `overdue` or `draft`, case-insensitive.
    fn from_str(s: &str) -> Result<Status, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pending" => Ok(Status::Pending),
            "paid" => Ok(Status::Paid),
            "overdue" => Ok(Status::Overdue),
            "draft" => Ok(Status::Draft),
            _ => Err(format!(
                "unknown status `{s}` (use pending, paid, overdue or draft)"
            )),
        }
    }
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

/// What a document is. A credit note reverses (part of) an earlier invoice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Invoice,
    CreditNote,
}

impl Kind {
    pub fn is_invoice(&self) -> bool {
        *self == Kind::Invoice
    }

    /// The front matter spelling: `invoice` or `credit_note`.
    pub fn label(&self) -> &'static str {
        match self {
            Kind::Invoice => "invoice",
            Kind::CreditNote => "credit_note",
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
    /// When the invoice was paid (front matter `paid_on`), if recorded.
    #[serde(default)]
    pub paid_on: Option<NaiveDate>,
    /// ISO code, e.g. "EUR".
    pub currency: String,
    pub from: Party,
    pub to: Party,
    pub payment: Payment,
    pub items: Vec<LineItem>,
    /// VAT/tax rate in percent (0 for reverse charge / exempt).
    #[serde(with = "num")]
    pub tax_rate: Decimal,
    /// Label for the tax row, default "VAT".
    pub tax_label: String,
    /// Legal mention printed under the totals (`tax_note:`): a preset key
    /// from [`TAX_NOTE_PRESETS`] (translated when rendering) or free text.
    /// `None` prints nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_note: Option<String>,
    /// Free markdown-derived notes shown under the table (plain paragraphs).
    pub notes: Vec<String>,
    /// Text rendered as the signature (defaults to `from.name`). None hides it.
    pub signature: Option<String>,
    /// Accent color as hex "#rrggbb" (default "#111827").
    pub accent: String,
    /// Label language, a code from `render::i18n::SUPPORTED` (default "en").
    pub language: String,
    /// Per-key label overrides (config `labels:`), applied over `language`.
    pub labels: BTreeMap<String, String>,
    /// Custom typst template (absolute path). None uses the built-in one.
    pub template: Option<PathBuf>,
    /// How numbers and dates are written on the PDF (`locale`,
    /// `number_format`, `date_format`). Empty follows `language`.
    #[serde(flatten)]
    pub format: FormatOptions,
    /// Invoice or credit note. Not serialized for plain invoices, so the
    /// issued-invoice hash (`crate::lock`) of existing invoices is unchanged.
    #[serde(default, skip_serializing_if = "Kind::is_invoice")]
    pub kind: Kind,
    /// Number of the invoice a credit note reverses (always set for credit
    /// notes, never for invoices).
    #[serde(default)]
    pub refers_to: Option<String>,
    /// Date of that invoice, when known.
    #[serde(default)]
    pub refers_to_date: Option<NaiveDate>,
}

/// Formatting overrides for amounts and dates on the PDF, set in the config,
/// per client or in the front matter (docs/FORMAT.md). Every field is
/// optional; what is not set follows the locale, and the locale follows the
/// invoice `language`. The CLI keeps its own fixed format.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatOptions {
    /// Formatting locale like `de` or `de-CH` (language plus optional region).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// Separators and currency placement; unset fields follow the locale.
    #[serde(skip_serializing_if = "NumberFormat::is_empty")]
    pub number_format: NumberFormat,
    /// strftime-style pattern for dates, e.g. `%d.%m.%Y`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_format: Option<String>,
}

impl FormatOptions {
    pub fn is_empty(&self) -> bool {
        self.locale.is_none() && self.number_format.is_empty() && self.date_format.is_none()
    }
}

/// Explicit number formatting (`number_format:`). Unset fields follow the locale.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NumberFormat {
    /// Thousands separator; `""` for none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thousands: Option<String>,
    /// Decimal separator.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimal: Option<String>,
    /// Write the currency as a symbol (`€`) or as its ISO code (`EUR`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<CurrencyStyle>,
    /// Currency before or after the amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency_position: Option<CurrencyPosition>,
}

impl NumberFormat {
    pub fn is_empty(&self) -> bool {
        *self == NumberFormat::default()
    }

    /// Fields set in `self`, the rest taken from `base`.
    pub fn or(&self, base: &NumberFormat) -> NumberFormat {
        NumberFormat {
            thousands: self.thousands.clone().or_else(|| base.thousands.clone()),
            decimal: self.decimal.clone().or_else(|| base.decimal.clone()),
            currency: self.currency.or(base.currency),
            currency_position: self.currency_position.or(base.currency_position),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CurrencyStyle {
    Symbol,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CurrencyPosition {
    Before,
    After,
}

/// One row of the tax breakdown: every line taxed at `rate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaxGroup {
    /// Tax rate in percent.
    pub rate: Decimal,
    /// Exact sum of the (already rounded) line totals at this rate.
    pub base: Decimal,
    /// `base * rate / 100`, rounded once to cents (see [`round_cents`]).
    pub tax: Decimal,
}

/// The `tax_note:` presets. Each one renders as the translated label
/// `tax_note_<preset>` (see `render::i18n`).
pub const TAX_NOTE_PRESETS: &[&str] = &["reverse_charge", "exempt", "outside_scope"];

/// The preset a `tax_note` names (`reverse_charge`, `Reverse-Charge`,
/// `reverse charge`...), or `None` for free text.
pub fn tax_note_preset(note: &str) -> Option<&'static str> {
    let key = note.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    TAX_NOTE_PRESETS.iter().find(|&&p| p == key).copied()
}

impl Invoice {
    /// Exact sum of the (already rounded) line totals.
    pub fn subtotal(&self) -> Decimal {
        self.items
            .iter()
            .fold(Decimal::ZERO, |acc, it| acc.saturating_add(it.total()))
    }

    /// The tax rate that applies to `item`: its own, else the invoice's.
    pub fn item_tax_rate(&self, item: &LineItem) -> Decimal {
        item.tax_rate.unwrap_or(self.tax_rate)
    }

    /// One group per distinct tax rate, highest rate first. The bases add up
    /// to `subtotal` and the taxes to `tax`. No items means no groups.
    pub fn tax_breakdown(&self) -> Vec<TaxGroup> {
        let mut bases: BTreeMap<Decimal, Decimal> = BTreeMap::new();
        for it in &self.items {
            let base = bases.entry(self.item_tax_rate(it)).or_default();
            *base = base.saturating_add(it.total());
        }
        bases
            .into_iter()
            .rev()
            .map(|(rate, base)| TaxGroup {
                rate,
                base,
                tax: round_cents(base.saturating_mul(rate) / Decimal::ONE_HUNDRED),
            })
            .collect()
    }

    /// Sum of the per-rate taxes of [`Invoice::tax_breakdown`]. With a single
    /// rate this is `subtotal * tax_rate / 100`, rounded to cents.
    pub fn tax(&self) -> Decimal {
        self.tax_breakdown()
            .iter()
            .fold(Decimal::ZERO, |acc, g| acc.saturating_add(g.tax))
    }

    /// `subtotal + tax`. Both are whole cents, so nothing is rounded here.
    pub fn total(&self) -> Decimal {
        self.subtotal().saturating_add(self.tax())
    }

    /// `-1` for a credit note, `1` otherwise. Credit note items are written
    /// with positive quantities and rates, so `subtotal`, `tax` and `total`
    /// are the credited amounts as written; multiply by `sign` (or use
    /// [`Invoice::signed_total`]) for reporting and display.
    pub fn sign(&self) -> Decimal {
        match self.kind {
            Kind::Invoice => Decimal::ONE,
            Kind::CreditNote => Decimal::NEGATIVE_ONE,
        }
    }

    /// `total` with the sign of the document: negative for a credit note, so
    /// summing `signed_total` over a library nets credit notes out.
    pub fn signed_total(&self) -> Decimal {
        self.total().saturating_mul(self.sign())
    }

    /// The total, or `None` when any step overflows `Decimal` (about 7.9e28).
    /// The parser rejects such invoices, so for a parsed invoice `subtotal`,
    /// `tax` and `total` never saturate.
    pub fn checked_total(&self) -> Option<Decimal> {
        let mut bases: BTreeMap<Decimal, Decimal> = BTreeMap::new();
        let mut subtotal = Decimal::ZERO;
        for it in &self.items {
            let line = it.checked_total()?;
            subtotal = subtotal.checked_add(line)?;
            let base = bases.entry(self.item_tax_rate(it)).or_default();
            *base = base.checked_add(line)?;
        }
        let mut total = subtotal;
        for (rate, base) in bases {
            let tax = round_cents(base.checked_mul(rate)?.checked_div(Decimal::ONE_HUNDRED)?);
            total = total.checked_add(tax)?;
        }
        Some(total)
    }

    /// A warning for an invoice that charges no tax, sets no `tax_note` and
    /// goes to a client with tax lines (usually a VAT ID): EU B2B invoices
    /// without VAT need a legal mention such as the reverse charge one.
    pub fn tax_note_warning(&self) -> Option<String> {
        let missing = !self.items.is_empty()
            && self.tax().is_zero()
            && self.tax_note.as_deref().is_none_or(|n| n.trim().is_empty())
            && self.to.tax.iter().any(|t| !t.trim().is_empty());
        missing.then(|| {
            "no tax is charged and no `tax_note` is set; EU B2B invoices without VAT \
             usually need a legal mention (e.g. `tax_note: reverse_charge`, `exempt` \
             or `outside_scope`)"
                .to_string()
        })
    }
}

/// Display status, derived from the written status and the due date.
/// The PDF always shows the written `status`; `invgen list` and the GUI
/// sidebar show this one, so unpaid invoices past due stand out.
impl Invoice {
    /// `pending` with a due date before `today` counts as `overdue`. Any other
    /// written status wins, and an invoice without a due date is never
    /// overdue automatically.
    pub fn effective_status(&self, today: NaiveDate) -> Status {
        match (self.status, self.due) {
            (Status::Pending, Some(due)) if due < today => Status::Overdue,
            (s, _) => s,
        }
    }

    /// Whole days past the due date when the effective status is `overdue`
    /// and there is a due date before `today`, else `None`.
    pub fn days_overdue(&self, today: NaiveDate) -> Option<i64> {
        let due = self.due?;
        (self.effective_status(today) == Status::Overdue && due < today)
            .then(|| (today - due).num_days())
    }

    /// Effective status label with days late, e.g. `pending` or `overdue 12d`.
    pub fn status_display(&self, today: NaiveDate) -> String {
        status_display(self.effective_status(today), self.days_overdue(today))
    }

    /// [`Invoice::status_display`] plus the payment date when one is recorded
    /// on a paid invoice: `paid 2026-11-20`.
    pub fn status_detail(&self, today: NaiveDate) -> String {
        match (self.status, self.paid_on) {
            (Status::Paid, Some(d)) => format!("paid {}", d.format("%Y-%m-%d")),
            _ => self.status_display(today),
        }
    }
}

/// `status_display` from already computed parts: `overdue 12d`, `paid`.
pub fn status_display(status: Status, days_overdue: Option<i64>) -> String {
    match days_overdue {
        Some(n) => format!("{} {n}d", status.label()),
        None => status.label().to_string(),
    }
}

/// Round to cents, half away from zero, as a person would on paper:
/// `1.005 -> 1.01`, `-1.005 -> -1.01`, `1.004 -> 1.00`.
///
/// This is the one rounding rule for money: each line total is rounded, the
/// subtotal is the exact sum of those, and the tax is rounded once.
pub fn round_cents(x: Decimal) -> Decimal {
    x.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
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

/// Format an amount with thousands separators and 2 decimals, rounded half
/// away from zero: 9450 -> "9,450.00".
pub fn format_amount(x: Decimal) -> String {
    let r = round_cents(x);
    // No "-0.00" for amounts that round to zero.
    let neg = r.is_sign_negative() && !r.is_zero();
    let digits = format!("{:.2}", r.abs());
    let (int, frac) = digits.split_once('.').unwrap_or((&digits, "00"));
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}{}.{}", if neg { "-" } else { "" }, out, frac)
}

/// "EUR 9,450.00"
pub fn format_money(currency: &str, x: Decimal) -> String {
    format!("{} {}", currency, format_amount(x))
}

/// "August 1, 2026"
pub fn format_date(d: NaiveDate) -> String {
    d.format("%B %-d, %Y").to_string()
}

/// Serde for money and rate fields: written as a plain YAML/JSON number,
/// read from a number (`21`, `21.5`) or a string (`"21"`, `"21%"`,
/// `"1,234.50"`, `"EUR 450"`). Use with `#[serde(with = "num")]`, or
/// `num::option` for `Option<Decimal>`.
pub mod num {
    use super::Decimal;
    use rust_decimal::prelude::ToPrimitive;
    use serde::{Deserializer, Serializer, de};
    use std::fmt;
    use std::str::FromStr;

    /// The exact decimal a float was written as: `7.7` becomes `7.7`, not
    /// `7.70000000000000017763...`. `None` for NaN, infinities and values
    /// out of `Decimal`'s range.
    pub fn from_f64(f: f64) -> Option<Decimal> {
        if !f.is_finite() {
            return None;
        }
        Decimal::from_str(&f.to_string()).ok()
    }

    /// A number or a string such as `"21%"` / `"EUR 1,234.50"`.
    pub fn from_text(s: &str) -> Option<Decimal> {
        crate::parse::parse_amount(s.trim().trim_end_matches('%')).ok()
    }

    pub fn serialize<S: Serializer>(d: &Decimal, s: S) -> Result<S::Ok, S::Error> {
        let d = d.normalize();
        if d.scale() == 0
            && let Some(i) = d.to_i64()
        {
            return s.serialize_i64(i);
        }
        match d.to_f64() {
            Some(f) if from_f64(f) == Some(d) => s.serialize_f64(f),
            _ => s.serialize_str(&d.to_string()),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Decimal, D::Error> {
        d.deserialize_any(NumVisitor)
    }

    struct NumVisitor;

    impl de::Visitor<'_> for NumVisitor {
        type Value = Decimal;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a number")
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Decimal, E> {
            Ok(Decimal::from(v))
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Decimal, E> {
            Ok(Decimal::from(v))
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Decimal, E> {
            from_f64(v).ok_or_else(|| E::custom(format!("{v} is not a usable number")))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Decimal, E> {
            from_text(v).ok_or_else(|| E::custom(format!("`{v}` is not a number")))
        }
    }

    /// `#[serde(with = "num::option")]` for `Option<Decimal>`.
    pub mod option {
        use super::Decimal;
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer>(d: &Option<Decimal>, s: S) -> Result<S::Ok, S::Error> {
            match d {
                Some(d) => super::serialize(d, s),
                None => s.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Decimal>, D::Error> {
            #[derive(Deserialize)]
            struct Wrap(#[serde(with = "super")] Decimal);
            Ok(Option::<Wrap>::deserialize(d)?.map(|Wrap(v)| v))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn item(quantity: Decimal, rate: Decimal) -> LineItem {
        LineItem {
            description: "x".into(),
            detail: None,
            quantity,
            unit: String::new(),
            rate,
            tax_rate: None,
        }
    }

    fn invoice(items: Vec<LineItem>, tax_rate: Decimal) -> Invoice {
        Invoice {
            number: "T-1".into(),
            date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            due: None,
            status: Status::Pending,
            paid_on: None,
            currency: "EUR".into(),
            from: Party::default(),
            to: Party::default(),
            payment: Payment::default(),
            items,
            tax_rate,
            tax_label: "VAT".into(),
            tax_note: None,
            notes: vec![],
            signature: None,
            accent: "#111827".into(),
            language: "en".into(),
            labels: BTreeMap::new(),
            template: None,
            format: FormatOptions::default(),
            kind: Kind::Invoice,
            refers_to: None,
            refers_to_date: None,
        }
    }

    #[test]
    fn amounts() {
        assert_eq!(format_amount(d("9450")), "9,450.00");
        assert_eq!(format_amount(Decimal::ZERO), "0.00");
        assert_eq!(format_amount(d("1234567.891")), "1,234,567.89");
        assert_eq!(format_amount(d("-12.5")), "-12.50");
        assert_eq!(format_amount(d("-0.001")), "0.00");
        assert_eq!(format_amount(d("-0.005")), "-0.01");
        assert_eq!(format_amount(d("999.995")), "1,000.00");
        assert_eq!(format_amount(d("10.015")), "10.02");
        assert_eq!(format_money("CHF", d("100")), "CHF 100.00");
    }

    #[test]
    fn rounding_is_half_away_from_zero() {
        assert_eq!(round_cents(d("1.005")), d("1.01"));
        assert_eq!(round_cents(d("2.675")), d("2.68"));
        assert_eq!(round_cents(d("-1.005")), d("-1.01"));
        assert_eq!(round_cents(d("1.004")), d("1.00"));
        assert_eq!(round_cents(d("0.125")), d("0.13"));
        assert_eq!(item(d("0.5"), d("2.01")).total(), d("1.01"));
    }

    #[test]
    fn rounding_is_per_line_then_summed() {
        // Each 1.005 line rounds to 1.01, so the subtotal is 3.03 and not
        // round(3.015) = 3.02.
        let lines = vec![item(d("1"), d("1.005")); 3];
        let inv = invoice(lines, d("10"));
        assert_eq!(inv.subtotal(), d("3.03"));
        // 10% of 3.03 = 0.303 -> 0.30
        assert_eq!(inv.tax(), d("0.30"));
        assert_eq!(inv.total(), d("3.33"));
    }

    #[test]
    fn tax_rounds_half_away_from_zero() {
        // 21% of 0.50 = 0.105 -> 0.11; 7.7% of 999.98 = 76.99846 -> 77.00
        assert_eq!(
            invoice(vec![item(d("1"), d("0.5"))], d("21")).tax(),
            d("0.11")
        );
        assert_eq!(
            invoice(vec![item(d("1"), d("-0.5"))], d("21")).tax(),
            d("-0.11")
        );
        assert_eq!(
            invoice(vec![item(d("7.5"), d("133.33"))], d("7.7")).tax(),
            d("77.00")
        );
    }

    #[test]
    fn tax_breakdown_groups_lines_by_rate() {
        let with_rate = |q: &str, r: &str, t: Option<&str>| LineItem {
            tax_rate: t.map(d),
            ..item(d(q), d(r))
        };
        // Single rate: one group, same numbers as subtotal * rate.
        let single = invoice(vec![item(d("21"), d("450"))], d("21"));
        assert_eq!(
            single.tax_breakdown(),
            vec![TaxGroup {
                rate: d("21"),
                base: d("9450"),
                tax: d("1984.50"),
            }]
        );
        assert_eq!(single.tax(), d("1984.50"));
        // Lines without a rate use the invoice one; 21 and 21.00 are one rate.
        let mixed = invoice(
            vec![
                with_rate("1", "100.05", None),
                with_rate("1", "50.05", Some("10")),
                with_rate("1", "0.05", Some("21.00")),
                with_rate("2", "30", Some("0")),
            ],
            d("21"),
        );
        let groups = mixed.tax_breakdown();
        assert_eq!(
            groups.iter().map(|g| g.rate).collect::<Vec<_>>(),
            vec![d("21"), d("10"), d("0")]
        );
        // 21% of 100.10 = 21.021 -> 21.02; 10% of 50.05 = 5.005 -> 5.01
        assert_eq!((groups[0].base, groups[0].tax), (d("100.10"), d("21.02")));
        assert_eq!((groups[1].base, groups[1].tax), (d("50.05"), d("5.01")));
        assert_eq!((groups[2].base, groups[2].tax), (d("60"), d("0")));
        assert_eq!(mixed.subtotal(), d("210.15"));
        assert_eq!(mixed.tax(), d("26.03"));
        assert_eq!(mixed.total(), d("236.18"));
        assert_eq!(mixed.checked_total(), Some(d("236.18")));
        assert!(invoice(vec![], d("21")).tax_breakdown().is_empty());
    }

    #[test]
    fn tax_note_presets_and_warning() {
        assert_eq!(tax_note_preset("reverse_charge"), Some("reverse_charge"));
        assert_eq!(tax_note_preset(" Reverse-Charge "), Some("reverse_charge"));
        assert_eq!(tax_note_preset("outside scope"), Some("outside_scope"));
        assert_eq!(tax_note_preset("EXEMPT"), Some("exempt"));
        assert_eq!(tax_note_preset("Exempt under section 4"), None);

        let mut inv = invoice(vec![item(d("1"), d("100"))], Decimal::ZERO);
        // No client tax lines: nothing to warn about.
        assert_eq!(inv.tax_note_warning(), None);
        inv.to.tax = vec!["VAT ID: ES B12345678".into()];
        let w = inv.tax_note_warning().unwrap();
        assert!(w.contains("tax_note"), "{w}");
        inv.tax_note = Some("reverse_charge".into());
        assert_eq!(inv.tax_note_warning(), None);
        inv.tax_note = Some("  ".into());
        assert!(inv.tax_note_warning().is_some());
        inv.tax_note = None;
        inv.tax_rate = d("21");
        assert_eq!(inv.tax_note_warning(), None);
    }

    #[test]
    fn overflow_is_detected() {
        let big = d("100000000000000000000");
        let inv = invoice(vec![item(big, big)], Decimal::ZERO);
        assert_eq!(inv.checked_total(), None);
        // The unchecked accessors saturate instead of panicking.
        assert_eq!(inv.total(), Decimal::MAX);
        let ok = invoice(vec![item(d("21"), d("450"))], d("21"));
        assert_eq!(ok.checked_total(), Some(ok.total()));
    }

    #[test]
    fn serde_numbers() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct T {
            #[serde(with = "num")]
            a: Decimal,
            #[serde(with = "num::option", default)]
            b: Option<Decimal>,
        }
        let t: T = serde_yaml_ng::from_str("a: 21\nb: 7.7").unwrap();
        assert_eq!(
            t,
            T {
                a: d("21"),
                b: Some(d("7.7"))
            }
        );
        let t: T = serde_yaml_ng::from_str("a: \"21%\"\nb: null").unwrap();
        assert_eq!(
            t,
            T {
                a: d("21"),
                b: None
            }
        );
        let t: T = serde_yaml_ng::from_str("a: \"EUR 1,234.50\"").unwrap();
        assert_eq!(t.a, d("1234.50"));
        assert!(serde_yaml_ng::from_str::<T>("a: abc").is_err());
        assert!(serde_yaml_ng::from_str::<T>("a: .nan").is_err());
        let t = T {
            a: d("21.50"),
            b: Some(d("450")),
        };
        assert_eq!(serde_yaml_ng::to_string(&t).unwrap(), "a: 21.5\nb: 450\n");
    }

    #[test]
    fn credit_notes_count_negatively() {
        let mut inv = invoice(vec![item(d("2"), d("50"))], d("21"));
        assert_eq!(inv.signed_total(), d("121"));
        // Plain invoices serialize without `kind`, so issued-invoice hashes
        // (crate::lock) of existing invoices do not change.
        let json = serde_json::to_value(&inv).unwrap();
        assert!(json.get("kind").is_none(), "{json}");
        inv.kind = Kind::CreditNote;
        assert_eq!(inv.total(), d("121"));
        assert_eq!(inv.signed_total(), d("-121"));
        assert_eq!(Kind::CreditNote.label(), "credit_note");
        let k: Kind = serde_yaml_ng::from_str("credit_note").unwrap();
        assert_eq!(k, Kind::CreditNote);
    }

    #[test]
    fn file_stems() {
        assert_eq!(file_stem("EINV-2026-01"), "EINV-2026-01");
        assert_eq!(file_stem("A/B\\C"), "A_B_C");
        assert_eq!(file_stem("  "), "invoice");
        assert_eq!(file_stem(".."), "invoice");
    }

    /// Up to 10 significant digits with 0 to 4 decimals, either sign.
    fn amount() -> impl Strategy<Value = Decimal> {
        (-9_999_999_999_i64..=9_999_999_999, 0u32..=4).prop_map(|(m, scale)| Decimal::new(m, scale))
    }

    fn rate() -> impl Strategy<Value = Decimal> {
        (0i64..=10_000, 0u32..=2).prop_map(|(m, scale)| Decimal::new(m, scale))
    }

    proptest! {
        #[test]
        fn line_totals_plus_tax_equal_total(
            lines in prop::collection::vec((amount(), amount()), 0..20),
            tax_rate in rate(),
        ) {
            let items: Vec<_> = lines.into_iter().map(|(q, r)| item(q, r)).collect();
            let inv = invoice(items, tax_rate);
            let line_sum: Decimal = inv.items.iter().map(LineItem::total).sum();
            prop_assert_eq!(inv.subtotal(), line_sum);
            prop_assert_eq!(line_sum + inv.tax(), inv.total());
            prop_assert_eq!(inv.checked_total(), Some(inv.total()));
            // Everything is whole cents.
            for v in [inv.subtotal(), inv.tax(), inv.total()] {
                prop_assert_eq!(round_cents(v), v);
            }
            for it in &inv.items {
                let t = it.total();
                prop_assert_eq!(round_cents(t), t);
                prop_assert!((t - it.quantity * it.rate).abs() <= Decimal::new(5, 3));
            }
            let tax = inv.tax();
            prop_assert!((tax - inv.subtotal() * tax_rate / Decimal::ONE_HUNDRED).abs() <= Decimal::new(5, 3));
        }

        #[test]
        fn per_rate_taxes_plus_subtotal_equal_total(
            lines in prop::collection::vec((amount(), amount(), prop::option::of(rate())), 0..20),
            tax_rate in rate(),
        ) {
            let items: Vec<_> = lines
                .into_iter()
                .map(|(q, r, t)| LineItem { tax_rate: t, ..item(q, r) })
                .collect();
            let inv = invoice(items, tax_rate);
            let groups = inv.tax_breakdown();
            let bases: Decimal = groups.iter().map(|g| g.base).sum();
            let taxes: Decimal = groups.iter().map(|g| g.tax).sum();
            prop_assert_eq!(bases, inv.subtotal());
            prop_assert_eq!(taxes, inv.tax());
            prop_assert_eq!(inv.subtotal() + taxes, inv.total());
            prop_assert_eq!(inv.checked_total(), Some(inv.total()));
            // One group per distinct rate, highest first, each rounded once.
            for w in groups.windows(2) {
                prop_assert!(w[0].rate > w[1].rate);
            }
            for g in &groups {
                prop_assert_eq!(g.tax, round_cents(g.base * g.rate / Decimal::ONE_HUNDRED));
                let base: Decimal = inv
                    .items
                    .iter()
                    .filter(|it| inv.item_tax_rate(it) == g.rate)
                    .map(LineItem::total)
                    .sum();
                prop_assert_eq!(g.base, base);
            }
        }

        #[test]
        fn format_amount_roundtrips(x in amount()) {
            let s = format_amount(x).replace(',', "");
            prop_assert_eq!(Decimal::from_str(&s).unwrap(), round_cents(x));
        }
    }

    fn dated_invoice(status: Status, due: Option<NaiveDate>) -> Invoice {
        Invoice {
            number: "EINV-2026-01".into(),
            date: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            due,
            status,
            paid_on: None,
            currency: "EUR".into(),
            from: Party::default(),
            to: Party::default(),
            payment: Payment::default(),
            items: Vec::new(),
            tax_rate: Decimal::ZERO,
            tax_label: "VAT".into(),
            tax_note: None,
            notes: Vec::new(),
            signature: None,
            accent: "#111827".into(),
            language: "en".into(),
            labels: BTreeMap::new(),
            template: None,
            format: FormatOptions::default(),
            kind: Kind::Invoice,
            refers_to: None,
            refers_to_date: None,
        }
    }

    #[test]
    fn effective_status_derives_overdue_from_due_date() {
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        let today = d(11, 2);
        // Pending and past due: overdue, with days late.
        let late = dated_invoice(Status::Pending, Some(d(10, 21)));
        assert_eq!(late.effective_status(today), Status::Overdue);
        assert_eq!(late.days_overdue(today), Some(12));
        // Due today is not late yet.
        let due_today = dated_invoice(Status::Pending, Some(today));
        assert_eq!(due_today.effective_status(today), Status::Pending);
        assert_eq!(due_today.days_overdue(today), None);
        // Not due yet.
        let future = dated_invoice(Status::Pending, Some(d(11, 30)));
        assert_eq!(future.effective_status(today), Status::Pending);
        // No due date: never overdue automatically.
        let no_due = dated_invoice(Status::Pending, None);
        assert_eq!(no_due.effective_status(today), Status::Pending);
        assert_eq!(no_due.days_overdue(today), None);
        // An explicit non-pending status wins.
        for s in [Status::Paid, Status::Draft] {
            let inv = dated_invoice(s, Some(d(1, 1)));
            assert_eq!(inv.effective_status(today), s);
            assert_eq!(inv.days_overdue(today), None);
        }
        // Written overdue stays overdue and still reports days late.
        let written = dated_invoice(Status::Overdue, Some(d(10, 31)));
        assert_eq!(written.effective_status(today), Status::Overdue);
        assert_eq!(written.days_overdue(today), Some(2));
        assert_eq!(
            dated_invoice(Status::Overdue, None).days_overdue(today),
            None
        );
        assert_eq!(late.status_display(today), "overdue 12d");
        assert_eq!(future.status_display(today), "pending");
        assert_eq!(late.status_detail(today), "overdue 12d");
        let mut paid = dated_invoice(Status::Paid, Some(d(10, 21)));
        assert_eq!(paid.status_detail(today), "paid");
        paid.paid_on = Some(d(10, 30));
        assert_eq!(paid.status_detail(today), "paid 2026-10-30");
        assert_eq!("Paid ".parse::<Status>(), Ok(Status::Paid));
        assert!(
            "sent"
                .parse::<Status>()
                .unwrap_err()
                .contains("unknown status")
        );
    }
}
