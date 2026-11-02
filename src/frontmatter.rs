//! Byte-preserving edits of the YAML front matter of an invoice file.
//!
//! These functions work on the markdown text, never on a parsed model, so
//! everything they do not touch (comments, key order, spacing, the body) stays
//! exactly as the user wrote it. Only top-level `key: value` lines are read or
//! changed; nested blocks (`to:`, `config:`...) are left alone.

use crate::model::Status;
use anyhow::{Result, bail};
use chrono::NaiveDate;

/// Where [`set`] inserts a key that is not in the front matter yet.
#[derive(Debug, Clone, Copy)]
pub enum Place<'a> {
    /// First line of the front matter.
    Start,
    /// After the first of these keys that exists (with its nested lines),
    /// else at the end.
    After(&'a [&'a str]),
    /// Last line of the front matter, before the closing `---`.
    End,
}

/// The front matter region of a source text.
struct Block {
    /// Byte offset of the first YAML line (after the opening `---` line).
    yaml_start: usize,
    /// Byte offset of the closing `---` line.
    yaml_end: usize,
    /// Byte offset where the markdown body starts.
    body_start: usize,
}

/// One top-level key with the lines that belong to it.
struct Entry<'a> {
    key: &'a str,
    /// Byte range of the key line, newline included.
    line: (usize, usize),
    /// End of the nested lines that follow (equals `line.1` when none).
    end: usize,
}

/// Byte offset where the markdown body starts (0 without front matter).
pub fn body_start(src: &str) -> Result<usize> {
    Ok(match locate(src)? {
        Some(b) => b.body_start,
        None => bom_len(src),
    })
}

/// The scalar value of a top-level `key`, unquoted and without a trailing
/// comment. `None` when the key is missing or has no inline value.
pub fn get(src: &str, key: &str) -> Result<Option<String>> {
    let Some(block) = locate(src)? else {
        return Ok(None);
    };
    let Some(e) = entries(src, &block).into_iter().find(|e| e.key == key) else {
        return Ok(None);
    };
    let parts = split_line(&src[e.line.0..e.line.1], key);
    let v = parts.value.trim();
    if v.is_empty() || v.starts_with('|') || v.starts_with('>') {
        return Ok(None);
    }
    Ok(Some(unquote(v)))
}

/// Set top-level `key` to `value` (written as is, so pass valid YAML scalar
/// text). An existing line keeps its spacing and trailing comment; a missing
/// key is inserted at `place`; a missing front matter block is created.
pub fn set(src: &str, key: &str, value: &str, place: Place) -> Result<String> {
    let nl = newline(src);
    let Some(block) = locate(src)? else {
        let at = bom_len(src);
        return Ok(format!(
            "{}---{nl}{key}: {value}{nl}---{nl}{}",
            &src[..at],
            &src[at..]
        ));
    };
    let entries = entries(src, &block);
    if let Some(e) = entries.iter().find(|e| e.key == key) {
        let line = &src[e.line.0..e.line.1];
        let new_line = replace_value(line, key, value);
        return Ok(format!("{}{}{}", &src[..e.line.0], new_line, &src[e.end..]));
    }
    let at = match place {
        Place::Start => block.yaml_start,
        Place::End => block.yaml_end,
        Place::After(keys) => keys
            .iter()
            .find_map(|k| entries.iter().find(|e| e.key == *k))
            .map_or(block.yaml_end, |e| e.end),
    };
    // `at` is always at a line start: the closing fence is a line of its own.
    Ok(format!("{}{key}: {value}{nl}{}", &src[..at], &src[at..]))
}

/// Remove every top-level `key` line (with its nested lines). Text without
/// front matter, or without the key, is returned unchanged.
pub fn remove(src: &str, key: &str) -> Result<String> {
    let Some(block) = locate(src)? else {
        return Ok(src.to_string());
    };
    let mut out = String::with_capacity(src.len());
    let mut pos = 0;
    for e in entries(src, &block).iter().filter(|e| e.key == key) {
        out.push_str(&src[pos..e.line.0]);
        pos = e.end;
    }
    out.push_str(&src[pos..]);
    Ok(out)
}

/// Set the written status. `paid` also records `paid_on`: `on` when given,
/// else a valid `paid_on` already in the file, else `today`. Any other
/// status removes `paid_on`.
pub fn set_status(
    src: &str,
    status: Status,
    on: Option<NaiveDate>,
    today: NaiveDate,
) -> Result<String> {
    let text = set(
        src,
        "status",
        status.label(),
        Place::After(&["due", "due_days", "date", "number"]),
    )?;
    if status != Status::Paid {
        return remove(&text, "paid_on");
    }
    let existing =
        get(&text, "paid_on")?.and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok());
    let day = on.or(existing).unwrap_or(today);
    set(
        &text,
        "paid_on",
        &day.format("%Y-%m-%d").to_string(),
        Place::After(&["status"]),
    )
}

// ---------------------------------------------------------------------------

fn bom_len(src: &str) -> usize {
    if src.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    }
}

/// `\r\n` when the first line ends with it, else `\n`.
fn newline(src: &str) -> &'static str {
    match src.find('\n') {
        Some(i) if i > 0 && src.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

/// Find the front matter the same way the parser does: a first line `---`,
/// closed by a `---` or `...` line.
fn locate(src: &str) -> Result<Option<Block>> {
    let start = bom_len(src);
    let mut lines = src[start..].split_inclusive('\n');
    let Some(first) = lines.next() else {
        return Ok(None);
    };
    if first.trim_end() != "---" {
        return Ok(None);
    }
    let yaml_start = start + first.len();
    let mut offset = yaml_start;
    for line in lines {
        let fence = line.trim_end();
        if fence == "---" || fence == "..." {
            return Ok(Some(Block {
                yaml_start,
                yaml_end: offset,
                body_start: offset + line.len(),
            }));
        }
        offset += line.len();
    }
    bail!("front matter is not closed: add a `---` line after the YAML")
}

/// The key of a top-level `key:` line, if it is one.
fn key_of(line: &str) -> Option<&str> {
    let (key, rest) = line.split_once(':')?;
    let ok = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && (rest.is_empty() || rest.starts_with([' ', '\t', '\r', '\n']));
    ok.then_some(key)
}

/// A line nested under the previous key: indented, a block sequence entry
/// at column 0 (`- a`), or blank.
fn is_nested(line: &str) -> bool {
    line.starts_with([' ', '\t'])
        || line.trim().is_empty()
        || (line.starts_with('-') && !line.starts_with("---"))
}

fn entries<'a>(src: &'a str, block: &Block) -> Vec<Entry<'a>> {
    let mut out: Vec<Entry> = Vec::new();
    let mut offset = block.yaml_start;
    // Whether lines after the current key may be nested under it.
    let mut open = false;
    for line in src[block.yaml_start..block.yaml_end].split_inclusive('\n') {
        let end = offset + line.len();
        if let Some(key) = key_of(line) {
            let v = split_line(line, key).value.trim();
            open = v.is_empty() || v.starts_with('|') || v.starts_with('>');
            out.push(Entry {
                key,
                line: (offset, end),
                end,
            });
        } else if open && is_nested(line) {
            if !line.trim().is_empty()
                && let Some(last) = out.last_mut()
            {
                // Blank lines only count once more nested text follows them.
                last.end = end;
            }
        } else {
            open = false;
        }
        offset = end;
    }
    out
}

/// A key line split into its parts: `key:` + `lead` + `value` + `pad` +
/// `comment` + `eol`.
struct Parts<'a> {
    lead: &'a str,
    value: &'a str,
    pad: &'a str,
    comment: &'a str,
    eol: &'a str,
}

fn split_line<'a>(line: &'a str, key: &str) -> Parts<'a> {
    let rest = &line[key.len() + 1..];
    let body_len = rest.trim_end_matches(['\r', '\n']).len();
    let (rest, eol) = rest.split_at(body_len);
    let lead_len = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let (lead, rest) = rest.split_at(lead_len);
    // A comment starts at `#` after whitespace, outside of quotes.
    let mut quote = None;
    let mut comment_at = rest.len();
    let mut prev_ws = true;
    for (i, c) in rest.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if (c == '"' || c == '\'') && i == 0 => quote = Some(c),
            None if c == '#' && prev_ws => {
                comment_at = i;
                break;
            }
            None => {}
        }
        prev_ws = c == ' ' || c == '\t';
    }
    let (before, comment) = rest.split_at(comment_at);
    let value = before.trim_end_matches([' ', '\t']);
    let pad = &before[value.len()..];
    Parts {
        lead,
        value,
        pad,
        comment,
        eol,
    }
}

/// The key line with `value` in place of the old one. A trailing comment
/// stays in the same column when there is room.
fn replace_value(line: &str, key: &str, value: &str) -> String {
    let p = split_line(line, key);
    let lead = if p.lead.is_empty() { " " } else { p.lead };
    let pad = if p.comment.is_empty() {
        String::new()
    } else if p.pad.contains('\t') {
        p.pad.to_string()
    } else {
        let width = p.value.chars().count() + p.pad.len();
        " ".repeat(width.saturating_sub(value.chars().count()).max(1))
    };
    format!("{key}:{lead}{value}{pad}{}{}", p.comment, p.eol)
}

fn unquote(v: &str) -> String {
    let inner = |q: char| v.strip_prefix(q).and_then(|s| s.strip_suffix(q));
    if let Some(s) = inner('"') {
        s.replace("\\\"", "\"")
    } else if let Some(s) = inner('\'') {
        s.replace("''", "'")
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "---\n\
number: EINV-2026-05   # leave out to auto-number\n\
date: 2026-10-01\n\
# a comment line\n\
due: 2026-10-15        # or due_days\n\
status: pending               # pending | paid | overdue | draft\n\
client: northwind\n\
to:\n\
\x20 name: Northwind Labs\n\
\x20 address: [Calle Mayor 12, Madrid]\n\
\n\
tax_rate: 0\n\
---\n\
\n\
# Senior Software Engineer\n\
status: this is body text, not front matter\n";

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Lines of `after` that are not in `before`, and the other way around.
    fn diff(before: &str, after: &str) -> (Vec<String>, Vec<String>) {
        let b: Vec<&str> = before.split_inclusive('\n').collect();
        let a: Vec<&str> = after.split_inclusive('\n').collect();
        let added = a
            .iter()
            .filter(|l| !b.contains(l))
            .map(|l| l.to_string())
            .collect();
        let removed = b
            .iter()
            .filter(|l| !a.contains(l))
            .map(|l| l.to_string())
            .collect();
        (added, removed)
    }

    #[test]
    fn get_reads_scalars() {
        assert_eq!(get(SRC, "number").unwrap().unwrap(), "EINV-2026-05");
        assert_eq!(get(SRC, "status").unwrap().unwrap(), "pending");
        assert_eq!(get(SRC, "to").unwrap(), None);
        assert_eq!(get(SRC, "name").unwrap(), None);
        assert_eq!(get(SRC, "paid_on").unwrap(), None);
        let q = "---\na: \"x # y\"  # c\nb: 'it''s'\nc: v#1\n---\n";
        assert_eq!(get(q, "a").unwrap().unwrap(), "x # y");
        assert_eq!(get(q, "b").unwrap().unwrap(), "it's");
        assert_eq!(get(q, "c").unwrap().unwrap(), "v#1");
    }

    #[test]
    fn set_replaces_only_the_value() {
        let out = set(SRC, "status", "paid", Place::End).unwrap();
        let (added, removed) = diff(SRC, &out);
        assert_eq!(
            removed,
            ["status: pending               # pending | paid | overdue | draft\n"]
        );
        // The comment keeps its column.
        assert_eq!(
            added,
            ["status: paid                  # pending | paid | overdue | draft\n"]
        );
        assert_eq!(out.len(), SRC.len());
        // The body line that looks like a key is untouched.
        assert!(out.ends_with("status: this is body text, not front matter\n"));
        // A longer value than the room before the comment keeps one space.
        let out = set(SRC, "due", "2026-10-15-and-more", Place::End).unwrap();
        assert!(
            out.contains("due: 2026-10-15-and-more # or due_days\n"),
            "{out}"
        );
    }

    #[test]
    fn set_inserts_missing_keys() {
        let out = set(SRC, "paid_on", "2026-11-02", Place::After(&["status"])).unwrap();
        let (added, removed) = diff(SRC, &out);
        assert!(removed.is_empty());
        assert_eq!(added, ["paid_on: 2026-11-02\n"]);
        assert!(out.contains("draft\npaid_on: 2026-11-02\nclient: northwind\n"));

        // After a key with a nested block: after the whole block.
        let out = set(SRC, "x", "1", Place::After(&["missing", "to"])).unwrap();
        assert!(out.contains("Madrid]\nx: 1\n\ntax_rate: 0\n"), "{out}");

        let out = set(SRC, "x", "1", Place::End).unwrap();
        assert!(out.contains("tax_rate: 0\nx: 1\n---\n"));
        let out = set(SRC, "x", "1", Place::Start).unwrap();
        assert!(out.starts_with("---\nx: 1\nnumber: EINV-2026-05 "));
        let out = set(SRC, "x", "1", Place::After(&["nope"])).unwrap();
        assert!(out.contains("tax_rate: 0\nx: 1\n---\n"));
    }

    #[test]
    fn set_creates_the_block_and_keeps_crlf_and_bom() {
        let body = "# Title\n\n- A: 1 @ 1\n";
        assert_eq!(
            set(body, "status", "paid", Place::End).unwrap(),
            format!("---\nstatus: paid\n---\n{body}")
        );
        let crlf = "\u{feff}---\r\nstatus: pending\r\n---\r\nbody\r\n";
        let out = set(crlf, "paid_on", "2026-11-02", Place::After(&["status"])).unwrap();
        assert_eq!(
            out,
            "\u{feff}---\r\nstatus: pending\r\npaid_on: 2026-11-02\r\n---\r\nbody\r\n"
        );
        let out = set(crlf, "status", "paid", Place::End).unwrap();
        assert_eq!(out, "\u{feff}---\r\nstatus: paid\r\n---\r\nbody\r\n");
        let out = set("\u{feff}body\r\n", "a", "1", Place::End).unwrap();
        assert_eq!(out, "\u{feff}---\r\na: 1\r\n---\r\nbody\r\n");
        // Empty front matter.
        let out = set("---\n---\nbody\n", "a", "1", Place::Start).unwrap();
        assert_eq!(out, "---\na: 1\n---\nbody\n");
    }

    #[test]
    fn set_replaces_a_block_value_and_its_lines() {
        let src = "---\nnotes: |\n  one\n  two\nstatus: pending\n---\nbody\n";
        let out = set(src, "notes", "short", Place::End).unwrap();
        assert_eq!(out, "---\nnotes: short\nstatus: pending\n---\nbody\n");
        let list = "---\ntags:\n- a\n- b\nstatus: draft\n---\n";
        assert_eq!(remove(list, "tags").unwrap(), "---\nstatus: draft\n---\n");
    }

    #[test]
    fn remove_drops_only_that_key() {
        let out = remove(SRC, "due").unwrap();
        let (added, removed) = diff(SRC, &out);
        assert!(added.is_empty());
        assert_eq!(removed, ["due: 2026-10-15        # or due_days\n"]);
        let out = remove(SRC, "to").unwrap();
        assert!(out.contains("client: northwind\n\ntax_rate: 0\n"), "{out}");
        assert_eq!(remove(SRC, "missing").unwrap(), SRC);
        assert_eq!(
            remove("no front matter\n", "a").unwrap(),
            "no front matter\n"
        );
    }

    #[test]
    fn unclosed_front_matter_is_an_error() {
        let err = set("---\nstatus: paid\n", "a", "1", Place::End).unwrap_err();
        assert!(err.to_string().contains("not closed"));
    }

    #[test]
    fn status_and_paid_on() {
        let today = day(2026, 11, 2);
        let paid = set_status(SRC, Status::Paid, None, today).unwrap();
        let (added, removed) = diff(SRC, &paid);
        assert_eq!(removed.len(), 1);
        assert_eq!(
            added,
            [
                "status: paid                  # pending | paid | overdue | draft\n",
                "paid_on: 2026-11-02\n"
            ]
        );
        // Marking again keeps the recorded date unless `on` is given.
        let again = set_status(&paid, Status::Paid, None, day(2026, 12, 1)).unwrap();
        assert_eq!(again, paid);
        let moved = set_status(&paid, Status::Paid, Some(day(2026, 10, 30)), today).unwrap();
        assert!(moved.contains("paid_on: 2026-10-30\n"));
        // Back to pending: paid_on goes away and the file is as before.
        let back = set_status(&paid, Status::Pending, None, today).unwrap();
        assert_eq!(back, SRC);
        // A file without status gets one after the dates.
        let src = "---\ndate: 2026-10-01\nclient: northwind\n---\n- A: 1 @ 1\n";
        let out = set_status(src, Status::Draft, None, today).unwrap();
        assert_eq!(
            out,
            "---\ndate: 2026-10-01\nstatus: draft\nclient: northwind\n---\n- A: 1 @ 1\n"
        );
    }

    #[test]
    fn body_start_offsets() {
        assert_eq!(body_start("---\na: 1\n---\nbody").unwrap(), 13);
        assert_eq!(body_start("body").unwrap(), 0);
        assert_eq!(body_start("\u{feff}body").unwrap(), 3);
    }
}
