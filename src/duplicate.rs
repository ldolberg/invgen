//! Duplicate an invoice for recurring work (`invgen new --from`).
//!
//! The copy is made on the markdown text, so the items, notes, overrides and
//! the user's formatting all survive. Only the per-invoice front matter keys
//! are reset and the billing period in the body is moved forward.

use crate::config::Config;
use crate::frontmatter::{self, Place};
use crate::model::file_stem;
use crate::parse::parse_invoice;
use anyhow::{Context, Result, bail};
use chrono::{Datelike, NaiveDate};
use std::path::{Path, PathBuf};

const MONTHS: [&str; 12] = [
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
];

/// The markdown of a copy of `src` dated `today`, and its number.
///
/// * `number`: the next free number, as `invgen new` assigns it;
/// * `date`: `today`; `status`: `pending` (when written); `paid_on` and
///   `issued_hash`: removed;
/// * `due`: an explicit date is removed so it is computed again from
///   `due_days` (`due: false` and `due_days` are kept);
/// * the body: month names with a year (`October 2026`) and `YYYY-MM`
///   periods that refer to the source invoice's month or the month before it
///   are moved forward by the months between the source date and `today`
///   (at least one).
pub fn duplicate(src: &str, cfg: &Config, today: NaiveDate) -> Result<(String, String)> {
    let source_date = frontmatter::get(src, "date")?
        .and_then(|d| NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").ok())
        .unwrap_or(today);
    let body_at = frontmatter::body_start(src)?;
    let shift = month_index(today)
        .saturating_sub(month_index(source_date))
        .max(1);
    let body = advance_months(&src[body_at..], source_date, shift);
    let mut text = format!("{}{}", &src[..body_at], body);

    text = frontmatter::remove(&text, "paid_on")?;
    // The copy is a new invoice: not issued yet.
    text = frontmatter::remove(&text, crate::lock::KEY)?;
    if frontmatter::get(&text, "status")?.is_some() {
        text = frontmatter::set(&text, "status", "pending", Place::End)?;
    }
    if let Some(due) = frontmatter::get(&text, "due")?
        && !matches!(
            due.trim().to_ascii_lowercase().as_str(),
            "false" | "none" | "no"
        )
    {
        text = frontmatter::remove(&text, "due")?;
    }
    let date = today.format("%Y-%m-%d").to_string();
    text = frontmatter::set(&text, "date", &date, Place::After(&["number"]))?;

    // Number it the way the parser would auto-number it, so a `config:`
    // override of `number_prefix` in the source is honored.
    let unnumbered = frontmatter::remove(&text, "number")?;
    let number = parse_invoice(&unnumbered, cfg)
        .map(|inv| inv.number)
        .unwrap_or_else(|_| cfg.next_number(today.year()));
    text = frontmatter::set(&text, "number", &number, Place::Start)?;
    Ok((number, text))
}

/// Duplicate the invoice at `source` into `cfg.invoices_dir/<number>.md` and
/// return the new path. An existing file is never overwritten.
pub fn duplicate_file(cfg: &Config, source: &Path, today: NaiveDate) -> Result<PathBuf> {
    let src =
        std::fs::read_to_string(source).with_context(|| format!("reading {}", source.display()))?;
    let (number, text) =
        duplicate(&src, cfg, today).with_context(|| format!("duplicating {}", source.display()))?;
    std::fs::create_dir_all(&cfg.invoices_dir)
        .with_context(|| format!("creating {}", cfg.invoices_dir.display()))?;
    let path = cfg.invoices_dir.join(format!("{}.md", file_stem(&number)));
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

fn month_index(d: NaiveDate) -> u32 {
    (d.year().max(0) as u32) * 12 + d.month0()
}

/// `(year, month 1-12)` moved by `n` months (backwards when negative).
fn add_months(year: i32, month: u32, n: i64) -> (i32, u32) {
    let i = year as i64 * 12 + (month as i64 - 1) + n;
    (i.div_euclid(12) as i32, (i.rem_euclid(12) + 1) as u32)
}

/// Move period mentions in `text` forward by `shift` months. Only mentions of
/// `source`'s month or the month before it change: `October 2026`, `Oct 2026`,
/// `OCTOBER 2026` and `2026-10` (but not a full date such as `2026-10-01`).
pub fn advance_months(text: &str, source: NaiveDate, shift: u32) -> String {
    let current = (source.year(), source.month());
    let previous = add_months(source.year(), source.month(), -1);
    let targets = [current, previous];
    let bytes = text.as_bytes();
    let boundary_before = |i: usize| i == 0 || !bytes[i - 1].is_ascii_alphanumeric();

    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if !boundary_before(i) {
            i += 1;
            continue;
        }
        let found = if bytes[i].is_ascii_alphabetic() {
            month_name_at(text, i)
        } else if bytes[i].is_ascii_digit() && (i == 0 || !matches!(bytes[i - 1], b'-' | b'/')) {
            numeric_period_at(text, i)
        } else {
            None
        };
        match found {
            Some(m) if targets.contains(&(m.year, m.month)) => {
                let (y, mo) = add_months(m.year, m.month, shift.into());
                out.push_str(&text[copied..i]);
                out.push_str(&m.render(y, mo));
                i = m.end;
                copied = i;
            }
            Some(m) => i = m.end,
            None => i += 1,
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// A month mention found in the text.
struct Mention<'a> {
    year: i32,
    month: u32,
    /// Byte offset just past the mention.
    end: usize,
    style: Style<'a>,
}

enum Style<'a> {
    /// Month word in the original case pattern, then the separator.
    Name {
        word: &'a str,
        abbrev: bool,
        sep: &'a str,
    },
    /// `YYYY-MM`.
    Numeric,
}

impl Mention<'_> {
    fn render(&self, year: i32, month: u32) -> String {
        match &self.style {
            Style::Numeric => format!("{year:04}-{month:02}"),
            Style::Name { word, abbrev, sep } => {
                let full = MONTHS[month as usize - 1];
                let name = if *abbrev { &full[..3] } else { full };
                let name = if word.chars().all(|c| c.is_ascii_uppercase()) && word.len() > 1 {
                    name.to_ascii_uppercase()
                } else if word.chars().all(|c| c.is_ascii_lowercase()) {
                    name.to_ascii_lowercase()
                } else {
                    name.to_string()
                };
                format!("{name}{sep}{year}")
            }
        }
    }
}

/// `October 2026` / `Oct 2026` / `Oct. 2026` at byte `i` (a word start).
fn month_name_at(text: &str, i: usize) -> Option<Mention<'_>> {
    let rest = &text[i..];
    let word_len = rest.bytes().take_while(|b| b.is_ascii_alphabetic()).count();
    let word = &rest[..word_len];
    let (month, abbrev) = MONTHS.iter().enumerate().find_map(|(n, name)| {
        if word.eq_ignore_ascii_case(name) {
            Some((n as u32 + 1, false))
        } else if word.len() == 3 && word.eq_ignore_ascii_case(&name[..3]) {
            Some((n as u32 + 1, true))
        } else {
            None
        }
    })?;
    let mut j = word_len;
    if abbrev && rest[j..].starts_with('.') {
        j += 1;
    }
    let spaces = rest[j..].bytes().take_while(|b| *b == b' ').count();
    if spaces == 0 {
        return None;
    }
    let sep = &rest[word_len..j + spaces];
    j += spaces;
    let (year, len) = four_digits(&rest[j..])?;
    Some(Mention {
        year,
        month,
        end: i + j + len,
        style: Style::Name { word, abbrev, sep },
    })
}

/// `2026-10` at byte `i`, not part of a longer number or a full date.
fn numeric_period_at(text: &str, i: usize) -> Option<Mention<'static>> {
    let rest = &text[i..];
    let (year, _) = four_digits(rest)?;
    let b = rest.as_bytes();
    if b.get(4) != Some(&b'-') {
        return None;
    }
    let mm = rest.get(5..7)?;
    if !mm.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // `2026-10-01` is a date, `2026-100` a different thing.
    match b.get(7) {
        Some(c) if c.is_ascii_alphanumeric() => return None,
        Some(b'-' | b'/') if b.get(8).is_some_and(u8::is_ascii_digit) => return None,
        _ => {}
    }
    let month: u32 = mm.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    Some(Mention {
        year,
        month,
        end: i + 7,
        style: Style::Numeric,
    })
}

/// Exactly four leading digits not followed by another alphanumeric.
fn four_digits(s: &str) -> Option<(i32, usize)> {
    let b = s.as_bytes();
    if b.len() < 4 || !b[..4].iter().all(u8::is_ascii_digit) {
        return None;
    }
    if b.get(4).is_some_and(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((s[..4].parse().ok()?, 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn advances_month_names_and_numeric_periods() {
        let src = day(2026, 11, 2);
        let adv = |t: &str| advance_months(t, src, 1);
        assert_eq!(adv("October 2026 period"), "November 2026 period");
        assert_eq!(adv("November 2026"), "December 2026");
        assert_eq!(
            adv("Oct 2026, OCTOBER 2026, october 2026"),
            "Nov 2026, NOVEMBER 2026, november 2026"
        );
        assert_eq!(adv("Oct. 2026"), "Nov. 2026");
        assert_eq!(adv("Period 2026-10."), "Period 2026-11.");
        // Only the source month and the one before it.
        assert_eq!(
            adv("September 2026 and May 2025"),
            "September 2026 and May 2025"
        );
        // Full dates, other numbers and words are left alone.
        assert_eq!(adv("2026-10-01 to 2026-10-31"), "2026-10-01 to 2026-10-31");
        assert_eq!(
            adv("12026-10 2026-100 Octobers 2026"),
            "12026-10 2026-100 Octobers 2026"
        );
        assert_eq!(adv("October 20266 Mayday"), "October 20266 Mayday");
        assert_eq!(adv("EINV-2026-10"), "EINV-2026-10");
        // Multi-byte text around the mentions is kept.
        assert_eq!(adv("Período: October 2026 €"), "Período: November 2026 €");
    }

    #[test]
    fn advances_across_year_end() {
        let src = day(2027, 1, 3);
        assert_eq!(
            advance_months("December 2026 period, 2026-12, Jan 2027", src, 1),
            "January 2027 period, 2027-01, Feb 2027"
        );
        assert_eq!(
            advance_months("December 2026", day(2026, 12, 20), 1),
            "January 2027"
        );
        // A bigger gap moves the period by the same number of months.
        assert_eq!(
            advance_months("July 2026 period", day(2026, 8, 1), 3),
            "October 2026 period"
        );
    }

    fn cfg(root: &Path) -> Config {
        let mut cfg = Config {
            invoices_dir: root.to_path_buf(),
            number_prefix: "EINV".into(),
            ..Config::default()
        };
        cfg.clients.insert(
            "northwind".into(),
            serde_yaml_ng::from_str("name: Northwind Labs SL\nrate: 450").unwrap(),
        );
        cfg
    }

    const SOURCE: &str = "---\n\
number: EINV-2026-05   # leave out to auto-number\n\
date: 2026-11-02\n\
due: 2026-11-16        # explicit\n\
due_days: 30\n\
status: paid           # pending | paid\n\
paid_on: 2026-11-20\n\
client: northwind\n\
tax_rate: 21\n\
issued_hash: \"0123456789abcdef\"\n\
---\n\
\n\
# Senior Software Engineer\n\
October 2026 period\n\
\n\
| Description | Qty | Unit | Rate |\n\
|---|---|---|---|\n\
| Consulting  | 21  | days | 450  |\n\
\n\
Thanks! Invoice for 2026-10, sent 2026-11-02.\n";

    #[test]
    fn duplicate_resets_per_invoice_keys() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("EINV-2026-05.md"), SOURCE).unwrap();
        let cfg = cfg(tmp.path());
        let (number, text) = duplicate(SOURCE, &cfg, day(2026, 12, 1)).unwrap();
        assert_eq!(number, "EINV-2026-06");
        assert_eq!(
            text,
            "---\n\
number: EINV-2026-06   # leave out to auto-number\n\
date: 2026-12-01\n\
due_days: 30\n\
status: pending        # pending | paid\n\
client: northwind\n\
tax_rate: 21\n\
---\n\
\n\
# Senior Software Engineer\n\
November 2026 period\n\
\n\
| Description | Qty | Unit | Rate |\n\
|---|---|---|---|\n\
| Consulting  | 21  | days | 450  |\n\
\n\
Thanks! Invoice for 2026-11, sent 2026-11-02.\n"
        );
        let inv = parse_invoice(&text, &cfg).unwrap();
        assert_eq!(inv.due, Some(day(2026, 12, 31)));
        assert_eq!(inv.paid_on, None);
    }

    #[test]
    fn duplicate_keeps_hidden_due_and_adds_missing_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = cfg(tmp.path());
        let src = "---\nclient: northwind\ndue: false\n---\n- Support: 3 hours @ 80\n";
        let (number, text) = duplicate(src, &cfg, day(2026, 11, 2)).unwrap();
        assert_eq!(number, "EINV-2026-01");
        assert_eq!(
            text,
            "---\nnumber: EINV-2026-01\nclient: northwind\ndue: false\ndate: 2026-11-02\n---\n- Support: 3 hours @ 80\n"
        );
        // No front matter at all.
        let (_, text) = duplicate("- A: 1 @ 1\n", &cfg, day(2026, 11, 2)).unwrap();
        assert_eq!(
            text,
            "---\nnumber: EINV-2026-01\ndate: 2026-11-02\n---\n- A: 1 @ 1\n"
        );
    }

    #[test]
    fn duplicate_file_writes_the_next_number() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = cfg(tmp.path());
        let source = tmp.path().join("EINV-2026-05.md");
        std::fs::write(&source, SOURCE).unwrap();
        let out = duplicate_file(&cfg, &source, day(2026, 12, 1)).unwrap();
        assert_eq!(out, tmp.path().join("EINV-2026-06.md"));
        assert!(
            std::fs::read_to_string(&out)
                .unwrap()
                .contains("November 2026 period")
        );
        // The source is untouched.
        assert_eq!(std::fs::read_to_string(&source).unwrap(), SOURCE);
        let next = duplicate_file(&cfg, &out, day(2026, 12, 1)).unwrap();
        assert_eq!(next, tmp.path().join("EINV-2026-07.md"));
    }
}
