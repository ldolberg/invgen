//! The semantic EN 16931 model: a resolved [`Invoice`] mapped onto the
//! business terms (BT-n) of the European e-invoicing standard. Both syntax
//! writers (CII for Factur-X, UBL for Peppol) read only this struct, so the
//! mapping decisions live in one place.

use chrono::NaiveDate;

use super::party::{self, Address, Endpoint};
use crate::model::{Decimal, Invoice, Kind, Status, round_cents, tax_note_preset};
use crate::render::i18n;

/// Commercial invoice or credit note. The only places that differ between
/// the two are the type code (BT-3), the preceding invoice reference
/// (BT-25, BT-26) and, in UBL, the root and line element names. Amounts are
/// positive in both, as in invgen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentKind {
    /// UNTDID 1001 code 380.
    Invoice,
    /// UNTDID 1001 code 381, with the invoice it corrects (BT-25) and that
    /// invoice's date (BT-26).
    CreditNote {
        refers_to: Option<String>,
        refers_to_date: Option<NaiveDate>,
    },
}

impl DocumentKind {
    /// The kind of an invgen invoice (`kind:` and `refers_to:`).
    pub fn of(inv: &Invoice) -> DocumentKind {
        match inv.kind {
            Kind::Invoice => DocumentKind::Invoice,
            Kind::CreditNote => DocumentKind::CreditNote {
                refers_to: inv
                    .refers_to
                    .as_deref()
                    .map(str::trim)
                    .filter(|r| !r.is_empty())
                    .map(str::to_string),
                refers_to_date: inv.refers_to_date,
            },
        }
    }

    /// Date of the preceding invoice (BT-26).
    pub fn preceding_invoice_date(&self) -> Option<NaiveDate> {
        match self {
            DocumentKind::Invoice => None,
            DocumentKind::CreditNote { refers_to_date, .. } => *refers_to_date,
        }
    }

    /// Invoice type code (BT-3, UNTDID 1001).
    pub fn type_code(&self) -> &'static str {
        match self {
            DocumentKind::Invoice => "380",
            DocumentKind::CreditNote { .. } => "381",
        }
    }

    /// Preceding invoice reference (BT-25).
    pub fn preceding_invoice(&self) -> Option<&str> {
        match self {
            DocumentKind::Invoice => None,
            DocumentKind::CreditNote { refers_to, .. } => refers_to.as_deref(),
        }
    }

    pub fn is_credit_note(&self) -> bool {
        matches!(self, DocumentKind::CreditNote { .. })
    }
}

/// VAT category code (BT-118 / BT-151, UNCL5305 subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VatCategory {
    /// `S`: standard (any rate above zero).
    Standard,
    /// `Z`: zero rated goods or services.
    ZeroRated,
    /// `E`: exempt from VAT.
    Exempt,
    /// `AE`: VAT reverse charge.
    ReverseCharge,
    /// `O`: services outside the scope of VAT.
    OutsideScope,
}

impl VatCategory {
    pub fn code(self) -> &'static str {
        match self {
            VatCategory::Standard => "S",
            VatCategory::ZeroRated => "Z",
            VatCategory::Exempt => "E",
            VatCategory::ReverseCharge => "AE",
            VatCategory::OutsideScope => "O",
        }
    }

    /// VATEX exemption reason code (BT-121), where one applies to every
    /// invoice of this category.
    pub fn exemption_code(self) -> Option<&'static str> {
        match self {
            VatCategory::ReverseCharge => Some("VATEX-EU-AE"),
            VatCategory::OutsideScope => Some("VATEX-EU-O"),
            _ => None,
        }
    }

    /// Category `O` carries no rate at all (BR-O-05 / BR-O-09).
    pub fn has_rate(self) -> bool {
        self != VatCategory::OutsideScope
    }

    /// The category for a line taxed at `rate` on an invoice with this
    /// `tax_note`. Any positive rate is standard. At 0%, the presets
    /// `reverse_charge`, `exempt` and `outside_scope` give `AE`, `E` and
    /// `O`; any other (free text) note means exempt with that text as the
    /// reason, and no note at all means zero rated.
    pub fn for_rate(rate: Decimal, tax_note: Option<&str>) -> VatCategory {
        if rate > Decimal::ZERO {
            return VatCategory::Standard;
        }
        match tax_note {
            None => VatCategory::ZeroRated,
            Some(note) => match tax_note_preset(note) {
                Some("reverse_charge") => VatCategory::ReverseCharge,
                Some("outside_scope") => VatCategory::OutsideScope,
                _ => VatCategory::Exempt,
            },
        }
    }
}

/// Seller (BG-4) or buyer (BG-7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    /// BT-27 / BT-44.
    pub name: String,
    /// BG-5 / BG-8.
    pub address: Address,
    /// BT-31 / BT-48.
    pub vat_id: Option<String>,
    /// BT-32 (seller only).
    pub tax_registration: Option<String>,
    /// BT-30 / BT-47.
    pub legal_id: Option<String>,
    /// BT-34 / BT-49: explicit `endpoint`, else derived from the VAT ID.
    pub endpoint: Option<Endpoint>,
    /// The email as an `EM` address: the CII fallback for `endpoint`.
    pub email_endpoint: Option<Endpoint>,
    /// Bad `endpoint:` value, reported by validation.
    pub bad_endpoint: Option<String>,
    /// BT-43 / BT-58.
    pub email: Option<String>,
    /// BT-42 / BT-57.
    pub phone: Option<String>,
}

/// Payment instructions (BG-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentMeans {
    /// BT-81 (UNCL4461): `58` SEPA credit transfer (IBAN, EUR), `30` credit
    /// transfer (IBAN, other currency), `1` not defined (no IBAN).
    pub code: &'static str,
    /// BT-83 remittance information: the invoice number.
    pub remittance: String,
    /// BT-84, without spaces.
    pub iban: Option<String>,
    /// BT-85.
    pub holder: Option<String>,
    /// BT-86.
    pub bic: Option<String>,
}

/// One invoice line (BG-25).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// BT-126, 1-based.
    pub id: String,
    /// BT-153.
    pub name: String,
    /// BT-154.
    pub description: Option<String>,
    /// BT-129.
    pub quantity: Decimal,
    /// BT-130 (UN/ECE Recommendation 20).
    pub unit_code: &'static str,
    /// BT-146.
    pub price: Decimal,
    /// BT-131, rounded to cents.
    pub net: Decimal,
    /// BT-151.
    pub category: VatCategory,
    /// BT-152, percent.
    pub rate: Decimal,
}

/// One VAT breakdown row (BG-23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VatBreakdown {
    /// BT-118.
    pub category: VatCategory,
    /// BT-119, percent.
    pub rate: Decimal,
    /// BT-116.
    pub basis: Decimal,
    /// BT-117.
    pub tax: Decimal,
    /// BT-120.
    pub exemption_reason: Option<String>,
    /// BT-121.
    pub exemption_code: Option<&'static str>,
}

/// Document totals (BG-22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals {
    /// BT-106.
    pub line_total: Decimal,
    /// BT-109.
    pub tax_exclusive: Decimal,
    /// BT-110.
    pub tax: Decimal,
    /// BT-112.
    pub tax_inclusive: Decimal,
    /// BT-113: the whole total when the invoice is marked paid.
    pub prepaid: Decimal,
    /// BT-115.
    pub payable: Decimal,
}

/// An invoice in EN 16931 terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub kind: DocumentKind,
    /// BT-1.
    pub number: String,
    /// BT-2.
    pub issue_date: NaiveDate,
    /// BT-9.
    pub due_date: Option<NaiveDate>,
    /// BT-5.
    pub currency: String,
    /// BT-10.
    pub buyer_reference: Option<String>,
    /// BT-22.
    pub notes: Vec<String>,
    /// BG-4.
    pub seller: Party,
    /// BG-7.
    pub buyer: Party,
    /// BG-16.
    pub payment: PaymentMeans,
    /// BG-25.
    pub lines: Vec<Line>,
    /// BG-23, highest rate first.
    pub vat: Vec<VatBreakdown>,
    /// BG-22.
    pub totals: Totals,
}

fn opt(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn party(p: &crate::model::Party, seller: bool) -> Party {
    let address = party::address(p);
    let vat_id = party::vat_id(p, address.country.as_deref());
    let (endpoint, bad_endpoint) = match party::endpoint(p, vat_id.as_deref()) {
        Ok(e) => (e, None),
        Err(bad) => (None, Some(bad)),
    };
    Party {
        name: p.name.trim().to_string(),
        address,
        vat_id,
        tax_registration: if seller {
            party::tax_registration(p)
        } else {
            None
        },
        legal_id: opt(&p.legal_id),
        endpoint,
        email_endpoint: party::email_endpoint(p),
        bad_endpoint,
        email: opt(&p.email),
        phone: opt(&p.phone),
    }
}

/// UN/ECE Recommendation 20 code for a unit label. Unknown or empty units
/// are `C62` ("one", a unit of count).
pub fn unit_code(unit: &str) -> &'static str {
    let u = unit.trim().to_lowercase();
    let u = u.trim_end_matches('.');
    match u {
        "day" | "days" | "d" | "día" | "días" | "dia" | "dias" | "tag" | "tage" | "jour"
        | "jours" | "j" | "giorno" | "giorni" | "dag" | "dagen" => "DAY",
        "hour" | "hours" | "h" | "hr" | "hrs" | "hora" | "horas" | "stunde" | "stunden" | "std"
        | "heure" | "heures" | "ora" | "ore" | "uur" => "HUR",
        "minute" | "minutes" | "min" | "mins" | "minuto" | "minutos" | "minuten" | "minuti" => {
            "MIN"
        }
        "week" | "weeks" | "wk" | "semana" | "semanas" | "woche" | "wochen" | "semaine"
        | "semaines" | "settimana" | "settimane" | "weken" => "WEE",
        "month" | "months" | "mo" | "mes" | "meses" | "monat" | "monate" | "mois" | "mese"
        | "mesi" | "maand" | "maanden" | "mês" => "MON",
        "year" | "years" | "yr" | "año" | "años" | "jahr" | "jahre" | "an" | "ans" | "anno"
        | "anni" | "jaar" | "ano" | "anos" => "ANN",
        "km" => "KMT",
        "kg" => "KGM",
        "l" | "litre" | "liter" | "litres" | "liters" => "LTR",
        "kwh" => "KWH",
        "page" | "pages" => "QA",
        "piece" | "pieces" | "pc" | "pcs" | "unit" | "units" | "unidad" | "unidades" | "stück"
        | "stk" | "pièce" | "pièces" | "pezzo" | "pezzi" | "stuk" | "stuks" => "H87",
        "set" | "sets" => "SET",
        _ => "C62",
    }
}

impl Document {
    /// Map a resolved invoice. Never fails: missing data shows up as empty
    /// fields that [`Document::validate`](super::validate) reports.
    pub fn from_invoice(inv: &Invoice) -> Document {
        let kind = DocumentKind::of(inv);
        let tax_note = inv.tax_note.as_deref();
        let lines: Vec<Line> = inv
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let rate = inv.item_tax_rate(it);
                Line {
                    id: (i + 1).to_string(),
                    name: it.description.trim().to_string(),
                    description: opt(&it.detail),
                    quantity: it.quantity,
                    unit_code: unit_code(&it.unit),
                    price: it.rate,
                    net: it.total(),
                    category: VatCategory::for_rate(rate, tax_note),
                    rate,
                }
            })
            .collect();

        let vat = inv
            .tax_breakdown()
            .into_iter()
            .map(|g| {
                let category = VatCategory::for_rate(g.rate, tax_note);
                let exemption_reason = match category {
                    VatCategory::Standard | VatCategory::ZeroRated => None,
                    _ => tax_note.map(|note| match tax_note_preset(note) {
                        Some(p) => {
                            i18n::label(&inv.language, &format!("tax_note_{p}"), &inv.labels)
                        }
                        None => note.trim().to_string(),
                    }),
                };
                VatBreakdown {
                    category,
                    rate: g.rate,
                    basis: g.base,
                    tax: g.tax,
                    exemption_reason,
                    exemption_code: category.exemption_code(),
                }
            })
            .collect();

        let line_total = inv.subtotal();
        let tax = inv.tax();
        let total = inv.total();
        let prepaid = if inv.status == Status::Paid {
            total
        } else {
            Decimal::ZERO
        };

        let iban: Option<String> = inv
            .payment
            .iban
            .as_deref()
            .map(|s| s.chars().filter(|c| !c.is_whitespace()).collect::<String>())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_uppercase());
        let code = match (&iban, inv.currency.as_str()) {
            (None, _) => "1",
            (Some(_), "EUR") => "58",
            (Some(_), _) => "30",
        };

        Document {
            kind,
            number: inv.number.trim().to_string(),
            issue_date: inv.date,
            due_date: inv.due,
            currency: inv.currency.trim().to_uppercase(),
            buyer_reference: opt(&inv.to.buyer_reference),
            notes: inv
                .notes
                .iter()
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .collect(),
            seller: party(&inv.from, true),
            buyer: party(&inv.to, false),
            payment: PaymentMeans {
                code,
                remittance: inv.number.trim().to_string(),
                iban,
                holder: opt(&inv.payment.holder),
                bic: opt(&inv.payment.bic).map(|b| b.replace(' ', "").to_uppercase()),
            },
            lines,
            vat,
            totals: Totals {
                line_total,
                tax_exclusive: line_total,
                tax,
                tax_inclusive: total,
                prepaid,
                payable: round_cents(total - prepaid),
            },
        }
    }

    /// The VAT categories used, without duplicates.
    pub fn categories(&self) -> Vec<VatCategory> {
        let mut c: Vec<VatCategory> = self.lines.iter().map(|l| l.category).collect();
        c.sort();
        c.dedup();
        c
    }

    /// Whether VAT identifiers must be left out (category `O`, BR-O-02..04).
    pub fn omit_vat_ids(&self) -> bool {
        self.categories() == [VatCategory::OutsideScope]
    }
}

/// Money: two decimals.
pub fn amount(x: Decimal) -> String {
    format!("{:.2}", round_cents(x))
}

/// Quantities, prices and percentages: as many decimals as needed.
pub fn decimal(x: Decimal) -> String {
    let n = x.normalize();
    if n.is_zero() {
        "0".into()
    } else {
        n.to_string()
    }
}
