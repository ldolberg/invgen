//! Tiny line-based highlighter for invoice markdown (front matter, headings,
//! tables, lists, numbers). Produces an egui `LayoutJob`.

use super::theme::Palette;
use eframe::egui::{
    Color32, FontId, Stroke,
    text::{ByteIndex, LayoutJob, LayoutSection, TextFormat},
};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Key,
    Str,
    Num,
    Comment,
    Punct,
    Heading,
    Bullet,
}

fn color(p: &Palette, k: Kind) -> Color32 {
    match k {
        Kind::Text => p.text,
        Kind::Key => p.syn_key,
        Kind::Str => p.syn_string,
        Kind::Num => p.syn_number,
        Kind::Comment => p.syn_comment,
        Kind::Punct => p.syn_punct,
        Kind::Heading => p.syn_heading,
        Kind::Bullet => p.accent,
    }
}

struct Builder<'a> {
    job: LayoutJob,
    font: FontId,
    p: &'a Palette,
    error_line: bool,
}

impl Builder<'_> {
    /// Like `LayoutJob::append`, but never merged into the previous section.
    fn append_unmerged(&mut self, s: &str, fmt: TextFormat) {
        let start = self.job.text.len();
        self.job.text += s;
        self.job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(start)..ByteIndex(self.job.text.len()),
            format: fmt,
        });
    }

    fn push(&mut self, s: &str, k: Kind) {
        if s.is_empty() {
            return;
        }
        let mut fmt = TextFormat::simple(self.font.clone(), color(self.p, k));
        if self.error_line {
            fmt.underline = Stroke::new(1.0, self.p.error.gamma_multiply(0.5));
        }
        // Coding fonts turn "--", "->", "=>"... into ligatures, and egui cannot
        // switch them off. Shaping is per section, so give each run of
        // punctuation one section per character.
        let mut start = 0;
        for (i, c) in s.char_indices() {
            if c.is_ascii_punctuation() {
                if start < i {
                    self.job.append(&s[start..i], 0.0, fmt.clone());
                }
                self.append_unmerged(&s[i..i + 1], fmt.clone());
                start = i + 1;
            }
        }
        if start < s.len() {
            self.job.append(&s[start..], 0.0, fmt);
        }
    }
}

/// Split `s` into runs of numbers vs other text, emitting numbers in `Num`.
fn push_with_numbers(b: &mut Builder, s: &str, base: Kind) {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut start = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let prev_alnum = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if c.is_ascii_digit() && !prev_alnum {
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_digit()
                    || ((bytes[j] == b'.' || bytes[j] == b',' || bytes[j] == b'-')
                        && j + 1 < bytes.len()
                        && bytes[j + 1].is_ascii_digit()))
            {
                j += 1;
            }
            // Skip identifiers like "2026abc".
            if j < bytes.len() && bytes[j].is_ascii_alphabetic() {
                i = j;
                continue;
            }
            b.push(&s[start..i], base);
            b.push(&s[i..j], Kind::Num);
            start = j;
            i = j;
        } else {
            i += 1;
        }
    }
    b.push(&s[start..], base);
}

/// Find the start of a YAML comment (`#` preceded by whitespace, outside quotes).
fn comment_start(s: &str) -> Option<usize> {
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

fn yaml_value(b: &mut Builder, v: &str) {
    let (body, comment) = match comment_start(v) {
        Some(i) => (&v[..i], &v[i..]),
        None => (v, ""),
    };
    let t = body.trim();
    if t.starts_with('"') || t.starts_with('\'') {
        b.push(body, Kind::Str);
    } else if !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | ',' | '+'))
    {
        b.push(body, Kind::Num);
    } else {
        // Brackets and commas of flow sequences in punct color.
        let mut start = 0;
        for (i, c) in body.char_indices() {
            if matches!(c, '[' | ']' | '{' | '}' | ',') {
                push_with_numbers(b, &body[start..i], Kind::Text);
                b.push(&body[i..i + 1], Kind::Punct);
                start = i + 1;
            }
        }
        push_with_numbers(b, &body[start..], Kind::Text);
    }
    b.push(comment, Kind::Comment);
}

fn yaml_line(b: &mut Builder, line: &str) {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    b.push(indent, Kind::Text);
    if trimmed.starts_with('#') {
        b.push(trimmed, Kind::Comment);
        return;
    }
    let mut rest = trimmed;
    if let Some(r) = rest.strip_prefix("- ") {
        b.push("- ", Kind::Bullet);
        rest = r;
    }
    // key: value
    if let Some(colon) = rest.find(':') {
        let key = &rest[..colon];
        if !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' '))
            && (rest.len() == colon + 1 || rest[colon + 1..].starts_with(' '))
        {
            b.push(key, Kind::Key);
            b.push(":", Kind::Punct);
            yaml_value(b, &rest[colon + 1..]);
            return;
        }
    }
    yaml_value(b, rest);
}

fn table_line(b: &mut Builder, line: &str) {
    let is_sep = line
        .chars()
        .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'));
    if is_sep {
        b.push(line, Kind::Punct);
        return;
    }
    let mut start = 0;
    for (i, c) in line.char_indices() {
        if c == '|' {
            push_with_numbers(b, &line[start..i], Kind::Text);
            b.push("|", Kind::Punct);
            start = i + 1;
        }
    }
    push_with_numbers(b, &line[start..], Kind::Text);
}

fn body_line(b: &mut Builder, line: &str) {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    if trimmed.starts_with('#') {
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        b.push(indent, Kind::Text);
        b.push(&trimmed[..hashes], Kind::Bullet);
        b.push(&trimmed[hashes..], Kind::Heading);
    } else if trimmed.starts_with('|') {
        b.push(indent, Kind::Text);
        table_line(b, trimmed);
    } else if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
    {
        b.push(indent, Kind::Text);
        b.push(&trimmed[..2], Kind::Bullet);
        // "Description: 21 days @ 450"
        match rest.find(':') {
            Some(c) => {
                b.push(&rest[..c], Kind::Text);
                b.push(":", Kind::Punct);
                let tail = &rest[c + 1..];
                let mut start = 0;
                for (i, ch) in tail.char_indices() {
                    if ch == '@' {
                        push_with_numbers(b, &tail[start..i], Kind::Text);
                        b.push("@", Kind::Bullet);
                        start = i + 1;
                    }
                }
                push_with_numbers(b, &tail[start..], Kind::Text);
            }
            None => push_with_numbers(b, rest, Kind::Text),
        }
    } else {
        push_with_numbers(b, line, Kind::Text);
    }
}

/// Highlight the whole document. `error_line` is 1-based (0 = none).
pub fn highlight(text: &str, font: FontId, p: &Palette, error_line: usize) -> LayoutJob {
    let mut b = Builder {
        job: LayoutJob::default(),
        font,
        p,
        error_line: false,
    };
    // Front matter only when the very first line is `---`.
    let mut in_front = false;
    let mut front_done = false;
    let mut in_comment = false;
    for (idx, raw) in text.split_inclusive('\n').enumerate() {
        let (line, nl) = match raw.strip_suffix('\n') {
            Some(l) => (l, "\n"),
            None => (raw, ""),
        };
        b.error_line = error_line == idx + 1;
        let is_delim = line.trim_end() == "---";
        if idx == 0 && is_delim {
            in_front = true;
            b.push(line, Kind::Punct);
        } else if in_front && is_delim && !front_done {
            in_front = false;
            front_done = true;
            b.push(line, Kind::Punct);
        } else if in_front {
            yaml_line(&mut b, line);
        } else if in_comment || line.trim_start().starts_with("<!--") {
            // HTML comments, possibly spanning lines.
            let opened = if in_comment {
                0
            } else {
                line.find("<!--").unwrap_or(0) + 4
            };
            match line[opened..].find("-->") {
                Some(end) => {
                    let end = opened + end + 3;
                    b.push(&line[..end], Kind::Comment);
                    body_line(&mut b, &line[end..]);
                    in_comment = false;
                }
                None => {
                    b.push(line, Kind::Comment);
                    in_comment = true;
                }
            }
        } else {
            body_line(&mut b, line);
        }
        if !nl.is_empty() {
            // Keep the error background off the newline glyph itself.
            b.error_line = false;
            b.push(nl, Kind::Text);
        }
    }
    b.job
}
