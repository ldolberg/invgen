//! Live check of an invoice source for the editor: every problem that can be
//! found at once (unknown keys, invalid values, bad item lines, malformed
//! tables), plus the parser's own error and the warnings of a parsed invoice.
//! GUI independent; the editor runs it on the render thread.

use crate::config::Config;
use crate::model::{Invoice, Status, num, tax_note_preset};
use crate::parse::{ParseError, item_problems, parse_invoice};
use crate::render::{epc, i18n, locale};
use crate::schema::{self, CONFIG, DATE_KEYS, FrontMatter, keys_at, unquote};
use chrono::NaiveDate;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line in the source, 0 when it is about the whole document.
    pub line: usize,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    fn error(line: usize, message: impl Into<String>) -> Self {
        Diagnostic {
            line,
            severity: Severity::Error,
            message: message.into(),
        }
    }

    fn warning(line: usize, message: impl Into<String>) -> Self {
        Diagnostic {
            line,
            severity: Severity::Warning,
            message: message.into(),
        }
    }
}

/// Check `src`, parsing it with `cfg`.
pub fn check(src: &str, cfg: &Config) -> Vec<Diagnostic> {
    let parsed = parse_invoice(src, cfg);
    check_parsed(src, cfg, parsed.as_ref())
}

/// Check `src` when it was already parsed (the render thread has the result).
/// `parsed` may also carry a render error (line 0).
pub fn check_parsed(
    src: &str,
    cfg: &Config,
    parsed: Result<&Invoice, &ParseError>,
) -> Vec<Diagnostic> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let fm = schema::front_matter(src);
    let mut out = Vec::new();
    if let Some(fm) = &fm {
        front_matter(fm, cfg, &mut out);
    }
    let body_start = fm.as_ref().map_or(0, |f| f.body_start);
    let body_first_line = src[..body_start].matches('\n').count() + 1;
    tables(&src[body_start..], body_first_line, &mut out);
    for e in item_problems(src) {
        push_unique(&mut out, Diagnostic::error(e.line, e.message));
    }

    let line_of = |key: &str| fm.as_ref().and_then(|f| f.top(key)).map_or(0, |l| l.line);
    match parsed {
        Err(e) => {
            // A more specific check may already explain this line, or a
            // line inside this block (`config:` reports at its own line).
            let block = fm.as_ref().and_then(|f| {
                f.keys_under(&[])
                    .find(|l| l.line == e.line && l.value.is_empty())
                    .and_then(|l| l.key)
            });
            // YAML rejects duplicate keys; the duplicate check names both lines.
            let duplicate = e.message.contains("duplicate entry")
                && out.iter().any(|d| d.message.starts_with("duplicate key"));
            let explained = duplicate
                || e.line != 0
                    && out.iter().any(|d| {
                        d.severity == Severity::Error
                            && (d.line == e.line
                                || block.is_some_and(|b| {
                                    fm.as_ref().is_some_and(|f| {
                                        f.lines.iter().any(|l| {
                                            l.line == d.line && l.parents.first() == Some(&b)
                                        })
                                    })
                                }))
                    });
            if !explained {
                push_unique(&mut out, Diagnostic::error(e.line, e.message.clone()));
            }
        }
        Ok(inv) => {
            if let Some(w) = inv.tax_note_warning() {
                let line = [line_of("tax_note"), line_of("client")]
                    .into_iter()
                    .find(|&l| l != 0)
                    .unwrap_or(0);
                out.push(Diagnostic::warning(line, w));
            }
            if let Some(w) = epc::warning(inv) {
                out.push(Diagnostic::warning(line_of("payment"), w));
            }
            if let Some(due) = inv.due
                && due < inv.date
            {
                let line = [line_of("due"), line_of("due_days")]
                    .into_iter()
                    .find(|&l| l != 0)
                    .unwrap_or(0);
                out.push(Diagnostic::warning(
                    line,
                    format!("the due date {due} is before the invoice date {}", inv.date),
                ));
            }
        }
    }
    out.sort_by_key(|d| (d.line, d.severity));
    out
}

fn push_unique(out: &mut Vec<Diagnostic>, d: Diagnostic) {
    if !out
        .iter()
        .any(|o| o.line == d.line && o.message == d.message)
    {
        out.push(d);
    }
}

// ---------------------------------------------------------------- front matter

fn front_matter(fm: &FrontMatter, cfg: &Config, out: &mut Vec<Diagnostic>) {
    // Clients a `config:` block defines for this invoice.
    let doc_clients: Vec<&str> = fm
        .keys_under(&["config", "clients"])
        .filter_map(|l| l.key)
        .collect();
    let mut seen: HashMap<(Vec<&str>, &str), usize> = HashMap::new();
    for l in &fm.lines {
        let Some(key) = l.key else { continue };
        if let Some(first) = seen.insert((l.parents.clone(), key), l.line) {
            out.push(Diagnostic::error(
                l.line,
                format!("duplicate key `{key}` (first on line {first})"),
            ));
            seen.insert((l.parents.clone(), key), first);
        }
        if let Some(known) = keys_at(&l.parents)
            && !known.iter().any(|k| k.name == key)
        {
            out.push(Diagnostic::warning(
                l.line,
                unknown_key(&l.parents, key, &known),
            ));
            continue;
        }
        let value = unquote(l.value).trim();
        if value.is_empty() || value.starts_with(['|', '>', '&', '*', '!']) {
            continue;
        }
        if let Some(d) = check_value(&l.parents, key, value, cfg, &doc_clients) {
            out.push(Diagnostic { line: l.line, ..d });
        }
    }
}

fn unknown_key(parents: &[&str], key: &str, known: &[schema::Key]) -> String {
    if parents.is_empty() && CONFIG.iter().any(|k| k.name == key) {
        return format!(
            "`{key}` is a config key, not an invoice key: put it under `config:` to set it \
             for this invoice (ignored)"
        );
    }
    let place = if parents.is_empty() {
        String::new()
    } else {
        format!(" under `{}`", parents.join("."))
    };
    let hint = known
        .iter()
        .map(|k| (distance(key, k.name), k.name))
        .filter(|&(d, name)| d <= if name.len() <= 4 { 1 } else { 2 })
        .min()
        .map(|(_, name)| format!("; did you mean `{name}`?"))
        .unwrap_or_default();
    format!("unknown key `{key}`{place} (ignored){hint}")
}

/// A problem with `key: value`, line left at 0 for the caller to fill in.
fn check_value(
    parents: &[&str],
    key: &str,
    value: &str,
    cfg: &Config,
    doc_clients: &[&str],
) -> Option<Diagnostic> {
    let err = |m: String| Some(Diagnostic::error(0, m));
    let warn = |m: String| Some(Diagnostic::warning(0, m));
    let lower = value.to_ascii_lowercase();
    let in_number_format = parents.last() == Some(&"number_format");
    match (parents, key) {
        ([], "status") => value.parse::<Status>().err().and_then(err),
        ([], k) if DATE_KEYS.contains(&k) => {
            let hidden = k == "due" && matches!(lower.as_str(), "false" | "none" | "no");
            match hidden || NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() {
                true => None,
                false => err(format!(
                    "`{k}` must be a date like 2026-11-02, got `{value}`"
                )),
            }
        }
        (_, "due_days") => match value.parse::<i64>() {
            Ok(_) => None,
            Err(_) => err(format!("`due_days` must be a whole number, got `{value}`")),
        },
        ([], "kind") => match crate::credit::is_credit_note_kind(value) || lower == "invoice" {
            true => None,
            false => err(format!(
                "unknown kind `{value}` (use invoice or credit_note)"
            )),
        },
        ([], "client") => {
            let known = cfg.clients.contains_key(value) || doc_clients.contains(&value);
            if known {
                return None;
            }
            let mut names: Vec<&str> = cfg.clients.keys().map(String::as_str).collect();
            names.extend(doc_clients);
            let hint = if names.is_empty() {
                "no clients are defined in the config".to_string()
            } else {
                format!("known clients: {}", names.join(", "))
            };
            err(format!("unknown client `{value}` ({hint})"))
        }
        (_, "tax_rate" | "rate") => match num::from_text(value) {
            Some(_) => None,
            None => err(format!("`{key}` must be a number, got `{value}`")),
        },
        ([], "accent") => {
            let hex = value.strip_prefix('#').unwrap_or("");
            match matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                true => None,
                false => err(format!(
                    "`accent` must be a hex color like \"#111827\", got `{value}`"
                )),
            }
        }
        (_, "language") => i18n::normalize_language(value).err().and_then(err),
        (_, "locale") => locale::normalize_locale(value).err().and_then(err),
        (_, "tax_note") => {
            let word = !value.contains(char::is_whitespace);
            if word && lower != "false" && tax_note_preset(value).is_none() {
                warn(format!(
                    "`{value}` is not a tax_note preset ({}); it is printed as written",
                    crate::model::TAX_NOTE_PRESETS.join(", ")
                ))
            } else {
                None
            }
        }
        (_, "currency") if in_number_format => match lower.as_str() {
            "symbol" | "code" => None,
            _ => err(format!(
                "`number_format.currency` must be symbol or code, got `{value}`"
            )),
        },
        (_, "currency_position") if in_number_format => match lower.as_str() {
            "before" | "after" => None,
            _ => err(format!(
                "`currency_position` must be before or after, got `{value}`"
            )),
        },
        (_, "currency") => {
            let iso = value.len() == 3 && value.chars().all(|c| c.is_ascii_alphabetic());
            match iso {
                true => None,
                false => warn(format!(
                    "`{value}` does not look like an ISO currency code (EUR, USD, GBP...)"
                )),
            }
        }
        (["config"], "e_invoice") => {
            let list = value.strip_prefix('[').and_then(|v| v.strip_suffix(']'));
            let names: Vec<&str> = match list {
                Some(inner) => inner.split(',').collect(),
                None => vec![value],
            };
            names
                .into_iter()
                .map(|n| unquote(n.trim()))
                .filter(|n| !matches!(n.to_ascii_lowercase().as_str(), "" | "none" | "false"))
                .find_map(|n| n.parse::<crate::einvoice::Format>().err())
                .and_then(err)
        }
        (_, "qr") => match lower.as_str() {
            "true" | "false" => None,
            _ => err(format!("`qr` must be true or false, got `{value}`")),
        },
        _ => None,
    }
}

/// Edit distance counting a swap of two neighbors as one edit (optimal
/// string alignment), for "did you mean" hints.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = v;
        }
    }
    d[a.len()][b.len()]
}

// ---------------------------------------------------------------- body

/// Rows of the first table whose cell count differs from the header's:
/// markdown silently drops extra cells and fills missing ones.
fn tables(body: &str, first_line: usize, out: &mut Vec<Diagnostic>) {
    let mut in_code = false;
    let mut header: Option<usize> = None;
    let mut lines = body.lines().enumerate().peekable();
    while let Some((i, line)) = lines.next() {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let is_row = t.starts_with('|') || (header.is_some() && t.contains('|'));
        match header {
            None if is_row => {
                // A table starts with a header row followed by a separator.
                if lines.peek().is_some_and(|(_, next)| is_separator(next)) {
                    header = Some(cell_count(t));
                    lines.next();
                }
            }
            Some(n) if is_row => {
                let got = cell_count(t);
                if got != n {
                    let what = if got > n {
                        "the extra cells are ignored"
                    } else {
                        "the missing cells are empty"
                    };
                    out.push(Diagnostic::warning(
                        first_line + i,
                        format!("this row has {got} cells but the header has {n}; {what}"),
                    ));
                }
            }
            // Only the first table holds items.
            Some(_) => return,
            None => {}
        }
    }
}

fn is_separator(line: &str) -> bool {
    let t = line.trim();
    t.contains('-')
        && t.contains('|')
        && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// Cells of a table row: `|` separated, outer pipes optional, `\|` escaped.
fn cell_count(row: &str) -> usize {
    let t = row.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = match t.strip_suffix('|') {
        Some(s) if !s.ends_with('\\') => s,
        _ => t,
    };
    let mut n = 1;
    let mut escaped = false;
    for c in t.chars() {
        match c {
            '\\' if !escaped => escaped = true,
            '|' if !escaped => n += 1,
            _ => escaped = false,
        }
        if c != '\\' {
            escaped = false;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClientConfig;

    fn cfg() -> Config {
        let mut cfg = Config::default();
        let mut c = ClientConfig::default();
        c.party.name = "Northwind Labs".into();
        cfg.clients.insert("northwind".into(), c);
        cfg.invoices_dir = std::env::temp_dir().join("invgen-lint-test-none");
        cfg
    }

    fn lines(d: &[Diagnostic], sev: Severity) -> Vec<usize> {
        d.iter()
            .filter(|d| d.severity == sev)
            .map(|d| d.line)
            .collect()
    }

    const ITEMS: &str = "| Description | Qty | Rate |\n|---|---|---|\n| Work | 1 | 10 |\n";

    #[test]
    fn clean_invoice_has_no_problems() {
        let src = format!("---\nnumber: X-1\ndate: 2026-01-02\nclient: northwind\n---\n{ITEMS}");
        assert_eq!(check(&src, &cfg()), vec![]);
    }

    #[test]
    fn reports_several_front_matter_problems() {
        let src = format!(
            "---\nstauts: paid\nstatus: payed\ndate: 2026-13-01\nkind: refund\nclient: nobody\n\
             language: xx\ntax_note: reverse_chrage\ne_invoice: ubl\nconfig:\n  e_invoice: pdf\n\
             from:\n  nmae: Alex\n---\n{ITEMS}"
        );
        let d = check(&src, &cfg());
        let msg = |line: usize| {
            d.iter()
                .filter(|d| d.line == line)
                .map(|d| d.message.as_str())
                .collect::<Vec<_>>()
                .join(" / ")
        };
        assert!(msg(2).contains("did you mean `status`"), "{}", msg(2));
        assert!(msg(3).contains("unknown status"), "{}", msg(3));
        assert!(msg(4).contains("must be a date"), "{}", msg(4));
        assert!(msg(5).contains("unknown kind"), "{}", msg(5));
        assert!(msg(6).contains("known clients: northwind"), "{}", msg(6));
        assert!(msg(7).contains("unknown language"), "{}", msg(7));
        assert!(msg(8).contains("not a tax_note preset"), "{}", msg(8));
        assert!(msg(9).contains("config key"), "{}", msg(9));
        assert!(msg(11).contains("pdf"), "{}", msg(11));
        assert!(msg(13).contains("did you mean `name`"), "{}", msg(13));
        assert_eq!(lines(&d, Severity::Error), vec![3, 4, 5, 6, 7, 11]);
        assert_eq!(lines(&d, Severity::Warning), vec![2, 8, 9, 13]);
    }

    #[test]
    fn parse_error_is_kept_when_nothing_else_explains_it() {
        // A credit note without refers_to: only the parser knows.
        let src = format!("---\nkind: credit_note\n---\n{ITEMS}");
        let d = check(&src, &cfg());
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].line, 2);
        assert!(d[0].message.contains("refers_to"));
        // ...but not repeated when a check already flagged that line.
        let src = format!("---\nstatus: nope\n---\n{ITEMS}");
        let d = check(&src, &cfg());
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn every_bad_item_is_reported() {
        let src = "---\ndate: 2026-01-02\n---\n\
                   | Description | Qty | Rate |\n|---|---|---|\n\
                   | A | x | 10 |\n| B | 1 | 10 |\n| C | 1 | ten |\n| D | 1 |\n| E | 1 | 2 | 3 |\n";
        let d = check(src, &cfg());
        assert_eq!(lines(&d, Severity::Error), vec![6, 8, 9], "{d:?}");
        assert_eq!(lines(&d, Severity::Warning), vec![9, 10], "{d:?}");
        assert!(d.iter().any(|d| d.message.contains("has 2 cells")));

        let src = "- A: 1 day @ 10\n- B: x @ 10\n- C 1 @ 2\n- D: 1 @ 3\n";
        let d = check(src, &cfg());
        assert_eq!(lines(&d, Severity::Error), vec![2, 3], "{d:?}");
    }

    #[test]
    fn invoice_warnings_point_at_their_keys() {
        let mut cfg = cfg();
        cfg.clients.get_mut("northwind").unwrap().party.tax = vec!["VAT DE123".into()];
        let src =
            format!("---\ndate: 2026-01-10\ndue: 2026-01-01\nclient: northwind\n---\n{ITEMS}");
        let d = check(&src, &cfg);
        assert_eq!(lines(&d, Severity::Warning), vec![3, 4], "{d:?}");
        assert!(d[0].message.contains("before the invoice date"));
        assert!(d[1].message.contains("tax_note"));
    }

    #[test]
    fn duplicates_and_nested_values() {
        let src = format!(
            "---\nstatus: paid\nstatus: draft\nnumber_format:\n  currency: euro\n\
             currency: euros\npayment:\n  qr: maybe\nconfig:\n  clients:\n    acme:\n      \
             name: ACME\n---\n{ITEMS}"
        );
        let d = check(&src, &cfg());
        assert!(
            d[0].message
                .contains("duplicate key `status` (first on line 2)"),
            "{d:?}"
        );
        assert_eq!(d[0].line, 3);
        assert_eq!(lines(&d, Severity::Error), vec![3, 5, 8], "{d:?}");
        assert_eq!(lines(&d, Severity::Warning), vec![6], "{d:?}");
        // A client defined in the config block is known.
        let src = format!(
            "---\nclient: acme\nconfig:\n  clients:\n    acme:\n      name: ACME\n---\n{ITEMS}"
        );
        assert_eq!(check(&src, &cfg()), vec![]);
    }

    #[test]
    fn free_form_levels_and_text_blocks_are_not_checked() {
        let src = format!(
            "---\nconfig:\n  labels:\n    anything: x\ntax_note: |\n  dear: you\n---\n{ITEMS}"
        );
        assert_eq!(check(&src, &cfg()), vec![]);
    }

    #[test]
    fn shipped_examples_are_clean() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
        let cfg = Config::load_from(&dir.join("config.yaml")).unwrap();
        for entry in std::fs::read_dir(dir.join("invoices")).unwrap() {
            let path = entry.unwrap().path();
            let src = std::fs::read_to_string(&path).unwrap();
            let d = check(&src, &cfg);
            assert!(
                d.iter().all(|d| d.severity == Severity::Warning),
                "{}: {d:?}",
                path.display()
            );
            // Only the known, intended warnings (none about keys or values).
            assert!(
                d.iter().all(|d| !d.message.contains("unknown key")),
                "{}: {d:?}",
                path.display()
            );
        }
    }

    #[test]
    fn cell_counts() {
        assert_eq!(cell_count("| a | b |"), 2);
        assert_eq!(cell_count("a | b"), 2);
        assert_eq!(cell_count("| a \\| b | c |"), 2);
        assert_eq!(cell_count("|  |"), 1);
    }

    #[test]
    fn suggestions() {
        assert_eq!(distance("stauts", "status"), 1);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("", "abc"), 3);
        let msg = unknown_key(&[], "foo", schema::INVOICE);
        assert!(!msg.contains("did you mean"), "{msg}");
    }
}
