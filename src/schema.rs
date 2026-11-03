//! What the invoice front matter may contain, and a forgiving line-based view
//! of it. Shared by the live check ([`crate::lint`]) and the editor's
//! autocomplete ([`crate::complete`]); both must work on half-typed text that
//! is not valid YAML yet, so this never calls the YAML parser.

/// A front matter key: its name, whether it holds a block of nested keys, and
/// a one-line description (shown next to completions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub name: &'static str,
    pub block: bool,
    pub doc: &'static str,
}

const fn k(name: &'static str, doc: &'static str) -> Key {
    Key {
        name,
        block: false,
        doc,
    }
}

const fn b(name: &'static str, doc: &'static str) -> Key {
    Key {
        name,
        block: true,
        doc,
    }
}

/// Top-level keys of an invoice (docs/FORMAT.md).
pub const INVOICE: &[Key] = &[
    k("number", "invoice number (default: next free one)"),
    k("date", "issue date, YYYY-MM-DD (default: today)"),
    k("due", "due date, or false to hide it"),
    k("due_days", "days until due (default from config)"),
    k("status", "pending, paid, overdue or draft"),
    k("paid_on", "date it was paid"),
    k("client", "key into config clients"),
    k("currency", "ISO currency code"),
    k("tax_rate", "tax in percent"),
    k("tax_label", "label of the tax line"),
    k("tax_note", "legal tax mention (preset or text)"),
    k("accent", "hex color like \"#111827\""),
    k("language", "label language"),
    k("locale", "number and date format, e.g. de-CH"),
    k("date_format", "strftime pattern for dates"),
    b("number_format", "thousands, decimal, currency..."),
    k("template", "custom typst template"),
    k("buyer_reference", "e-invoicing buyer reference"),
    k("signature", "name under the invoice, or false"),
    k("kind", "invoice or credit_note"),
    k("refers_to", "invoice a credit note reverses"),
    k("refers_to_date", "date of that invoice"),
    b("from", "override your details"),
    b("to", "inline client details"),
    b("payment", "override payment details"),
    b("config", "override any config key"),
    k("sent_on", "date it was sent (older versions)"),
    k("issued_hash", "lock written on first export"),
];

/// Fields of `from:`, `to:` and a config client.
pub const PARTY: &[Key] = &[
    k("name", "name"),
    k("address", "address lines, [..]"),
    k("tax", "tax lines, [..]"),
    k("email", "email address"),
    k("phone", "phone number"),
    k("logo", "logo image path"),
    k("vat_id", "VAT ID with country prefix"),
    k("legal_id", "company registration number"),
    k("country", "ISO country code"),
    k("endpoint", "Peppol address"),
    k("buyer_reference", "e-invoicing buyer reference"),
];

/// Fields of `payment:`.
pub const PAYMENT: &[Key] = &[
    k("holder", "account holder"),
    k("iban", "IBAN"),
    k("bic", "BIC"),
    k("notes", "payment notes, [..]"),
    k("qr", "EPC payment QR code, true or false"),
];

/// Fields of `number_format:`.
pub const NUMBER_FORMAT: &[Key] = &[
    k("thousands", "thousands separator"),
    k("decimal", "decimal separator"),
    k("currency", "symbol or code"),
    k("currency_position", "before or after"),
];

/// Top-level keys of config.yaml (and of a `config:` block).
pub const CONFIG: &[Key] = &[
    k("invoices_dir", "where invoices live"),
    k("number_prefix", "invoice number prefix"),
    k("credit_prefix", "credit note number prefix"),
    k("currency", "default currency"),
    k("tax_rate", "default tax in percent"),
    k("due_days", "default days until due"),
    k("language", "default label language"),
    k("locale", "default number and date format"),
    k("date_format", "strftime pattern for dates"),
    b("number_format", "thousands, decimal, currency..."),
    k("template", "custom typst template"),
    k("tax_note", "default legal tax mention"),
    k("e_invoice", "facturx, ubl or [facturx, ubl]"),
    b("labels", "override single labels"),
    b("from", "your details"),
    b("payment", "payment details"),
    b("clients", "clients by key"),
];

/// Extra fields of a config client, on top of [`PARTY`].
pub const CLIENT_EXTRA: &[Key] = &[
    k("currency", "currency for this client"),
    k("tax_rate", "tax in percent for this client"),
    k("rate", "default rate for invgen new"),
    k("unit", "default unit for invgen new"),
    k("description", "default item for invgen new"),
    k("language", "label language for this client"),
    k("tax_note", "legal tax mention for this client"),
    k("locale", "number and date format"),
    k("date_format", "strftime pattern for dates"),
    b("number_format", "thousands, decimal, currency..."),
];

pub const STATUSES: &[&str] = &["pending", "paid", "overdue", "draft"];
pub const KINDS: &[&str] = &["invoice", "credit_note"];
pub const E_INVOICE: &[&str] = &["facturx", "ubl", "[facturx, ubl]"];
/// Keys that are still accepted but no longer suggested.
pub const LEGACY: &[&str] = &["sent_on"];
/// Keys whose value is a `YYYY-MM-DD` date.
pub const DATE_KEYS: &[&str] = &["date", "due", "paid_on", "refers_to_date", "sent_on"];

/// The keys allowed under `parents` (outermost first), or `None` when that
/// level is free-form (labels, client keys) or unknown.
pub fn keys_at(parents: &[&str]) -> Option<Vec<Key>> {
    let party_client = || PARTY.iter().chain(CLIENT_EXTRA).copied().collect();
    Some(match parents {
        [] => INVOICE.to_vec(),
        ["from" | "to"] | ["config", "from"] => PARTY.to_vec(),
        ["payment"] | ["config", "payment"] => PAYMENT.to_vec(),
        ["number_format"]
        | ["config", "number_format"]
        | ["config", "clients", _, "number_format"] => NUMBER_FORMAT.to_vec(),
        ["config"] => CONFIG.to_vec(),
        ["config", "clients", _] => party_client(),
        _ => return None,
    })
}

/// One line of the front matter, as far as a line-based reading can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line<'a> {
    /// 1-based line in the whole file.
    pub line: usize,
    /// Byte offset of the line start in the whole file.
    pub start: usize,
    /// The line without its newline.
    pub text: &'a str,
    pub indent: usize,
    /// `key` of a `key: value` line.
    pub key: Option<&'a str>,
    /// What follows `key:`, without a trailing comment, trimmed.
    pub value: &'a str,
    /// Keys of the enclosing blocks, outermost first.
    pub parents: Vec<&'a str>,
    /// A `- item` line or part of a multi-line string: not a key line.
    pub nested_text: bool,
}

/// The front matter of `src`, line by line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter<'a> {
    /// Every line between the fences (blank and comment lines included).
    pub lines: Vec<Line<'a>>,
    /// 1-based line of the closing `---`, `None` when it is missing.
    pub close_line: Option<usize>,
    /// Byte offset where the body starts (the end of `src` when unclosed).
    pub body_start: usize,
}

impl<'a> FrontMatter<'a> {
    /// `key` lines directly under `parents`.
    pub fn keys_under<'s>(&'s self, parents: &'s [&str]) -> impl Iterator<Item = &'s Line<'a>> {
        self.lines
            .iter()
            .filter(move |l| l.key.is_some() && l.parents == parents)
    }

    /// The top-level `key:` line.
    pub fn top(&self, key: &str) -> Option<&Line<'a>> {
        self.keys_under(&[]).find(|l| l.key == Some(key))
    }
}

/// Split `src` into its front matter lines. `None` when the file does not
/// start with `---`.
pub fn front_matter(src: &str) -> Option<FrontMatter<'_>> {
    let mut offset = 0;
    let mut iter = src.split_inclusive('\n');
    let first = iter.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    offset += first.len();
    let mut out = FrontMatter {
        lines: Vec::new(),
        close_line: None,
        body_start: src.len(),
    };
    let mut stack: Vec<(usize, &str)> = Vec::new();
    // Indentation of a `key: |` multi-line string being read.
    let mut text_block: Option<usize> = None;
    for (i, raw) in iter.enumerate() {
        let line_no = i + 2;
        let text = raw.strip_suffix('\n').unwrap_or(raw);
        let text = text.strip_suffix('\r').unwrap_or(text);
        let start = offset;
        offset += raw.len();
        let fence = text.trim_end();
        if fence == "---" || fence == "..." {
            out.close_line = Some(line_no);
            out.body_start = offset;
            break;
        }
        let trimmed = text.trim_start();
        let indent = text.len() - trimmed.len();
        let blank = trimmed.is_empty() || trimmed.starts_with('#');
        if let Some(block_indent) = text_block {
            if blank || indent > block_indent {
                out.lines.push(Line {
                    line: line_no,
                    start,
                    text,
                    indent,
                    key: None,
                    value: "",
                    parents: stack.iter().map(|s| s.1).collect(),
                    nested_text: true,
                });
                continue;
            }
            text_block = None;
        }
        if !blank {
            while stack.last().is_some_and(|&(ind, _)| ind >= indent) {
                stack.pop();
            }
        }
        let parents: Vec<&str> = stack.iter().map(|s| s.1).collect();
        let is_item = trimmed.starts_with("- ") || trimmed == "-";
        let (key, value) = match (blank || is_item, split_key(trimmed)) {
            (false, Some((key, value))) => (Some(key), value),
            _ => (None, ""),
        };
        if let Some(key) = key {
            if value.is_empty() {
                stack.push((indent, key));
            } else if value.starts_with('|') || value.starts_with('>') {
                text_block = Some(indent);
            }
        }
        out.lines.push(Line {
            line: line_no,
            start,
            text,
            indent,
            key,
            value,
            parents,
            nested_text: is_item,
        });
    }
    Some(out)
}

/// `key: value # comment` -> (`key`, `value`). The key must look like an
/// identifier (letters, digits, `_`, `-`), optionally quoted.
pub fn split_key(s: &str) -> Option<(&str, &str)> {
    let colon = s.find(':')?;
    let key = s[..colon].trim_end();
    let key = unquote(key);
    let rest = &s[colon + 1..];
    if !(rest.is_empty() || rest.starts_with([' ', '\t'])) || !is_ident(key) {
        return None;
    }
    let value = match comment_start(rest) {
        Some(i) => &rest[..i],
        None => rest,
    };
    Some((key, value.trim()))
}

pub fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_ident_char)
}

pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// `"x"` or `'x'` -> `x`; anything else unchanged.
pub fn unquote(s: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

/// Start of a YAML comment (`#` after whitespace, outside quotes).
pub fn comment_start(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut prev_ws = true;
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                } else if c == '#' && prev_ws {
                    return Some(i);
                }
            }
        }
        prev_ws = c.is_whitespace();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "---\nnumber: X-1 # the number\nfrom:\n  name: Alex Morgan\n  address:\n    - Main Street 1\n  email: a@example.com\nconfig:\n  email:\n    body: |\n      Hello: there\n      bye\n  clients:\n    northwind:\n      rate: 1\nstatus: paid\n---\nbody\n";

    #[test]
    fn lines_know_their_parents() {
        let fm = front_matter(SRC).unwrap();
        let get = |n: usize| fm.lines.iter().find(|l| l.line == n).unwrap();
        assert_eq!(get(2).key, Some("number"));
        assert_eq!(get(2).value, "X-1");
        assert_eq!(get(4).parents, vec!["from"]);
        assert!(get(6).nested_text);
        assert_eq!(get(6).parents, vec!["from", "address"]);
        assert_eq!(get(7).key, Some("email"));
        assert_eq!(get(7).parents, vec!["from"]);
        assert_eq!(get(10).value, "|");
        // Inside the multi-line string: no key, even with a colon.
        assert_eq!(get(11).key, None);
        assert!(get(11).nested_text);
        assert_eq!(get(15).parents, vec!["config", "clients", "northwind"]);
        assert_eq!(get(16).key, Some("status"));
        assert!(get(16).parents.is_empty());
        assert_eq!(fm.close_line, Some(17));
        assert_eq!(&SRC[fm.body_start..], "body\n");
        assert_eq!(fm.top("status").unwrap().value, "paid");
    }

    #[test]
    fn unclosed_and_missing_front_matter() {
        assert!(front_matter("# just a body\n").is_none());
        let fm = front_matter("---\nstatus: paid\nsta").unwrap();
        assert_eq!(fm.close_line, None);
        assert_eq!(fm.lines.len(), 2);
        assert_eq!(fm.body_start, "---\nstatus: paid\nsta".len());
    }

    #[test]
    fn split_key_handles_comments_and_urls() {
        assert_eq!(split_key("a: b # c"), Some(("a", "b")));
        assert_eq!(split_key("accent: \"#111\""), Some(("accent", "\"#111\"")));
        assert_eq!(split_key("\"quoted\": 1"), Some(("quoted", "1")));
        assert_eq!(split_key("http://x"), None);
        assert_eq!(split_key("two words: x"), None);
    }

    #[test]
    fn keys_at_levels() {
        assert!(keys_at(&[]).unwrap().iter().any(|k| k.name == "status"));
        assert!(keys_at(&["to"]).unwrap().iter().any(|k| k.name == "vat_id"));
        let client = keys_at(&["config", "clients", "acme"]).unwrap();
        assert!(client.iter().any(|k| k.name == "rate"));
        assert!(client.iter().any(|k| k.name == "name"));
        assert!(keys_at(&["config", "labels"]).is_none());
        assert!(keys_at(&["config", "clients"]).is_none());
    }
}
