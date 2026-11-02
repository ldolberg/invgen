//! Markdown + front matter -> Invoice. See docs/FORMAT.md for the format.

use crate::config::Config;
use crate::model::{Decimal, Invoice, LineItem, Party, Payment, Status, num};
use chrono::{Datelike, Duration, Local, NaiveDate};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_yaml_ng::{Mapping, Value};
use std::ops::Range;
use std::path::Path;

use crate::render::i18n;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct ParseError {
    /// 1-based line in the source markdown (0 if unknown).
    pub line: usize,
    pub message: String,
}

impl ParseError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        ParseError {
            line,
            message: message.into(),
        }
    }
}

type Result<T> = std::result::Result<T, ParseError>;

const DEFAULT_ACCENT: &str = "#111827";

/// Parse an invoice. `cfg` supplies defaults (sender, payment, clients, numbering).
pub fn parse_invoice(src: &str, cfg: &Config) -> Result<Invoice> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let lines = LineIndex::new(src);
    let doc = split_front_matter(src)?;
    let fm = FrontMatter::parse(&doc)?;
    let body = parse_body(&src[doc.body_start..], doc.body_start, &lines)?;

    // A `config:` block overrides any config key for this invoice only.
    let overridden;
    let cfg = match fm.get("config") {
        None | Some(Value::Null) => cfg,
        Some(Value::Mapping(over)) => {
            overridden =
                override_config(cfg, over).map_err(|m| ParseError::new(fm.line_of("config"), m))?;
            &overridden
        }
        Some(_) => {
            return Err(ParseError::new(
                fm.line_of("config"),
                "`config` must be a block of config keys (e.g. `config:` then `  due_days: 30`)",
            ));
        }
    };

    // Client defaults, then inline overrides.
    let client = match fm.str("client")? {
        Some(key) => Some(cfg.clients.get(&key).ok_or_else(|| {
            let known: Vec<&str> = cfg.clients.keys().map(String::as_str).collect();
            let hint = if known.is_empty() {
                format!("no clients are defined in {}", Config::path().display())
            } else {
                format!("known clients: {}", known.join(", "))
            };
            ParseError::new(
                fm.line_of("client"),
                format!("unknown client `{key}` ({hint})"),
            )
        })?),
        None => None,
    };

    let mut from: Party = fm.overlay("from", &cfg.from)?;
    from.logo = from.logo.as_deref().map(|p| cfg.resolve_path(p));
    let base_to = client.map(|c| c.party.clone()).unwrap_or_default();
    let to: Party = fm.overlay("to", &base_to)?;
    let payment: Payment = fm.overlay("payment", &cfg.payment)?;

    let date = fm
        .date("date")?
        .unwrap_or_else(|| Local::now().date_naive());
    let hide_due = match fm.get("due") {
        Some(Value::Bool(false)) => true,
        Some(Value::String(s)) => {
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "none" | "false" | "no"
            )
        }
        _ => false,
    };
    let due = if hide_due {
        None
    } else {
        match fm.date("due")? {
            Some(d) => Some(d),
            None => {
                let (days, line) = match fm.int("due_days")? {
                    Some(d) => (d, fm.line_of("due_days")),
                    None => (cfg.due_days, 0),
                };
                let due = Duration::try_days(days).and_then(|d| date.checked_add_signed(d));
                Some(due.ok_or_else(|| {
                    ParseError::new(line, format!("`due_days: {days}` is out of range"))
                })?)
            }
        }
    };
    let number = fm
        .str("number")?
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| cfg.next_number(date.year()));
    let status = match fm.str("status")? {
        None => Status::default(),
        Some(s) => s
            .parse()
            .map_err(|m: String| ParseError::new(fm.line_of("status"), m))?,
    };
    let paid_on = fm.date("paid_on")?;
    let currency = fm
        .str("currency")?
        .or_else(|| client.and_then(|c| c.currency.clone()))
        .unwrap_or_else(|| cfg.currency.clone())
        .trim()
        .to_uppercase();
    let tax_rate = fm
        .number("tax_rate")?
        .or_else(|| client.and_then(|c| c.tax_rate))
        .unwrap_or(cfg.tax_rate);
    let language = match fm.str("language")? {
        Some(l) => {
            i18n::normalize_language(&l).map_err(|m| ParseError::new(fm.line_of("language"), m))?
        }
        None => match client.and_then(|c| c.language.as_deref()) {
            Some(l) => i18n::normalize_language(l).map_err(|m| {
                ParseError::new(fm.line_of("client"), format!("client language: {m}"))
            })?,
            None => i18n::normalize_language(&cfg.language).map_err(|m| {
                let line = if fm.get("config").is_some() {
                    fm.line_of("config")
                } else {
                    0
                };
                ParseError::new(line, format!("config language: {m}"))
            })?,
        },
    }
    .to_string();
    let labels = cfg.labels.clone();
    let template = match fm.str("template")? {
        Some(t) if !t.trim().is_empty() => Some(cfg.resolve_path(Path::new(t.trim()))),
        _ => cfg.template.as_deref().map(|p| cfg.resolve_path(p)),
    };
    // `tax_note: false` (or empty) on the invoice drops a client/config note.
    let tax_note = match fm.get("tax_note") {
        Some(Value::Bool(false)) => None,
        Some(_) => fm.str("tax_note")?,
        None => client
            .and_then(|c| c.tax_note.clone())
            .or_else(|| cfg.tax_note.clone()),
    }
    .map(|n| n.trim().to_string())
    .filter(|n| !n.is_empty());
    let tax_label = fm
        .str("tax_label")?
        .unwrap_or_else(|| i18n::label(&language, "tax", &labels));
    let accent = match fm.str("accent")? {
        None => DEFAULT_ACCENT.to_string(),
        Some(a) => normalize_hex(&a).ok_or_else(|| {
            ParseError::new(
                fm.line_of("accent"),
                format!("`accent` must be a hex color like \"#111827\", got `{a}`"),
            )
        })?,
    };
    let default_signature = Some(from.name.clone()).filter(|n| !n.trim().is_empty());
    let signature = match fm.get("signature") {
        None | Some(Value::Null) | Some(Value::Bool(true)) => default_signature,
        Some(Value::Bool(false)) => None,
        Some(_) => fm.str("signature")?.filter(|s| !s.trim().is_empty()),
    };

    if body.items.is_empty() {
        return Err(ParseError::new(
            lines.line_of(doc.body_start),
            "no line items: add a table with Description, Qty and Rate columns, \
             or a list like `- Description: 21 days @ 450`",
        ));
    }

    let invoice = Invoice {
        number: number.trim().to_string(),
        date,
        due,
        status,
        paid_on,
        currency,
        from,
        to,
        payment,
        items: body.items,
        tax_rate,
        tax_label,
        tax_note,
        notes: body.notes,
        signature,
        accent,
        language,
        labels,
        template,
    };
    if invoice.checked_total().is_none() {
        return Err(ParseError::new(
            lines.line_of(doc.body_start),
            "amounts are too large to total",
        ));
    }
    Ok(invoice)
}

// ---------------------------------------------------------------------------
// Line bookkeeping

/// Maps byte offsets of the full source to 1-based line numbers.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(src: &str) -> Self {
        let starts = std::iter::once(0)
            .chain(src.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        LineIndex { starts }
    }

    fn line_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|&s| s <= offset).max(1)
    }
}

// ---------------------------------------------------------------------------
// Front matter

struct Document<'a> {
    /// YAML text between the `---` fences (empty if there is no front matter).
    yaml: &'a str,
    /// 1-based line number of the first YAML line.
    yaml_line: usize,
    /// Byte offset in the source where the markdown body starts.
    body_start: usize,
}

fn split_front_matter(src: &str) -> Result<Document<'_>> {
    let mut lines = src.split_inclusive('\n');
    let none = Document {
        yaml: "",
        yaml_line: 1,
        body_start: 0,
    };
    let Some(first) = lines.next() else {
        return Ok(none);
    };
    if first.trim_end() != "---" {
        return Ok(none);
    }
    let yaml_start = first.len();
    let mut offset = yaml_start;
    for line in lines {
        let fence = line.trim_end();
        if fence == "---" || fence == "..." {
            let body_start = offset + line.len();
            let body_line = src[..body_start].matches('\n').count() + 1;
            check_second_front_matter(&src[body_start..], body_line)?;
            return Ok(Document {
                yaml: &src[yaml_start..offset],
                yaml_line: 2,
                body_start,
            });
        }
        offset += line.len();
    }
    Err(ParseError::new(
        1,
        "front matter is not closed: add a `---` line after the YAML",
    ))
}

/// A second `---` block of `key: value` lines right after the front matter is
/// almost always two front matters pasted together. Markdown would silently turn
/// it into a rule plus a paragraph (or a heading), so reject it with a clear hint.
fn check_second_front_matter(body: &str, first_line: usize) -> Result<()> {
    let mut lines = body
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    let Some((fence_idx, fence)) = lines.next() else {
        return Ok(());
    };
    if fence.trim_end() != "---" {
        return Ok(());
    }
    let looks_like_key = |l: &str| {
        let l = l.trim_start_matches("# ");
        match l.split_once(':') {
            Some((key, _)) => {
                !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            }
            None => false,
        }
    };
    if lines.next().is_some_and(|(_, l)| looks_like_key(l)) {
        return Err(ParseError::new(
            first_line + fence_idx,
            "a second front matter block starts here; move its keys into the `---` block at the top (one block per file)",
        ));
    }
    Ok(())
}

struct FrontMatter<'a> {
    map: Mapping,
    yaml: &'a str,
    first_line: usize,
}

impl<'a> FrontMatter<'a> {
    fn parse(doc: &Document<'a>) -> Result<Self> {
        let value: Value = if doc.yaml.trim().is_empty() {
            Value::Null
        } else {
            serde_yaml_ng::from_str(doc.yaml).map_err(|e| {
                let line = e.location().map_or(doc.yaml_line, |l| {
                    doc.yaml_line + l.line().saturating_sub(1)
                });
                ParseError::new(line, format!("front matter: {e}"))
            })?
        };
        let map = match value {
            Value::Null => Mapping::new(),
            Value::Mapping(m) => m,
            _ => {
                return Err(ParseError::new(
                    doc.yaml_line,
                    "front matter must be `key: value` pairs",
                ));
            }
        };
        Ok(FrontMatter {
            map,
            yaml: doc.yaml,
            first_line: doc.yaml_line,
        })
    }

    /// Line of a top-level `key:` in the source, or the first front matter line.
    fn line_of(&self, key: &str) -> usize {
        self.yaml
            .lines()
            .position(|l| {
                l.strip_prefix(key)
                    .is_some_and(|rest| rest.trim_start().starts_with(':'))
            })
            .map_or(self.first_line, |i| self.first_line + i)
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key)
    }

    fn err(&self, key: &str, what: &str) -> ParseError {
        ParseError::new(self.line_of(key), format!("`{key}` must be {what}"))
    }

    fn str(&self, key: &str) -> Result<Option<String>> {
        match self.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(Value::Number(n)) => Ok(Some(n.to_string())),
            Some(Value::Bool(b)) => Ok(Some(b.to_string())),
            Some(_) => Err(self.err(key, "a single value")),
        }
    }

    fn number(&self, key: &str) -> Result<Option<Decimal>> {
        let n = match self.get(key) {
            None | Some(Value::Null) => return Ok(None),
            Some(Value::Number(n)) => match (n.as_i64(), n.as_u64()) {
                (Some(i), _) => Some(Decimal::from(i)),
                (_, Some(u)) => Some(Decimal::from(u)),
                _ => n.as_f64().and_then(num::from_f64),
            },
            Some(Value::String(s)) => num::from_text(s),
            Some(_) => None,
        };
        n.map(Some).ok_or_else(|| self.err(key, "a number"))
    }

    fn int(&self, key: &str) -> Result<Option<i64>> {
        match self.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(n)) => n
                .as_i64()
                .map(Some)
                .ok_or_else(|| self.err(key, "a whole number")),
            Some(Value::String(s)) => s
                .trim()
                .parse()
                .map(Some)
                .map_err(|_| self.err(key, "a whole number")),
            Some(_) => Err(self.err(key, "a whole number")),
        }
    }

    fn date(&self, key: &str) -> Result<Option<NaiveDate>> {
        match self.str(key)? {
            None => Ok(None),
            Some(s) => NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                .map(Some)
                .map_err(|_| self.err(key, "a date like 2026-11-02")),
        }
    }

    /// Replace fields of `base` with those given inline under `key`.
    fn overlay<T: Serialize + DeserializeOwned + Clone>(&self, key: &str, base: &T) -> Result<T> {
        let inline = match self.get(key) {
            None | Some(Value::Null) => return Ok(base.clone()),
            Some(Value::Mapping(m)) => m,
            Some(_) => return Err(self.err(key, "a block of fields (name, address, ...)")),
        };
        let mut merged = match serde_yaml_ng::to_value(base) {
            Ok(Value::Mapping(m)) => m,
            _ => Mapping::new(),
        };
        for (k, v) in inline {
            merged.insert(k.clone(), v.clone());
        }
        serde_yaml_ng::from_value(Value::Mapping(merged))
            .map_err(|e| ParseError::new(self.line_of(key), format!("`{key}`: {e}")))
    }
}

fn normalize_hex(s: &str) -> Option<String> {
    let hex = s.trim().strip_prefix('#')?;
    let ok = matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit());
    match (ok, hex.len()) {
        (true, 6) => Some(format!("#{}", hex.to_ascii_lowercase())),
        (true, 3) => Some(format!(
            "#{}",
            hex.chars()
                .flat_map(|c| [c, c])
                .collect::<String>()
                .to_ascii_lowercase()
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Body: markdown -> blocks

#[derive(Debug)]
struct ListEntry {
    line: usize,
    /// Text of the item itself; soft and hard line breaks are kept as '\n'.
    text: String,
    /// Plain text of each nested list item.
    children: Vec<String>,
}

#[derive(Debug)]
enum Block {
    Heading(String),
    Paragraph(String),
    Table {
        header_line: usize,
        header: Vec<String>,
        rows: Vec<(usize, Vec<String>)>,
    },
    List(Vec<ListEntry>),
    Other,
}

type Events<'a> =
    std::iter::Peekable<pulldown_cmark::OffsetIter<'a, pulldown_cmark::DefaultBrokenLinkCallback>>;

/// Lists nested deeper than this are rejected: list parsing recurses per level,
/// and pathological input (thousands of `- - - ...`) would overflow the stack.
const MAX_LIST_DEPTH: usize = 64;

struct BodyParser<'a, 'l> {
    events: Events<'a>,
    base: usize,
    lines: &'l LineIndex,
    /// Current list nesting level.
    depth: usize,
    /// Line of the first list nested deeper than `MAX_LIST_DEPTH`.
    too_deep: Option<usize>,
}

impl BodyParser<'_, '_> {
    fn line(&self, range: &Range<usize>) -> usize {
        self.lines.line_of(self.base + range.start)
    }

    fn blocks(mut self) -> Result<Vec<Block>> {
        let mut out = Vec::new();
        while let Some((event, range)) = self.events.next() {
            let block = match event {
                Event::Start(Tag::Heading { .. }) => {
                    Block::Heading(self.inline_text(TagEnd::Heading(heading_level(&event))))
                }
                Event::Start(Tag::Paragraph) => {
                    Block::Paragraph(self.inline_text(TagEnd::Paragraph))
                }
                Event::Start(Tag::Table(_)) => self.table(self.line(&range)),
                Event::Start(Tag::List(_)) => Block::List(self.list(self.line(&range))),
                Event::Start(_) => {
                    self.skip_to_end();
                    Block::Other
                }
                Event::Rule => Block::Other,
                // HTML comments, stray text etc. are invisible for data purposes.
                _ => continue,
            };
            out.push(block);
        }
        match self.too_deep {
            Some(line) => Err(ParseError::new(
                line,
                format!("lists are nested more than {MAX_LIST_DEPTH} levels deep"),
            )),
            None => Ok(out),
        }
    }

    /// Consume events up to and including the end of the current element.
    fn skip_to_end(&mut self) {
        let mut depth = 1usize;
        for (event, _) in self.events.by_ref() {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    /// Collect plain text until `end`, keeping line breaks as '\n'.
    fn inline_text(&mut self, end: TagEnd) -> String {
        let mut text = String::new();
        for (event, _) in self.events.by_ref() {
            match event {
                Event::End(e) if e == end => break,
                Event::Text(t) | Event::Code(t) => text.push_str(&t),
                Event::SoftBreak | Event::HardBreak => text.push('\n'),
                _ => {}
            }
        }
        text.trim().to_string()
    }

    fn table(&mut self, header_line: usize) -> Block {
        let mut header = Vec::new();
        let mut rows = Vec::new();
        while let Some((event, range)) = self.events.next() {
            match event {
                Event::Start(Tag::TableHead) => header = self.cells(TagEnd::TableHead),
                Event::Start(Tag::TableRow) => {
                    let line = self.line(&range);
                    rows.push((line, self.cells(TagEnd::TableRow)));
                }
                Event::End(TagEnd::Table) => break,
                _ => {}
            }
        }
        Block::Table {
            header_line,
            header,
            rows,
        }
    }

    fn cells(&mut self, end: TagEnd) -> Vec<String> {
        let mut cells = Vec::new();
        while let Some((event, _)) = self.events.next() {
            match event {
                Event::Start(Tag::TableCell) => cells.push(self.inline_text(TagEnd::TableCell)),
                Event::End(e) if e == end => break,
                _ => {}
            }
        }
        cells
    }

    /// Called right after `Start(List)` (at `line`); consumes through the matching end.
    fn list(&mut self, line: usize) -> Vec<ListEntry> {
        if self.depth >= MAX_LIST_DEPTH {
            self.too_deep.get_or_insert(line);
            self.skip_to_end();
            return Vec::new();
        }
        self.depth += 1;
        let entries = self.list_entries();
        self.depth -= 1;
        entries
    }

    fn list_entries(&mut self) -> Vec<ListEntry> {
        let mut entries = Vec::new();
        while let Some((event, range)) = self.events.next() {
            match event {
                Event::Start(Tag::Item) => {
                    let line = self.line(&range);
                    entries.push(self.item(line));
                }
                Event::End(TagEnd::List(_)) => break,
                _ => {}
            }
        }
        entries
    }

    fn item(&mut self, line: usize) -> ListEntry {
        let mut text = String::new();
        let mut children = Vec::new();
        while let Some((event, range)) = self.events.next() {
            match event {
                Event::End(TagEnd::Item) => break,
                Event::Text(t) | Event::Code(t) => text.push_str(&t),
                Event::SoftBreak | Event::HardBreak => text.push('\n'),
                Event::End(TagEnd::Paragraph) => text.push('\n'),
                Event::Start(Tag::List(_)) => children.extend(
                    self.list(self.line(&range))
                        .into_iter()
                        .map(|e| join_lines(&e.text))
                        .filter(|t| !t.is_empty()),
                ),
                _ => {}
            }
        }
        ListEntry {
            line,
            text: text.trim().to_string(),
            children,
        }
    }
}

fn heading_level(event: &Event) -> pulldown_cmark::HeadingLevel {
    match event {
        Event::Start(Tag::Heading { level, .. }) => *level,
        _ => pulldown_cmark::HeadingLevel::H1,
    }
}

/// Collapse line breaks into single spaces.
fn join_lines(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Body: blocks -> items and notes

#[derive(Debug, Default)]
struct Body {
    items: Vec<LineItem>,
    notes: Vec<String>,
}

fn parse_body(body: &str, base: usize, lines: &LineIndex) -> Result<Body> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let parser = BodyParser {
        events: Parser::new_ext(body, opts).into_offset_iter().peekable(),
        base,
        lines,
        depth: 0,
        too_deep: None,
    };
    let blocks = parser.blocks()?;
    let has_table = blocks.iter().any(|b| matches!(b, Block::Table { .. }));

    let mut out = Body::default();
    let mut in_notes = false;
    let mut after_heading = false;
    let mut items_seen = false;
    let mut table_used = false;
    // Index in `notes` of the paragraph right after a heading, before the items.
    let mut lead: Option<usize> = None;

    for block in blocks {
        let was_after_heading = std::mem::replace(&mut after_heading, false);
        match block {
            Block::Heading(text) => {
                in_notes = text
                    .trim()
                    .trim_end_matches(':')
                    .eq_ignore_ascii_case("notes");
                after_heading = !in_notes;
            }
            Block::Paragraph(text) => {
                let text = join_lines(&text);
                if text.is_empty() {
                    continue;
                }
                out.notes.push(text);
                if was_after_heading && !items_seen && lead.is_none() {
                    lead = Some(out.notes.len() - 1);
                }
            }
            Block::Table {
                header_line,
                header,
                rows,
            } => {
                if !table_used {
                    table_used = true;
                    items_seen = true;
                    out.items.extend(table_items(header_line, &header, &rows)?);
                }
            }
            Block::List(entries) => {
                let is_items = !has_table
                    && !in_notes
                    && entries
                        .first()
                        .is_some_and(|e| split_list_item(first_line(&e.text)).is_some());
                if is_items {
                    items_seen = true;
                    for entry in &entries {
                        out.items.push(list_item(entry)?);
                    }
                } else {
                    out.notes.extend(
                        entries
                            .iter()
                            .map(|e| join_lines(&e.text))
                            .filter(|t| !t.is_empty()),
                    );
                }
            }
            Block::Other => {}
        }
    }

    if let (Some(i), [item]) = (lead, out.items.as_mut_slice())
        && item.detail.is_none()
    {
        item.detail = Some(out.notes.remove(i));
    }
    Ok(out)
}

#[derive(Clone, Copy, PartialEq)]
enum Column {
    Description,
    Qty,
    Unit,
    Rate,
    Detail,
    Tax,
}

/// Header cell -> column. `Rate (EUR)` matches `rate`; `Days`/`Hours` are a
/// quantity column that also implies the unit.
fn classify_header(cell: &str) -> Option<(Column, Option<&'static str>)> {
    let name = cell
        .split('(')
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.');
    let name = name.to_ascii_lowercase();
    Some(match name.as_str() {
        "description" | "item" | "items" | "service" | "services" => (Column::Description, None),
        "qty" | "quantity" => (Column::Qty, None),
        "days" => (Column::Qty, Some("days")),
        "hours" => (Column::Qty, Some("hours")),
        "unit" | "units" => (Column::Unit, None),
        "rate" | "price" | "unit price" => (Column::Rate, None),
        "detail" | "details" => (Column::Detail, None),
        "tax" | "vat" | "tax rate" | "vat rate" | "tax %" | "vat %" => (Column::Tax, None),
        _ => return None,
    })
}

fn table_items(
    header_line: usize,
    header: &[String],
    rows: &[(usize, Vec<String>)],
) -> Result<Vec<LineItem>> {
    let mut implied_unit = None;
    let find = |col: Column| {
        header
            .iter()
            .position(|h| classify_header(h).is_some_and(|(c, _)| c == col))
    };
    let (Some(desc), Some(qty), Some(rate)) = (
        find(Column::Description),
        find(Column::Qty),
        find(Column::Rate),
    ) else {
        return Err(ParseError::new(
            header_line,
            format!(
                "items table needs Description, Qty and Rate columns (found: {})",
                header.join(", ")
            ),
        ));
    };
    if let Some((_, u)) = classify_header(&header[qty]) {
        implied_unit = u;
    }
    let unit_col = find(Column::Unit);
    let detail_col = find(Column::Detail);
    let tax_col = find(Column::Tax);

    let mut items = Vec::new();
    for (line, cells) in rows {
        let cell = |i: usize| cells.get(i).map(|s| join_lines(s)).unwrap_or_default();
        let opt_cell = |i: Option<usize>| i.map(cell).filter(|s| !s.is_empty());
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let description = cell(desc);
        if description.is_empty() {
            return Err(ParseError::new(*line, "item has no description"));
        }
        let (quantity, qty_unit) =
            parse_quantity(&cell(qty)).map_err(|m| ParseError::new(*line, m))?;
        let rate = parse_rate(&cell(rate)).map_err(|m| ParseError::new(*line, m))?;
        let unit = opt_cell(unit_col)
            .or((!qty_unit.is_empty()).then_some(qty_unit))
            .or(implied_unit.map(String::from))
            .unwrap_or_default();
        let tax_rate = opt_cell(tax_col)
            .map(|t| parse_tax_rate(&t))
            .transpose()
            .map_err(|m| ParseError::new(*line, m))?;
        items.push(LineItem {
            description,
            detail: opt_cell(detail_col),
            quantity,
            unit,
            rate,
            tax_rate,
        });
    }
    Ok(items)
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

/// `Desc: 21 days @ 450` -> ("Desc", "21 days", "450"). `x` works instead of `@`.
fn split_list_item(line: &str) -> Option<(&str, &str, &str)> {
    let (left, rate) = match line.rfind('@') {
        Some(i) => (&line[..i], &line[i + 1..]),
        None => {
            let i = line.to_ascii_lowercase().rfind(" x ")?;
            (&line[..i], &line[i + 3..])
        }
    };
    let colon = left.rfind(':')?;
    let desc = left[..colon].trim();
    (!desc.is_empty()).then(|| (desc, left[colon + 1..].trim(), rate.trim()))
}

/// `21 days @ 450, tax 10%` -> (`21 days @ 450`, Some(`10%`)). Also
/// `vat 10%`, `tax: 10%`; `,` or `;` before it. The suffix must come after
/// the `@` (or ` x `) and start with `tax`/`vat`, so commas in the
/// description or in a rate like `1,234.50` are left alone.
fn split_tax_suffix(line: &str) -> (&str, Option<&str>) {
    let Some(at) = line
        .rfind('@')
        .or_else(|| line.to_ascii_lowercase().rfind(" x "))
    else {
        return (line, None);
    };
    for (i, _) in line.rmatch_indices([',', ';']).filter(|&(i, _)| i > at) {
        let rest = line[i + 1..].trim_start();
        let lower = rest.to_ascii_lowercase();
        for kw in ["tax", "vat"] {
            if let Some(after) = lower.strip_prefix(kw)
                && after.starts_with([' ', ':'])
            {
                let value = rest[kw.len()..].trim_start_matches([' ', ':']).trim();
                return (line[..i].trim_end(), Some(value));
            }
        }
    }
    (line, None)
}

fn list_item(entry: &ListEntry) -> Result<LineItem> {
    let mut text_lines = entry.text.lines();
    let (head, tax) = split_tax_suffix(text_lines.next().unwrap_or(""));
    let err = |m: String| ParseError::new(entry.line, m);
    let tax_rate = tax.map(parse_tax_rate).transpose().map_err(err)?;
    let (desc, qty, rate) = split_list_item(head).ok_or_else(|| {
        err(format!(
            "`{head}` is not an item: write it as `Description: 21 days @ 450`"
        ))
    })?;
    let (quantity, unit) = parse_quantity(qty).map_err(err)?;
    let rate = parse_rate(rate).map_err(err)?;
    let mut detail_parts: Vec<String> = Vec::new();
    let rest = join_lines(&text_lines.collect::<Vec<_>>().join(" "));
    if !rest.is_empty() {
        detail_parts.push(rest);
    }
    detail_parts.extend(entry.children.iter().cloned());
    Ok(LineItem {
        description: desc.to_string(),
        detail: (!detail_parts.is_empty()).then(|| detail_parts.join("; ")),
        quantity,
        unit,
        rate,
        tax_rate,
    })
}

// ---------------------------------------------------------------------------
// Numbers

/// `21`, `21 days`, `7.5h`... -> (21, "days").
fn parse_quantity(s: &str) -> std::result::Result<(Decimal, String), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("missing quantity".into());
    }
    let end = s
        .char_indices()
        .find(|&(i, c)| {
            !(c.is_ascii_digit() || c == '.' || c == ',' || (i == 0 && (c == '-' || c == '+')))
        })
        .map_or(s.len(), |(i, _)| i);
    let number =
        parse_number(&s[..end]).ok_or_else(|| format!("quantity `{s}` is not a number"))?;
    Ok((number, s[end..].trim().to_string()))
}

fn parse_rate(s: &str) -> std::result::Result<Decimal, String> {
    if s.trim().is_empty() {
        return Err("missing rate".into());
    }
    parse_amount(s).map_err(|_| format!("rate `{}` is not a number", s.trim()))
}

/// A per-line tax rate: `21`, `21%`, `7.7 %`. Must not be negative.
fn parse_tax_rate(s: &str) -> std::result::Result<Decimal, String> {
    let t = s.trim();
    parse_number(t.strip_suffix('%').unwrap_or(t))
        .filter(|n| *n >= Decimal::ZERO)
        .ok_or_else(|| format!("tax rate `{t}` is not a percentage like `21%`"))
}

/// `1,234.50`, `EUR 450`, `€450`, `450 EUR`, `-50` -> exact decimal.
pub(crate) fn parse_amount(s: &str) -> std::result::Result<Decimal, ()> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let s = s
        .trim_start_matches(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.'))
        .trim_end_matches(|c: char| !c.is_ascii_digit());
    parse_number(s).map(|n| if neg { -n } else { n }).ok_or(())
}

/// Plain decimal with optional sign and `,` thousands separators in groups of three.
fn parse_number(s: &str) -> Option<Decimal> {
    let s = s.trim();
    let unsigned = s.strip_prefix(['-', '+']).unwrap_or(s);
    let (int, frac) = match unsigned.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (unsigned, None),
    };
    let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    if int.is_empty() && frac.is_none_or(str::is_empty) {
        return None;
    }
    if frac.is_some_and(|f| !digits(f)) {
        return None;
    }
    if int.contains(',') {
        let mut groups = int.split(',');
        let head = groups.next()?;
        if head.is_empty() || head.len() > 3 || !digits(head) {
            return None;
        }
        if !groups.all(|g| g.len() == 3 && digits(g)) {
            return None;
        }
    } else if !digits(int) {
        return None;
    }
    // `Decimal` rejects a leading `+` and a bare `.5` / `5.`, so spell them out.
    let neg = s.starts_with('-');
    let int = int.replace(',', "");
    let int = if int.is_empty() { "0" } else { &int };
    let text = match frac {
        Some(f) if !f.is_empty() => format!("{int}.{f}"),
        _ => int.to_string(),
    };
    let n = text.parse::<Decimal>().ok()?;
    Some(if neg { -n } else { n })
}

/// Deep-merge `over` onto `cfg`: mappings merge key by key (recursively),
/// anything else (scalars, lists, null) replaces the config value.
fn override_config(cfg: &Config, over: &Mapping) -> std::result::Result<Config, String> {
    fn merge(base: &mut Value, over: &Value) {
        match (base, over) {
            (Value::Mapping(b), Value::Mapping(o)) => {
                for (k, v) in o {
                    match b.get_mut(k) {
                        Some(slot) => merge(slot, v),
                        None => {
                            b.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            (slot, v) => *slot = v.clone(),
        }
    }
    let mut value = serde_yaml_ng::to_value(cfg).map_err(|e| format!("config: {e}"))?;
    merge(&mut value, &Value::Mapping(over.clone()));
    let mut merged: Config =
        serde_yaml_ng::from_value(value).map_err(|e| format!("config override: {e}"))?;
    merged.invoices_dir = crate::config::expand_tilde(&merged.invoices_dir);
    merged.dir = cfg.dir.clone();
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClientConfig;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn config_block_overrides_any_key() {
        let src = "---\nclient: northwind\nconfig:\n  due_days: 30\n  number_prefix: INV\n  tax_rate: 3\n  from:\n    name: Morgan Studio\n    phone: null\n  payment:\n    iban: XX00 0000\n  clients:\n    northwind:\n      currency: CHF\n      tax_rate: null\n      name: Northwind GmbH\n---\n- A: 1 @ 100\n";
        let inv = parse(src).unwrap();
        assert_eq!(inv.due, Some(inv.date + Duration::days(30)));
        assert!(inv.number.starts_with("INV-"));
        assert_eq!(inv.from.name, "Morgan Studio");
        assert_eq!(inv.from.phone, None);
        assert_eq!(inv.from.email.as_deref(), Some("alex@example.com"));
        assert_eq!(inv.payment.iban.as_deref(), Some("XX00 0000"));
        assert_eq!(inv.payment.bic.as_deref(), Some("COBADEFFXXX"));
        assert_eq!(inv.to.name, "Northwind GmbH");
        assert_eq!(inv.to.tax, vec!["Tax ID: B12345678".to_string()]);
        assert_eq!(inv.currency, "CHF");
        // client tax_rate removed, so the overridden config default applies
        assert_eq!(inv.tax_rate, d("3"));
    }

    #[test]
    fn config_block_errors_point_at_key() {
        let err = parse("---\ndate: 2026-01-01\nconfig: 5\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 3);
        let err = parse("---\ndate: 2026-01-01\nconfig:\n  due_days: soon\n---\n- A: 1 @ 1\n")
            .unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("config override"), "{}", err.message);
    }

    #[test]
    fn second_front_matter_block_is_an_error() {
        let src = "---\nclient: northwind\nconfig:\n  due_days: 30\n---\n\n---\nnumber: EINV-2026-24\ndate: 2026-11-02\n---\n\n- A: 1 @ 1\n";
        let err = parse(src).unwrap_err();
        assert_eq!(err.line, 7);
        assert!(
            err.message.contains("second front matter"),
            "{}",
            err.message
        );
        // a horizontal rule followed by prose is still fine
        let ok = "---\nclient: northwind\n---\n\n---\n\nThanks for your business.\n\n- A: 1 @ 1\n";
        assert!(parse(ok).is_ok());
    }

    /// Found by fuzzing: thousands of nested list markers overflowed the stack.
    #[test]
    fn deeply_nested_lists_are_an_error_not_a_stack_overflow() {
        let src = format!("---\n---\n- A: 1 @ 1\n\n{}B: 1 @ 1\n", "- ".repeat(5000));
        let err = parse(&src).unwrap_err();
        assert_eq!(err.line, 5);
        assert!(err.message.contains("nested"), "{}", err.message);

        let src: String = (0..3000)
            .map(|i| format!("{}- A: 1 @ 1\n", "  ".repeat(i)))
            .collect();
        assert_eq!(parse(&src).unwrap_err().line, 65);

        // a few levels are fine
        let ok = "- A: 1 @ 1\n  - detail\n    - more\n      - and more\n";
        assert_eq!(parse(ok).unwrap().items.len(), 1);
    }

    #[test]
    fn inline_overrides_beat_config_block() {
        let src = "---\nconfig:\n  from:\n    name: From Config Block\nfrom:\n  name: From Inline\n---\n- A: 1 @ 1\n";
        assert_eq!(parse(src).unwrap().from.name, "From Inline");
    }

    fn cfg() -> Config {
        let mut cfg = Config {
            invoices_dir: std::env::temp_dir().join("invgen-test-nonexistent-dir"),
            ..Config::default()
        };
        cfg.from = Party {
            name: "Alex Morgan".into(),
            address: vec!["Rue de la Gare".into(), "Luxembourg".into()],
            tax: vec!["VAT ID: LU12345678".into()],
            email: Some("alex@example.com".into()),
            phone: Some("+352 600 000 000".into()),
            logo: None,
        };
        cfg.payment = Payment {
            holder: Some("ALEX MORGAN".into()),
            iban: Some("DE89 3704 0044 0532 0130 00".into()),
            bic: Some("COBADEFFXXX".into()),
            notes: vec![],
        };
        cfg.clients.insert(
            "northwind".into(),
            ClientConfig {
                party: Party {
                    name: "Northwind Labs SL".into(),
                    address: vec!["Calle Mayor, 12".into(), "Madrid".into()],
                    tax: vec!["Tax ID: B12345678".into()],
                    ..Party::default()
                },
                currency: Some("USD".into()),
                tax_rate: Some(d("21")),
                rate: Some(d("450")),
                unit: Some("days".into()),
                description: Some("Senior Software Engineer".into()),
                language: None,
                tax_note: None,
            },
        );
        cfg.clients.insert("acme".into(), ClientConfig::default());
        cfg
    }

    fn parse(src: &str) -> Result<Invoice> {
        parse_invoice(src, &cfg())
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    const SPEC_EXAMPLE: &str = r##"---
number: EINV-2026-23          # optional: auto = next number after the highest in the invoices dir
date: 2026-11-02              # optional: defaults to today
due: 2026-11-16               # optional: or `due_days: 14`
status: pending               # pending | paid | overdue | draft
client: northwind       # key into config `clients:`  ...or an inline `to:` block
currency: EUR                 # optional: falls back to client currency, then config default
tax_rate: 0                   # percent; optional, falls back to client, then config
tax_label: VAT                # optional
accent: "#111827"             # optional
---

# Senior Software Engineer
October 2026 period

| Description              | Qty | Unit | Rate   |
|--------------------------|-----|------|--------|
| Senior Software Engineer | 21  | days | 450.00 |

Any other paragraph becomes a note under the table.
"##;

    #[test]
    fn spec_example() {
        let inv = parse(SPEC_EXAMPLE).unwrap();
        assert_eq!(inv.number, "EINV-2026-23");
        assert_eq!(inv.date, date(2026, 11, 2));
        assert_eq!(inv.due, Some(date(2026, 11, 16)));
        assert_eq!(inv.status, Status::Pending);
        assert_eq!(inv.currency, "EUR");
        assert_eq!(inv.tax_rate, d("0"));
        assert_eq!(inv.tax_label, "VAT");
        assert_eq!(inv.accent, "#111827");
        assert_eq!(inv.to.name, "Northwind Labs SL");
        assert_eq!(inv.from.name, "Alex Morgan");
        assert_eq!(inv.payment.bic.as_deref(), Some("COBADEFFXXX"));
        assert_eq!(inv.signature.as_deref(), Some("Alex Morgan"));
        assert_eq!(
            inv.items,
            vec![LineItem {
                description: "Senior Software Engineer".into(),
                detail: Some("October 2026 period".into()),
                quantity: d("21"),
                unit: "days".into(),
                rate: d("450"),
                tax_rate: None,
            }]
        );
        assert_eq!(
            inv.notes,
            vec!["Any other paragraph becomes a note under the table."]
        );
        assert_eq!(inv.total(), d("9450"));
    }

    #[test]
    fn no_front_matter_uses_defaults() {
        let inv = parse("| Item | Quantity | Price |\n|---|---|---|\n| Work | 2 | 10 |\n").unwrap();
        assert_eq!(inv.currency, "EUR");
        assert_eq!(inv.number, format!("EINV-{}-01", inv.date.year()));
        assert_eq!(inv.due, Some(inv.date + Duration::days(14)));
        assert_eq!(inv.to, Party::default());
        assert_eq!(inv.items[0].unit, "");
        assert_eq!(inv.signature.as_deref(), Some("Alex Morgan"));
    }

    #[test]
    fn merge_precedence() {
        // client beats config
        let inv = parse("---\nclient: northwind\n---\n- A: 1 @ 1\n").unwrap();
        assert_eq!(inv.currency, "USD");
        assert_eq!(inv.tax_rate, d("21"));
        // front matter beats client
        let inv = parse("---\nclient: northwind\ncurrency: chf\ntax_rate: 7.7\n---\n- A: 1 @ 1\n")
            .unwrap();
        assert_eq!(inv.currency, "CHF");
        assert_eq!(inv.tax_rate, d("7.7"));
        // config default when the client has none
        let inv = parse("---\nclient: acme\n---\n- A: 1 @ 1\n").unwrap();
        assert_eq!(inv.currency, "EUR");
        assert_eq!(inv.tax_rate, d("0"));
    }

    #[test]
    fn inline_parties_override_field_by_field() {
        let src = "---\nclient: northwind\nto:\n  name: Other SL\n  email: a@b.c\nfrom:\n  phone: \"+1\"\npayment:\n  iban: XX00\n---\n- A: 1 @ 1\n";
        let inv = parse(src).unwrap();
        assert_eq!(inv.to.name, "Other SL");
        assert_eq!(inv.to.tax, vec!["Tax ID: B12345678"]);
        assert_eq!(inv.to.email.as_deref(), Some("a@b.c"));
        assert_eq!(inv.from.name, "Alex Morgan");
        assert_eq!(inv.from.phone.as_deref(), Some("+1"));
        assert_eq!(inv.payment.iban.as_deref(), Some("XX00"));
        assert_eq!(inv.payment.bic.as_deref(), Some("COBADEFFXXX"));
    }

    #[test]
    fn due_days_signature_accent_status() {
        let src = "---\ndate: 2026-01-30\ndue_days: 5\nsignature: false\naccent: \"#ABC\"\nstatus: Paid\nunknown: whatever\n---\n- A: 1 @ 1\n";
        let inv = parse(src).unwrap();
        assert_eq!(inv.due, Some(date(2026, 2, 4)));
        assert_eq!(inv.signature, None);
        assert_eq!(inv.accent, "#aabbcc");
        assert_eq!(inv.status, Status::Paid);
        assert_eq!(inv.paid_on, None);

        let inv = parse("---\nstatus: paid\npaid_on: 2026-02-10\n---\n- A: 1 @ 1\n").unwrap();
        assert_eq!(inv.paid_on, Some(date(2026, 2, 10)));

        let inv = parse("---\nsignature: L. D.\n---\n- A: 1 @ 1\n").unwrap();
        assert_eq!(inv.signature.as_deref(), Some("L. D."));
    }

    #[test]
    fn list_form_with_details() {
        let src = "\
# October

- Senior Software Engineer: 21 days @ 450
  October 2026 period
- Hosting: 1 x EUR 1,234.50
  - Server rental
- Discount: 1 @ -50

Thanks!
";
        let inv = parse(src).unwrap();
        assert_eq!(inv.items.len(), 3);
        assert_eq!(inv.items[0].description, "Senior Software Engineer");
        assert_eq!(inv.items[0].quantity, d("21"));
        assert_eq!(inv.items[0].unit, "days");
        assert_eq!(inv.items[0].rate, d("450"));
        assert_eq!(inv.items[0].detail.as_deref(), Some("October 2026 period"));
        assert_eq!(inv.items[1].rate, d("1234.5"));
        assert_eq!(inv.items[1].unit, "");
        assert_eq!(inv.items[1].detail.as_deref(), Some("Server rental"));
        assert_eq!(inv.items[2].rate, d("-50"));
        assert_eq!(inv.items[2].detail, None);
        assert_eq!(inv.notes, vec!["Thanks!"]);
    }

    #[test]
    fn table_variants() {
        let src = "\
| Item | Qty | Rate (EUR) | Total | Detail |
|------|-----|------------|-------|--------|
| Dev  | 21 days | €450 | 9,450 | Oct |
| Ops  | 2.5 | EUR 1,234.50 | | |
|      |     |      |  | |
| Fee  | 1 | 99 EUR | | |
";
        let inv = parse(src).unwrap();
        assert_eq!(inv.items.len(), 3);
        assert_eq!(
            (inv.items[0].quantity, inv.items[0].unit.as_str()),
            (d("21"), "days")
        );
        assert_eq!(inv.items[0].rate, d("450"));
        assert_eq!(inv.items[0].detail.as_deref(), Some("Oct"));
        assert_eq!(inv.items[1].rate, d("1234.5"));
        assert_eq!(inv.items[1].quantity, d("2.5"));
        assert_eq!(inv.items[2].rate, d("99"));

        let inv = parse("| Description | Hours | Rate |\n|-|-|-|\n| X | 3 | 10 |\n").unwrap();
        assert_eq!(inv.items[0].unit, "hours");
    }

    #[test]
    fn lead_paragraph_is_note_with_several_items() {
        let src = "# Work\nOctober 2026\n\n| Description | Qty | Rate |\n|-|-|-|\n| A | 1 | 1 |\n| B | 1 | 1 |\n";
        let inv = parse(src).unwrap();
        assert!(inv.items.iter().all(|i| i.detail.is_none()));
        assert_eq!(inv.notes, vec!["October 2026"]);
    }

    #[test]
    fn notes_heading_and_inline_markup() {
        let src = "\
| Description | Qty | Rate |
|-|-|-|
| A | 1 | 1 |

## Notes

Pay **within** `14` days,
please.

- Reverse charge: 1 @ 0
- Second bullet
";
        let inv = parse(src).unwrap();
        assert_eq!(inv.items.len(), 1);
        assert_eq!(
            inv.notes,
            vec![
                "Pay within 14 days, please.",
                "Reverse charge: 1 @ 0",
                "Second bullet"
            ]
        );
    }

    #[test]
    fn error_unknown_client_lists_known() {
        let err = parse("---\ndate: 2026-01-01\nclient: nope\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("acme, northwind"), "{}", err.message);
    }

    #[test]
    fn error_no_items() {
        let err = parse("---\nclient: acme\n---\n\nJust a note.\n").unwrap_err();
        assert_eq!(err.line, 4);
        assert!(err.message.contains("no line items"));
        assert!(parse("").is_err());
    }

    #[test]
    fn error_bad_numbers_point_at_line() {
        let src = "---\nclient: acme\n---\n\n| Description | Qty | Rate |\n|-|-|-|\n| A | 1 | 1 |\n| B | many | 1 |\n";
        let err = parse(src).unwrap_err();
        assert_eq!(err.line, 8);
        assert!(err.message.contains("many"), "{}", err.message);

        let src = "---\nclient: acme\n---\n- A: 1 @ 1\n- B: 2 days @ lots\n";
        let err = parse(src).unwrap_err();
        assert_eq!(err.line, 5);
        assert!(err.message.contains("lots"), "{}", err.message);

        let src = "- A: 1 @ 1\n- B 2 days\n";
        assert_eq!(parse(src).unwrap_err().line, 2);

        let src = "| Description | Qty | Rate |\n|-|-|-|\n| A | 1 | 1.234,50 |\n";
        assert_eq!(parse(src).unwrap_err().line, 3);
    }

    #[test]
    fn error_table_missing_columns() {
        let err = parse("intro\n\n| Description | Rate |\n|-|-|\n| A | 1 |\n").unwrap_err();
        assert_eq!(err.line, 3);
    }

    #[test]
    fn yaml_errors_map_to_file_lines() {
        let err = parse("---\nnumber: X\ndate: [oops\n---\n- A: 1 @ 1\n").unwrap_err();
        assert!(err.line >= 3 && err.line <= 4, "{err}");
        let err = parse("---\nnumber: X\n  bad: indent\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 3, "{err}");
        let err = parse("---\nnumber: X\ndate: 2026-13-01\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 3);
        let err = parse("---\nstatus: unpaid\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 2);
        let err = parse("---\naccent: red\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 2);
        let err = parse("---\nstatus: paid\npaid_on: last week\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("`paid_on` must be a date"), "{err}");
        let err = parse("---\nnumber: X\n").unwrap_err();
        assert_eq!(err.line, 1);
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_number("1,234.50"), Some(d("1234.5")));
        assert_eq!(parse_number("1234"), Some(d("1234")));
        assert_eq!(parse_number(".5"), Some(d("0.5")));
        assert_eq!(parse_number("12,34"), None);
        assert_eq!(parse_number("1.2.3"), None);
        assert_eq!(parse_number(""), None);
        assert_eq!(parse_amount("€450"), Ok(d("450")));
        assert_eq!(parse_amount("EUR 450"), Ok(d("450")));
        assert_eq!(parse_amount("$ 1,000,000.00"), Ok(d("1000000")));
        assert_eq!(parse_amount("-€50"), Ok(d("-50")));
        assert_eq!(parse_amount("abc"), Err(()));
        assert_eq!(parse_quantity("21 days"), Ok((d("21"), "days".into())));
        assert_eq!(parse_quantity("7.5h"), Ok((d("7.5"), "h".into())));
        assert!(parse_quantity("days").is_err());
    }

    #[test]
    fn numbers_are_exact_decimals() {
        // Kept digit for digit, with no binary noise.
        assert_eq!(parse_number("1.005").unwrap().to_string(), "1.005");
        assert_eq!(parse_number("0.1").unwrap().to_string(), "0.1");
        assert_eq!(parse_number("+5."), Some(d("5")));
        assert_eq!(parse_number("-.25"), Some(d("-0.25")));
        assert_eq!(parse_number("-0"), Some(Decimal::ZERO));
        // Too many digits for `Decimal` is not a number rather than a panic.
        assert_eq!(parse_number("100000000000000000000000000000000"), None);
        assert_eq!(parse_amount("EUR 1,234.56"), Ok(d("1234.56")));

        let inv =
            parse("---\ntax_rate: 7.7\n---\n- A: 7.5 hours @ 133.33\n- B: 0.5 @ 2.01\n").unwrap();
        assert_eq!(inv.items[0].total(), d("999.98"));
        assert_eq!(inv.items[1].total(), d("1.01"));
        assert_eq!(inv.subtotal(), d("1000.99"));
        assert_eq!(inv.tax(), d("77.08"));
        assert_eq!(inv.total(), d("1078.07"));
    }

    #[test]
    fn tax_rate_forms() {
        for (yaml, want) in [
            ("21", "21"),
            ("21.5", "21.5"),
            ("\"21%\"", "21"),
            ("\"7.7 %\"", "7.7"),
            ("0", "0"),
        ] {
            let inv = parse(&format!("---\ntax_rate: {yaml}\n---\n- A: 1 @ 1\n")).unwrap();
            assert_eq!(inv.tax_rate, d(want), "{yaml}");
        }
        for bad in [".nan", ".inf", "abc", "[1]"] {
            let err = parse(&format!("---\ntax_rate: {bad}\n---\n- A: 1 @ 1\n")).unwrap_err();
            assert_eq!(err.message, "`tax_rate` must be a number", "{bad}");
        }
    }

    #[test]
    fn overflowing_amounts_are_an_error() {
        let big = "9".repeat(20);
        let err = parse(&format!("---\nnumber: X\n---\n- A: {big} @ {big}\n")).unwrap_err();
        assert_eq!(err.message, "amounts are too large to total");
    }

    #[test]
    fn number_auto_uses_config_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("EINV-2025-13.pdf"), "").unwrap();
        let mut c = cfg();
        c.invoices_dir = dir.path().to_path_buf();
        let inv = parse_invoice("---\ndate: 2026-03-01\n---\n- A: 1 @ 1\n", &c).unwrap();
        assert_eq!(inv.number, "EINV-2026-14");
    }

    #[test]
    fn due_can_be_hidden() {
        for v in ["false", "none", "None", "\"false\""] {
            let inv = parse(&format!(
                "---\ndate: 2026-03-01\ndue: {v}\n---\n- A: 1 @ 1\n"
            ))
            .unwrap();
            assert_eq!(inv.due, None, "due: {v}");
        }
        let inv = parse("---\ndate: 2026-03-01\ndue_days: 0\n---\n- A: 1 @ 1\n").unwrap();
        assert_eq!(inv.due, Some(date(2026, 3, 1)));
    }

    #[test]
    fn huge_due_days_is_an_error_not_a_panic() {
        for days in ["99999999999", "-99999999999", "9223372036854775807"] {
            let err = parse(&format!(
                "---\ndate: 2026-03-01\ndue_days: {days}\n---\n- A: 1 @ 1\n"
            ))
            .unwrap_err();
            assert_eq!(err.line, 3, "{err}");
        }
    }

    #[test]
    fn language_precedence_front_matter_client_config() {
        let mut c = cfg();
        c.language = "de".into();
        let src = "---\nclient: northwind\n---\n- A: 1 @ 1\n";
        let inv = parse_invoice(src, &c).unwrap();
        assert_eq!(inv.language, "de");
        assert_eq!(inv.tax_label, "USt.");

        c.clients.get_mut("northwind").unwrap().language = Some("ES".into());
        let inv = parse_invoice(src, &c).unwrap();
        assert_eq!(inv.language, "es");
        assert_eq!(inv.tax_label, "IVA");

        let src = "---\nclient: northwind\nlanguage: fr-BE\n---\n- A: 1 @ 1\n";
        let inv = parse_invoice(src, &c).unwrap();
        assert_eq!(inv.language, "fr");
        assert_eq!(inv.tax_label, "TVA");

        // An explicit tax_label still wins over the translated default.
        let src = "---\nlanguage: es\ntax_label: IGIC\n---\n- A: 1 @ 1\n";
        assert_eq!(parse_invoice(src, &c).unwrap().tax_label, "IGIC");

        // Default is English.
        assert_eq!(parse("- A: 1 @ 1\n").unwrap().language, "en");
        assert_eq!(parse("- A: 1 @ 1\n").unwrap().tax_label, "VAT");
    }

    #[test]
    fn unknown_language_is_an_error_listing_supported() {
        let err = parse("---\ndate: 2026-03-01\nlanguage: klingon\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("klingon"), "{err}");
        assert!(err.message.contains("en, es, de, fr, it, pt, nl"), "{err}");

        let mut c = cfg();
        c.clients.get_mut("northwind").unwrap().language = Some("xx".into());
        let err = parse_invoice("---\nclient: northwind\n---\n- A: 1 @ 1\n", &c).unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("`xx`"), "{err}");

        let err = parse("---\nconfig:\n  language: zz\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("`zz`"), "{err}");
    }

    #[test]
    fn labels_come_from_config_and_config_block() {
        let mut c = cfg();
        c.labels.insert("bill_to".into(), "Customer".into());
        let inv = parse_invoice("- A: 1 @ 1\n", &c).unwrap();
        assert_eq!(inv.labels["bill_to"], "Customer");
        c.labels.insert("tax".into(), "Sales tax".into());
        let src = "---\nconfig:\n  labels:\n    notes: Remarks\n---\n- A: 1 @ 1\n";
        let inv = parse_invoice(src, &c).unwrap();
        assert_eq!(inv.labels["bill_to"], "Customer");
        assert_eq!(inv.labels["notes"], "Remarks");
        // A `tax` label override is the default tax label.
        assert_eq!(inv.tax_label, "Sales tax");
    }

    #[test]
    fn logo_and_template_paths_resolve_against_config_dir() {
        let mut c = cfg();
        c.dir = Some("/cfg/dir".into());
        c.from.logo = Some("brand/logo.svg".into());
        c.template = Some("tpl/invoice.typ".into());
        let inv = parse_invoice("- A: 1 @ 1\n", &c).unwrap();
        assert_eq!(
            inv.from.logo.as_deref(),
            Some(Path::new("/cfg/dir/brand/logo.svg"))
        );
        assert_eq!(
            inv.template.as_deref(),
            Some(Path::new("/cfg/dir/tpl/invoice.typ"))
        );

        // Front matter overrides, via `from:`, `config:` and `template:`.
        let src = "---\nfrom:\n  logo: /abs/other.png\ntemplate: ~/t.typ\n---\n- A: 1 @ 1\n";
        let inv = parse_invoice(src, &c).unwrap();
        assert_eq!(inv.from.logo.as_deref(), Some(Path::new("/abs/other.png")));
        assert_eq!(
            inv.template.as_deref(),
            Some(crate::config::expand_tilde(Path::new("~/t.typ")).as_path())
        );
        assert!(!inv.template.as_ref().unwrap().starts_with("~"));

        let src = "---\nconfig:\n  from:\n    logo: x.png\n  template: y.typ\n---\n- A: 1 @ 1\n";
        let inv = parse_invoice(src, &c).unwrap();
        assert_eq!(inv.from.logo.as_deref(), Some(Path::new("/cfg/dir/x.png")));
        assert_eq!(inv.template.as_deref(), Some(Path::new("/cfg/dir/y.typ")));

        // `logo: null` removes the config logo for one invoice.
        let src = "---\nfrom:\n  logo: null\n---\n- A: 1 @ 1\n";
        assert_eq!(parse_invoice(src, &c).unwrap().from.logo, None);

        // No logo and no template by default.
        let inv = parse("- A: 1 @ 1\n").unwrap();
        assert_eq!(inv.from.logo, None);
        assert_eq!(inv.template, None);
    }

    #[test]
    fn tax_column_sets_per_line_rates() {
        let src = "---\ntax_rate: 21\n---\n\
                   | Description | Qty | Rate | VAT (%) |\n|---|---|---|---|\n\
                   | Consulting | 2 | 450 | |\n| Training | 1 | 600 | 10% |\n\
                   | Books | 3 | 40 | 0 |\n| Food | 1 | 10 | 5.5 % |\n";
        let inv = parse(src).unwrap();
        let rates: Vec<_> = inv.items.iter().map(|it| it.tax_rate).collect();
        assert_eq!(
            rates,
            vec![None, Some(d("10")), Some(d("0")), Some(d("5.5"))]
        );
        assert_eq!(inv.item_tax_rate(&inv.items[0]), d("21"));
        assert_eq!(inv.tax_breakdown().len(), 4);
        // 189 + 60 + 0 + 0.55
        assert_eq!(inv.tax(), d("249.55"));
        assert_eq!(inv.total(), d("1630") + d("249.55"));

        for header in ["Tax", "tax rate", "VAT %", "Tax (%)"] {
            let src = format!(
                "| Description | Qty | Rate | {header} |\n|---|---|---|---|\n| A | 1 | 100 | 7 |\n"
            );
            assert_eq!(
                parse(&src).unwrap().items[0].tax_rate,
                Some(d("7")),
                "{header}"
            );
        }

        let err = parse(
            "\n| Description | Qty | Rate | Tax |\n|---|---|---|---|\n| A | 1 | 1 | lots |\n",
        )
        .unwrap_err();
        assert_eq!(err.line, 4);
        assert!(err.message.contains("tax rate `lots`"), "{}", err.message);
        let err =
            parse("| Description | Qty | Rate | Tax |\n|---|---|---|---|\n| A | 1 | 1 | -5% |\n")
                .unwrap_err();
        assert!(err.message.contains("tax rate"), "{}", err.message);
    }

    #[test]
    fn list_items_take_a_tax_suffix() {
        let src = "- Consulting: 21 days @ 450, tax 10%\n\
                   - Books: 2 x 1,234.50; vat: 0%\n\
                   - Support: 3 hours @ EUR 80 , TAX 21\n\
                   - Plain: 1 @ 1,000\n\
                   - Note, taxi: 1 @ 5\n";
        let inv = parse(src).unwrap();
        let got: Vec<_> = inv
            .items
            .iter()
            .map(|it| (it.description.as_str(), it.rate, it.tax_rate))
            .collect();
        assert_eq!(
            got,
            vec![
                ("Consulting", d("450"), Some(d("10"))),
                ("Books", d("1234.50"), Some(d("0"))),
                ("Support", d("80"), Some(d("21"))),
                ("Plain", d("1000"), None),
                ("Note, taxi", d("5"), None),
            ]
        );
        assert_eq!(inv.items[0].unit, "days");
        let err = parse("- A: 1 @ 1, tax many\n").unwrap_err();
        assert!(err.message.contains("tax rate `many`"), "{}", err.message);
        assert_eq!(split_tax_suffix("A: 1 @ 1"), ("A: 1 @ 1", None));
        assert_eq!(
            split_tax_suffix("Hotel, tax: 1 @ 5"),
            ("Hotel, tax: 1 @ 5", None)
        );
        assert_eq!(
            split_tax_suffix("A: 1 @ 1,tax:7%"),
            ("A: 1 @ 1", Some("7%"))
        );
    }

    #[test]
    fn single_rate_invoices_are_unchanged() {
        // No tax column or suffix: the invoice rate applies to everything,
        // exactly as before per-line rates existed.
        let inv =
            parse("---\ntax_rate: 7.7\n---\n- A: 7.5 hours @ 133.33\n- B: 1 @ 0.99\n").unwrap();
        assert!(inv.items.iter().all(|it| it.tax_rate.is_none()));
        let groups = inv.tax_breakdown();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].base, inv.subtotal());
        assert_eq!(
            inv.tax(),
            crate::model::round_cents(inv.subtotal() * d("7.7") / d("100"))
        );
    }

    #[test]
    fn tax_note_precedence() {
        let mut c = cfg();
        assert_eq!(parse_invoice("- A: 1 @ 1\n", &c).unwrap().tax_note, None);
        // Config default.
        c.tax_note = Some("exempt".into());
        let note = |src: &str, c: &Config| parse_invoice(src, c).unwrap().tax_note;
        assert_eq!(note("- A: 1 @ 1\n", &c).as_deref(), Some("exempt"));
        // Client beats config; a client without a note falls back to it.
        c.clients.get_mut("northwind").unwrap().tax_note = Some("reverse_charge".into());
        assert_eq!(
            note("---\nclient: northwind\n---\n- A: 1 @ 1\n", &c).as_deref(),
            Some("reverse_charge")
        );
        assert_eq!(
            note("---\nclient: acme\n---\n- A: 1 @ 1\n", &c).as_deref(),
            Some("exempt")
        );
        // Front matter beats both; free text is kept (trimmed).
        let src = "---\nclient: northwind\ntax_note: \"  Exempt under Article 132.  \"\n---\n- A: 1 @ 1\n";
        assert_eq!(note(src, &c).as_deref(), Some("Exempt under Article 132."));
        // `false` or an empty value drops the inherited note.
        for v in ["false", "\"\""] {
            let src = format!("---\nclient: northwind\ntax_note: {v}\n---\n- A: 1 @ 1\n");
            assert_eq!(note(&src, &c), None, "{v}");
        }
        // A `config:` block can set it too.
        let src = "---\nconfig:\n  tax_note: outside_scope\n---\n- A: 1 @ 1\n";
        assert_eq!(note(src, &cfg()).as_deref(), Some("outside_scope"));
        let err = parse("---\ntax_note: [a, b]\n---\n- A: 1 @ 1\n").unwrap_err();
        assert_eq!(err.line, 2);
    }
}
