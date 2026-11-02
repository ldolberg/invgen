//! Reporting over the whole invoice library: loading every invoice once,
//! filtering, yearly totals, outstanding balances with aging buckets,
//! CSV/JSON export and integrity checks. Used by `invgen list` and
//! `invgen check`; the CLI only parses arguments and prints.

use crate::config::Config;
use crate::library;
use crate::model::{Decimal, Invoice, Status, round_cents};
use crate::parse::parse_invoice;
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Loading

/// One parsed invoice of the library and where it came from.
#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    /// The `client:` key of the front matter, if any.
    pub client_key: Option<String>,
    /// 1-based line of the `number:` key, `None` when the number is not written
    /// in the file (then it was auto-assigned while parsing).
    pub number_line: Option<usize>,
    /// 1-based line of the `date:` key.
    pub date_line: Option<usize>,
    /// 1-based line of the `client:` key, else of `to:`.
    pub client_line: Option<usize>,
    /// 1-based line of the `from:` key, else of `config:` (which may override it).
    pub from_line: Option<usize>,
    pub invoice: Invoice,
}

/// A library file that could not be read or parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadError {
    pub path: PathBuf,
    /// 1-based line, 0 when unknown.
    pub line: usize,
    pub message: String,
}

/// Every invoice of the library, sorted in issue order (global sequence
/// number, then date, then number), plus the files that failed.
#[derive(Debug, Clone, Default)]
pub struct Library {
    pub entries: Vec<Entry>,
    pub errors: Vec<LoadError>,
}

impl Library {
    /// Discover and parse every invoice under `cfg.invoices_dir`.
    pub fn load(cfg: &Config) -> Library {
        let mut lib = Library::default();
        for path in library::discover(&cfg.invoices_dir) {
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    lib.errors.push(LoadError {
                        path,
                        line: 0,
                        message: format!("cannot read: {e}"),
                    });
                    continue;
                }
            };
            match entry_from_source(path.clone(), &src, cfg) {
                Ok(entry) => lib.entries.push(entry),
                Err(e) => lib.errors.push(LoadError {
                    path,
                    line: e.line,
                    message: e.message,
                }),
            }
        }
        sort_entries(&mut lib.entries);
        lib
    }
}

/// Parse one invoice source into an [`Entry`].
pub fn entry_from_source(
    path: PathBuf,
    src: &str,
    cfg: &Config,
) -> Result<Entry, crate::parse::ParseError> {
    let invoice = parse_invoice(src, cfg)?;
    let fm = FrontMatterLines::new(src);
    Ok(Entry {
        path,
        client_key: fm.scalar("client"),
        number_line: fm.line_of("number"),
        date_line: fm.line_of("date"),
        client_line: fm.line_of("client").or_else(|| fm.line_of("to")),
        from_line: fm.line_of("from").or_else(|| fm.line_of("config")),
        invoice,
    })
}

/// Issue order: sequences are global and monotonic, so they give the order.
pub fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        let ka = (seq_of(&a.invoice.number), a.invoice.date, &a.invoice.number);
        let kb = (seq_of(&b.invoice.number), b.invoice.date, &b.invoice.number);
        ka.cmp(&kb)
    });
}

/// Trailing sequence number of an invoice number, 0 when there is none.
fn seq_of(number: &str) -> u64 {
    number
        .rsplit('-')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Top-level keys of the front matter, for line numbers and the raw
/// `client:` key (the parsed [`Invoice`] only keeps the resolved party).
struct FrontMatterLines<'a> {
    /// (1-based line, text) of each front matter line.
    lines: Vec<(usize, &'a str)>,
}

impl<'a> FrontMatterLines<'a> {
    fn new(src: &'a str) -> Self {
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        let mut it = src.lines().enumerate();
        let mut lines = Vec::new();
        if it.next().is_some_and(|(_, l)| l.trim_end() == "---") {
            for (i, l) in it {
                if l.trim_end() == "---" {
                    break;
                }
                lines.push((i + 1, l));
            }
        }
        FrontMatterLines { lines }
    }

    fn find(&self, key: &str) -> Option<(usize, &'a str)> {
        self.lines.iter().find_map(|&(n, l)| {
            let rest = l.strip_prefix(key)?.trim_start();
            rest.strip_prefix(':').map(|v| (n, v))
        })
    }

    fn line_of(&self, key: &str) -> Option<usize> {
        self.find(key).map(|(n, _)| n)
    }

    /// The value of a single-line `key: value`, without a trailing comment
    /// or surrounding quotes. `None` when absent or empty.
    fn scalar(&self, key: &str) -> Option<String> {
        let (_, raw) = self.find(key)?;
        let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(raw.trim()).ok()?;
        match v {
            serde_yaml_ng::Value::String(s) => Some(s),
            serde_yaml_ng::Value::Number(n) => Some(n.to_string()),
            serde_yaml_ng::Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    }
}

// ---------------------------------------------------------------------------
// Filters

/// `invgen list` filters. All set filters must match (AND).
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Year of the invoice date.
    pub year: Option<i32>,
    /// Client config key (exact, case-insensitive) or a case-insensitive
    /// substring of the client name.
    pub client: Option<String>,
    /// Any of these effective statuses. Empty means any.
    pub statuses: Vec<Status>,
    /// Only unpaid invoices: effective status `pending` or `overdue`.
    pub unpaid: bool,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.year.is_none() && self.client.is_none() && self.statuses.is_empty() && !self.unpaid
    }

    pub fn matches(&self, e: &Entry, today: NaiveDate) -> bool {
        let inv = &e.invoice;
        let status = inv.effective_status(today);
        if self.year.is_some_and(|y| inv.date.year() != y) {
            return false;
        }
        if let Some(needle) = self.client.as_deref().map(str::trim) {
            let by_key = e
                .client_key
                .as_deref()
                .is_some_and(|k| k.eq_ignore_ascii_case(needle));
            let by_name = inv.to.name.to_lowercase().contains(&needle.to_lowercase());
            if !by_key && !by_name {
                return false;
            }
        }
        if !self.statuses.is_empty() && !self.statuses.contains(&status) {
            return false;
        }
        if self.unpaid && !is_unpaid(status) {
            return false;
        }
        true
    }

    /// The entries of `entries` that match, in the same order.
    pub fn apply<'a>(&self, entries: &'a [Entry], today: NaiveDate) -> Vec<&'a Entry> {
        entries.iter().filter(|e| self.matches(e, today)).collect()
    }
}

fn is_unpaid(s: Status) -> bool {
    matches!(s, Status::Pending | Status::Overdue)
}

/// Parse a status name (case-insensitive), for `--status`.
pub fn parse_status(s: &str) -> Result<Status, String> {
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

// ---------------------------------------------------------------------------
// Totals and aging

/// year -> currency -> sum of totals.
pub fn year_totals(entries: &[&Entry]) -> BTreeMap<i32, BTreeMap<String, Decimal>> {
    let mut out: BTreeMap<i32, BTreeMap<String, Decimal>> = BTreeMap::new();
    for e in entries {
        let sum = out
            .entry(e.invoice.date.year())
            .or_default()
            .entry(e.invoice.currency.clone())
            .or_default();
        *sum = sum.saturating_add(e.invoice.total());
    }
    out
}

/// Unpaid amounts of one currency by days past the due date.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Aging {
    /// Due today or later, or no due date.
    pub not_due: Decimal,
    pub days_1_30: Decimal,
    pub days_31_60: Decimal,
    pub days_61_plus: Decimal,
}

impl Aging {
    pub fn total(&self) -> Decimal {
        self.not_due
            .saturating_add(self.days_1_30)
            .saturating_add(self.days_31_60)
            .saturating_add(self.days_61_plus)
    }

    fn add(&mut self, days_late: i64, amount: Decimal) {
        let bucket = match days_late {
            ..=0 => &mut self.not_due,
            1..=30 => &mut self.days_1_30,
            31..=60 => &mut self.days_31_60,
            _ => &mut self.days_61_plus,
        };
        *bucket = bucket.saturating_add(amount);
    }
}

/// Outstanding balance (effective status `pending` or `overdue`) per
/// currency, split into aging buckets.
pub fn outstanding(entries: &[&Entry], today: NaiveDate) -> BTreeMap<String, Aging> {
    let mut out: BTreeMap<String, Aging> = BTreeMap::new();
    for e in entries {
        let inv = &e.invoice;
        if !is_unpaid(inv.effective_status(today)) {
            continue;
        }
        let days_late = inv.due.map_or(0, |d| (today - d).num_days());
        out.entry(inv.currency.clone())
            .or_default()
            .add(days_late, inv.total());
    }
    out
}

// ---------------------------------------------------------------------------
// Export

/// CSV columns, in order. Documented in docs/EXPORT.md.
pub const CSV_HEADER: [&str; 11] = [
    "number",
    "date",
    "due",
    "status",
    "client_key",
    "client",
    "currency",
    "subtotal",
    "tax",
    "total",
    "file",
];

/// Money as a plain decimal string with exactly 2 decimals: `9450.00`.
pub fn money_str(x: Decimal) -> String {
    format!("{:.2}", round_cents(x))
}

/// One exported invoice. Field order is the CSV column order, plus `items`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportRow {
    pub number: String,
    pub date: String,
    pub due: Option<String>,
    pub status: &'static str,
    pub client_key: Option<String>,
    pub client: String,
    pub currency: String,
    pub subtotal: String,
    pub tax: String,
    pub total: String,
    pub file: String,
    pub items: Vec<ExportItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportItem {
    pub description: String,
    pub detail: Option<String>,
    /// Exact decimal as written, e.g. `21` or `7.5`.
    pub quantity: String,
    pub unit: String,
    /// Exact decimal as written, e.g. `450` or `12.345`.
    pub rate: String,
    /// `quantity * rate` rounded to cents.
    pub total: String,
}

impl ExportRow {
    pub fn new(e: &Entry, today: NaiveDate) -> ExportRow {
        let inv = &e.invoice;
        ExportRow {
            number: inv.number.clone(),
            date: inv.date.to_string(),
            due: inv.due.map(|d| d.to_string()),
            status: inv.effective_status(today).label(),
            client_key: e.client_key.clone(),
            client: inv.to.name.clone(),
            currency: inv.currency.clone(),
            subtotal: money_str(inv.subtotal()),
            tax: money_str(inv.tax()),
            total: money_str(inv.total()),
            file: e.path.display().to_string(),
            items: inv
                .items
                .iter()
                .map(|it| ExportItem {
                    description: it.description.clone(),
                    detail: it.detail.clone(),
                    quantity: it.quantity.normalize().to_string(),
                    unit: it.unit.clone(),
                    rate: it.rate.normalize().to_string(),
                    total: money_str(it.total()),
                })
                .collect(),
        }
    }

    fn csv_fields(&self) -> [&str; 11] {
        [
            &self.number,
            &self.date,
            self.due.as_deref().unwrap_or(""),
            self.status,
            self.client_key.as_deref().unwrap_or(""),
            &self.client,
            &self.currency,
            &self.subtotal,
            &self.tax,
            &self.total,
            &self.file,
        ]
    }
}

/// Quote a CSV field per RFC 4180 when it contains a comma, a double quote,
/// CR or LF; inner quotes are doubled.
pub fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_line(out: &mut String, fields: &[&str]) {
    let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
    out.push_str(&line.join(","));
    out.push_str("\r\n");
}

/// RFC 4180 CSV (CRLF line endings) with a header row.
pub fn to_csv(entries: &[&Entry], today: NaiveDate) -> String {
    let mut out = String::new();
    csv_line(&mut out, &CSV_HEADER);
    for e in entries {
        csv_line(&mut out, &ExportRow::new(e, today).csv_fields());
    }
    out
}

/// A pretty-printed JSON array of [`ExportRow`], with a trailing newline.
pub fn to_json(entries: &[&Entry], today: NaiveDate) -> String {
    let rows: Vec<ExportRow> = entries.iter().map(|e| ExportRow::new(e, today)).collect();
    let mut s = serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".into());
    s.push('\n');
    s
}

// ---------------------------------------------------------------------------
// Integrity checks

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

impl Severity {
    pub fn label(&self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// One problem found by [`check`].
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub path: PathBuf,
    /// 1-based line, `None` when unknown.
    pub line: Option<usize>,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    fn new(path: &Path, line: Option<usize>, severity: Severity, message: String) -> Self {
        Diagnostic {
            path: path.to_path_buf(),
            line,
            severity,
            message,
        }
    }

    /// `file:line:` (or `file:` when the line is unknown).
    pub fn location(&self) -> String {
        match self.line {
            Some(n) => format!("{}:{n}:", self.path.display()),
            None => format!("{}:", self.path.display()),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.location(),
            self.severity.label(),
            self.message
        )
    }
}

/// A number of the form `<prefix>-<YYYY>-<seq>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqNumber {
    pub prefix: String,
    pub year: i32,
    pub seq: u64,
}

/// Split `EINV-2026-07` into prefix, year and sequence. `None` when the
/// number does not have that shape.
pub fn parse_number(number: &str) -> Option<SeqNumber> {
    let mut parts = number.trim().rsplitn(3, '-');
    let seq = parts.next()?;
    let year = parts.next()?;
    let prefix = parts.next()?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if prefix.is_empty() || year.len() != 4 || !digits(year) || !digits(seq) {
        return None;
    }
    Some(SeqNumber {
        prefix: prefix.to_string(),
        year: year.parse().ok()?,
        seq: seq.parse().ok()?,
    })
}

/// Check the library: parse failures, duplicate and missing sequence
/// numbers, dates out of order, missing client name or sender tax lines.
/// `pdf_stems` are the file stems of the PDFs in the library; an invoice
/// that only exists as a PDF still fills its place in the sequence.
/// Diagnostics are sorted by file, then line.
pub fn check(lib: &Library, pdf_stems: &[String]) -> Vec<Diagnostic> {
    use Severity::{Error, Warning};
    let mut out = Vec::new();

    for err in &lib.errors {
        let line = (err.line > 0).then_some(err.line);
        out.push(Diagnostic::new(&err.path, line, Error, err.message.clone()));
    }

    // Numbered entries grouped by prefix.
    let mut by_prefix: BTreeMap<String, Vec<(&Entry, SeqNumber)>> = BTreeMap::new();
    let mut by_number: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
    for e in &lib.entries {
        let inv = &e.invoice;
        if e.number_line.is_none() {
            out.push(Diagnostic::new(
                &e.path,
                None,
                Warning,
                format!(
                    "no `number:` in the front matter (auto-assigned {}); write it explicitly",
                    inv.number
                ),
            ));
            continue;
        }
        by_number.entry(inv.number.clone()).or_default().push(e);
        match parse_number(&inv.number) {
            Some(n) => {
                if n.year != inv.date.year() {
                    out.push(Diagnostic::new(
                        &e.path,
                        e.number_line,
                        Warning,
                        format!(
                            "number {} has year {} but the invoice is dated {}",
                            inv.number, n.year, inv.date
                        ),
                    ));
                }
                by_prefix.entry(n.prefix.clone()).or_default().push((e, n));
            }
            None => out.push(Diagnostic::new(
                &e.path,
                e.number_line,
                Warning,
                format!(
                    "number {} does not look like PREFIX-YYYY-N, so it is left out of the sequence checks",
                    inv.number
                ),
            )),
        }

        if inv.to.name.trim().is_empty() {
            out.push(Diagnostic::new(
                &e.path,
                e.client_line,
                Error,
                "the client has no name (set `client:` or `to: name:`)".into(),
            ));
        }
        if inv.from.tax.iter().all(|t| t.trim().is_empty()) {
            out.push(Diagnostic::new(
                &e.path,
                e.from_line,
                Warning,
                "the sender has no tax lines (`from: tax:` in the config)".into(),
            ));
        }
    }

    // Same number written twice (padding aside, see below).
    for (number, es) in &by_number {
        if es.len() > 1 {
            for e in es {
                let others = others_list(es, e);
                out.push(Diagnostic::new(
                    &e.path,
                    e.number_line,
                    Error,
                    format!("duplicate number {number} (also in {others})"),
                ));
            }
        }
    }

    for (prefix, mut seqs) in by_prefix {
        seqs.sort_by_key(|(a, na)| (na.seq, a.invoice.date));

        // Same sequence under different spellings or years: the sequence is
        // global, so EINV-2025-5 and EINV-2026-05 collide.
        let mut by_seq: BTreeMap<u64, Vec<&Entry>> = BTreeMap::new();
        for (e, n) in &seqs {
            by_seq.entry(n.seq).or_default().push(e);
        }
        for (seq, es) in &by_seq {
            let distinct: BTreeSet<&str> = es.iter().map(|e| e.invoice.number.as_str()).collect();
            if distinct.len() > 1 {
                for e in es {
                    let others = others_list(es, e);
                    out.push(Diagnostic::new(
                        &e.path,
                        e.number_line,
                        Error,
                        format!(
                            "sequence {seq} of {prefix} is used more than once ({} here, also in {others})",
                            e.invoice.number
                        ),
                    ));
                }
            }
        }

        // A higher number dated before a lower one. Drafts are ignored.
        let mut latest: Option<&Entry> = None;
        for (e, _) in &seqs {
            if e.invoice.status == Status::Draft {
                continue;
            }
            if let Some(prev) = latest
                && e.invoice.date < prev.invoice.date
                && seq_of(&prev.invoice.number) < seq_of(&e.invoice.number)
            {
                out.push(Diagnostic::new(
                    &e.path,
                    e.date_line.or(e.number_line),
                    Error,
                    format!(
                        "{} is dated {}, before the lower number {} ({})",
                        e.invoice.number, e.invoice.date, prev.invoice.number, prev.invoice.date
                    ),
                ));
            }
            if latest.is_none_or(|p| e.invoice.date > p.invoice.date) {
                latest = Some(e);
            }
        }

        // Gaps between the lowest and highest sequence of this prefix.
        let mut present: BTreeSet<u64> = by_seq.keys().copied().collect();
        present.extend(
            pdf_stems
                .iter()
                .filter_map(|s| parse_number(s))
                .filter(|n| n.prefix == prefix)
                .map(|n| n.seq),
        );
        let mut prev: Option<u64> = None;
        for &seq in &present {
            if let Some(p) = prev
                && seq > p + 1
                && let Some(es) = by_seq.get(&seq)
            {
                let e = es[0];
                let missing = if seq - p == 2 {
                    format!("{}", p + 1)
                } else {
                    format!("{}-{}", p + 1, seq - 1)
                };
                out.push(Diagnostic::new(
                    &e.path,
                    e.number_line,
                    Warning,
                    format!(
                        "gap in the {prefix} sequence before {}: {missing} missing",
                        e.invoice.number
                    ),
                ));
            }
            prev = Some(seq);
        }
    }

    out.sort_by(|a, b| (&a.path, a.line, &a.message).cmp(&(&b.path, b.line, &b.message)));
    out
}

fn others_list(es: &[&Entry], me: &Entry) -> String {
    let others: Vec<String> = es
        .iter()
        .filter(|o| o.path != me.path)
        .map(|o| o.path.display().to_string())
        .collect();
    if others.is_empty() {
        "this file".into()
    } else {
        others.join(", ")
    }
}

/// File stems of the PDFs in the library, for [`check`].
pub fn pdf_stems(cfg: &Config) -> Vec<String> {
    library::discover_with_extensions(&cfg.invoices_dir, &["pdf"])
        .iter()
        .filter_map(|p| p.file_stem()?.to_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LineItem, Party};
    use std::str::FromStr;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn entry(number: &str, date: &str, status: Status, total: &str) -> Entry {
        Entry {
            path: PathBuf::from(format!("/inv/{number}.md")),
            client_key: Some("northwind".into()),
            number_line: Some(2),
            date_line: Some(3),
            client_line: Some(5),
            from_line: None,
            invoice: Invoice {
                number: number.into(),
                date: d(date),
                due: Some(d(date) + chrono::Duration::days(14)),
                status,
                currency: "EUR".into(),
                from: Party {
                    name: "Alex Morgan".into(),
                    tax: vec!["VAT ID: LU12345678".into()],
                    ..Party::default()
                },
                to: Party {
                    name: "Northwind Labs SL".into(),
                    ..Party::default()
                },
                payment: Default::default(),
                items: vec![LineItem {
                    description: "Work".into(),
                    detail: None,
                    quantity: Decimal::ONE,
                    unit: String::new(),
                    rate: dec(total),
                }],
                tax_rate: Decimal::ZERO,
                tax_label: "VAT".into(),
                notes: vec![],
                signature: None,
                accent: "#111827".into(),
                language: "en".into(),
                labels: Default::default(),
                template: None,
            },
        }
    }

    fn lib(mut entries: Vec<Entry>) -> Library {
        sort_entries(&mut entries);
        Library {
            entries,
            errors: vec![],
        }
    }

    fn numbers(es: &[&Entry]) -> Vec<String> {
        es.iter().map(|e| e.invoice.number.clone()).collect()
    }

    #[test]
    fn front_matter_lines_and_client_key() {
        let src = "---\nnumber: INV-2026-3   # note\ndate: 2026-01-02\nclient: \"northwind\" # c\nto:\n  name: X\n---\nclient: body\n";
        let fm = FrontMatterLines::new(src);
        assert_eq!(fm.line_of("number"), Some(2));
        assert_eq!(fm.line_of("date"), Some(3));
        assert_eq!(fm.line_of("to"), Some(5));
        assert_eq!(fm.line_of("name"), None);
        assert_eq!(fm.scalar("client").as_deref(), Some("northwind"));
        assert_eq!(fm.scalar("missing"), None);
        assert!(FrontMatterLines::new("no front matter").lines.is_empty());
    }

    #[test]
    fn filters_combine_with_and() {
        let today = d("2026-06-01");
        let mut other = entry("INV-2026-4", "2026-05-25", Status::Pending, "50");
        other.client_key = Some("contoso".into());
        other.invoice.to.name = "Contoso Ltd".into();
        let es = vec![
            entry("INV-2025-1", "2025-03-01", Status::Paid, "100"),
            entry("INV-2026-2", "2026-01-10", Status::Pending, "200"), // overdue
            entry("INV-2026-3", "2026-05-20", Status::Draft, "300"),
            other, // pending, not due yet
        ];

        let f = |f: Filter| numbers(&f.apply(&es, today));
        assert_eq!(f(Filter::default()).len(), 4);
        assert_eq!(
            f(Filter {
                year: Some(2026),
                ..Filter::default()
            }),
            ["INV-2026-2", "INV-2026-3", "INV-2026-4"]
        );
        assert_eq!(
            f(Filter {
                client: Some("NORTHWIND".into()),
                ..Filter::default()
            }),
            ["INV-2025-1", "INV-2026-2", "INV-2026-3"]
        );
        assert_eq!(
            f(Filter {
                client: Some("toso".into()),
                ..Filter::default()
            }),
            ["INV-2026-4"]
        );
        assert_eq!(
            f(Filter {
                unpaid: true,
                ..Filter::default()
            }),
            ["INV-2026-2", "INV-2026-4"]
        );
        // Effective status: the written `pending` past due is `overdue`.
        assert_eq!(
            f(Filter {
                statuses: vec![Status::Overdue],
                ..Filter::default()
            }),
            ["INV-2026-2"]
        );
        assert_eq!(
            f(Filter {
                statuses: vec![Status::Paid, Status::Draft],
                ..Filter::default()
            }),
            ["INV-2025-1", "INV-2026-3"]
        );
        assert_eq!(
            f(Filter {
                year: Some(2026),
                client: Some("northwind".into()),
                unpaid: true,
                ..Filter::default()
            }),
            ["INV-2026-2"]
        );
        assert!(
            f(Filter {
                year: Some(2025),
                unpaid: true,
                ..Filter::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn parse_status_names() {
        assert_eq!(parse_status("Overdue"), Ok(Status::Overdue));
        assert!(parse_status("sent").unwrap_err().contains("unknown status"));
    }

    #[test]
    fn year_totals_respect_the_given_entries() {
        let es = [
            entry("INV-2025-1", "2025-03-01", Status::Paid, "100.10"),
            entry("INV-2026-2", "2026-01-10", Status::Paid, "0.20"),
            entry("INV-2026-3", "2026-02-10", Status::Paid, "0.10"),
        ];
        let refs: Vec<&Entry> = es.iter().skip(1).collect();
        let t = year_totals(&refs);
        assert!(!t.contains_key(&2025));
        assert_eq!(t[&2026]["EUR"], dec("0.30"));
    }

    #[test]
    fn aging_buckets() {
        let today = d("2026-06-30");
        let mk = |n: &str, due: Option<&str>, status: Status, amount: &str| {
            let mut e = entry(n, "2026-01-01", status, amount);
            e.invoice.due = due.map(d);
            e
        };
        let es = [
            mk("INV-2026-1", Some("2026-06-30"), Status::Pending, "1"), // due today
            mk("INV-2026-2", None, Status::Pending, "2"),               // no due date
            mk("INV-2026-3", Some("2026-06-29"), Status::Pending, "4"), // 1 day
            mk("INV-2026-4", Some("2026-05-31"), Status::Pending, "8"), // 30 days
            mk("INV-2026-5", Some("2026-05-30"), Status::Pending, "16"), // 31 days
            mk("INV-2026-6", Some("2026-05-01"), Status::Pending, "32"), // 60 days
            mk("INV-2026-7", Some("2026-04-30"), Status::Pending, "64"), // 61 days
            mk("INV-2026-8", Some("2020-01-01"), Status::Paid, "128"),  // paid: ignored
            mk("INV-2026-9", Some("2020-01-01"), Status::Draft, "256"), // draft: ignored
        ];
        let mut usd = mk("INV-2026-10", Some("2026-07-15"), Status::Overdue, "5");
        usd.invoice.currency = "USD".into();
        let mut refs: Vec<&Entry> = es.iter().collect();
        refs.push(&usd);

        let o = outstanding(&refs, today);
        let eur = o["EUR"];
        assert_eq!(eur.not_due, dec("3"));
        assert_eq!(eur.days_1_30, dec("12"));
        assert_eq!(eur.days_31_60, dec("48"));
        assert_eq!(eur.days_61_plus, dec("64"));
        assert_eq!(eur.total(), dec("127"));
        // Written `overdue` before its due date counts as not due yet.
        assert_eq!(o["USD"].not_due, dec("5"));
    }

    #[test]
    fn csv_quoting() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
        assert_eq!(csv_field(""), "");
    }

    #[test]
    fn csv_and_json_export() {
        let today = d("2026-06-01");
        let mut e = entry("INV-2026-2", "2026-01-10", Status::Pending, "1234.5");
        e.invoice.to.name = "Northwind Labs, SL".into();
        e.invoice.tax_rate = dec("21");
        e.invoice.items[0].quantity = dec("7.50");
        let csv = to_csv(&[&e], today);
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines[0], CSV_HEADER.join(","));
        assert_eq!(
            lines[1],
            "INV-2026-2,2026-01-10,2026-01-24,overdue,northwind,\"Northwind Labs, SL\",EUR,9258.75,1944.34,11203.09,/inv/INV-2026-2.md"
        );
        assert_eq!(lines[2], "");

        let json: serde_json::Value = serde_json::from_str(&to_json(&[&e], today)).unwrap();
        let row = &json[0];
        assert_eq!(row["number"], "INV-2026-2");
        assert_eq!(row["status"], "overdue");
        assert_eq!(row["total"], "11203.09");
        assert_eq!(row["items"][0]["quantity"], "7.5");
        assert_eq!(row["items"][0]["rate"], "1234.5");
        assert_eq!(row["items"][0]["total"], "9258.75");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&to_json(&[], today)).unwrap(),
            serde_json::json!([])
        );
    }

    #[test]
    fn parse_numbers() {
        assert_eq!(
            parse_number("EINV-2026-07"),
            Some(SeqNumber {
                prefix: "EINV".into(),
                year: 2026,
                seq: 7
            })
        );
        assert_eq!(parse_number("A-B-2026-1").unwrap().prefix, "A-B");
        assert_eq!(parse_number("2026-1"), None);
        assert_eq!(parse_number("INV-26-1"), None);
        assert_eq!(parse_number("INV-2026-x"), None);
    }

    fn messages(diags: &[Diagnostic]) -> Vec<String> {
        diags.iter().map(|d| d.to_string()).collect()
    }

    #[test]
    fn clean_library_has_no_diagnostics() {
        let l = lib(vec![
            entry("INV-2025-1", "2025-12-01", Status::Paid, "1"),
            entry("INV-2026-2", "2026-01-10", Status::Paid, "1"),
            entry("INV-2026-03", "2026-02-10", Status::Pending, "1"),
        ]);
        assert_eq!(messages(&check(&l, &[])), Vec::<String>::new());
    }

    #[test]
    fn detects_gaps_and_pdfs_fill_them() {
        let l = lib(vec![
            entry("INV-2025-1", "2025-12-01", Status::Paid, "1"),
            entry("INV-2026-3", "2026-01-10", Status::Paid, "1"),
            entry("INV-2026-7", "2026-02-10", Status::Paid, "1"),
            entry("OTHER-2026-9", "2026-02-11", Status::Paid, "1"),
        ]);
        let diags = check(&l, &[]);
        assert_eq!(
            messages(&diags),
            [
                "/inv/INV-2026-3.md:2: warning: gap in the INV sequence before INV-2026-3: 2 missing",
                "/inv/INV-2026-7.md:2: warning: gap in the INV sequence before INV-2026-7: 4-6 missing",
            ]
        );
        assert!(diags.iter().all(|d| d.severity == Severity::Warning));

        let pdfs: Vec<String> = ["INV-2026-02", "INV-2026-4", "INV-2026-5", "INV-2026-6"]
            .map(String::from)
            .to_vec();
        assert!(check(&l, &pdfs).is_empty());
    }

    #[test]
    fn detects_duplicates() {
        let mut a = entry("INV-2026-2", "2026-01-10", Status::Paid, "1");
        a.path = PathBuf::from("/inv/a.md");
        let mut b = entry("INV-2026-2", "2026-01-10", Status::Paid, "1");
        b.path = PathBuf::from("/inv/b.md");
        let c = entry("INV-2025-1", "2025-01-10", Status::Paid, "1");
        let l = lib(vec![a, b, c]);
        assert_eq!(
            messages(&check(&l, &[])),
            [
                "/inv/a.md:2: error: duplicate number INV-2026-2 (also in /inv/b.md)",
                "/inv/b.md:2: error: duplicate number INV-2026-2 (also in /inv/a.md)",
            ]
        );

        // The sequence is global: the same N in another year or padded collides.
        let l = lib(vec![
            entry("INV-2025-5", "2025-01-10", Status::Paid, "1"),
            entry("INV-2026-05", "2026-01-10", Status::Paid, "1"),
        ]);
        let diags = check(&l, &[]);
        assert_eq!(diags.len(), 2, "{diags:?}");
        assert!(diags.iter().all(|d| {
            d.severity == Severity::Error
                && d.message
                    .contains("sequence 5 of INV is used more than once")
        }));
    }

    #[test]
    fn detects_dates_out_of_order() {
        let l = lib(vec![
            entry("INV-2026-1", "2026-03-01", Status::Paid, "1"),
            entry("INV-2026-2", "2026-02-01", Status::Paid, "1"),
            entry("INV-2026-3", "2026-01-01", Status::Draft, "1"), // drafts are ignored
            entry("INV-2026-4", "2026-03-01", Status::Paid, "1"),  // same day is fine
        ]);
        assert_eq!(
            messages(&check(&l, &[])),
            [
                "/inv/INV-2026-2.md:3: error: INV-2026-2 is dated 2026-02-01, before the lower number INV-2026-1 (2026-03-01)"
            ]
        );
    }

    #[test]
    fn detects_missing_parties_parse_errors_and_odd_numbers() {
        let mut no_client = entry("INV-2026-1", "2026-01-01", Status::Paid, "1");
        no_client.invoice.to.name = " ".into();
        let mut no_tax = entry("INV-2026-2", "2026-01-02", Status::Paid, "1");
        no_tax.invoice.from.tax.clear();
        let mut auto = entry("INV-2026-9", "2026-01-03", Status::Paid, "1");
        auto.number_line = None;
        let odd = entry("custom", "2026-01-04", Status::Paid, "1");
        let wrong_year = entry("INV-2025-3", "2026-01-05", Status::Paid, "1");
        let mut l = lib(vec![no_client, no_tax, auto, odd, wrong_year]);
        l.errors.push(LoadError {
            path: PathBuf::from("/inv/broken.md"),
            line: 4,
            message: "no line items".into(),
        });
        let diags = check(&l, &[]);
        let errs: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.to_string())
            .collect();
        assert_eq!(
            errs,
            [
                "/inv/INV-2026-1.md:5: error: the client has no name (set `client:` or `to: name:`)",
                "/inv/broken.md:4: error: no line items",
            ]
        );
        let warns: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .map(|d| d.to_string())
            .collect();
        assert_eq!(warns.len(), 4, "{warns:?}");
        assert!(
            warns[0].starts_with("/inv/INV-2025-3.md:2: warning: number INV-2025-3 has year 2025")
        );
        assert!(warns[1].starts_with("/inv/INV-2026-2.md: warning: the sender has no tax lines"));
        assert!(warns[2].starts_with("/inv/INV-2026-9.md: warning: no `number:`"));
        assert!(warns[3].contains("does not look like PREFIX-YYYY-N"));
    }
}
