//! Structured e-invoices following the European standard EN 16931:
//!
//! - Factur-X / ZUGFeRD: the invoice PDF as PDF/A-3b with the CII XML
//!   embedded as `factur-x.xml` (profile EN 16931).
//! - UBL 2.1 following Peppol BIS Billing 3.0, written next to the PDF.
//!
//! The flow is always the same: [`Document::from_invoice`] maps the
//! resolved invoice onto EN 16931 business terms, [`validate`] lists every
//! mandatory term that is missing, and only then a syntax writer runs. See
//! docs/E-INVOICING.md for the mapping table and the heuristics.

mod cii;
mod document;
mod facturx;
mod party;
mod ubl;
mod validate;
mod xml;

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use document::{
    Document, DocumentKind, Line, PaymentMeans, Totals, VatBreakdown, VatCategory, unit_code,
};
pub use facturx::{
    FACTURX_FILE_NAME, facturx_attachment, facturx_export, facturx_pdf, patch_xmp,
    xref_is_consistent,
};
pub use party::{Address, Endpoint};
pub use validate::{Issue, ValidationError, issues, validate};

use crate::model::Invoice;

/// An e-invoice format `invgen build` can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Format {
    /// Factur-X / ZUGFeRD hybrid PDF (CII XML inside a PDF/A-3b).
    FacturX,
    /// UBL 2.1 XML, Peppol BIS Billing 3.0.
    Ubl,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::FacturX => "Factur-X",
            Format::Ubl => "UBL (Peppol BIS 3.0)",
        }
    }

    /// The config spelling.
    pub fn key(self) -> &'static str {
        match self {
            Format::FacturX => "facturx",
            Format::Ubl => "ubl",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

impl FromStr for Format {
    type Err = String;

    /// `facturx` (also `factur-x`, `zugferd`) or `ubl` (also `peppol`),
    /// case-insensitive.
    fn from_str(s: &str) -> Result<Format, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "facturx" | "factur-x" | "factur_x" | "zugferd" => Ok(Format::FacturX),
            "ubl" | "peppol" => Ok(Format::Ubl),
            _ => Err(format!(
                "unknown e-invoice format `{s}` (use facturx or ubl)"
            )),
        }
    }
}

impl Serialize for Format {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for Format {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Format, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Serde for the config key `e_invoice`: one format, a list, or
/// `none` / `false` / empty for none.
pub mod formats {
    use super::Format;
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &[Format], s: S) -> Result<S::Ok, S::Error> {
        match v {
            [one] => one.serialize(s),
            many => many.serialize(s),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Format>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Bool(bool),
            One(String),
            Many(Vec<String>),
        }
        let names = match Option::<Raw>::deserialize(d)? {
            None | Some(Raw::Bool(false)) => return Ok(Vec::new()),
            Some(Raw::Bool(true)) => {
                return Err(D::Error::custom(
                    "`e_invoice: true` is ambiguous, use facturx or ubl",
                ));
            }
            Some(Raw::One(s)) => vec![s],
            Some(Raw::Many(v)) => v,
        };
        let mut out = Vec::new();
        for n in names {
            if matches!(n.trim().to_ascii_lowercase().as_str(), "" | "none") {
                continue;
            }
            let f: Format = n.parse().map_err(D::Error::custom)?;
            if !out.contains(&f) {
                out.push(f);
            }
        }
        out.sort();
        Ok(out)
    }
}

/// Map and validate `inv` for `format`.
pub fn document(inv: &Invoice, format: Format) -> Result<Document, ValidationError> {
    let doc = Document::from_invoice(inv);
    validate(&doc, format)?;
    Ok(doc)
}

/// The Factur-X (CII, EN 16931 profile) XML for `inv`.
pub fn cii_xml(inv: &Invoice) -> Result<String, ValidationError> {
    document(inv, Format::FacturX).map(|d| cii::write(&d))
}

/// The UBL (Peppol BIS Billing 3.0) XML for `inv`.
pub fn ubl_xml(inv: &Invoice) -> Result<String, ValidationError> {
    document(inv, Format::Ubl).map(|d| ubl::write(&d))
}

#[cfg(test)]
mod tests;
