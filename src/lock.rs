//! Issued invoice lock: once a non-draft invoice is exported, its front
//! matter records `issued_hash`, a short hash of the fully resolved invoice
//! (config defaults, client data and overrides applied). A later build that
//! resolves to different content (an edit in the file, or a config change
//! such as a new address) is flagged, because the PDF already sent no
//! longer matches what a rebuild would produce.
//!
//! The written `status`, `paid_on` and the lock fields themselves are left
//! out of the hash, so marking an invoice as paid does not break the lock.

use crate::frontmatter::{self, Place};
use crate::model::{Invoice, Status};
use anyhow::Result;
use sha2::{Digest, Sha256};

/// Front matter key holding the hash.
pub const KEY: &str = "issued_hash";

/// Hex digits kept from the SHA-256 digest (64 bits).
const HASH_LEN: usize = 16;

/// Model fields that do not count as content: payment tracking can change
/// after the invoice is sent without changing what was sent.
const IGNORED: &[&str] = &["status", "paid_on", KEY, "sent_on"];

/// Short SHA-256 of the canonical serialization of `inv`: the JSON of the
/// model (fields in declaration order, maps sorted) without the ignored
/// fields above.
pub fn hash(inv: &Invoice) -> String {
    let mut value = serde_json::to_value(inv).expect("an invoice always serializes");
    if let Some(map) = value.as_object_mut() {
        for key in IGNORED {
            map.remove(*key);
        }
    }
    let canonical = serde_json::to_string(&value).expect("a JSON value always serializes");
    let digest = Sha256::digest(canonical.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex[..HASH_LEN].to_string()
}

/// The lock state of an invoice source and its parsed invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Never exported as an issued invoice (no `issued_hash`), or a draft.
    Open,
    /// Issued and the resolved content still matches.
    Locked,
    /// Issued, but the resolved content changed since.
    Changed { recorded: String, current: String },
}

impl State {
    pub fn is_locked(&self) -> bool {
        !matches!(self, State::Open)
    }
}

/// The recorded `issued_hash` of a source, if any.
pub fn recorded(src: &str) -> Option<String> {
    frontmatter::get(src, KEY)
        .ok()
        .flatten()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
}

/// Compare the recorded hash of `src` with `inv` (parsed from `src`).
/// Drafts are never locked.
pub fn state(src: &str, inv: &Invoice) -> State {
    compare(recorded(src).as_deref(), inv)
}

/// Compare a recorded hash with `inv`. Drafts are never locked.
pub fn compare(recorded: Option<&str>, inv: &Invoice) -> State {
    if inv.status == Status::Draft {
        return State::Open;
    }
    match recorded.map(str::trim).filter(|h| !h.is_empty()) {
        None => State::Open,
        Some(recorded) => {
            let current = hash(inv);
            if current == recorded {
                State::Locked
            } else {
                State::Changed {
                    recorded: recorded.to_string(),
                    current,
                }
            }
        }
    }
}

/// The source with the lock recorded, or `None` when nothing changes (a
/// draft, or already locked: the first export is the one that was sent).
/// A missing `number:` or `date:` is written too, because both would
/// otherwise be resolved again on every build (next free number, today).
pub fn record(src: &str, inv: &Invoice) -> Result<Option<String>> {
    if inv.status == Status::Draft || recorded(src).is_some() {
        return Ok(None);
    }
    let mut text = src.to_string();
    if frontmatter::get(&text, "number")?.is_none_or(|n| n.trim().is_empty()) {
        text = frontmatter::set(&text, "number", &yaml_string(&inv.number), Place::Start)?;
    }
    if frontmatter::get(&text, "date")?.is_none_or(|d| d.trim().is_empty()) {
        let date = inv.date.format("%Y-%m-%d").to_string();
        text = frontmatter::set(&text, "date", &date, Place::After(&["number"]))?;
    }
    // Quoted: an all-digit or `1e5`-like hash must stay a string.
    let value = format!("\"{}\"", hash(inv));
    Ok(Some(frontmatter::set(&text, KEY, &value, Place::End)?))
}

/// `s` as a YAML scalar: plain when that reads back as the same string,
/// else double-quoted.
fn yaml_string(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
        && matches!(
            serde_yaml_ng::from_str::<serde_yaml_ng::Value>(s),
            Ok(serde_yaml_ng::Value::String(_))
        );
    if plain {
        s.to_string()
    } else {
        serde_json::to_string(s).expect("a string always serializes")
    }
}

/// The source without the lock (the editor's "Unlock").
pub fn unlock(src: &str) -> Result<String> {
    frontmatter::remove(src, KEY)
}

/// Multi-line warning for a changed issued invoice, without a leading label.
pub fn changed_message(number: &str) -> String {
    format!(
        "invoice {number} was issued and its resolved content changed since \
         (an edit in the file or in the config, e.g. a new address). The PDF \
         you sent no longer matches. Prefer a credit note and a new invoice; \
         to accept the change, remove `{KEY}` from the front matter"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::parse::parse_invoice;

    const SRC: &str = "---\nnumber: EINV-2026-07  # keep\ndate: 2026-11-02\nstatus: pending\nclient: northwind\n---\n- Consulting: 2 days @ 450\n";

    fn cfg(street: &str) -> Config {
        let yaml = format!(
            "invoices_dir: /nonexistent/invgen-test\nfrom:\n  name: Alex Morgan\n  address: [\"{street}\", Berlin]\nclients:\n  northwind:\n    name: Northwind Labs\n    email: billing@example.com\n"
        );
        serde_yaml_ng::from_str(&yaml).unwrap()
    }

    fn parse(src: &str, cfg: &Config) -> Invoice {
        parse_invoice(src, cfg).unwrap()
    }

    #[test]
    fn hash_is_short_and_stable() {
        let c = cfg("Main St 1");
        let h = hash(&parse(SRC, &c));
        assert_eq!(h.len(), 16);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(h, hash(&parse(SRC, &c)));
    }

    #[test]
    fn record_then_state() {
        let c = cfg("Main St 1");
        let inv = parse(SRC, &c);
        assert_eq!(state(SRC, &inv), State::Open);
        let locked = record(SRC, &inv).unwrap().unwrap();
        // Only one line is added; everything else stays byte for byte.
        assert_eq!(
            locked,
            SRC.replace(
                "---\n-",
                &format!("issued_hash: \"{}\"\n---\n-", hash(&inv))
            )
        );
        assert_eq!(recorded(&locked), Some(hash(&inv)));
        assert_eq!(state(&locked, &parse(&locked, &c)), State::Locked);
        // Already locked: the first export is the one that counts.
        assert_eq!(record(&locked, &inv).unwrap(), None);
        // Unlock removes the line again.
        assert_eq!(unlock(&locked).unwrap(), SRC);
    }

    #[test]
    fn edits_and_config_changes_break_the_lock() {
        let c = cfg("Main St 1");
        let locked = record(SRC, &parse(SRC, &c)).unwrap().unwrap();
        // An edit in the file.
        let edited = locked.replace("2 days", "3 days");
        assert!(matches!(
            state(&edited, &parse(&edited, &c)),
            State::Changed { .. }
        ));
        // A new address in the config.
        let moved = cfg("Harbour Rd 9");
        match state(&locked, &parse(&locked, &moved)) {
            State::Changed {
                recorded: r,
                current,
            } => {
                assert_eq!(Some(r.clone()), recorded(&locked));
                assert_ne!(r, current);
            }
            other => panic!("expected Changed, got {other:?}"),
        }
    }

    #[test]
    fn payment_tracking_does_not_break_the_lock() {
        let c = cfg("Main St 1");
        let locked = record(SRC, &parse(SRC, &c)).unwrap().unwrap();
        let paid = locked.replace(
            "status: pending",
            "status: paid\npaid_on: 2026-11-20\nsent_on: 2026-11-03",
        );
        assert_eq!(state(&paid, &parse(&paid, &c)), State::Locked);
    }

    #[test]
    fn drafts_are_never_locked() {
        let c = cfg("Main St 1");
        let draft = SRC.replace("pending", "draft");
        let inv = parse(&draft, &c);
        assert_eq!(record(&draft, &inv).unwrap(), None);
        // Even with a hash written by hand.
        let with_hash = draft.replace("---\n-", "issued_hash: \"0000\"\n---\n-");
        assert_eq!(state(&with_hash, &parse(&with_hash, &c)), State::Open);
    }

    #[test]
    fn record_pins_number_and_date() {
        let c = cfg("Main St 1");
        let src = "---\nclient: northwind\n---\n- A: 1 @ 100\n";
        let inv = parse(src, &c);
        let locked = record(src, &inv).unwrap().unwrap();
        let today = inv.date.format("%Y-%m-%d").to_string();
        assert_eq!(
            locked,
            format!(
                "---\nnumber: {}\ndate: {today}\nclient: northwind\nissued_hash: \"{}\"\n---\n- A: 1 @ 100\n",
                inv.number,
                hash(&inv)
            )
        );
        assert_eq!(state(&locked, &parse(&locked, &c)), State::Locked);
    }

    #[test]
    fn yaml_strings() {
        assert_eq!(yaml_string("EINV-2026-07"), "EINV-2026-07");
        assert_eq!(yaml_string("2026"), "\"2026\"");
        assert_eq!(yaml_string("A: B"), "\"A: B\"");
        assert_eq!(yaml_string("#1"), "\"#1\"");
    }
}
