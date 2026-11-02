//! First-run setup: turn the answers of the editor's welcome form into a
//! commented `config.yaml`, plus the small input checks the forms share.

use crate::config::{ClientConfig, DEFAULT_CONFIG};
use crate::config_edit::{self, Edit, EditError, Field};
use crate::model::{Decimal, Party, Payment, num};

/// Everything the welcome form asks for.
#[derive(Debug, Clone, Default)]
pub struct Setup {
    pub from: Party,
    pub payment: Payment,
    pub client_key: String,
    pub client: ClientConfig,
    pub number_prefix: String,
    pub currency: String,
    pub tax_rate: Decimal,
    /// As typed, `~` allowed (it is written to the file unexpanded).
    pub invoices_dir: String,
}

impl Setup {
    /// The default config template with these answers filled in. Comments,
    /// key order and the commented-out examples of the template are kept.
    /// Empty optional answers drop the template's placeholder line.
    pub fn render(&self) -> Result<String, EditError> {
        let mut text = config_edit::apply(
            DEFAULT_CONFIG,
            &[
                Edit::set(&["invoices_dir"], Field::Str(self.invoices_dir.clone())),
                Edit::set(&["number_prefix"], Field::Str(self.number_prefix.clone())),
                Edit::set(&["currency"], Field::Str(self.currency.clone())),
                Edit::set(&["tax_rate"], Field::Num(self.tax_rate)),
            ],
        )?;
        text = config_edit::set_from(&text, &self.from)?;
        text = config_edit::set_payment(&text, &self.payment)?;
        let template = config_edit::parse_config(DEFAULT_CONFIG)?;
        let sample = template.clients.keys().next().cloned();
        text =
            config_edit::upsert_client(&text, sample.as_deref(), &self.client_key, &self.client)?;
        Ok(text)
    }
}

/// Non-empty trimmed lines of a multi-line text field.
pub fn lines(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// `None` for an empty field.
pub fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Parse an optional number field exactly as typed (`450`, `7.7`, `21%`).
/// A single comma not followed by three digits is a decimal comma (`12,5`);
/// otherwise commas group thousands (`1,234.50`).
pub fn number(s: &str) -> Result<Option<Decimal>, String> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let bad = || format!("`{t}` is not a number");
    let mut v = t.trim_end_matches('%').trim().to_string();
    if v.matches(',').count() == 1
        && !v.contains('.')
        && v.split(',').nth(1).is_some_and(|f| f.len() != 3)
    {
        v = v.replace(',', ".");
    }
    let plain = v.replace(',', "");
    let d: Decimal = plain.parse().map_err(|_| bad())?;
    // Commas must be real thousands groups: the invoice parser decides that.
    if v.contains(',') && num::from_text(&v) != Some(d) {
        return Err(bad());
    }
    Ok(Some(d))
}

/// Check an IBAN: country code, length and the ISO 13616 mod-97 checksum.
/// Spaces are ignored. Returns a message suitable for an inline warning.
pub fn check_iban(iban: &str) -> Result<(), String> {
    let s: String = iban
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    if s.len() < 15 || s.len() > 34 {
        return Err("an IBAN has 15 to 34 characters".into());
    }
    let b = s.as_bytes();
    if !b[0].is_ascii_alphabetic() || !b[1].is_ascii_alphabetic() {
        return Err("an IBAN starts with a two-letter country code".into());
    }
    if !b[2].is_ascii_digit() || !b[3].is_ascii_digit() {
        return Err("the country code is followed by two check digits".into());
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("an IBAN has only letters and digits".into());
    }
    if let Some(len) = iban_length(&s[..2])
        && s.len() != len
    {
        return Err(format!(
            "a {} IBAN has {len} characters, this one has {}",
            &s[..2],
            s.len()
        ));
    }
    let rearranged = s[4..].chars().chain(s[..4].chars());
    let mut rem: u32 = 0;
    for c in rearranged {
        let v = c.to_digit(36).expect("alphanumeric checked above");
        rem = if v < 10 {
            (rem * 10 + v) % 97
        } else {
            (rem * 100 + v) % 97
        };
    }
    if rem != 1 {
        return Err("the IBAN checksum does not match (typo?)".into());
    }
    Ok(())
}

/// Lengths for common SEPA countries; others only get the generic checks.
fn iban_length(country: &str) -> Option<usize> {
    Some(match country {
        "AT" => 20,
        "BE" => 16,
        "CH" => 21,
        "CZ" => 24,
        "DE" => 22,
        "DK" => 18,
        "EE" => 20,
        "ES" => 24,
        "FI" => 18,
        "FR" => 27,
        "GB" => 22,
        "GR" => 27,
        "HR" => 21,
        "HU" => 28,
        "IE" => 22,
        "IT" => 27,
        "LT" => 20,
        "LU" => 20,
        "LV" => 21,
        "MT" => 31,
        "NL" => 18,
        "NO" => 15,
        "PL" => 28,
        "PT" => 25,
        "RO" => 24,
        "SE" => 24,
        "SI" => 19,
        "SK" => 24,
        _ => return None,
    })
}

/// A number prefix must be usable in file names (`<prefix>-<year>-<seq>.md`).
pub fn check_prefix(prefix: &str) -> Result<(), String> {
    let p = prefix.trim();
    if p.is_empty() {
        return Err("the prefix cannot be empty".into());
    }
    if !p
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("use letters, digits, `-` or `_`".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn sample() -> Setup {
        Setup {
            from: Party {
                name: "Alex Morgan".into(),
                address: vec!["Rue de la Gare".into(), "1616 Luxembourg".into()],
                tax: vec!["VAT ID: LU12345678".into()],
                email: Some("alex@example.com".into()),
                phone: None,
            },
            payment: Payment {
                holder: Some("ALEX MORGAN".into()),
                iban: Some("DE89 3704 0044 0532 0130 00".into()),
                bic: Some("COBADEFFXXX".into()),
                notes: vec![],
            },
            client_key: "northwind".into(),
            client: ClientConfig {
                party: Party {
                    name: "Northwind Labs SL".into(),
                    address: vec!["Calle Mayor, 12".into(), "28001 Madrid".into()],
                    tax: vec!["Tax ID: B12345678".into()],
                    ..Party::default()
                },
                rate: Some(d("450")),
                unit: Some("days".into()),
                description: Some("Senior Software Engineer".into()),
                ..ClientConfig::default()
            },
            number_prefix: "EINV".into(),
            currency: "EUR".into(),
            tax_rate: d("17"),
            invoices_dir: "~/Documents/Invoices".into(),
        }
    }

    #[test]
    fn render_fills_template_and_keeps_comments() {
        let text = sample().render().unwrap();
        // Comments of the template survive.
        assert!(text.starts_with("# invgen configuration."), "{text}");
        assert!(
            text.contains("invoices_dir: ~/Documents/Invoices   # where `invgen new`"),
            "{text}"
        );
        assert!(
            text.contains("number_prefix: EINV                  # numbers look like"),
            "{text}"
        );
        assert!(
            text.contains("tax_rate: 17                         # percent"),
            "{text}"
        );
        assert!(text.contains("  # phone: \"+00 000 000 000\""), "{text}");
        assert!(text.contains("  # notes: [\"Bank name\""), "{text}");
        assert!(
            text.contains("    # currency: EUR      # optional"),
            "{text}"
        );
        assert!(
            text.contains("    rate: 450            # default rate"),
            "{text}"
        );
        assert!(
            text.contains("address: [\"Calle Mayor, 12\", 28001 Madrid]"),
            "{text}"
        );
        assert!(!text.contains("acme"), "{text}");
        assert!(!text.contains("Your Name"), "{text}");

        let cfg: Config = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(cfg.from.name, "Alex Morgan");
        assert_eq!(cfg.from.tax, vec!["VAT ID: LU12345678"]);
        assert_eq!(cfg.from.email.as_deref(), Some("alex@example.com"));
        assert_eq!(
            cfg.payment.iban.as_deref(),
            Some("DE89 3704 0044 0532 0130 00")
        );
        assert_eq!(cfg.number_prefix, "EINV");
        assert_eq!(cfg.tax_rate, d("17"));
        let c = &cfg.clients["northwind"];
        assert_eq!(c.party.name, "Northwind Labs SL");
        assert_eq!(c.party.address, vec!["Calle Mayor, 12", "28001 Madrid"]);
        assert_eq!(c.rate, Some(d("450")));
        assert_eq!(c.description.as_deref(), Some("Senior Software Engineer"));
        assert_eq!(cfg.clients.len(), 1);
    }

    #[test]
    fn render_drops_placeholders_for_empty_answers() {
        let mut s = sample();
        s.from.email = None;
        s.from.tax.clear();
        s.payment.bic = None;
        s.client.rate = None;
        s.client.party.tax.clear();
        let text = s.render().unwrap();
        assert!(!text.contains("you@example.com"), "{text}");
        assert!(!text.contains("XX00000000"), "{text}");
        assert!(!text.contains("XXXXXXXXXXX"), "{text}");
        assert!(!text.contains("rate: 450"), "{text}");
        let cfg: Config = serde_yaml_ng::from_str(&text).unwrap();
        assert_eq!(cfg.from.email, None);
        assert!(cfg.from.tax.is_empty());
        assert_eq!(cfg.clients["northwind"].rate, None);
    }

    #[test]
    fn rendered_config_creates_a_parsable_first_invoice() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = sample();
        s.invoices_dir = dir.path().join("inv").display().to_string();
        let text = s.render().unwrap();
        let path = dir.path().join("config.yaml");
        config_edit::write_atomic(&path, &text).unwrap();
        let cfg = Config::load_from(&path).unwrap();
        let first = cfg.write_new_invoice(Some("northwind")).unwrap();
        let year = chrono::Local::now().format("%Y").to_string();
        assert_eq!(first, dir.path().join(format!("inv/EINV-{year}-01.md")));
        let md = std::fs::read_to_string(&first).unwrap();
        let inv = crate::parse::parse_invoice(&md, &cfg).unwrap();
        assert_eq!(inv.to.name, "Northwind Labs SL");
        assert_eq!(inv.from.name, "Alex Morgan");
        assert_eq!(inv.items[0].rate, d("450"));
        // The same number is not handed out twice.
        let second = cfg.write_new_invoice(Some("northwind")).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn iban_checks() {
        assert!(check_iban("DE89 3704 0044 0532 0130 00").is_ok());
        assert!(check_iban("de89370400440532013000").is_ok());
        assert!(check_iban("GB82 WEST 1234 5698 7654 32").is_ok());
        assert!(check_iban("LU28 0019 4006 4475 0000").is_ok());
        let typo = check_iban("DE89 3704 0044 0532 0130 01").unwrap_err();
        assert!(typo.contains("checksum"), "{typo}");
        let short = check_iban("DE89 3704 0044 0532 0130").unwrap_err();
        assert!(short.contains("22"), "{short}");
        assert!(check_iban("XX00 0000 0000 0000 0000 00").is_err());
        assert!(check_iban("1234").is_err());
        assert!(check_iban("DE8937040044053201300!").is_err());
    }

    #[test]
    fn field_helpers() {
        assert_eq!(lines(" a \n\n b\n"), vec!["a", "b"]);
        assert_eq!(opt("  "), None);
        assert_eq!(opt(" x "), Some("x".into()));
        assert_eq!(number(""), Ok(None));
        assert_eq!(number("12,5"), Ok(Some(d("12.5"))));
        assert_eq!(number(" 450 "), Ok(Some(d("450"))));
        assert!(number("abc").is_err());
        assert!(number("inf").is_err());
        assert_eq!(number("7.7"), Ok(Some(d("7.7"))));
        assert_eq!(number("450.0").unwrap().unwrap().to_string(), "450.0");
        assert_eq!(number("21%"), Ok(Some(d("21"))));
        assert_eq!(number("1,234.50"), Ok(Some(d("1234.50"))));
        assert_eq!(number("1,500"), Ok(Some(d("1500"))));
        assert!(number("12x").is_err());
        assert!(number("1,2,3").is_err());
        assert!(check_prefix("EINV").is_ok());
        assert!(check_prefix("").is_err());
        assert!(check_prefix("A/B").is_err());
    }
}
