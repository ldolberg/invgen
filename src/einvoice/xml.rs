//! A tiny XML writer: elements, attributes and escaped text, indented two
//! spaces per level. Enough for the CII and UBL documents, with no
//! dependency. All text goes through [`escape`], so user input can never
//! produce markup.

pub(super) struct Xml {
    out: String,
    stack: Vec<&'static str>,
}

impl Xml {
    /// A document starting with the XML declaration.
    pub fn new() -> Xml {
        Xml {
            out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
            stack: Vec::new(),
        }
    }

    fn start_tag(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        for _ in 0..self.stack.len() {
            self.out.push_str("  ");
        }
        self.out.push('<');
        self.out.push_str(tag);
        for (k, v) in attrs {
            self.out.push(' ');
            self.out.push_str(k);
            self.out.push_str("=\"");
            self.out.push_str(&escape(v));
            self.out.push('"');
        }
    }

    /// `<tag attrs>` on its own line; children follow until [`Xml::close`].
    pub fn open(&mut self, tag: &'static str, attrs: &[(&str, &str)]) {
        self.start_tag(tag, attrs);
        self.out.push_str(">\n");
        self.stack.push(tag);
    }

    /// Close the innermost open element.
    pub fn close(&mut self) {
        let tag = self.stack.pop().expect("close without open");
        for _ in 0..self.stack.len() {
            self.out.push_str("  ");
        }
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    /// `<tag attrs>text</tag>`.
    pub fn leaf_attrs(&mut self, tag: &str, attrs: &[(&str, &str)], text: &str) {
        self.start_tag(tag, attrs);
        self.out.push('>');
        self.out.push_str(&escape(text));
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    pub fn leaf(&mut self, tag: &str, text: &str) {
        self.leaf_attrs(tag, &[], text);
    }

    /// A leaf only when there is a value.
    pub fn opt(&mut self, tag: &str, text: Option<&str>) {
        if let Some(t) = text {
            self.leaf(tag, t);
        }
    }

    pub fn finish(self) -> String {
        assert!(self.stack.is_empty(), "unclosed elements: {:?}", self.stack);
        self.out
    }
}

/// Escape text for element content and attribute values, dropping
/// characters XML 1.0 does not allow (control characters other than tab,
/// newline and carriage return).
pub(super) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_and_drops_control_chars() {
        assert_eq!(
            escape("a<b>&\"c'\u{1}\td"),
            "a&lt;b&gt;&amp;&quot;c&apos;\td"
        );
    }

    #[test]
    fn writes_nested_elements() {
        let mut x = Xml::new();
        x.open("a", &[("xmlns", "urn:x")]);
        x.leaf_attrs("b", &[("unit", "D&Y")], "1 < 2");
        x.opt("c", None);
        x.close();
        assert_eq!(
            x.finish(),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a xmlns=\"urn:x\">\n  \
             <b unit=\"D&amp;Y\">1 &lt; 2</b>\n</a>\n"
        );
    }
}
