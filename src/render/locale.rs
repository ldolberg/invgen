//! Locale-aware formatting of amounts, quantities, percentages and dates on
//! the PDF.
//!
//! The locale is the invoice `language` unless `locale:` names another one
//! (optionally with a region, like `de-CH`); `number_format:` and
//! `date_format:` override single parts of it. Month and weekday names come
//! from the tables below, never from the system locale. The CLI keeps the
//! fixed English format of `model::format_money`.

use std::fmt::Write as _;

use chrono::{Datelike, NaiveDate};

use super::{data::format_qty, i18n};
use crate::model::{
    CurrencyPosition, CurrencyStyle, Decimal, FormatOptions, Invoice, NumberFormat, round_cents,
};

const NBSP: &str = "\u{a0}";
/// Narrow no-break space (French thousands separator).
const NNBSP: &str = "\u{202f}";

/// Number and date conventions of one locale.
#[derive(Debug, Clone, Copy)]
struct Conventions {
    thousands: &'static str,
    decimal: &'static str,
    currency: CurrencyStyle,
    position: CurrencyPosition,
    /// Between the currency and the amount.
    space: &'static str,
    /// A currency symbol written before the amount touches it (`$9.00`),
    /// unless it ends in a letter (`CHF 9.00`).
    tight: bool,
    /// Between a number and `%`.
    percent: &'static str,
    /// Default date pattern (see `expand_names` for the localized specifiers).
    date: &'static str,
}

/// The conventions for a supported language and an optional region (upper
/// case). Unknown regions use the language's own conventions.
fn conventions(lang: &str, region: Option<&str>) -> Conventions {
    use CurrencyPosition::*;
    use CurrencyStyle::*;
    // Continental default: `9.450,00 €`.
    let base = Conventions {
        thousands: ".",
        decimal: ",",
        currency: Symbol,
        position: After,
        space: NBSP,
        tight: false,
        percent: "",
        date: "%-d %B %Y",
    };
    let swiss = Conventions {
        thousands: "\u{2019}",
        decimal: ".",
        position: Before,
        ..base
    };
    match lang {
        "en" => {
            // Plain `en` is the historical format: `EUR 9,450.00`,
            // `November 2, 2026`. Regions switch to symbols.
            let en = Conventions {
                thousands: ",",
                decimal: ".",
                currency: Code,
                position: Before,
                space: " ",
                tight: true,
                percent: "",
                date: "%B %-d, %Y",
            };
            let regional = Conventions {
                currency: Symbol,
                space: NBSP,
                ..en
            };
            match region {
                Some("US" | "CA" | "PH") => regional,
                Some("GB" | "IE" | "AU" | "NZ" | "MT") => Conventions {
                    date: "%-d %B %Y",
                    ..regional
                },
                _ => en,
            }
        }
        "de" => {
            let de = Conventions {
                percent: NBSP,
                date: "%-d. %B %Y",
                ..base
            };
            match region {
                Some("AT") => Conventions {
                    thousands: NBSP,
                    position: Before,
                    ..de
                },
                Some("CH" | "LI") => Conventions {
                    percent: "",
                    date: "%-d. %B %Y",
                    ..swiss
                },
                _ => de,
            }
        }
        "es" => {
            let es = Conventions {
                percent: NBSP,
                date: "%-d de %B de %Y",
                ..base
            };
            match region {
                Some("MX" | "US" | "PR" | "GT" | "HN" | "NI" | "PA" | "SV" | "DO") => Conventions {
                    thousands: ",",
                    decimal: ".",
                    position: Before,
                    tight: true,
                    percent: "",
                    ..es
                },
                _ => es,
            }
        }
        "fr" => {
            let fr = Conventions {
                thousands: NNBSP,
                percent: NNBSP,
                ..base
            };
            match region {
                Some("CA") => Conventions {
                    thousands: NBSP,
                    percent: NBSP,
                    ..fr
                },
                _ => fr,
            }
        }
        "it" => match region {
            Some("CH") => swiss,
            _ => base,
        },
        "pt" => {
            let pt = Conventions {
                thousands: NBSP,
                date: "%-d de %B de %Y",
                ..base
            };
            match region {
                Some("BR") => Conventions {
                    thousands: ".",
                    position: Before,
                    ..pt
                },
                _ => pt,
            }
        }
        "nl" => match region {
            Some("BE") => base,
            _ => Conventions {
                position: Before,
                ..base
            },
        },
        _ => conventions("en", None),
    }
}

/// Month and weekday names of one language.
struct Names {
    months: [&'static str; 12],
    months_abbr: [&'static str; 12],
    /// Monday first.
    weekdays: [&'static str; 7],
    weekdays_abbr: [&'static str; 7],
}

const EN: Names = Names {
    months: [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    months_abbr: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    weekdays: [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ],
    weekdays_abbr: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
};

const DE: Names = Names {
    months: [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ],
    months_abbr: [
        "Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.",
        "Dez.",
    ],
    weekdays: [
        "Montag",
        "Dienstag",
        "Mittwoch",
        "Donnerstag",
        "Freitag",
        "Samstag",
        "Sonntag",
    ],
    weekdays_abbr: ["Mo.", "Di.", "Mi.", "Do.", "Fr.", "Sa.", "So."],
};

/// Austrian German: `Jänner` instead of `Januar`.
const DE_AT: Names = Names {
    months: [
        "Jänner",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ],
    months_abbr: [
        "Jän.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sep.", "Okt.", "Nov.",
        "Dez.",
    ],
    ..DE
};

const ES: Names = Names {
    months: [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ],
    months_abbr: [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic",
    ],
    weekdays: [
        "lunes",
        "martes",
        "miércoles",
        "jueves",
        "viernes",
        "sábado",
        "domingo",
    ],
    weekdays_abbr: ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"],
};

const FR: Names = Names {
    months: [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ],
    months_abbr: [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ],
    weekdays: [
        "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
    ],
    weekdays_abbr: ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."],
};

const IT: Names = Names {
    months: [
        "gennaio",
        "febbraio",
        "marzo",
        "aprile",
        "maggio",
        "giugno",
        "luglio",
        "agosto",
        "settembre",
        "ottobre",
        "novembre",
        "dicembre",
    ],
    months_abbr: [
        "gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic",
    ],
    weekdays: [
        "lunedì",
        "martedì",
        "mercoledì",
        "giovedì",
        "venerdì",
        "sabato",
        "domenica",
    ],
    weekdays_abbr: ["lun", "mar", "mer", "gio", "ven", "sab", "dom"],
};

const PT: Names = Names {
    months: [
        "janeiro",
        "fevereiro",
        "março",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ],
    months_abbr: [
        "jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.", "nov.",
        "dez.",
    ],
    weekdays: [
        "segunda-feira",
        "terça-feira",
        "quarta-feira",
        "quinta-feira",
        "sexta-feira",
        "sábado",
        "domingo",
    ],
    weekdays_abbr: ["seg.", "ter.", "qua.", "qui.", "sex.", "sáb.", "dom."],
};

const NL: Names = Names {
    months: [
        "januari",
        "februari",
        "maart",
        "april",
        "mei",
        "juni",
        "juli",
        "augustus",
        "september",
        "oktober",
        "november",
        "december",
    ],
    months_abbr: [
        "jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec",
    ],
    weekdays: [
        "maandag",
        "dinsdag",
        "woensdag",
        "donderdag",
        "vrijdag",
        "zaterdag",
        "zondag",
    ],
    weekdays_abbr: ["ma", "di", "wo", "do", "vr", "za", "zo"],
};

fn names(lang: &str, region: Option<&str>) -> &'static Names {
    match (lang, region) {
        ("de", Some("AT")) => &DE_AT,
        ("de", _) => &DE,
        ("es", _) => &ES,
        ("fr", _) => &FR,
        ("it", _) => &IT,
        ("pt", _) => &PT,
        ("nl", _) => &NL,
        _ => &EN,
    }
}

/// Replace the name specifiers of a strftime pattern (`%B`, `%b`, `%h`,
/// `%A`, `%a`) with the localized names for `d`; everything else is left for
/// chrono. `%%` stays a literal percent sign.
fn expand_names(pattern: &str, d: NaiveDate, names: &Names) -> String {
    let month = d.month0() as usize;
    let weekday = d.weekday().num_days_from_monday() as usize;
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('B') => out.push_str(names.months[month]),
            Some('b' | 'h') => out.push_str(names.months_abbr[month]),
            Some('A') => out.push_str(names.weekdays[weekday]),
            Some('a') => out.push_str(names.weekdays_abbr[weekday]),
            Some(k) => {
                out.push('%');
                out.push(k);
            }
            None => out.push('%'),
        }
    }
    out
}

/// Format `d` with a strftime pattern and localized names, or `None` when
/// the pattern is invalid for a date (unknown specifier, time fields).
fn format_date_with(d: NaiveDate, pattern: &str, names: &Names) -> Option<String> {
    let expanded = expand_names(pattern, d, names);
    let mut out = String::new();
    write!(out, "{}", d.format(&expanded)).ok()?;
    Some(out)
}

/// The symbol for an ISO currency code, or the code itself when it has none
/// (`CHF`, unknown codes). `region` picks the local dollar (`$` for CAD in
/// `en-CA`) and marks foreign dollars (`US$` there).
pub fn currency_symbol(code: &str, region: Option<&str>) -> String {
    let local_dollar = match region {
        Some("US") => Some("USD"),
        Some("CA") => Some("CAD"),
        Some("AU") => Some("AUD"),
        Some("NZ") => Some("NZD"),
        Some("MX") => Some("MXN"),
        _ => None,
    };
    if local_dollar == Some(code) {
        return "$".into();
    }
    let sym = match code {
        "EUR" => "€",
        "USD" if local_dollar.is_some() => "US$",
        "USD" => "$",
        "GBP" => "£",
        "JPY" => "¥",
        "CNY" => "CN¥",
        "INR" => "₹",
        "KRW" => "₩",
        "BRL" => "R$",
        "CAD" => "CA$",
        "AUD" => "A$",
        "NZD" => "NZ$",
        "MXN" => "MX$",
        "HKD" => "HK$",
        "ILS" => "₪",
        "TRY" => "₺",
        "RUB" => "₽",
        "UAH" => "₴",
        "PLN" => "zł",
        "CZK" => "Kč",
        "HUF" => "Ft",
        "SEK" | "NOK" | "DKK" | "ISK" => "kr",
        "VND" => "₫",
        "PHP" => "₱",
        "NGN" => "₦",
        "THB" => "฿",
        other => other,
    };
    sym.to_string()
}

/// Split and validate a locale like `de-CH`, `de_ch` or `fr`: a supported
/// language, optionally followed by a two-letter or three-digit region.
fn parse_locale(s: &str) -> Result<(&'static str, Option<String>), String> {
    let t = s.trim();
    let mut parts = t.split(['-', '_']);
    let lang = parts.next().unwrap_or("");
    let lang = i18n::normalize_language(lang).map_err(|_| {
        format!(
            "unknown locale `{t}` (use a language code like `de` or `de-CH`; languages: {})",
            i18n::SUPPORTED.join(", ")
        )
    })?;
    let region = match (parts.next(), parts.next()) {
        (None, _) => None,
        (Some(r), None)
            if (r.len() == 2 && r.chars().all(|c| c.is_ascii_alphabetic()))
                || (r.len() == 3 && r.chars().all(|c| c.is_ascii_digit())) =>
        {
            Some(r.to_ascii_uppercase())
        }
        _ => {
            return Err(format!(
                "invalid locale `{t}` (use a language code like `de` or `de-CH`)"
            ));
        }
    };
    Ok((lang, region))
}

/// Normalize a user-written locale (`DE_ch` -> `de-CH`), or explain the problem.
pub fn normalize_locale(s: &str) -> Result<String, String> {
    let (lang, region) = parse_locale(s)?;
    Ok(match region {
        Some(r) => format!("{lang}-{r}"),
        None => lang.to_string(),
    })
}

/// Validate `number_format:`: a non-empty decimal separator, separators
/// without digits or signs, and different thousands and decimal separators.
pub fn check_number_format(nf: &NumberFormat) -> Result<(), String> {
    let bad = |s: &str| {
        s.chars()
            .any(|c| c.is_ascii_digit() || c == '-' || c == '+')
    };
    if let Some(d) = &nf.decimal
        && (d.is_empty() || bad(d) || d.chars().count() > 3)
    {
        return Err(format!(
            "`number_format.decimal` must be a separator like \",\" or \".\", got {d:?}"
        ));
    }
    if let Some(t) = &nf.thousands {
        if bad(t) || t.chars().count() > 3 {
            return Err(format!(
                "`number_format.thousands` must be a separator like \".\", \",\", \"'\" or \"\", got {t:?}"
            ));
        }
        if !t.is_empty() && nf.decimal.as_deref() == Some(t.as_str()) {
            return Err("`number_format.thousands` and `decimal` must differ".into());
        }
    }
    Ok(())
}

/// Validate `date_format:` by formatting a sample date with it.
pub fn check_date_format(pattern: &str) -> Result<(), String> {
    let sample = NaiveDate::from_ymd_opt(2026, 11, 2).expect("valid date");
    format_date_with(sample, pattern, &EN).map(|_| ()).ok_or_else(|| {
        format!(
            "invalid `date_format` {pattern:?} (use date specifiers like %d, %-d, %m, %B, %b, %Y; \
             see docs/FORMAT.md)"
        )
    })
}

/// Validate a whole set of format options (config, client or invoice).
pub fn check(opts: &FormatOptions) -> Result<(), String> {
    if let Some(l) = &opts.locale {
        parse_locale(l).map_err(|e| format!("`locale`: {e}"))?;
    }
    check_number_format(&opts.number_format)?;
    if let Some(p) = &opts.date_format {
        check_date_format(p)?;
    }
    Ok(())
}

/// Formats numbers and dates for one invoice. Built from the invoice's
/// `language` and `format` options; invalid options (only possible when an
/// `Invoice` is built by hand) fall back to the language defaults.
#[derive(Debug, Clone)]
pub struct Formatter {
    locale: String,
    lang: &'static str,
    region: Option<String>,
    thousands: String,
    decimal: String,
    currency: CurrencyStyle,
    position: CurrencyPosition,
    space: &'static str,
    tight: bool,
    percent: &'static str,
    date: String,
    default_date: &'static str,
}

impl Formatter {
    pub fn new(inv: &Invoice) -> Self {
        let language = i18n::normalize_language(&inv.language).unwrap_or("en");
        Self::with_options(language, &inv.format)
    }

    /// `language` is a supported language code; `opts` may name another locale.
    pub fn with_options(language: &str, opts: &FormatOptions) -> Self {
        let (lang, region) = opts
            .locale
            .as_deref()
            .and_then(|l| parse_locale(l).ok())
            .unwrap_or_else(|| {
                let lang = i18n::normalize_language(language).unwrap_or("en");
                (lang, None)
            });
        let c = conventions(lang, region.as_deref());
        let nf = if check_number_format(&opts.number_format).is_ok() {
            opts.number_format.clone()
        } else {
            NumberFormat::default()
        };
        let date = opts
            .date_format
            .as_deref()
            .filter(|p| !p.trim().is_empty() && check_date_format(p).is_ok())
            .unwrap_or(c.date);
        Formatter {
            locale: match &region {
                Some(r) => format!("{lang}-{r}"),
                None => lang.to_string(),
            },
            lang,
            thousands: nf.thousands.unwrap_or_else(|| c.thousands.into()),
            decimal: nf.decimal.unwrap_or_else(|| c.decimal.into()),
            currency: nf.currency.unwrap_or(c.currency),
            position: nf.currency_position.unwrap_or(c.position),
            region,
            space: c.space,
            tight: c.tight,
            percent: c.percent,
            date: date.to_string(),
            default_date: c.date,
        }
    }

    /// The resolved locale, e.g. `en` or `de-CH`.
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// `9450` -> `9,450.00` / `9.450,00` / `9 450,00`. Always two decimals,
    /// rounded like the totals (half away from zero), no `-0,00`.
    pub fn amount(&self, x: Decimal) -> String {
        let r = round_cents(x);
        let neg = r.is_sign_negative() && !r.is_zero();
        let digits = format!("{:.2}", r.abs());
        let (int, frac) = digits.split_once('.').unwrap_or((&digits, "00"));
        let mut out = String::with_capacity(digits.len() + 8);
        if neg {
            out.push('-');
        }
        for (i, c) in int.chars().enumerate() {
            if i > 0 && (int.len() - i).is_multiple_of(3) {
                out.push_str(&self.thousands);
            }
            out.push(c);
        }
        out.push_str(&self.decimal);
        out.push_str(frac);
        out
    }

    /// An amount with its currency: `EUR 9,450.00`, `9.450,00 €`, `$9,450.00`.
    pub fn money(&self, currency: &str, x: Decimal) -> String {
        let code = currency.trim();
        let amount = self.amount(x);
        let cur = match self.currency {
            CurrencyStyle::Code => code.to_string(),
            CurrencyStyle::Symbol => currency_symbol(code, self.region.as_deref()),
        };
        match self.position {
            CurrencyPosition::After => format!("{amount}{}{cur}", self.space),
            CurrencyPosition::Before => {
                let touches = self.tight
                    && self.currency == CurrencyStyle::Symbol
                    && !cur.is_empty()
                    && !cur.ends_with(char::is_alphabetic);
                match (touches, amount.strip_prefix('-')) {
                    (true, Some(abs)) => format!("-{cur}{abs}"),
                    (true, None) => format!("{cur}{amount}"),
                    (false, _) => format!("{cur}{}{amount}", self.space),
                }
            }
        }
    }

    /// A quantity: at most two decimals, no trailing zeros, no grouping
    /// (`7.5` -> `7,5` with a decimal comma).
    pub fn qty(&self, q: Decimal) -> String {
        format_qty(q).replace('.', &self.decimal)
    }

    /// A percentage: `21%`, `7,5 %`.
    pub fn percent(&self, rate: Decimal) -> String {
        format!("{}{}%", self.qty(rate), self.percent)
    }

    /// A date in the locale's (or `date_format`'s) pattern with localized names.
    pub fn date(&self, d: NaiveDate) -> String {
        let names = names(self.lang, self.region.as_deref());
        format_date_with(d, &self.date, names)
            .or_else(|| format_date_with(d, self.default_date, names))
            .unwrap_or_else(|| d.format("%Y-%m-%d").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{format_amount, format_date, format_money};
    use proptest::prelude::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()
    }

    fn lang(l: &str) -> Formatter {
        Formatter::with_options(l, &FormatOptions::default())
    }

    fn locale(l: &str) -> Formatter {
        let opts = FormatOptions {
            locale: Some(l.into()),
            ..Default::default()
        };
        Formatter::with_options("en", &opts)
    }

    #[test]
    fn english_is_the_historical_format() {
        let f = lang("en");
        assert_eq!(f.locale(), "en");
        assert_eq!(f.money("EUR", d("9450")), "EUR 9,450.00");
        assert_eq!(f.money("CHF", d("-1234.5")), "CHF -1,234.50");
        assert_eq!(f.amount(d("450")), "450.00");
        assert_eq!(f.date(day()), "November 2, 2026");
        assert_eq!(f.qty(d("7.5")), "7.5");
        assert_eq!(f.percent(d("21")), "21%");
        assert_eq!(f.percent(d("7.7")), "7.7%");
    }

    proptest! {
        #[test]
        fn english_matches_cli_format(
            m in -9_999_999_999_999_i64..=9_999_999_999_999, scale in 0u32..=4,
            cur in "[A-Z€$ ]{0,4}",
            days in 0i64..80_000,
        ) {
            let x = Decimal::new(m, scale);
            let f = lang("en");
            prop_assert_eq!(f.amount(x), format_amount(x));
            prop_assert_eq!(f.money(&cur, x), format_money(cur.trim(), x));
            let date = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap() + chrono::Duration::days(days);
            prop_assert_eq!(f.date(date), format_date(date));
        }
    }

    #[test]
    fn each_language() {
        let cases = [
            (
                "de",
                "9.450,00\u{a0}€",
                "2. November 2026",
                "7,5",
                "21\u{a0}%",
            ),
            (
                "es",
                "9.450,00\u{a0}€",
                "2 de noviembre de 2026",
                "7,5",
                "21\u{a0}%",
            ),
            (
                "fr",
                "9\u{202f}450,00\u{a0}€",
                "2 novembre 2026",
                "7,5",
                "21\u{202f}%",
            ),
            ("it", "9.450,00\u{a0}€", "2 novembre 2026", "7,5", "21%"),
            (
                "pt",
                "9\u{a0}450,00\u{a0}€",
                "2 de novembro de 2026",
                "7,5",
                "21%",
            ),
            ("nl", "€\u{a0}9.450,00", "2 november 2026", "7,5", "21%"),
        ];
        for (l, money, date, qty, pct) in cases {
            let f = lang(l);
            assert_eq!(f.money("EUR", d("9450")), money, "{l}");
            assert_eq!(f.date(day()), date, "{l}");
            assert_eq!(f.qty(d("7.50")), qty, "{l}");
            assert_eq!(f.percent(d("21")), pct, "{l}");
        }
    }

    #[test]
    fn big_and_negative_amounts() {
        assert_eq!(lang("de").amount(d("1234567.891")), "1.234.567,89");
        assert_eq!(lang("de").money("EUR", d("-12.5")), "-12,50\u{a0}€");
        assert_eq!(lang("de").amount(d("-0.001")), "0,00");
        assert_eq!(lang("nl").money("EUR", d("-5")), "€\u{a0}-5,00");
        assert_eq!(locale("en-US").money("USD", d("-5")), "-$5.00");
        assert_eq!(lang("fr").amount(d("999.995")), "1\u{202f}000,00");
    }

    #[test]
    fn regional_locales() {
        let f = locale("de-CH");
        assert_eq!(f.locale(), "de-CH");
        assert_eq!(f.money("CHF", d("9450")), "CHF\u{a0}9\u{2019}450.00");
        assert_eq!(f.money("EUR", d("9450")), "€\u{a0}9\u{2019}450.00");
        assert_eq!(f.date(day()), "2. November 2026");

        let f = locale("de_at");
        assert_eq!(f.locale(), "de-AT");
        assert_eq!(f.money("EUR", d("9450")), "€\u{a0}9\u{a0}450,00");
        assert_eq!(
            f.date(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()),
            "5. Jänner 2026"
        );

        let f = locale("en-US");
        assert_eq!(f.money("USD", d("9450")), "$9,450.00");
        assert_eq!(f.money("EUR", d("9450")), "€9,450.00");
        assert_eq!(f.money("CHF", d("9450")), "CHF\u{a0}9,450.00");
        assert_eq!(f.date(day()), "November 2, 2026");

        let f = locale("en-GB");
        assert_eq!(f.money("GBP", d("9450")), "£9,450.00");
        assert_eq!(f.date(day()), "2 November 2026");

        assert_eq!(locale("en-CA").money("CAD", d("1")), "$1.00");
        assert_eq!(locale("en-CA").money("USD", d("1")), "US$1.00");
        assert_eq!(locale("pt-BR").money("BRL", d("9450")), "R$\u{a0}9.450,00");
        assert_eq!(locale("es-MX").money("MXN", d("9450")), "$9,450.00");
        assert_eq!(
            locale("fr-CA").money("CAD", d("9450")),
            "9\u{a0}450,00\u{a0}$"
        );
        assert_eq!(locale("nl-BE").money("EUR", d("9450")), "9.450,00\u{a0}€");
        assert_eq!(
            locale("it-CH").money("CHF", d("9450")),
            "CHF\u{a0}9\u{2019}450.00"
        );
        // Unknown regions use the language's own conventions.
        assert_eq!(locale("de-XY").money("EUR", d("1")), "1,00\u{a0}€");
    }

    #[test]
    fn locale_overrides_language() {
        let opts = FormatOptions {
            locale: Some("de".into()),
            ..Default::default()
        };
        let f = Formatter::with_options("fr", &opts);
        assert_eq!(f.date(day()), "2. November 2026");
        // An invalid locale (hand-built invoice) falls back to the language.
        let opts = FormatOptions {
            locale: Some("xx".into()),
            ..Default::default()
        };
        assert_eq!(Formatter::with_options("es", &opts).locale(), "es");
    }

    #[test]
    fn number_format_overrides_single_fields() {
        let opts = FormatOptions {
            locale: Some("de".into()),
            number_format: NumberFormat {
                thousands: Some("'".into()),
                decimal: Some(".".into()),
                currency: Some(CurrencyStyle::Code),
                currency_position: Some(CurrencyPosition::Before),
            },
            ..Default::default()
        };
        let f = Formatter::with_options("en", &opts);
        assert_eq!(f.money("CHF", d("9450")), "CHF\u{a0}9'450.00");
        assert_eq!(f.qty(d("1.5")), "1.5");

        let opts = FormatOptions {
            number_format: NumberFormat {
                thousands: Some(String::new()),
                currency_position: Some(CurrencyPosition::After),
                ..Default::default()
            },
            ..Default::default()
        };
        let f = Formatter::with_options("en", &opts);
        assert_eq!(f.money("EUR", d("9450")), "9450.00 EUR");

        let opts = FormatOptions {
            number_format: NumberFormat {
                currency: Some(CurrencyStyle::Symbol),
                ..Default::default()
            },
            ..Default::default()
        };
        let f = Formatter::with_options("en", &opts);
        assert_eq!(f.money("EUR", d("9450")), "€9,450.00");
        assert_eq!(f.money("CHF", d("9450")), "CHF 9,450.00");
        assert_eq!(f.money("XYZ", d("1")), "XYZ 1.00");
    }

    #[test]
    fn date_format_override() {
        let with = |l: &str, p: &str| {
            let opts = FormatOptions {
                date_format: Some(p.into()),
                ..Default::default()
            };
            Formatter::with_options(l, &opts).date(day())
        };
        assert_eq!(with("en", "%d.%m.%Y"), "02.11.2026");
        assert_eq!(with("en", "%Y-%m-%d"), "2026-11-02");
        assert_eq!(with("de", "%A, %-d. %B %Y"), "Montag, 2. November 2026");
        assert_eq!(with("fr", "%a %-d %b %Y"), "lun. 2 nov. 2026");
        assert_eq!(with("es", "%-d %b %Y"), "2 nov 2026");
        assert_eq!(with("pt", "%A"), "segunda-feira");
        assert_eq!(with("en", "100%% %B"), "100% November");
        // Invalid patterns fall back to the locale default instead of failing.
        assert_eq!(with("de", "%H:%M"), "2. November 2026");
        assert_eq!(with("de", "  "), "2. November 2026");
    }

    #[test]
    fn name_tables_are_complete() {
        for l in i18n::SUPPORTED {
            let n = names(l, None);
            for s in n.months.iter().chain(&n.months_abbr) {
                assert!(!s.trim().is_empty() && !s.contains('%'), "{l}");
            }
            for s in n.weekdays.iter().chain(&n.weekdays_abbr) {
                assert!(!s.trim().is_empty() && !s.contains('%'), "{l}");
            }
        }
    }

    #[test]
    fn validation() {
        assert_eq!(normalize_locale(" DE_ch "), Ok("de-CH".into()));
        assert_eq!(normalize_locale("es-419"), Ok("es-419".into()));
        assert_eq!(normalize_locale("fr"), Ok("fr".into()));
        let err = normalize_locale("xx-YY").unwrap_err();
        assert!(err.contains("`xx-YY`") && err.contains("de-CH"), "{err}");
        assert!(normalize_locale("de-CHE").is_err());
        assert!(normalize_locale("zh-Hant-TW").is_err());

        assert!(check_date_format("%d.%m.%Y").is_ok());
        assert!(check_date_format("%A %-d %B").is_ok());
        assert!(check_date_format("%H:%M").is_err());
        assert!(check_date_format("%Q").is_err());
        assert!(check_date_format("%").is_err());

        let nf = |t: Option<&str>, dec: Option<&str>| NumberFormat {
            thousands: t.map(Into::into),
            decimal: dec.map(Into::into),
            ..Default::default()
        };
        assert!(check_number_format(&nf(Some("'"), Some("."))).is_ok());
        assert!(check_number_format(&nf(Some(""), None)).is_ok());
        assert!(check_number_format(&nf(None, Some(""))).is_err());
        assert!(check_number_format(&nf(Some("."), Some("."))).is_err());
        assert!(check_number_format(&nf(Some("1"), None)).is_err());
        assert!(check_number_format(&nf(None, Some("-"))).is_err());

        let opts = FormatOptions {
            locale: Some("klingon".into()),
            ..Default::default()
        };
        assert!(check(&opts).unwrap_err().contains("`locale`"));
    }

    #[test]
    fn currency_symbols() {
        assert_eq!(currency_symbol("EUR", None), "€");
        assert_eq!(currency_symbol("USD", None), "$");
        assert_eq!(currency_symbol("GBP", None), "£");
        assert_eq!(currency_symbol("CHF", None), "CHF");
        assert_eq!(currency_symbol("SEK", None), "kr");
        assert_eq!(currency_symbol("ABC", None), "ABC");
        assert_eq!(currency_symbol("USD", Some("MX")), "US$");
        assert_eq!(currency_symbol("MXN", Some("MX")), "$");
    }
}
