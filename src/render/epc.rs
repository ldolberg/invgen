//! EPC QR code ("GiroCode", EPC069-12) for SEPA credit transfers.
//!
//! With `payment.qr: true` the invoice carries a QR code that banking apps
//! scan to prefill a transfer: beneficiary, IBAN, BIC, amount and the
//! invoice number as the remittance text. We emit version 002 (BIC
//! optional), character set 1 (UTF-8) and error correction level M.
//!
//! The code is only produced when it can be correct: EUR invoices with a
//! valid IBAN and a positive total. Otherwise [`check`] says why it was
//! skipped, so `invgen build` can warn while the GUI stays silent.

use std::fmt;

use qrcode::{EcLevel, QrCode};

use crate::model::{Decimal, Invoice, round_cents};

/// Maximum payload size allowed by EPC069-12, in bytes.
const MAX_BYTES: usize = 331;
/// Largest amount the format accepts.
const MAX_AMOUNT: Decimal = Decimal::from_parts(0x4876_E7FF, 0x17, 0, false, 2);

/// The fields of an EPC QR payload, already cleaned. Exactly one of
/// `reference` (structured, e.g. RF creditor reference) and `text`
/// (unstructured remittance) should be non-empty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Epc {
    pub bic: String,
    pub name: String,
    pub iban: String,
    pub amount: Option<Decimal>,
    pub purpose: String,
    pub reference: String,
    pub text: String,
    pub info: String,
}

impl Epc {
    /// The payload string: one element per line (LF separated), with
    /// trailing empty elements dropped as the spec requires.
    pub fn payload(&self) -> String {
        let amount = self
            .amount
            .map(|a| format!("EUR{}", round_cents(a).normalize()))
            .unwrap_or_default();
        let mut lines = vec![
            "BCD",
            "002",
            "1",
            "SCT",
            &self.bic,
            &self.name,
            &self.iban,
            &amount,
            &self.purpose,
            &self.reference,
            &self.text,
            &self.info,
        ];
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
}

/// Why no QR code was rendered.
#[derive(Debug, Clone, PartialEq)]
pub enum Skip {
    /// The invoice currency (as written) is not EUR.
    Currency(String),
    NoIban,
    /// The IBAN (as written) fails the format or checksum test.
    BadIban(String),
    NotPositive,
    NoName,
    TooLong,
}

impl fmt::Display for Skip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Skip::Currency(c) => write!(f, "currency is {c}, the EPC QR code needs EUR"),
            Skip::NoIban => write!(f, "no payment IBAN is set"),
            Skip::BadIban(i) => write!(f, "IBAN `{i}` is not valid (wrong format or checksum)"),
            Skip::NotPositive => write!(f, "the invoice total is not positive"),
            Skip::NoName => write!(f, "no beneficiary name (payment.holder or from.name)"),
            Skip::TooLong => write!(f, "the payment data exceeds {MAX_BYTES} bytes"),
        }
    }
}

/// The EPC data for an invoice, or why it cannot have one. Ignores
/// `payment.qr`; see [`check`].
pub fn epc(inv: &Invoice) -> Result<Epc, Skip> {
    let cur = inv.currency.trim();
    if !cur.eq_ignore_ascii_case("EUR") {
        return Err(Skip::Currency(cur.to_string()));
    }
    let raw_iban = inv.payment.iban.as_deref().unwrap_or("").trim();
    if raw_iban.is_empty() {
        return Err(Skip::NoIban);
    }
    let iban = compact(raw_iban);
    if !iban_ok(&iban) {
        return Err(Skip::BadIban(raw_iban.to_string()));
    }
    let total = round_cents(inv.total());
    if total <= Decimal::ZERO {
        return Err(Skip::NotPositive);
    }
    let holder = inv.payment.holder.as_deref().unwrap_or("");
    let name = match field(holder, 70) {
        n if n.is_empty() => field(&inv.from.name, 70),
        n => n,
    };
    if name.is_empty() {
        return Err(Skip::NoName);
    }
    let bic = compact(inv.payment.bic.as_deref().unwrap_or(""));
    Ok(Epc {
        // Optional in version 002: leave a malformed one out rather than
        // produce a code banking apps reject.
        bic: if bic_ok(&bic) { bic } else { String::new() },
        name,
        iban,
        // Above the format's limit the payer types the amount.
        amount: (total <= MAX_AMOUNT).then_some(total),
        text: field(&inv.number, 140),
        ..Epc::default()
    })
}

/// `None` when `payment.qr` is off; otherwise the payload or why it was skipped.
pub fn check(inv: &Invoice) -> Option<Result<String, Skip>> {
    if !inv.payment.qr {
        return None;
    }
    Some(epc(inv).and_then(|e| {
        let p = e.payload();
        if p.len() > MAX_BYTES {
            Err(Skip::TooLong)
        } else {
            Ok(p)
        }
    }))
}

/// A one-line warning for the CLI when the QR code was requested but skipped.
pub fn warning(inv: &Invoice) -> Option<String> {
    match check(inv)? {
        Ok(_) => None,
        Err(why) => Some(format!("payment QR code skipped: {why}")),
    }
}

/// The QR code for `payload` (level M) as an SVG without quiet zone; the
/// template keeps the surrounding area blank instead.
pub(super) fn svg(payload: &str) -> Option<String> {
    let code = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M).ok()?;
    Some(
        code.render::<qrcode::render::svg::Color>()
            .quiet_zone(false)
            .module_dimensions(1, 1)
            .build(),
    )
}

/// The QR module matrix for `payload`, row-major (`true` = dark), with its side.
#[cfg(test)]
pub(super) fn matrix(payload: &str) -> (usize, Vec<bool>) {
    let code = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M).unwrap();
    (
        code.width(),
        code.to_colors()
            .iter()
            .map(|c| *c == qrcode::Color::Dark)
            .collect(),
    )
}

/// Uppercase with all whitespace removed.
fn compact(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase()
}

/// One line of text: control characters become spaces, trimmed, at most
/// `max` characters.
fn field(s: &str, max: usize) -> String {
    let one_line: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    one_line
        .trim()
        .chars()
        .take(max)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// ISO 13616: country code, two check digits, up to 30 alphanumerics, and
/// the mod-97 checksum equals 1. `iban` must already be compacted.
pub fn iban_ok(iban: &str) -> bool {
    let b = iban.as_bytes();
    if !(15..=34).contains(&b.len())
        || !b[..2].iter().all(u8::is_ascii_uppercase)
        || !b[2..4].iter().all(u8::is_ascii_digit)
        || !b.iter().all(u8::is_ascii_alphanumeric)
    {
        return false;
    }
    let mut rem: u32 = 0;
    for &c in b[4..].iter().chain(&b[..4]) {
        let v = if c.is_ascii_digit() {
            u32::from(c - b'0')
        } else {
            u32::from(c - b'A') + 10
        };
        rem = if v >= 10 {
            (rem * 100 + v) % 97
        } else {
            (rem * 10 + v) % 97
        };
    }
    rem == 1
}

/// ISO 9362: 4 letters (bank), 2 letters (country), 2 alphanumerics
/// (location), optionally 3 alphanumerics (branch).
fn bic_ok(bic: &str) -> bool {
    let b = bic.as_bytes();
    matches!(b.len(), 8 | 11)
        && b[..6].iter().all(u8::is_ascii_uppercase)
        && b[6..].iter().all(u8::is_ascii_alphanumeric)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LineItem, Party, Payment, Status};
    use chrono::NaiveDate;
    use std::str::FromStr;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    pub(crate) fn invoice() -> Invoice {
        Invoice {
            number: "EINV-2026-23".into(),
            date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
            due: None,
            status: Status::Pending,
            paid_on: None,
            currency: "EUR".into(),
            from: Party {
                name: "Alex Morgan".into(),
                ..Default::default()
            },
            to: Party {
                name: "Northwind Labs".into(),
                ..Default::default()
            },
            payment: Payment {
                holder: Some("ALEX MORGAN".into()),
                iban: Some("DE89 3704 0044 0532 0130 00".into()),
                bic: Some("cobadeffxxx".into()),
                notes: vec![],
                qr: true,
            },
            items: vec![LineItem {
                description: "Consulting".into(),
                detail: None,
                quantity: dec("2"),
                rate: dec("61.725"),
                unit: String::new(),
            }],
            tax_rate: Decimal::ZERO,
            tax_label: "VAT".into(),
            notes: vec![],
            signature: None,
            accent: "#111827".into(),
            language: "en".into(),
            labels: Default::default(),
            template: None,
        }
    }

    /// The example in EPC069-12 / the GiroCode guidelines (structured
    /// creditor reference, purpose code, amount without trailing zero).
    #[test]
    fn spec_example_structured_reference() {
        let e = Epc {
            bic: "BHBLDEHHXXX".into(),
            name: "Franz Mustermänn".into(),
            iban: "DE71110220330123456789".into(),
            amount: Some(dec("12.3")),
            purpose: "GDDS".into(),
            reference: "RF18539007547034".into(),
            ..Epc::default()
        };
        assert_eq!(
            e.payload(),
            "BCD\n002\n1\nSCT\nBHBLDEHHXXX\nFranz Mustermänn\nDE71110220330123456789\nEUR12.3\nGDDS\nRF18539007547034"
        );
    }

    /// The Belgian Red Cross example of EPC069-12, as version 002:
    /// empty structured reference, unstructured text, beneficiary info.
    #[test]
    fn spec_example_unstructured_text() {
        let e = Epc {
            bic: "BPOTBEB1".into(),
            name: "Red Cross of Belgium".into(),
            iban: "BE72000000001616".into(),
            amount: Some(dec("1.00")),
            purpose: "CHAR".into(),
            text: "Urgency fund".into(),
            info: "Sample EPC QR code".into(),
            ..Epc::default()
        };
        assert_eq!(
            e.payload(),
            "BCD\n002\n1\nSCT\nBPOTBEB1\nRed Cross of Belgium\nBE72000000001616\nEUR1\nCHAR\n\nUrgency fund\nSample EPC QR code"
        );
    }

    #[test]
    fn invoice_payload() {
        let p = check(&invoice()).unwrap().unwrap();
        assert_eq!(
            p,
            "BCD\n002\n1\nSCT\nCOBADEFFXXX\nALEX MORGAN\nDE89370400440532013000\nEUR123.45\n\n\nEINV-2026-23"
        );
    }

    #[test]
    fn name_falls_back_bic_is_optional_and_fields_are_limited() {
        let mut inv = invoice();
        inv.payment.holder = Some("  ".into());
        inv.payment.bic = None;
        inv.from.name = format!("Alex\nMorgan {}", "x".repeat(100));
        inv.number = "N".repeat(200);
        let e = epc(&inv).unwrap();
        assert_eq!(e.bic, "");
        assert!(e.name.starts_with("Alex Morgan "));
        assert_eq!(e.name.chars().count(), 70);
        assert_eq!(e.text.chars().count(), 140);
        let p = e.payload();
        assert!(p.starts_with("BCD\n002\n1\nSCT\n\nAlex Morgan "), "{p}");
        assert!(p.len() <= MAX_BYTES);

        // A malformed BIC is left out instead of breaking the code.
        inv.payment.bic = Some("NOT A BIC".into());
        assert_eq!(epc(&inv).unwrap().bic, "");
        inv.payment.bic = Some("BPOTBEB1".into());
        assert_eq!(epc(&inv).unwrap().bic, "BPOTBEB1");
    }

    #[test]
    fn amount_formatting_and_limit() {
        let mut inv = invoice();
        inv.items[0].quantity = dec("1");
        inv.items[0].rate = dec("100");
        assert!(check(&inv).unwrap().unwrap().contains("\nEUR100\n"));
        inv.items[0].rate = dec("999999999.99");
        assert!(
            check(&inv)
                .unwrap()
                .unwrap()
                .contains("\nEUR999999999.99\n")
        );
        // Above the limit the amount is left for the payer to fill in.
        inv.items[0].rate = dec("1000000000");
        let p = check(&inv).unwrap().unwrap();
        assert!(
            p.contains("DE89370400440532013000\n\n\n\nEINV-2026-23"),
            "{p}"
        );
    }

    #[test]
    fn skip_rules() {
        let mut inv = invoice();
        inv.payment.qr = false;
        assert_eq!(check(&inv), None);
        assert_eq!(warning(&inv), None);

        let mut inv = invoice();
        inv.currency = "usd".into();
        assert_eq!(check(&inv), Some(Err(Skip::Currency("usd".into()))));
        assert!(warning(&inv).unwrap().contains("currency is usd"));
        inv.currency = " eur ".into();
        assert!(matches!(check(&inv), Some(Ok(_))));

        let mut inv = invoice();
        inv.payment.iban = None;
        assert_eq!(check(&inv), Some(Err(Skip::NoIban)));
        inv.payment.iban = Some(" ".into());
        assert_eq!(check(&inv), Some(Err(Skip::NoIban)));
        inv.payment.iban = Some("DE89 3704 0044 0532 0130 01".into());
        assert!(matches!(check(&inv), Some(Err(Skip::BadIban(_)))));
        inv.payment.iban = Some("XX00 0000 0000 0000 0000 00".into());
        assert!(warning(&inv).unwrap().contains("XX00 0000"));

        let mut inv = invoice();
        inv.items[0].rate = Decimal::ZERO;
        assert_eq!(check(&inv), Some(Err(Skip::NotPositive)));
        inv.items[0].rate = dec("-5");
        assert_eq!(check(&inv), Some(Err(Skip::NotPositive)));

        let mut inv = invoice();
        inv.payment.holder = None;
        inv.from.name = String::new();
        assert_eq!(check(&inv), Some(Err(Skip::NoName)));

        let mut inv = invoice();
        inv.payment.holder = Some("💶".repeat(70));
        inv.number = "💶".repeat(140);
        assert_eq!(check(&inv), Some(Err(Skip::TooLong)));
    }

    #[test]
    fn iban_checksum() {
        for ok in [
            "DE89370400440532013000",
            "GB82WEST12345698765432",
            "BE71096123456769",
            "FR1420041010050500013M02606",
            "LU280019400644750000",
        ] {
            assert!(iban_ok(ok), "{ok}");
        }
        for bad in [
            "DE89370400440532013001",
            "DE8937040044",
            "1E89370400440532013000",
            "DE8937040044053201300-",
            "",
        ] {
            assert!(!iban_ok(bad), "{bad}");
        }
    }

    /// The generated QR matrix decodes back to the exact payload.
    #[test]
    fn qr_matrix_decodes_to_payload() {
        let payload = check(&invoice()).unwrap().unwrap();
        let (size, dark) = matrix(&payload);
        let grid = rqrr::SimpleGrid::from_func(size, |x, y| dark[y * size + x]);
        let (meta, text) = rqrr::Grid::new(grid).decode().unwrap();
        assert_eq!(text, payload);
        assert_eq!(meta.ecc_level, 0, "level M");
        let svg = svg(&payload).unwrap();
        assert!(svg.contains("<svg"));
    }
}
