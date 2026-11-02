//! Checks run before any XML is written: every mandatory business term
//! (and the VAT category rules of EN 16931) that the invoice cannot fill in
//! is reported, all at once, with the invgen key that fixes it.

use std::fmt;

use super::Format;
use super::document::{Document, Party, VatCategory};
use crate::model::Decimal;

/// One missing or invalid business term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// The EN 16931 business term(s), e.g. `BT-40`.
    pub term: &'static str,
    /// What is wrong and how to fix it.
    pub message: String,
}

/// Everything that keeps an invoice from becoming a valid e-invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub format: Format,
    pub issues: Vec<Issue>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot write a {} e-invoice, {} field{} missing or invalid:",
            self.format.name(),
            self.issues.len(),
            if self.issues.len() == 1 {
                " is"
            } else {
                "s are"
            }
        )?;
        for i in &self.issues {
            write!(f, "\n  {}: {}", i.term, i.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

fn is_currency(c: &str) -> bool {
    c.len() == 3 && c.chars().all(|c| c.is_ascii_uppercase())
}

fn is_country(c: &str) -> bool {
    c.len() == 2 && c.chars().all(|c| c.is_ascii_uppercase())
}

/// Which side of the invoice a party is, for messages.
struct Side {
    /// `seller` / `buyer`.
    role: &'static str,
    /// Where to fix it: `from:` in the config / the client.
    key: &'static str,
    name_bt: &'static str,
    country_bt: &'static str,
    vat_bt: &'static str,
    endpoint_bt: &'static str,
}

const SELLER: Side = Side {
    role: "seller",
    key: "`from:` in the config",
    name_bt: "BT-27",
    country_bt: "BT-40",
    vat_bt: "BT-31",
    endpoint_bt: "BT-34",
};

const BUYER: Side = Side {
    role: "buyer",
    key: "the client (or `to:` in the invoice)",
    name_bt: "BT-44",
    country_bt: "BT-55",
    vat_bt: "BT-48",
    endpoint_bt: "BT-49",
};

fn check_party(p: &Party, side: &Side, format: Format, out: &mut Vec<Issue>) {
    if p.name.is_empty() {
        out.push(Issue {
            term: side.name_bt,
            message: format!(
                "{} name is empty: set `name:` under {}",
                side.role, side.key
            ),
        });
    }
    match p.address.country.as_deref() {
        None => out.push(Issue {
            term: side.country_bt,
            message: format!(
                "{} country is unknown: set `country:` (ISO code like `DE`) under {}, \
                 or end `address:` with the country name",
                side.role, side.key
            ),
        }),
        Some(c) if !is_country(c) => out.push(Issue {
            term: side.country_bt,
            message: format!(
                "{} country `{c}` is not a two-letter ISO 3166-1 code (like `DE`)",
                side.role
            ),
        }),
        Some(_) => {}
    }
    if let Some(v) = &p.vat_id
        && !(v.len() > 2 && v[..2].chars().all(|c| c.is_ascii_uppercase()))
    {
        out.push(Issue {
            term: side.vat_bt,
            message: format!(
                "{} VAT ID `{v}` must start with its two-letter country prefix \
                 (set `vat_id:` under {})",
                side.role, side.key
            ),
        });
    }
    if let Some(bad) = &p.bad_endpoint {
        out.push(Issue {
            term: side.endpoint_bt,
            message: format!(
                "{} `endpoint: {bad}` must look like `<scheme>:<id>`, e.g. `0208:0123456789`",
                side.role
            ),
        });
    } else if format == Format::Ubl && p.endpoint.is_none() {
        out.push(Issue {
            term: side.endpoint_bt,
            message: format!(
                "Peppol needs the {} electronic address (Peppol participant ID): set \
                 `endpoint: <scheme>:<id>` under {} (it defaults to the VAT ID for most EU \
                 countries)",
                side.role, side.key
            ),
        });
    }
}

/// All issues for `doc` in `format`. Empty means the document can be written.
pub fn issues(doc: &Document, format: Format) -> Vec<Issue> {
    let mut out = Vec::new();
    if doc.number.is_empty() {
        out.push(Issue {
            term: "BT-1",
            message: "invoice number is empty: set `number:`".into(),
        });
    }
    if !is_currency(&doc.currency) {
        out.push(Issue {
            term: "BT-5",
            message: format!(
                "currency `{}` is not a three-letter ISO 4217 code (like `EUR`)",
                doc.currency
            ),
        });
    }
    if doc.due_date.is_none() && doc.totals.payable > Decimal::ZERO {
        out.push(Issue {
            term: "BT-9",
            message: "an amount is due but there is no due date: set `due:` or `due_days:`".into(),
        });
    }
    if format == Format::Ubl && doc.buyer_reference.is_none() {
        out.push(Issue {
            term: "BT-10",
            message: "Peppol needs a buyer reference: set `buyer_reference:` in the invoice \
                      or on the client (the client's purchase order or reference)"
                .into(),
        });
    }
    if doc.kind.is_credit_note() && doc.kind.preceding_invoice().is_none() {
        out.push(Issue {
            term: "BT-25",
            message: "a credit note needs the invoice it corrects: set `refers_to:`".into(),
        });
    }
    check_party(&doc.seller, &SELLER, format, &mut out);
    check_party(&doc.buyer, &BUYER, format, &mut out);

    if doc.lines.is_empty() {
        out.push(Issue {
            term: "BG-25",
            message: "the invoice has no lines".into(),
        });
    }
    for l in &doc.lines {
        if l.name.is_empty() {
            out.push(Issue {
                term: "BT-153",
                message: format!("line {} has no description", l.id),
            });
        }
        if l.price < Decimal::ZERO {
            out.push(Issue {
                term: "BT-146",
                message: format!(
                    "line {} has a negative rate; use a negative quantity instead",
                    l.id
                ),
            });
        }
    }

    // VAT category rules (BR-S/Z/E/AE/O-02 and friends).
    let cats = doc.categories();
    let seller_id = doc.seller.vat_id.is_some() || doc.seller.tax_registration.is_some();
    for c in &cats {
        match c {
            VatCategory::Standard | VatCategory::ZeroRated | VatCategory::Exempt if !seller_id => {
                out.push(Issue {
                    term: "BT-31",
                    message: format!(
                        "VAT category {} needs the seller VAT ID or tax number: add a `tax:` \
                         line like \"VAT ID: DE123456789\" or set `vat_id:` under `from:`",
                        c.code()
                    ),
                });
            }
            VatCategory::ReverseCharge => {
                if doc.seller.vat_id.is_none() {
                    out.push(Issue {
                        term: "BT-31",
                        message: "reverse charge needs the seller VAT ID: add a `tax:` line \
                                  like \"VAT ID: DE123456789\" or set `vat_id:` under `from:`"
                            .into(),
                    });
                }
                if doc.buyer.vat_id.is_none() {
                    out.push(Issue {
                        term: "BT-48",
                        message: "reverse charge needs the client's VAT ID: set `vat_id:` on \
                                  the client (with its country prefix, like `ESB12345678`) or \
                                  add a `tax:` line like \"VAT ID: ESB12345678\""
                            .into(),
                    });
                }
            }
            _ => {}
        }
    }
    if cats.contains(&VatCategory::OutsideScope) && cats.len() > 1 {
        out.push(Issue {
            term: "BT-151",
            message: "`tax_note: outside_scope` cannot be combined with lines taxed above \
                      0% (VAT category O must be the only category)"
                .into(),
        });
    }
    // BR-CO-26: the seller must be identifiable by VAT ID or legal ID.
    let seller_vat_shown = doc.seller.vat_id.is_some() && !doc.omit_vat_ids();
    if !seller_vat_shown && doc.seller.legal_id.is_none() {
        let why = if doc.omit_vat_ids() {
            "outside the scope of VAT no VAT ID is shown, so the seller needs"
        } else {
            "the seller has no VAT ID, so it needs"
        };
        out.push(Issue {
            term: "BT-30",
            message: format!("{why} a company registration number: set `legal_id:` under `from:`"),
        });
    }
    if doc.payment.code != "1" && doc.payment.iban.is_none() {
        out.push(Issue {
            term: "BT-84",
            message: "set `iban:` under `payment:`".into(),
        });
    }
    out
}

/// `Ok` when `doc` can be written in `format`.
pub fn validate(doc: &Document, format: Format) -> Result<(), ValidationError> {
    let issues = issues(doc, format);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ValidationError { format, issues })
    }
}
