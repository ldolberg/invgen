//! Context-aware completions for the invoice editor. GUI independent: given
//! the text, the cursor (byte offset) and what is known (config, library),
//! say what is being typed and what could go there.

use crate::config::Config;
use crate::model::TAX_NOTE_PRESETS;
use crate::render::i18n;
use crate::schema::{self, DATE_KEYS, E_INVOICE, KINDS, STATUSES, is_ident_char, keys_at};
use chrono::{Duration, NaiveDate};
use std::ops::Range;

/// Everything completions can draw from besides the text itself.
pub struct Sources<'a> {
    pub cfg: &'a Config,
    pub today: NaiveDate,
    /// Units used in the library, most used first.
    pub units: &'a [String],
    /// Item descriptions used in the library, most used first.
    pub descriptions: &'a [String],
    /// Invoice numbers in the library (for `refers_to:`).
    pub numbers: &'a [String],
}

/// What the cursor is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Context {
    /// A front matter key under these parent keys (outermost first).
    Key { parents: Vec<String> },
    /// The value of `key:` under these parent keys.
    Value { parents: Vec<String>, key: String },
    /// An item description in the body (table cell or list item).
    Description,
    /// An item unit in the body (table cell or after the list quantity).
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Shown in the list.
    pub label: String,
    /// Replaces the typed prefix.
    pub insert: String,
    /// Short hint shown next to the label.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub context: Context,
    /// Byte range of the typed prefix; accepting an item replaces it.
    pub range: Range<usize>,
    /// Candidates that start with the prefix (case-insensitive), best first.
    pub items: Vec<Item>,
}

const MAX_ITEMS: usize = 50;

/// Common ISO 4217 codes offered after `currency:`.
pub const CURRENCIES: &[&str] = &[
    "EUR", "USD", "GBP", "CHF", "JPY", "CAD", "AUD", "NZD", "SEK", "NOK", "DKK", "PLN", "CZK",
    "HUF", "RON", "BGN", "ISK", "TRY", "MXN", "BRL", "ARS", "CLP", "COP", "UYU", "INR", "CNY",
    "HKD", "SGD", "ZAR",
];

/// Locales offered after `locale:` (any supported language plus region works).
pub const LOCALES: &[&str] = &[
    "en-US", "en-GB", "en-IE", "de-DE", "de-AT", "de-CH", "fr-FR", "fr-BE", "fr-CH", "es-ES",
    "es-AR", "es-MX", "it-IT", "it-CH", "pt-PT", "pt-BR", "nl-NL", "nl-BE",
];

/// The completion at byte offset `cursor` of `text`, if the cursor is
/// somewhere completions make sense. The item list may be empty.
pub fn complete(text: &str, cursor: usize, src: &Sources) -> Option<Completion> {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    let line_start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[cursor..].find('\n').map_or(text.len(), |i| cursor + i);
    let before = &text[line_start..cursor];
    let after = &text[cursor..line_end];
    let line_no = text[..line_start].matches('\n').count() + 1;

    let fm = schema::front_matter(text);
    let in_front = fm
        .as_ref()
        .is_some_and(|f| line_no >= 2 && f.close_line.is_none_or(|c| line_no < c));
    let (context, prefix) = match &fm {
        Some(fm) if in_front => front_context(fm, line_no, before, after)?,
        _ => body_context(text, line_start, before)?,
    };
    let range = cursor - prefix.len()..cursor;
    let candidates = match &context {
        Context::Key { parents } => {
            let fm = fm.as_ref()?;
            key_items(
                fm,
                line_no,
                parents,
                before.len() - before.trim_start().len(),
                src,
            )
        }
        Context::Value { parents, key } => value_items(parents, key, fm.as_ref(), src),
        Context::Description => {
            let client = src
                .cfg
                .clients
                .values()
                .filter_map(|c| c.description.clone());
            plain(src.descriptions.iter().cloned().chain(client), "")
        }
        Context::Unit => {
            let client = src.cfg.clients.values().filter_map(|c| c.unit.clone());
            let common = ["days", "hours"].map(String::from);
            plain(src.units.iter().cloned().chain(client).chain(common), "")
        }
    };
    let typed = prefix.trim_start_matches(['"', '\'']).to_lowercase();
    let mut items: Vec<Item> = Vec::new();
    for it in candidates {
        let fits = it.label.to_lowercase().starts_with(&typed) && it.insert != prefix;
        if fits && !items.iter().any(|o| o.label == it.label) {
            items.push(it);
        }
    }
    items.truncate(MAX_ITEMS);
    Some(Completion {
        context,
        range,
        items,
    })
}

fn plain(values: impl Iterator<Item = String>, detail: &str) -> Vec<Item> {
    values
        .filter(|v| !v.trim().is_empty())
        .map(|v| Item {
            label: v.clone(),
            insert: v,
            detail: detail.to_string(),
        })
        .collect()
}

fn words(values: &[&str], detail: &str) -> Vec<Item> {
    plain(values.iter().map(|s| s.to_string()), detail)
}

// ---------------------------------------------------------------- front matter

fn front_context<'t>(
    fm: &schema::FrontMatter,
    line_no: usize,
    before: &'t str,
    after: &str,
) -> Option<(Context, &'t str)> {
    let t = before.trim_start();
    let indent = before.len() - t.len();
    let parents = parents_at(fm, line_no, indent);
    if t.chars().all(is_ident_char) {
        // `sta|` at the start of a line: a key, unless more text follows.
        return after
            .trim()
            .is_empty()
            .then_some((Context::Key { parents }, t));
    }
    let (key, rest) = t.split_once(':')?;
    let key = schema::unquote(key.trim_end());
    if !schema::is_ident(key) || !rest.starts_with([' ', '\t']) {
        return None;
    }
    let prefix = rest.trim_start();
    if prefix.contains('#') || prefix.contains(": ") {
        return None;
    }
    Some((
        Context::Value {
            parents,
            key: key.to_string(),
        },
        prefix,
    ))
}

/// Parent keys of a line at `indent` on `line_no` (the line itself may not
/// be a `key:` line yet).
fn parents_at(fm: &schema::FrontMatter, line_no: usize, indent: usize) -> Vec<String> {
    let mut stack: Vec<(usize, &str)> = Vec::new();
    for l in fm.lines.iter().take_while(|l| l.line < line_no) {
        let Some(key) = l.key else { continue };
        while stack.last().is_some_and(|&(i, _)| i >= l.indent) {
            stack.pop();
        }
        if l.value.is_empty() {
            stack.push((l.indent, key));
        }
    }
    while stack.last().is_some_and(|&(i, _)| i >= indent) {
        stack.pop();
    }
    stack.into_iter().map(|(_, k)| k.to_string()).collect()
}

fn key_items(
    fm: &schema::FrontMatter,
    line_no: usize,
    parents: &[String],
    indent: usize,
    src: &Sources,
) -> Vec<Item> {
    let parents: Vec<&str> = parents.iter().map(String::as_str).collect();
    let present: Vec<&str> = fm
        .keys_under(&parents)
        .filter(|l| l.line != line_no)
        .filter_map(|l| l.key)
        .collect();
    let mut out = Vec::new();
    match keys_at(&parents) {
        Some(keys) => {
            for k in keys.iter().filter(|k| !present.contains(&k.name)) {
                let insert = if k.block {
                    format!("{}:\n{}", k.name, " ".repeat(indent + 2))
                } else {
                    format!("{}: ", k.name)
                };
                out.push(Item {
                    label: k.name.to_string(),
                    insert,
                    detail: k.doc.to_string(),
                });
            }
        }
        // Under `config: clients:`, the keys are client names.
        None if parents == ["config", "clients"] => {
            for (key, c) in &src.cfg.clients {
                if !present.contains(&key.as_str()) {
                    out.push(Item {
                        label: key.clone(),
                        insert: format!("{key}:\n{}", " ".repeat(indent + 2)),
                        detail: c.party.name.clone(),
                    });
                }
            }
        }
        None => {}
    }
    out
}

fn value_items(
    parents: &[String],
    key: &str,
    fm: Option<&schema::FrontMatter>,
    src: &Sources,
) -> Vec<Item> {
    let parents: Vec<&str> = parents.iter().map(String::as_str).collect();
    let in_number_format = parents.last() == Some(&"number_format");
    let date = |d: NaiveDate, detail: &str| Item {
        label: d.format("%Y-%m-%d").to_string(),
        insert: d.format("%Y-%m-%d").to_string(),
        detail: detail.to_string(),
    };
    match (parents.as_slice(), key) {
        ([], "client") => {
            let mut out: Vec<Item> = src
                .cfg
                .clients
                .iter()
                .map(|(k, c)| Item {
                    label: k.clone(),
                    insert: k.clone(),
                    detail: c.party.name.clone(),
                })
                .collect();
            if let Some(fm) = fm {
                let doc = fm.keys_under(&["config", "clients"]).filter_map(|l| l.key);
                out.extend(plain(doc.map(String::from), "defined in config:"));
            }
            out
        }
        ([], "status") => words(STATUSES, ""),
        ([], "kind") => words(KINDS, ""),
        ([], "due") => {
            let days = src.cfg.due_days;
            let mut out = Vec::new();
            if let Some(d) = Duration::try_days(days).and_then(|d| src.today.checked_add_signed(d))
            {
                out.push(date(d, &format!("today + {days} days")));
            }
            out.push(date(src.today, "today"));
            out.extend(words(&["false"], "no due date"));
            out
        }
        ([], k) if DATE_KEYS.contains(&k) => vec![date(src.today, "today")],
        ([], "refers_to") => plain(src.numbers.iter().cloned(), "in the library"),
        ([], "signature") => words(&["false"], "no signature"),
        (_, "tax_note") => words(TAX_NOTE_PRESETS, "preset, translated"),
        (_, "language") => words(i18n::SUPPORTED, ""),
        (_, "locale") => words(LOCALES, ""),
        (["config"], "e_invoice") => words(E_INVOICE, ""),
        (_, "currency") if in_number_format => words(&["symbol", "code"], ""),
        (_, "currency_position") if in_number_format => words(&["before", "after"], ""),
        (_, "currency") => {
            let mut codes = vec![src.cfg.currency.to_uppercase()];
            codes.extend(src.cfg.clients.values().filter_map(|c| c.currency.clone()));
            codes.extend(CURRENCIES.iter().map(|s| s.to_string()));
            plain(codes.into_iter(), "")
        }
        (_, "qr") => words(&["true", "false"], ""),
        (_, "unit") => plain(src.units.iter().cloned(), ""),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------- body

fn body_context<'t>(text: &str, line_start: usize, before: &'t str) -> Option<(Context, &'t str)> {
    let t = before.trim_start();
    if t.starts_with('|') {
        return table_context(text, line_start, before);
    }
    let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "))?;
    if !rest.contains([':', '@']) {
        return Some((Context::Description, rest.trim_start()));
    }
    // `- Description: 21 da|`
    let (_, qty) = rest.rsplit_once(':')?;
    let qty = qty.trim_start();
    let digits = qty
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(qty.len());
    let unit = qty[digits..].trim_start();
    (digits > 0 && unit.chars().all(char::is_alphabetic)).then_some((Context::Unit, unit))
}

/// A cell of an items table: which column, from the header row above.
fn table_context<'t>(text: &str, line_start: usize, before: &'t str) -> Option<(Context, &'t str)> {
    // Table rows above, nearest first; the last one is the header.
    let above: Vec<&str> = text[..line_start]
        .lines()
        .rev()
        .take_while(|l| l.trim_start().starts_with('|'))
        .collect();
    // Need a header and a separator row above this one.
    if above.len() < 2 {
        return None;
    }
    let header = above[above.len() - 1];
    let column = split_cells(before).len().checked_sub(1)?;
    let name = split_cells(header).get(column)?.trim().to_ascii_lowercase();
    let name = name.split('(').next().unwrap_or("").trim().to_string();
    let context = match name.as_str() {
        "description" | "item" | "items" | "service" | "services" => Context::Description,
        "unit" | "units" => Context::Unit,
        _ => return None,
    };
    let cell = before.rsplit('|').next().unwrap_or("").trim_start();
    Some((context, cell))
}

/// Cells of a (possibly partial) table row after the leading pipe.
fn split_cells(row: &str) -> Vec<&str> {
    let t = row.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let mut cells: Vec<&str> = t.split('|').collect();
    if t.ends_with('|') {
        cells.pop();
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClientConfig;

    struct Fixture {
        cfg: Config,
        units: Vec<String>,
        descriptions: Vec<String>,
        numbers: Vec<String>,
    }

    impl Fixture {
        fn new() -> Self {
            let mut cfg = Config::default();
            let mut c = ClientConfig::default();
            c.party.name = "Northwind Labs".into();
            c.unit = Some("sessions".into());
            c.description = Some("Consulting services".into());
            cfg.clients.insert("northwind".into(), c);
            let mut c = ClientConfig::default();
            c.party.name = "Contoso".into();
            cfg.clients.insert("contoso".into(), c);
            Fixture {
                cfg,
                units: vec!["days".into(), "pages".into()],
                descriptions: vec!["Code review".into(), "Senior Software Engineer".into()],
                numbers: vec!["EINV-2026-01".into()],
            }
        }

        /// Complete at the `§` marker in `text`.
        fn at(&self, text: &str) -> Option<Completion> {
            let cursor = text.find('§').expect("cursor marker");
            let text = text.replace('§', "");
            let src = Sources {
                cfg: &self.cfg,
                today: NaiveDate::from_ymd_opt(2026, 11, 3).unwrap(),
                units: &self.units,
                descriptions: &self.descriptions,
                numbers: &self.numbers,
            };
            complete(&text, cursor, &src)
        }

        fn labels(&self, text: &str) -> Vec<String> {
            self.at(text)
                .map(|c| c.items.into_iter().map(|i| i.label).collect())
                .unwrap_or_default()
        }
    }

    #[test]
    fn top_level_keys_skip_present_ones() {
        let f = Fixture::new();
        let c = f.at("---\nstatus: paid\ns§\n---\n").unwrap();
        assert_eq!(c.context, Context::Key { parents: vec![] });
        assert_eq!(c.range, 17..18);
        let labels: Vec<_> = c.items.iter().map(|i| i.label.as_str()).collect();
        assert!(!labels.contains(&"status"), "{labels:?}");
        assert!(labels.contains(&"sent_on"));
        assert_eq!(c.items[0].insert, "signature: ");
        // Empty prefix: every missing key.
        let all = f.labels("---\n§\n---\n");
        assert_eq!(all.len(), schema::INVOICE.len());
    }

    #[test]
    fn nested_keys_and_block_inserts() {
        let f = Fixture::new();
        let c = f.at("---\nfrom:\n  name: A\n  e§\n---\n").unwrap();
        assert_eq!(
            c.context,
            Context::Key {
                parents: vec!["from".into()]
            }
        );
        let labels: Vec<_> = c.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["email", "endpoint"]);
        // Back at indent 0 after a block: top level again.
        assert!(
            f.labels("---\nfrom:\n  name: A\nto§\n---\n")
                .contains(&"to".to_string())
        );
        let c = f.at("---\nnumber_f§").unwrap();
        assert_eq!(c.items[0].insert, "number_format:\n  ");
        let c = f.at("---\nconfig:\n  clients:\n    §\n---\n").unwrap();
        let labels: Vec<_> = c.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["contoso", "northwind"]);
        // Not at a key when the line already has text after the cursor.
        assert!(f.at("---\nsta§tus: paid\n---\n").is_none());
    }

    #[test]
    fn values_by_key() {
        let f = Fixture::new();
        assert_eq!(f.labels("---\nclient: n§\n---\n"), vec!["northwind"]);
        assert_eq!(
            f.labels("---\nclient: §\n---\n"),
            vec!["contoso", "northwind"]
        );
        assert_eq!(f.labels("---\nstatus: p§\n---\n"), vec!["pending", "paid"]);
        assert_eq!(f.labels("---\nkind: c§"), vec!["credit_note"]);
        assert_eq!(f.labels("---\ntax_note: r§"), vec!["reverse_charge"]);
        assert_eq!(f.labels("---\nlanguage: d§"), vec!["de"]);
        assert_eq!(f.labels("---\nconfig:\n  e_invoice: §"), E_INVOICE.to_vec());
        assert_eq!(f.labels("---\ncurrency: ch§"), vec!["CHF"]);
        assert_eq!(f.labels("---\ndate: §"), vec!["2026-11-03"]);
        assert_eq!(
            f.labels("---\ndue: §"),
            vec!["2026-11-17", "2026-11-03", "false"]
        );
        assert_eq!(f.labels("---\nrefers_to: E§"), vec!["EINV-2026-01"]);
        assert_eq!(
            f.labels("---\nnumber_format:\n  currency: §"),
            vec!["symbol", "code"]
        );
        // Fully typed values are not offered again.
        assert!(f.labels("---\nstatus: paid§").is_empty());
        // Unknown keys and comments: nothing.
        assert!(f.labels("---\nnumber: §").is_empty());
        assert!(f.at("---\nstatus: paid # p§").is_none());
        // After the front matter, keys are not completed.
        assert!(f.at("---\nstatus: paid\n---\nsta§").is_none());
    }

    #[test]
    fn suggested_locales_are_valid() {
        for l in LOCALES {
            crate::render::locale::normalize_locale(l).unwrap();
        }
    }

    #[test]
    fn body_descriptions_and_units() {
        let f = Fixture::new();
        let table = "---\nx: 1\n---\n| Description | Qty | Unit | Rate |\n|---|---|---|---|\n";
        let c = f.at(&format!("{table}| Co§")).unwrap();
        assert_eq!(c.context, Context::Description);
        let labels: Vec<_> = c.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["Code review", "Consulting services"]);
        assert_eq!(f.labels(&format!("{table}| A | 1 | s§")), vec!["sessions"]);
        // Qty and rate columns: nothing to complete.
        assert!(f.at(&format!("{table}| A | 1§")).is_none());
        // The header row itself is not completed.
        assert!(f.at("| Desc§").is_none());

        assert_eq!(f.labels("- Sen§"), vec!["Senior Software Engineer"]);
        let c = f.at("- Review: 3 pa§").unwrap();
        assert_eq!(c.context, Context::Unit);
        assert_eq!(c.items[0].label, "pages");
        assert_eq!(
            f.labels("- Review: 3 §"),
            vec!["days", "pages", "sessions", "hours"]
        );
        assert!(f.at("- Review: 3 days @ 4§").is_none());
        assert!(f.at("Plain paragraph§").is_none());
    }
}
