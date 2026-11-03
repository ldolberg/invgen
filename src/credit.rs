//! Credit notes: the date of the invoice a credit note refers to, and
//! creating a credit note prefilled from that invoice
//! (`invgen new --credit <NUMBER>`).
//!
//! Convention: a credit note's items are written like the invoice's, with
//! positive quantities and rates. `kind: credit_note` makes the document
//! count negatively (see [`crate::model::Invoice::sign`]): the PDF shows
//! negative line totals and totals, and `invgen list` and the exports net it
//! out.

use crate::config::Config;
use crate::frontmatter::{self, Place};
use crate::model::{Kind, file_stem};
use crate::parse::parse_invoice;
use anyhow::{Context, Result, bail};
use chrono::{Datelike, NaiveDate};
use std::path::{Path, PathBuf};

/// Top-level front matter keys a credit note does not inherit from the
/// invoice it reverses.
const DROPPED_KEYS: &[&str] = &[
    "number",
    "date",
    "due",
    "due_days",
    "status",
    "paid_on",
    "kind",
    "refers_to",
    "refers_to_date",
    crate::lock::KEY,
];

/// Whether front matter `kind` text means a credit note (`credit_note`,
/// `credit-note`, `Credit Note`, `credit`).
pub fn is_credit_note_kind(kind: &str) -> bool {
    matches!(
        kind.trim()
            .to_ascii_lowercase()
            .replace(['-', ' '], "_")
            .as_str(),
        "credit_note" | "credit"
    )
}

/// The written `date` of invoice `number` in `cfg.invoices_dir` (matched like
/// [`crate::lookup::index`]: front matter `number`, else the file name).
/// Only front matter is read; credit notes and files without a valid
/// `date:` are skipped.
pub fn invoice_date(cfg: &Config, number: &str) -> Option<NaiveDate> {
    let number = number.trim();
    crate::lookup::index(&cfg.invoices_dir)
        .into_iter()
        .filter(|f| f.number == number)
        .find_map(|f| {
            let text = std::fs::read_to_string(&f.path).ok()?;
            let kind = frontmatter::get(&text, "kind").ok().flatten();
            if kind.as_deref().is_some_and(is_credit_note_kind) {
                return None;
            }
            let date = frontmatter::get(&text, "date").ok().flatten()?;
            NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()
        })
}

/// The markdown of a credit note dated `today` reversing the invoice whose
/// source is `src`, and its number. The invoice's front matter (client,
/// currency, tax rate, inline parties, `config:`...) and body (items, notes)
/// are kept as written; `kind`, `number`, `date`, `refers_to`,
/// `refers_to_date` and `status: pending` are set, and `due`, `due_days`,
/// `paid_on` and `issued_hash` are removed.
pub fn credit_note_markdown(src: &str, cfg: &Config, today: NaiveDate) -> Result<(String, String)> {
    let original = parse_invoice(src, cfg).map_err(|e| anyhow::anyhow!("{e}"))?;
    if original.kind == Kind::CreditNote {
        bail!(
            "{} is a credit note; credit the invoice it refers to instead",
            original.number
        );
    }
    let mut text = src.to_string();
    for key in DROPPED_KEYS {
        text = frontmatter::remove(&text, key)?;
    }
    // Inserted at the start in reverse, so they read in this order.
    let date = today.format("%Y-%m-%d").to_string();
    let refers_to_date = original.date.format("%Y-%m-%d").to_string();
    for (key, value) in [
        ("status", "pending"),
        ("refers_to_date", refers_to_date.as_str()),
        ("refers_to", original.number.as_str()),
        ("date", date.as_str()),
        ("kind", "credit_note"),
    ] {
        text = frontmatter::set(&text, key, value, Place::Start)?;
    }
    // Number it the way the parser would auto-number it, so a `config:`
    // override of `credit_prefix` in the source is honored.
    let number = parse_invoice(&text, cfg)
        .map(|inv| inv.number)
        .unwrap_or_else(|_| cfg.next_credit_number(today.year()));
    text = frontmatter::set(&text, "number", &number, Place::After(&["kind"]))?;

    let at = frontmatter::body_start(&text)?;
    let note = format!(
        "\n<!-- Credit note for {}. Keep quantities and rates positive: the credit\n     \
         note shows and counts them as negative amounts. Remove or reduce lines\n     \
         to credit only part of the invoice. -->\n",
        original.number
    );
    text.insert_str(at, &note);
    Ok((number, text))
}

/// Write a credit note reversing `invoice` (a number, a bare sequence number
/// or a file, see [`crate::lookup::resolve`]) into `invoices_dir` as
/// `<credit number>.md` and return its path. Fails if the invoice cannot be
/// found, is itself a credit note, or the file already exists.
pub fn write_credit_note(cfg: &Config, invoice: &str, today: NaiveDate) -> Result<PathBuf> {
    let source = crate::lookup::resolve(cfg, invoice)?;
    let src = std::fs::read_to_string(&source)
        .with_context(|| format!("reading {}", source.display()))?;
    let (number, text) = credit_note_markdown(&src, cfg, today)
        .with_context(|| format!("crediting {}", source.display()))?;
    std::fs::create_dir_all(&cfg.invoices_dir)
        .with_context(|| format!("creating {}", cfg.invoices_dir.display()))?;
    let path = cfg.invoices_dir.join(format!("{}.md", file_stem(&number)));
    write_new(&path, &text)?;
    Ok(path)
}

fn write_new(path: &Path, text: &str) -> Result<()> {
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Decimal;

    const INVOICE: &str = "---\nnumber: EINV-2026-20   # explicit\ndate: 2026-10-01\ndue_days: 30\nstatus: paid\npaid_on: 2026-10-20\nissued_hash: 0123456789abcdef\ntax_rate: 21\nto:\n  name: Northwind Labs\n  address: [Madrid]\n---\n\n| Description | Qty | Unit | Rate |\n|---|---|---|---|\n| Consulting | 2 | days | 450 |\n\nThank you!\n";

    fn cfg_in(dir: &Path) -> Config {
        Config {
            invoices_dir: dir.to_path_buf(),
            ..Config::default()
        }
    }

    #[test]
    fn credit_note_from_invoice_parses_and_negates() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("EINV-2026-20.md"), INVOICE).unwrap();
        std::fs::write(dir.path().join("CN-2026-01.pdf"), "").unwrap();
        let cfg = cfg_in(dir.path());
        let today = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let path = write_credit_note(&cfg, "EINV-2026-20", today).unwrap();
        assert_eq!(path, dir.path().join("CN-2026-02.md"));
        let md = std::fs::read_to_string(&path).unwrap();
        assert!(
            md.starts_with(
                "---\nkind: credit_note\nnumber: CN-2026-02\ndate: 2026-11-02\nrefers_to: EINV-2026-20\nrefers_to_date: 2026-10-01\nstatus: pending\ntax_rate: 21\nto:\n"
            ),
            "{md}"
        );
        for key in ["due_days", "paid_on", "issued_hash"] {
            assert!(!md.contains(key), "{key} in {md}");
        }
        let cn = parse_invoice(&md, &cfg).unwrap();
        assert_eq!(cn.kind, Kind::CreditNote);
        assert_eq!(cn.number, "CN-2026-02");
        assert_eq!(cn.date, today);
        assert_eq!(cn.due, None);
        assert_eq!(cn.refers_to.as_deref(), Some("EINV-2026-20"));
        assert_eq!(cn.refers_to_date, NaiveDate::from_ymd_opt(2026, 10, 1));
        assert_eq!(cn.to.name, "Northwind Labs");
        assert_eq!(cn.items[0].quantity, Decimal::from(2));
        assert_eq!(cn.total(), Decimal::from(1089));
        assert_eq!(cn.signed_total(), Decimal::from(-1089));
        assert_eq!(cn.notes, vec!["Thank you!".to_string()]);
        // The invoice sequence is untouched by credit notes.
        assert_eq!(cfg.next_number(2026), "EINV-2026-21");
        // Writing again takes the next credit number; a bare sequence works.
        let again = write_credit_note(&cfg, "20", today).unwrap();
        assert_eq!(again, dir.path().join("CN-2026-03.md"));
        // A credit note cannot be credited.
        let err = write_credit_note(&cfg, "CN-2026-02", today).unwrap_err();
        assert!(format!("{err:#}").contains("is a credit note"), "{err:#}");
        let err = write_credit_note(&cfg, "EINV-2026-99", today).unwrap_err();
        assert!(format!("{err:#}").contains("no invoice"), "{err:#}");
    }

    #[test]
    fn config_override_of_credit_prefix_is_honored() {
        let cfg = cfg_in(Path::new("/nonexistent-invgen-dir"));
        let src = "---\nnumber: X-2026-1\ndate: 2026-10-01\nconfig:\n  credit_prefix: GS\n---\n- A: 1 @ 1\n";
        let today = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let (number, md) = credit_note_markdown(src, &cfg, today).unwrap();
        assert_eq!(number, "GS-2026-01");
        assert!(md.contains("number: GS-2026-01\n"), "{md}");
        // Without front matter, one is created.
        let (_, md) = credit_note_markdown("- A: 1 @ 5\n", &cfg, today).unwrap();
        assert!(md.starts_with("---\nkind: credit_note\n"), "{md}");
        assert!(md.ends_with("- A: 1 @ 5\n"), "{md}");
    }

    #[test]
    fn invoice_date_reads_front_matter_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("2026")).unwrap();
        std::fs::write(dir.path().join("2026/northwind-oct.md"), INVOICE).unwrap();
        let cn = "---\nkind: Credit Note\nnumber: CN-2026-01\ndate: 2026-11-01\nrefers_to: EINV-2026-20\n---\n- A: 1 @ 1\n";
        std::fs::write(dir.path().join("CN-2026-01.md"), cn).unwrap();
        let cfg = cfg_in(dir.path());
        assert_eq!(
            invoice_date(&cfg, " EINV-2026-20 "),
            NaiveDate::from_ymd_opt(2026, 10, 1)
        );
        assert_eq!(invoice_date(&cfg, "CN-2026-01"), None);
        assert_eq!(invoice_date(&cfg, "EINV-2026-21"), None);
        assert!(is_credit_note_kind("credit-note") && !is_credit_note_kind("invoice"));
    }
}
