//! Comment-preserving edits of `config.yaml`.
//!
//! The config is a hand-edited file, so the editor must not reserialize it
//! (that would drop comments, reorder keys and reformat lists). Instead the
//! edits here work on lines of the block-style YAML that invgen writes: a key
//! is set by rewriting only its own line(s), a missing key is inserted at the
//! end of its parent block, and a removed key takes only its own lines with it.
//! Every other line stays byte-identical.
//!
//! Every edit is verified before it is returned: the new text is parsed again
//! and must equal the old document with exactly the requested change applied
//! (compared as YAML values), and it must still load as a [`Config`]. When the
//! file uses a structure the line editor cannot handle (flow mappings like
//! `from: {name: x}`, anchors, tags, an indented document...) the edit fails
//! with [`EditError::Unsupported`] instead of producing a wrong file, and the
//! caller can offer to open the file in a text editor.

use crate::config::{ClientConfig, Config};
use crate::model::{Party, Payment};
use serde_yaml_ng::{Mapping, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// A value to write at a key.
#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    Str(String),
    Num(f64),
    /// Written as a flow sequence on one line: `[a, b, "c, d"]`.
    List(Vec<String>),
}

/// One change to the document. Paths are mapping keys from the top level,
/// e.g. `["clients", "acme", "rate"]`.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Set a key, creating it (and missing parent mappings) when needed.
    Set(Vec<String>, Field),
    /// Remove a key and its whole value. Missing keys are ignored.
    Remove(Vec<String>),
    /// Rename the last key of the path in place, keeping its value and position.
    Rename(Vec<String>, String),
}

impl Edit {
    pub fn set(path: &[&str], field: Field) -> Edit {
        Edit::Set(owned(path), field)
    }
    pub fn remove(path: &[&str]) -> Edit {
        Edit::Remove(owned(path))
    }
    pub fn rename(path: &[&str], to: &str) -> Edit {
        Edit::Rename(owned(path), to.to_string())
    }
}

fn owned(path: &[&str]) -> Vec<String> {
    path.iter().map(|s| s.to_string()).collect()
}

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// The file uses YAML the line editor does not handle; nothing was changed.
    #[error("{0}")]
    Unsupported(String),
    /// The result would not be a valid config (or the request itself is invalid).
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

fn unsupported(msg: impl Into<String>) -> EditError {
    EditError::Unsupported(msg.into())
}

// ------------------------------------------------------------------ high level

/// Add or update client `key`. With `old_key` set and different from `key`,
/// the client is renamed in place first. Only fields whose value changed are
/// rewritten; unknown keys inside the client block are kept.
pub fn upsert_client(
    text: &str,
    old_key: Option<&str>,
    key: &str,
    client: &ClientConfig,
) -> Result<String, EditError> {
    valid_client_key(key).map_err(EditError::Invalid)?;
    if client.party.name.trim().is_empty() {
        return Err(EditError::Invalid("the client needs a name".into()));
    }
    let cfg = parse_config(text)?;
    let mut edits = Vec::new();
    let mut current = None;
    match old_key {
        Some(old) if old != key => {
            if cfg.clients.contains_key(key) {
                return Err(EditError::Invalid(format!(
                    "a client with key `{key}` already exists"
                )));
            }
            current = cfg.clients.get(old);
            if current.is_none() {
                return Err(EditError::Invalid(format!("no client with key `{old}`")));
            }
            edits.push(Edit::rename(&["clients", old], key));
        }
        Some(old) => current = cfg.clients.get(old),
        None => {
            if cfg.clients.contains_key(key) {
                return Err(EditError::Invalid(format!(
                    "a client with key `{key}` already exists"
                )));
            }
        }
    }
    let empty = ClientConfig::default();
    let cur = current.unwrap_or(&empty);
    let base = ["clients", key];
    party_edits(&base, &cur.party, &client.party, &mut edits);
    opt_str(
        &base,
        "currency",
        &cur.currency,
        &client.currency,
        &mut edits,
    );
    opt_num(&base, "tax_rate", cur.tax_rate, client.tax_rate, &mut edits);
    opt_num(&base, "rate", cur.rate, client.rate, &mut edits);
    opt_str(&base, "unit", &cur.unit, &client.unit, &mut edits);
    opt_str(
        &base,
        "description",
        &cur.description,
        &client.description,
        &mut edits,
    );
    apply(text, &edits)
}

/// Remove client `key` and every line of its block.
pub fn delete_client(text: &str, key: &str) -> Result<String, EditError> {
    let cfg = parse_config(text)?;
    if !cfg.clients.contains_key(key) {
        return Err(EditError::Invalid(format!("no client with key `{key}`")));
    }
    apply(text, &[Edit::remove(&["clients", key])])
}

/// Update `from:` (your own details), rewriting only the changed fields.
pub fn set_from(text: &str, party: &Party) -> Result<String, EditError> {
    let cfg = parse_config(text)?;
    let mut edits = Vec::new();
    party_edits(&["from"], &cfg.from, party, &mut edits);
    apply(text, &edits)
}

/// Update `payment:`, rewriting only the changed fields.
pub fn set_payment(text: &str, payment: &Payment) -> Result<String, EditError> {
    let cfg = parse_config(text)?;
    let cur = &cfg.payment;
    let mut edits = Vec::new();
    let base = ["payment"];
    opt_str(&base, "holder", &cur.holder, &payment.holder, &mut edits);
    opt_str(&base, "iban", &cur.iban, &payment.iban, &mut edits);
    opt_str(&base, "bic", &cur.bic, &payment.bic, &mut edits);
    list(&base, "notes", &cur.notes, &payment.notes, &mut edits);
    apply(text, &edits)
}

fn path_of(base: &[&str], key: &str) -> Vec<String> {
    let mut p = owned(base);
    p.push(key.to_string());
    p
}

fn party_edits(base: &[&str], cur: &Party, new: &Party, out: &mut Vec<Edit>) {
    if cur.name != new.name {
        out.push(Edit::Set(
            path_of(base, "name"),
            Field::Str(new.name.clone()),
        ));
    }
    list(base, "address", &cur.address, &new.address, out);
    list(base, "tax", &cur.tax, &new.tax, out);
    opt_str(base, "email", &cur.email, &new.email, out);
    opt_str(base, "phone", &cur.phone, &new.phone, out);
}

fn list(base: &[&str], key: &str, cur: &[String], new: &[String], out: &mut Vec<Edit>) {
    if cur == new {
        return;
    }
    out.push(if new.is_empty() {
        Edit::Remove(path_of(base, key))
    } else {
        Edit::Set(path_of(base, key), Field::List(new.to_vec()))
    });
}

fn opt_str(
    base: &[&str],
    key: &str,
    cur: &Option<String>,
    new: &Option<String>,
    out: &mut Vec<Edit>,
) {
    if cur == new {
        return;
    }
    out.push(match new {
        Some(s) => Edit::Set(path_of(base, key), Field::Str(s.clone())),
        None => Edit::Remove(path_of(base, key)),
    });
}

fn opt_num(base: &[&str], key: &str, cur: Option<f64>, new: Option<f64>, out: &mut Vec<Edit>) {
    if cur == new {
        return;
    }
    out.push(match new {
        Some(n) => Edit::Set(path_of(base, key), Field::Num(n)),
        None => Edit::Remove(path_of(base, key)),
    });
}

/// Parse `text` as a config the way `Config::load_from` does (empty is default).
pub fn parse_config(text: &str) -> Result<Config, EditError> {
    if text.trim().is_empty() {
        return Ok(Config::default());
    }
    serde_yaml_ng::from_str(text).map_err(|e| EditError::Invalid(format!("config: {e}")))
}

/// A client key must be usable as a plain YAML key and in `client: KEY`:
/// letters, digits, `-`, `_` and `.`, starting with a letter or digit, and not
/// something YAML reads as a number, boolean or null.
pub fn valid_client_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("the key cannot be empty".into());
    }
    if key.len() > 64 {
        return Err("the key is too long (64 characters at most)".into());
    }
    let first_ok = key
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !first_ok || !rest_ok {
        return Err("use letters, digits, `-`, `_` or `.`, starting with a letter or digit".into());
    }
    match serde_yaml_ng::from_str::<Value>(key) {
        Ok(Value::String(s)) if s == key => Ok(()),
        _ => Err(format!("`{key}` reads as a number or keyword in YAML")),
    }
}

/// Suggest a client key from a display name: `Northwind Labs SL` -> `northwind-labs-sl`.
pub fn suggest_client_key(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(40).collect();
    let out = out.trim_end_matches('-').to_string();
    if valid_client_key(&out).is_ok() {
        out
    } else if out.is_empty() {
        "client".into()
    } else {
        format!("client-{out}")
    }
}

// ------------------------------------------------------------------ writing

/// Write `text` to `path` atomically: write a temp file next to it, then
/// rename it over the target. The previous version is kept as `<path>.bak`.
/// Parent directories are created.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(d) => d.to_path_buf(),
        None => PathBuf::from("."),
    };
    std::fs::create_dir_all(&dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.yaml".into());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        if let Ok(meta) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
            std::fs::copy(path, backup_path(path))?;
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// `config.yaml` -> `config.yaml.bak`.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".bak");
    PathBuf::from(s)
}

// ------------------------------------------------------------------ core

/// Apply `edits` in order and verify the result (see the module docs).
pub fn apply(text: &str, edits: &[Edit]) -> Result<String, EditError> {
    let before = parse_value(text)?;
    let mut doc = Doc::new(text);
    let mut expected = before;
    for edit in edits {
        doc.apply(edit)?;
        apply_value(&mut expected, edit)?;
    }
    let out = doc.text();
    let after = parse_value(&out).map_err(|_| {
        unsupported("the config uses YAML the editor cannot safely change (the edit did not parse)")
    })?;
    if after != expected {
        return Err(unsupported(
            "the config uses YAML the editor cannot safely change (for example flow \
             mappings like `{ ... }`, anchors or an unusual indentation)",
        ));
    }
    parse_config(&out)?;
    Ok(out)
}

fn parse_value(text: &str) -> Result<Value, EditError> {
    if text.trim().is_empty() {
        return Ok(Value::Mapping(Mapping::new()));
    }
    match serde_yaml_ng::from_str::<Value>(text) {
        Ok(Value::Null) => Ok(Value::Mapping(Mapping::new())),
        Ok(v) => Ok(v),
        Err(e) => Err(EditError::Invalid(format!("config: {e}"))),
    }
}

/// The same edit on the parsed document, to know what the result must look like.
fn apply_value(root: &mut Value, edit: &Edit) -> Result<(), EditError> {
    let (path, last) = match edit {
        Edit::Set(p, _) | Edit::Remove(p) | Edit::Rename(p, _) => match p.split_last() {
            Some((last, parents)) => (parents, last),
            None => return Err(EditError::Invalid("empty key path".into())),
        },
    };
    let create = matches!(edit, Edit::Set(..));
    let mut node = root;
    for key in path {
        if node.is_null() && create {
            *node = Value::Mapping(Mapping::new());
        }
        let Value::Mapping(m) = node else {
            return Err(unsupported(format!("`{key}`'s parent is not a mapping")));
        };
        let k = Value::String(key.clone());
        if !m.contains_key(&k) {
            if !create {
                return Ok(());
            }
            m.insert(k.clone(), Value::Mapping(Mapping::new()));
        }
        node = m.get_mut(&k).expect("inserted above");
    }
    if node.is_null() && create {
        *node = Value::Mapping(Mapping::new());
    }
    let Value::Mapping(m) = node else {
        return Err(unsupported(format!("`{last}`'s parent is not a mapping")));
    };
    let k = Value::String(last.clone());
    match edit {
        Edit::Set(_, field) => {
            m.insert(k, field_value(field)?);
        }
        Edit::Remove(_) => {
            m.remove(&k);
        }
        Edit::Rename(_, to) => {
            let v = m
                .remove(&k)
                .ok_or_else(|| EditError::Invalid(format!("no key `{last}` to rename")))?;
            m.insert(Value::String(to.clone()), v);
        }
    }
    Ok(())
}

/// What the rendered field parses to (so `450` is an integer, like on disk).
fn field_value(field: &Field) -> Result<Value, EditError> {
    serde_yaml_ng::from_str(&render(field)?)
        .map_err(|e| EditError::Invalid(format!("cannot write value: {e}")))
}

/// Render a field as YAML for the value position of `key: <here>`.
fn render(field: &Field) -> Result<String, EditError> {
    match field {
        Field::Str(s) => Ok(scalar(s, false)),
        Field::Num(n) => {
            if !n.is_finite() {
                return Err(EditError::Invalid(format!("{n} is not a number")));
            }
            Ok(if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            })
        }
        Field::List(items) => {
            let parts: Vec<String> = items.iter().map(|s| scalar(s, true)).collect();
            Ok(format!("[{}]", parts.join(", ")))
        }
    }
}

/// A string as a plain scalar when YAML reads it back unchanged, otherwise
/// double-quoted. `in_flow` is for items of a `[...]` sequence.
fn scalar(s: &str, in_flow: bool) -> String {
    if plain_ok(s, in_flow) {
        s.to_string()
    } else {
        quote(s)
    }
}

fn plain_ok(s: &str, in_flow: bool) -> bool {
    if s.is_empty() || s.trim() != s || s.contains(['\n', '\r', '\t']) {
        return false;
    }
    if s.starts_with([
        '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@',
        '`',
    ]) {
        return false;
    }
    if s.contains(": ") || s.contains(" #") || s.ends_with(':') {
        return false;
    }
    if in_flow && s.contains([',', '[', ']', '{', '}']) {
        return false;
    }
    if s.chars().any(|c| c.is_control()) {
        return false;
    }
    // Quote what any YAML reader (1.1 or 1.2) could take for a number, date
    // or boolean, even when serde_yaml_ng would keep it a string: `00000`, `yes`.
    let numberish = s.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '+' | '.'))
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() || "_.:+-xXoO".contains(c));
    let keyword = matches!(
        s.to_ascii_lowercase().as_str(),
        "y" | "n" | "yes" | "no" | "on" | "off" | "true" | "false" | "null" | "~"
    );
    if numberish || keyword {
        return false;
    }
    let probe = if in_flow {
        format!("[{s}]")
    } else {
        format!("k: {s}")
    };
    match serde_yaml_ng::from_str::<Value>(&probe) {
        Ok(Value::Sequence(v)) if in_flow => v.len() == 1 && v[0] == Value::String(s.into()),
        Ok(Value::Mapping(m)) if !in_flow => m.get("k") == Some(&Value::String(s.into())),
        _ => false,
    }
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The document as lines. Each line keeps a trailing `\r` if it had one, and
/// a final newline shows up as a last empty line, so joining with `\n`
/// gives the input back byte for byte.
struct Doc {
    lines: Vec<String>,
    crlf: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Blank,
    Comment,
    Content,
}

/// A `key: value  # comment` line.
#[derive(Debug, Clone)]
struct KeyLine {
    indent: usize,
    /// The key as written (with quotes, if any).
    raw_key: String,
    /// The key's string value.
    key: String,
    /// Inline value without the comment, trimmed. Empty for a block.
    value: String,
    /// The comment (from `#`) and the character column it started at.
    comment: Option<(usize, String)>,
}

/// A located mapping entry.
#[derive(Debug, Clone)]
struct Node {
    line: usize,
    kl: KeyLine,
    /// End (exclusive) of the last content line of the value.
    content_end: usize,
    /// `content_end` plus directly following comments indented deeper than the key.
    end: usize,
}

impl Doc {
    fn new(text: &str) -> Doc {
        let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        let crlf = lines.first().is_some_and(|l| l.ends_with('\r'));
        Doc { lines, crlf }
    }

    fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Lines that hold the document (the final empty "line" after the last
    /// newline is not one of them).
    fn len(&self) -> usize {
        if self.lines.last().is_some_and(|l| l.is_empty()) {
            self.lines.len() - 1
        } else {
            self.lines.len()
        }
    }

    fn body(&self, i: usize) -> &str {
        self.lines[i].strip_suffix('\r').unwrap_or(&self.lines[i])
    }

    fn kind(&self, i: usize) -> Kind {
        let t = self.body(i).trim_start_matches(' ');
        if t.trim().is_empty() {
            Kind::Blank
        } else if t.starts_with('#') {
            Kind::Comment
        } else {
            Kind::Content
        }
    }

    fn indent(&self, i: usize) -> usize {
        self.body(i).len() - self.body(i).trim_start_matches(' ').len()
    }

    fn eol(&self, s: String) -> String {
        if self.crlf { s + "\r" } else { s }
    }

    fn check_document(&self) -> Result<(), EditError> {
        for i in 0..self.len() {
            let b = self.body(i);
            if self.kind(i) != Kind::Content {
                continue;
            }
            if b.starts_with('\t') || b.trim_start_matches(' ').starts_with('\t') {
                return Err(unsupported("the config is indented with tabs"));
            }
            if self.indent(i) == 0 && (b.starts_with('%') || b.starts_with("...")) {
                return Err(unsupported(
                    "the config uses YAML directives or document markers",
                ));
            }
            if self.indent(i) == 0 && b.starts_with("---") && i != self.first_content() {
                return Err(unsupported("the config holds more than one YAML document"));
            }
        }
        Ok(())
    }

    fn first_content(&self) -> usize {
        (0..self.len())
            .find(|&i| self.kind(i) == Kind::Content)
            .unwrap_or(self.len())
    }

    /// Where the top-level mapping starts (after an optional `---`).
    fn top_start(&self) -> usize {
        let first = self.first_content();
        if first < self.len() && self.body(first).starts_with("---") {
            if self.body(first).trim_end() != "---" {
                return usize::MAX;
            }
            first + 1
        } else {
            0
        }
    }

    /// Extent of the entry whose key line is `line`.
    fn node(&self, line: usize, kl: KeyLine) -> Node {
        let n = self.len();
        let seq_same_indent = kl.value.is_empty();
        let mut content_end = line + 1;
        let mut j = line + 1;
        while j < n {
            match self.kind(j) {
                Kind::Content => {
                    let ind = self.indent(j);
                    let body = self.body(j).trim_start_matches(' ');
                    let is_item = body == "-" || body.starts_with("- ");
                    if ind > kl.indent || (ind == kl.indent && seq_same_indent && is_item) {
                        content_end = j + 1;
                    } else {
                        break;
                    }
                }
                Kind::Blank | Kind::Comment => {}
            }
            j += 1;
        }
        let mut end = content_end;
        while end < n && self.kind(end) == Kind::Comment && self.indent(end) > kl.indent {
            end += 1;
        }
        Node {
            line,
            kl,
            content_end,
            end,
        }
    }

    /// Children of `parent` (or of the top level): (lines, child indent).
    fn children(&self, parent: Option<&Node>) -> Result<(Vec<Node>, Option<usize>), EditError> {
        let (start, stop, min_indent) = match parent {
            Some(p) => {
                if !p.kl.value.is_empty() {
                    return Err(not_a_block(p));
                }
                (p.line + 1, p.content_end, p.kl.indent + 1)
            }
            None => {
                let s = self.top_start();
                if s == usize::MAX {
                    return Err(unsupported("the config starts with an unusual `---` line"));
                }
                (s, self.len(), 0)
            }
        };
        let mut child_indent = None;
        let mut out = Vec::new();
        let mut i = start;
        while i < stop {
            if self.kind(i) != Kind::Content {
                i += 1;
                continue;
            }
            let ind = self.indent(i);
            let ci = *child_indent.get_or_insert(ind);
            if ind < min_indent || ind < ci {
                return Err(unsupported("the config has inconsistent indentation"));
            }
            if ind > ci {
                // Continuation of a multi-line value; `node()` skips these.
                i += 1;
                continue;
            }
            let Some(kl) = parse_key_line(self.body(i)) else {
                let what = match parent {
                    Some(p) => format!("`{}` is not a simple mapping", p.kl.key),
                    None => "the config is not a simple mapping".to_string(),
                };
                return Err(unsupported(what));
            };
            if kl.key == "<<" {
                return Err(unsupported("the config uses YAML merge keys (`<<`)"));
            }
            let node = self.node(i, kl);
            i = node.content_end;
            out.push(node);
        }
        Ok((out, child_indent))
    }

    fn find(&self, parent: Option<&Node>, key: &str) -> Result<Option<Node>, EditError> {
        let (kids, _) = self.children(parent)?;
        let mut found = kids.into_iter().filter(|n| n.kl.key == key);
        let first = found.next();
        if found.next().is_some() {
            return Err(unsupported(format!("the key `{key}` appears twice")));
        }
        Ok(first)
    }

    /// Indentation step used by the document (2 unless it clearly uses another).
    fn step(&self) -> usize {
        for i in 0..self.len() {
            if self.kind(i) == Kind::Content && self.indent(i) > 0 {
                return self.indent(i);
            }
        }
        2
    }

    fn apply(&mut self, edit: &Edit) -> Result<(), EditError> {
        self.check_document()?;
        match edit {
            Edit::Set(path, field) => self.set(path, field),
            Edit::Remove(path) => self.remove(path),
            Edit::Rename(path, to) => self.rename(path, to),
        }
    }

    /// Walk `path`, returning the deepest existing node and how many keys matched.
    fn walk(&self, path: &[String]) -> Result<(Option<Node>, usize), EditError> {
        let mut cur: Option<Node> = None;
        for (depth, key) in path.iter().enumerate() {
            if let Some(p) = &cur
                && !p.kl.value.is_empty()
            {
                // Descending into an entry with an inline value.
                if is_empty_inline(&p.kl.value) {
                    return Ok((cur, depth));
                }
                return Err(not_a_block(p));
            }
            match self.find(cur.as_ref(), key)? {
                Some(n) => cur = Some(n),
                None => return Ok((cur, depth)),
            }
        }
        Ok((cur, path.len()))
    }

    fn set(&mut self, path: &[String], field: &Field) -> Result<(), EditError> {
        let value = render(field)?;
        let (node, depth) = self.walk(path)?;
        if depth == path.len() {
            let node = node.expect("full match has a node");
            let line = format_entry(
                &" ".repeat(node.kl.indent),
                &node.kl.raw_key,
                &value,
                node.kl.comment.as_ref(),
            );
            let line = self.eol(line);
            self.lines.splice(node.line..node.content_end, [line]);
            return Ok(());
        }
        // Insert the missing keys path[depth..] under `node`.
        let step = self.step();
        let (at, indent) = match &node {
            Some(p) => {
                let ci = if p.kl.value.is_empty() {
                    self.children(Some(p))?.1
                } else {
                    // `key: {}` or `key: null`: becomes a block.
                    self.set_inline(p, "");
                    None
                };
                (p.end, ci.unwrap_or(p.kl.indent + step))
            }
            None => (self.top_insert_at(path.len() - depth == 1), 0),
        };
        let mut new = Vec::new();
        if node.is_none() && path.len() - depth > 1 && at > 0 && self.kind(at - 1) != Kind::Blank {
            new.push(self.eol(String::new()));
        }
        for (k, key) in path[depth..].iter().enumerate() {
            let pad = " ".repeat(indent + k * step);
            let last = depth + k + 1 == path.len();
            let line = if last {
                format_entry(&pad, &key_text(key), &value, None)
            } else {
                format_entry(&pad, &key_text(key), "", None)
            };
            new.push(self.eol(line));
        }
        self.lines.splice(at..at, new);
        Ok(())
    }

    /// Where a new top-level entry goes: scalars before the first block
    /// entry (`from:` and friends), blocks at the end.
    fn top_insert_at(&self, scalar: bool) -> usize {
        if scalar
            && let Ok((kids, _)) = self.children(None)
            && let Some(first_block) = kids.iter().find(|n| n.kl.value.is_empty())
        {
            // Right after the last entry before the first block.
            if let Some(prev) = kids.iter().take_while(|n| n.line < first_block.line).last() {
                return prev.end;
            }
            return first_block.line;
        }
        self.len()
    }

    fn remove(&mut self, path: &[String]) -> Result<(), EditError> {
        let (node, depth) = self.walk(path)?;
        if depth < path.len() {
            return Ok(());
        }
        let node = node.expect("full match has a node");
        self.lines.drain(node.line..node.end);
        // A mapping left without entries would read as null: write `{}`.
        if path.len() > 1 {
            let (parent, _) = self.walk(&path[..path.len() - 1])?;
            if let Some(p) = parent
                && p.kl.value.is_empty()
                && self.children(Some(&p))?.0.is_empty()
            {
                self.set_inline(&p, "{}");
            }
        }
        Ok(())
    }

    /// Rewrite the inline value of `node`'s key line, keeping its comment.
    fn set_inline(&mut self, node: &Node, value: &str) {
        let line = format_entry(
            &" ".repeat(node.kl.indent),
            &node.kl.raw_key,
            value,
            node.kl.comment.as_ref(),
        );
        self.lines[node.line] = self.eol(line);
    }

    fn rename(&mut self, path: &[String], to: &str) -> Result<(), EditError> {
        let (node, depth) = self.walk(path)?;
        if depth < path.len() {
            return Err(EditError::Invalid(format!(
                "no key `{}` to rename",
                path.join(".")
            )));
        }
        let node = node.expect("full match has a node");
        let parent_path = &path[..path.len() - 1];
        let (parent, _) = self.walk(parent_path)?;
        if self.find(parent.as_ref(), to)?.is_some() {
            return Err(EditError::Invalid(format!("the key `{to}` already exists")));
        }
        let body = self.body(node.line).to_string();
        let start = node.kl.indent;
        let rest = &body[start + node.kl.raw_key.len()..];
        let line = format!("{}{}{}", &body[..start], key_text(to), rest);
        self.lines[node.line] = self.eol(line);
        Ok(())
    }
}

fn not_a_block(p: &Node) -> EditError {
    let v = &p.kl.value;
    if v.starts_with('{') {
        unsupported(format!(
            "`{}` is written as a flow mapping (`{{ ... }}`)",
            p.kl.key
        ))
    } else if v.starts_with('&') || v.starts_with('*') {
        unsupported(format!("`{}` uses a YAML anchor or alias", p.kl.key))
    } else if v.starts_with('!') {
        unsupported(format!("`{}` uses a YAML tag", p.kl.key))
    } else {
        unsupported(format!("`{}` is not a mapping", p.kl.key))
    }
}

fn is_empty_inline(v: &str) -> bool {
    matches!(v, "{}" | "~" | "null" | "Null" | "NULL")
}

fn key_text(key: &str) -> String {
    scalar(key, false)
}

/// `pad key: value` with the old comment kept, at its old column when it fits.
fn format_entry(pad: &str, key: &str, value: &str, comment: Option<&(usize, String)>) -> String {
    let mut s = format!("{pad}{key}:");
    if !value.is_empty() {
        s.push(' ');
        s.push_str(value);
    }
    if let Some((col, c)) = comment {
        let w = s.chars().count();
        let pad = if *col > w { col - w } else { 1 };
        s.push_str(&" ".repeat(pad));
        s.push_str(c);
    }
    s
}

/// Parse `  key: value  # comment`. `None` when the line is not a mapping entry.
fn parse_key_line(line: &str) -> Option<KeyLine> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    if rest.starts_with("- ") || rest == "-" || rest.starts_with('?') {
        return None;
    }
    let (raw_key, key, after) = if rest.starts_with('"') || rest.starts_with('\'') {
        let end = quoted_end(rest)?;
        let raw = &rest[..end];
        let key: String = serde_yaml_ng::from_str::<Value>(raw)
            .ok()?
            .as_str()?
            .to_string();
        let after = rest[end..].trim_start_matches(' ');
        (raw.to_string(), key, after.strip_prefix(':')?)
    } else {
        let bytes = rest.as_bytes();
        let mut colon = None;
        for i in 0..bytes.len() {
            if bytes[i] == b'#' && i > 0 && bytes[i - 1] == b' ' {
                return None;
            }
            if bytes[i] == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ') {
                colon = Some(i);
                break;
            }
        }
        let c = colon?;
        let raw = rest[..c].trim_end();
        if raw.is_empty() || raw.starts_with(['[', '{', '&', '*', '!', '|', '>', '#']) {
            return None;
        }
        (raw.to_string(), raw.to_string(), &rest[c + 1..])
    };
    if !after.is_empty() && !after.starts_with(' ') {
        return None;
    }
    let value_start = line.len() - after.len();
    let (value, comment) = split_comment(after);
    let comment = comment.map(|(off, c)| {
        let byte = value_start + off;
        (line[..byte].chars().count(), c.to_string())
    });
    Some(KeyLine {
        indent,
        raw_key,
        key,
        value: value.trim().to_string(),
        comment,
    })
}

/// Byte index just past the closing quote of the quoted scalar starting `s`.
fn quoted_end(s: &str) -> Option<usize> {
    let q = s.as_bytes()[0];
    let b = s.as_bytes();
    let mut i = 1;
    while i < b.len() {
        if q == b'"' && b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == q {
            if q == b'\'' && b.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

/// Split a value from a trailing `# comment` (a `#` after whitespace, outside quotes).
fn split_comment(s: &str) -> (&str, Option<(usize, &str)>) {
    let b = s.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(b'"') if c == b'\\' => i += 1,
            Some(q) if c == q => {
                if q == b'\'' && b.get(i + 1) == Some(&b'\'') {
                    i += 1;
                } else {
                    quote = None;
                }
            }
            Some(_) => {}
            None => {
                if c == b'#' && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') {
                    return (&s[..i], Some((i, &s[i..])));
                }
                // Quotes only open a quoted scalar at the start of a token.
                if (c == b'"' || c == b'\'')
                    && (i == 0 || matches!(b[i - 1], b' ' | b'[' | b'{' | b','))
                {
                    quote = Some(c);
                }
            }
        }
        i += 1;
    }
    (s, None)
}

#[cfg(test)]
mod tests;
