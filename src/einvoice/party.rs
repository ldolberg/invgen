//! Heuristics that read structured EN 16931 party data (VAT ID, country,
//! postcode, city) out of invgen's free-form `tax:` and `address:` lines.
//! Explicit fields (`vat_id`, `legal_id`, `country`, `endpoint`) always
//! win. The rules are documented in docs/E-INVOICING.md; keep them in sync.

use crate::model;

/// Prefixes of VAT identifiers: the EU member states (Greece is `EL`,
/// Northern Ireland `XI`) plus a few non-EU VAT systems that use the same
/// shape (United Kingdom, Switzerland, Norway).
const VAT_PREFIXES: &[&str] = &[
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "EL", "ES", "FI", "FR", "HR", "HU", "IE", "IT",
    "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK", "XI", "GB", "CH", "NO",
];

/// Words that mark a `tax:` line as a VAT number, in the languages invgen
/// supports and a few more (lowercase, matched as substrings of the label).
const VAT_LABELS: &[&str] = &[
    "vat", "ust", "tva", "iva", "btw", "mwst", "moms", "alv", "dph", "dic", "nip", "pvm", "afm",
];

/// Country names (English and native, lowercase) to ISO 3166-1 alpha-2.
const COUNTRIES: &[(&str, &str)] = &[
    ("austria", "AT"),
    ("österreich", "AT"),
    ("osterreich", "AT"),
    ("belgium", "BE"),
    ("belgië", "BE"),
    ("belgie", "BE"),
    ("belgique", "BE"),
    ("belgien", "BE"),
    ("bulgaria", "BG"),
    ("croatia", "HR"),
    ("hrvatska", "HR"),
    ("cyprus", "CY"),
    ("czech republic", "CZ"),
    ("czechia", "CZ"),
    ("česko", "CZ"),
    ("denmark", "DK"),
    ("danmark", "DK"),
    ("estonia", "EE"),
    ("eesti", "EE"),
    ("finland", "FI"),
    ("suomi", "FI"),
    ("france", "FR"),
    ("frankreich", "FR"),
    ("germany", "DE"),
    ("deutschland", "DE"),
    ("allemagne", "DE"),
    ("alemania", "DE"),
    ("germania", "DE"),
    ("duitsland", "DE"),
    ("greece", "GR"),
    ("hellas", "GR"),
    ("hungary", "HU"),
    ("magyarország", "HU"),
    ("ireland", "IE"),
    ("éire", "IE"),
    ("italy", "IT"),
    ("italia", "IT"),
    ("italie", "IT"),
    ("italien", "IT"),
    ("latvia", "LV"),
    ("latvija", "LV"),
    ("lithuania", "LT"),
    ("lietuva", "LT"),
    ("luxembourg", "LU"),
    ("luxemburg", "LU"),
    ("lëtzebuerg", "LU"),
    ("malta", "MT"),
    ("netherlands", "NL"),
    ("the netherlands", "NL"),
    ("nederland", "NL"),
    ("holland", "NL"),
    ("niederlande", "NL"),
    ("pays-bas", "NL"),
    ("países bajos", "NL"),
    ("poland", "PL"),
    ("polska", "PL"),
    ("portugal", "PT"),
    ("romania", "RO"),
    ("românia", "RO"),
    ("slovakia", "SK"),
    ("slovensko", "SK"),
    ("slovenia", "SI"),
    ("slovenija", "SI"),
    ("spain", "ES"),
    ("españa", "ES"),
    ("espana", "ES"),
    ("espagne", "ES"),
    ("spanien", "ES"),
    ("spagna", "ES"),
    ("spanje", "ES"),
    ("sweden", "SE"),
    ("sverige", "SE"),
    ("switzerland", "CH"),
    ("schweiz", "CH"),
    ("suisse", "CH"),
    ("svizzera", "CH"),
    ("norway", "NO"),
    ("norge", "NO"),
    ("iceland", "IS"),
    ("liechtenstein", "LI"),
    ("united kingdom", "GB"),
    ("uk", "GB"),
    ("great britain", "GB"),
    ("england", "GB"),
    ("scotland", "GB"),
    ("wales", "GB"),
    ("northern ireland", "GB"),
    ("united states", "US"),
    ("united states of america", "US"),
    ("usa", "US"),
    ("canada", "CA"),
    ("mexico", "MX"),
    ("méxico", "MX"),
    ("argentina", "AR"),
    ("brazil", "BR"),
    ("brasil", "BR"),
    ("chile", "CL"),
    ("uruguay", "UY"),
    ("australia", "AU"),
    ("new zealand", "NZ"),
    ("japan", "JP"),
    ("india", "IN"),
    ("singapore", "SG"),
    ("israel", "IL"),
    ("turkey", "TR"),
    ("türkiye", "TR"),
];

/// The VAT ID of a party: the explicit `vat_id`, else the first `tax:` line
/// whose value looks like a prefixed VAT ID (`LU12345678`) or that carries a
/// VAT label (`IVA: B12345678`, prefixed with the country code when the
/// value has none). Spaces, dots and dashes are removed.
pub(super) fn vat_id(p: &model::Party, country: Option<&str>) -> Option<String> {
    if let Some(v) = p
        .vat_id
        .as_deref()
        .map(normalize_id)
        .filter(|v| !v.is_empty())
    {
        return Some(v);
    }
    vat_line(p).map(|(i, labelled)| {
        let value = normalize_id(split_tax_line(&p.tax[i]).1);
        if labelled && !has_vat_prefix(&value) {
            match country.map(vat_prefix_of) {
                Some(prefix) => format!("{prefix}{value}"),
                None => value,
            }
        } else {
            value
        }
    })
}

/// Index of the `tax:` line used as the VAT ID, and whether it was picked
/// for its label (rather than the shape of its value).
fn vat_line(p: &model::Party) -> Option<(usize, bool)> {
    let lines = p.tax.iter().map(|t| split_tax_line(t));
    for (i, (_, value)) in lines.clone().enumerate() {
        if has_vat_prefix(&normalize_id(value)) {
            return Some((i, false));
        }
    }
    for (i, (label, value)) in lines.enumerate() {
        let label = label.to_lowercase();
        if !normalize_id(value).is_empty() && VAT_LABELS.iter().any(|w| label.contains(w)) {
            return Some((i, true));
        }
    }
    None
}

/// The seller's tax registration (BT-32): the first `tax:` line that is not
/// the VAT ID, as written (value after the label). Only used when the party
/// has no explicit `vat_id` covering that line.
pub(super) fn tax_registration(p: &model::Party) -> Option<String> {
    let skip = if p.vat_id.as_deref().is_some_and(|v| !v.trim().is_empty()) {
        None
    } else {
        vat_line(p).map(|(i, _)| i)
    };
    p.tax
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != skip)
        .map(|(_, t)| split_tax_line(t).1.trim())
        .find(|v| !v.is_empty())
        .map(str::to_string)
}

/// `"VAT ID: LU 123"` -> `("VAT ID", "LU 123")`; a line without a colon is
/// all value.
fn split_tax_line(line: &str) -> (&str, &str) {
    match line.split_once(':') {
        Some((label, value)) => (label.trim(), value.trim()),
        None => ("", line.trim()),
    }
}

/// Uppercase, without spaces, dots, dashes or slashes.
fn normalize_id(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '.' | '-' | '/'))
        .flat_map(char::to_uppercase)
        .collect()
}

/// `LU12345678`, `ESB12345678`, `CHE123456789MWST`: a known prefix followed
/// by letters and digits with at least two digits.
fn has_vat_prefix(v: &str) -> bool {
    v.len() >= 4
        && v.len() <= 20
        && v.chars().all(|c| c.is_ascii_alphanumeric())
        && VAT_PREFIXES.contains(&&v[..2])
        && v[2..].chars().filter(char::is_ascii_digit).count() >= 2
}

/// The VAT prefix for a country code (Greece uses `EL`).
fn vat_prefix_of(country: &str) -> &str {
    if country == "GR" { "EL" } else { country }
}

/// The party's country: explicit `country`, else the last address line
/// when it is a known country name or a two-letter code, else the prefix
/// of a VAT ID found in `tax:`. Returns the code and whether the last
/// address line was consumed as the country.
pub(super) fn country(p: &model::Party) -> (Option<String>, bool) {
    if let Some(c) = p
        .country
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let last_is_country = p.address.last().and_then(|l| country_code(l)).as_deref()
            == Some(&c.to_ascii_uppercase());
        return (Some(c.to_ascii_uppercase()), last_is_country);
    }
    if let Some(code) = p.address.last().and_then(|l| country_code(l)) {
        return (Some(code), true);
    }
    let from_vat = vat_line(p).and_then(|(i, _)| {
        let v = normalize_id(split_tax_line(&p.tax[i]).1);
        has_vat_prefix(&v).then(|| match &v[..2] {
            "EL" => "GR".to_string(),
            "XI" => "GB".to_string(),
            other => other.to_string(),
        })
    });
    (from_vat, false)
}

/// A country name or an uppercase two-letter code on its own.
fn country_code(line: &str) -> Option<String> {
    let t = line.trim().trim_end_matches(['.', ',']).trim();
    if t.len() == 2 && t.chars().all(|c| c.is_ascii_uppercase()) {
        return Some(t.to_string());
    }
    let lower = t.to_lowercase();
    COUNTRIES
        .iter()
        .find(|(name, _)| *name == lower)
        .map(|(_, code)| code.to_string())
}

/// A postal address split into what EN 16931 calls street lines, city and
/// postcode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Address {
    /// Street lines in order (BT-35, BT-36, BT-162; extra lines are joined
    /// into the third).
    pub lines: Vec<String>,
    pub city: Option<String>,
    pub postcode: Option<String>,
    pub country: Option<String>,
}

/// Split `address:` lines. The first line is always a street line. Among
/// the others, `10115 Berlin` / `L-1616 Luxembourg` (a token with digits,
/// then a name) gives postcode and city, as does `Madrid 28001`; a line
/// that is only a postcode (`28001`) takes the closest digit-free line
/// before it (else after it) as the city. Everything else stays a street
/// line. The country line was already removed by [`country`].
pub(super) fn address(p: &model::Party) -> Address {
    let (country, consumed) = country(p);
    let mut lines: Vec<String> = p
        .address
        .iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if consumed {
        lines.pop();
    }
    let mut out = Address {
        country,
        ..Address::default()
    };
    if lines.is_empty() {
        return out;
    }
    let mut city_idx = None;
    let mut postcode_idx = None;
    // `10115 Berlin` or `Berlin 10115`.
    for (i, l) in lines.iter().enumerate().skip(1) {
        let tokens: Vec<&str> = l.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }
        let (first, rest) = (tokens[0], tokens[1..].join(" "));
        let (last, init) = (
            tokens[tokens.len() - 1],
            tokens[..tokens.len() - 1].join(" "),
        );
        if is_postcode(first) && !has_digit(&rest) {
            out.postcode = Some(first.to_string());
            out.city = Some(rest);
        } else if is_postcode(last) && !has_digit(&init) {
            out.postcode = Some(last.to_string());
            out.city = Some(init.trim_end_matches(',').to_string());
        } else {
            continue;
        }
        city_idx = Some(i);
        break;
    }
    // A postcode alone on its line, with the city on a neighbour line.
    if city_idx.is_none()
        && let Some(i) = (1..lines.len()).find(|&i| is_postcode(&lines[i]))
    {
        postcode_idx = Some(i);
        out.postcode = Some(lines[i].clone());
        let before = (1..i).rev().find(|&j| !has_digit(&lines[j]));
        let after = (i + 1..lines.len()).find(|&j| !has_digit(&lines[j]));
        if let Some(j) = before.or(after) {
            city_idx = Some(j);
            out.city = Some(lines[j].clone());
        }
    }
    out.lines = lines
        .into_iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != city_idx && Some(*i) != postcode_idx)
        .map(|(_, l)| l)
        .collect();
    out
}

fn has_digit(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit())
}

/// One token of 3 to 10 characters, letters, digits and dashes, with at
/// least two digits: `28001`, `L-1616`, `1010`, `NL-1012`.
fn is_postcode(s: &str) -> bool {
    let s = s.trim();
    (3..=10).contains(&s.len())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && s.chars().filter(char::is_ascii_digit).count() >= 2
}

/// An electronic address (BT-34 / BT-49): scheme and identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Peppol EAS code, e.g. `EM` (email) or `0208`.
    pub scheme: String,
    pub id: String,
}

/// Peppol EAS schemes for VAT numbers, by VAT prefix. The participant
/// identifier is the VAT ID itself (with its prefix).
const VAT_EAS: &[(&str, &str)] = &[
    ("AT", "9914"),
    ("BE", "9925"),
    ("BG", "9926"),
    ("CH", "9927"),
    ("CY", "9928"),
    ("CZ", "9929"),
    ("DE", "9930"),
    ("EE", "9931"),
    ("EL", "9933"),
    ("ES", "9920"),
    ("FR", "9957"),
    ("GB", "9932"),
    ("HR", "9934"),
    ("HU", "9910"),
    ("IE", "9935"),
    ("IT", "0211"),
    ("LT", "9937"),
    ("LU", "9938"),
    ("LV", "9939"),
    ("MT", "9943"),
    ("NL", "9944"),
    ("PL", "9945"),
    ("PT", "9946"),
    ("RO", "9947"),
    ("SI", "9949"),
    ("SK", "9950"),
];

/// The explicit `endpoint: "<scheme>:<id>"`, else the VAT ID under its
/// country's Peppol VAT scheme (`9930:DE123456789`), else none. `Err`
/// carries a malformed `endpoint` value.
pub(super) fn endpoint(p: &model::Party, vat_id: Option<&str>) -> Result<Option<Endpoint>, String> {
    if let Some(e) = p
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
    {
        return match e.split_once(':') {
            Some((scheme, id))
                if !scheme.trim().is_empty()
                    && !id.trim().is_empty()
                    && scheme.trim().chars().all(|c| c.is_ascii_alphanumeric()) =>
            {
                Ok(Some(Endpoint {
                    scheme: scheme.trim().to_ascii_uppercase(),
                    id: id.trim().to_string(),
                }))
            }
            _ => Err(e.to_string()),
        };
    }
    Ok(vat_id.and_then(|v| {
        let prefix = v.get(..2)?;
        VAT_EAS
            .iter()
            .find(|(p, _)| *p == prefix)
            .map(|(_, scheme)| Endpoint {
                scheme: scheme.to_string(),
                id: v.to_string(),
            })
    }))
}

/// The email address as an `EM` electronic address. CII (Factur-X) accepts
/// it; Peppol does not.
pub(super) fn email_endpoint(p: &model::Party) -> Option<Endpoint> {
    p.email
        .as_deref()
        .map(str::trim)
        .filter(|e| e.contains('@'))
        .map(|e| Endpoint {
            scheme: "EM".into(),
            id: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn party(address: &[&str], tax: &[&str]) -> model::Party {
        model::Party {
            name: "X".into(),
            address: address.iter().map(|s| s.to_string()).collect(),
            tax: tax.iter().map(|s| s.to_string()).collect(),
            ..model::Party::default()
        }
    }

    #[test]
    fn vat_id_from_prefixed_value_or_label() {
        let p = party(&[], &["Tax ID: B000", "VAT ID: lu 1234.5678"]);
        assert_eq!(vat_id(&p, None).as_deref(), Some("LU12345678"));
        let p = party(&[], &["IVA: B12345678"]);
        assert_eq!(vat_id(&p, Some("ES")).as_deref(), Some("ESB12345678"));
        let p = party(&[], &["USt-IdNr.: DE 123 456 789"]);
        assert_eq!(vat_id(&p, None).as_deref(), Some("DE123456789"));
        let p = party(&[], &["ΑΦΜ / VAT: 123456789"]);
        assert_eq!(vat_id(&p, Some("GR")).as_deref(), Some("EL123456789"));
        // A plain tax number is not a VAT ID.
        let p = party(&[], &["Tax ID: B12345678", "Steuernummer: 12/345/67890"]);
        assert_eq!(vat_id(&p, Some("ES")), None);
        // The explicit field wins.
        let mut p = party(&[], &["VAT ID: LU12345678"]);
        p.vat_id = Some("lu-999".into());
        assert_eq!(vat_id(&p, None).as_deref(), Some("LU999"));
    }

    #[test]
    fn tax_registration_skips_the_vat_line() {
        let p = party(&[], &["VAT ID: LU12345678", "Tax ID: R.C.S. B000"]);
        assert_eq!(tax_registration(&p).as_deref(), Some("R.C.S. B000"));
        let p = party(&[], &["VAT ID: LU12345678"]);
        assert_eq!(tax_registration(&p), None);
    }

    #[test]
    fn country_from_field_address_or_vat() {
        let p = party(&["Street 1", "Madrid", "28001", "España"], &[]);
        assert_eq!(country(&p), (Some("ES".into()), true));
        let p = party(&["Street 1", "1616 Luxembourg", "LU"], &[]);
        assert_eq!(country(&p), (Some("LU".into()), true));
        let p = party(&["Street 1", "10115 Berlin"], &["VAT: DE123456789"]);
        assert_eq!(country(&p), (Some("DE".into()), false));
        let mut p = party(&["Street 1", "Paris", "France"], &[]);
        p.country = Some("fr".into());
        assert_eq!(country(&p), (Some("FR".into()), true));
        assert_eq!(country(&party(&["Street 1"], &[])), (None, false));
    }

    #[test]
    fn address_splits_postcode_and_city() {
        let a = address(&party(
            &["Calle Mayor, 12", "Madrid", "28001", "Spain"],
            &[],
        ));
        assert_eq!(a.lines, vec!["Calle Mayor, 12"]);
        assert_eq!(a.city.as_deref(), Some("Madrid"));
        assert_eq!(a.postcode.as_deref(), Some("28001"));
        assert_eq!(a.country.as_deref(), Some("ES"));

        let a = address(&party(
            &["Hauptstr. 5", "Hinterhaus", "10115 Berlin", "Germany"],
            &[],
        ));
        assert_eq!(a.lines, vec!["Hauptstr. 5", "Hinterhaus"]);
        assert_eq!(a.city.as_deref(), Some("Berlin"));
        assert_eq!(a.postcode.as_deref(), Some("10115"));

        let a = address(&party(&["1 Main St", "L-1616 Luxembourg"], &[]));
        assert_eq!(a.postcode.as_deref(), Some("L-1616"));
        assert_eq!(a.city.as_deref(), Some("Luxembourg"));
        assert_eq!(a.country, None);

        // Nothing recognizable: all street lines.
        let a = address(&party(&["Somewhere", "Over the rainbow"], &[]));
        assert_eq!(a.lines.len(), 2);
        assert_eq!(a.city, None);
    }

    #[test]
    fn endpoint_explicit_vat_or_email() {
        let mut p = party(&[], &[]);
        assert_eq!(endpoint(&p, None), Ok(None));
        p.email = Some("a@example.com".into());
        assert_eq!(endpoint(&p, None), Ok(None));
        assert_eq!(email_endpoint(&p).unwrap().scheme, "EM");
        let de = endpoint(&p, Some("DE123456789")).unwrap().unwrap();
        assert_eq!(
            (de.scheme.as_str(), de.id.as_str()),
            ("9930", "DE123456789")
        );
        assert_eq!(endpoint(&p, Some("SE123456789001")), Ok(None));
        p.endpoint = Some("0208:0123456789".into());
        assert_eq!(
            endpoint(&p, Some("DE123456789")),
            Ok(Some(Endpoint {
                scheme: "0208".into(),
                id: "0123456789".into()
            }))
        );
        p.endpoint = Some("nonsense".into());
        assert!(endpoint(&p, None).is_err());
    }
}
