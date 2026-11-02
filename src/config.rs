//! User configuration (~/.config/invgen/config.yaml). See docs/FORMAT.md.

use crate::model::{Decimal, Party, Payment, num, round_cents};
use anyhow::Context;
use chrono::{Datelike, Local, Months, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Commented starter config written by `invgen config` and by the editor's
/// first-run setup (which fills in the values and keeps the comments).
pub const DEFAULT_CONFIG: &str = r#"# invgen configuration. See docs/FORMAT.md for the invoice format.
# Static data lives here once; invoice files only say what changes.

invoices_dir: ~/Documents/Invoices   # where `invgen new` writes .md and exports .pdf
number_prefix: INV                   # numbers look like INV-<year>-<seq>
currency: EUR
tax_rate: 0                          # percent
# tax_note: reverse_charge           # legal mention under the totals: reverse_charge,
#                                    # exempt, outside_scope or free text
due_days: 14
# language: en                       # labels: en, es, de, fr, it, pt, nl
# labels:                            # override single labels
#   bill_to: Customer
# template: my-invoice.typ           # custom typst template (`invgen template --export`)

from:
  name: Your Name
  address: [Street 1, City, "00000", Country]
  tax: ["VAT ID: XX00000000"]
  email: you@example.com
  # phone: "+00 000 000 000"
  # logo: logo.svg                   # SVG or PNG; relative to this file

payment:
  holder: YOUR NAME
  iban: XX00 0000 0000 0000 0000 00
  bic: XXXXXXXXXXX
  # notes: ["Bank name", "Please quote the invoice number"]

clients:
  acme:
    name: ACME Corp
    address: [Street 1, City, "00000", Country]
    tax: ["Tax ID: X00000000"]
    # currency: EUR      # optional per-client defaults
    # tax_rate: 0
    # tax_note: reverse_charge
    # language: es
    rate: 450            # default rate used by `invgen new`
    unit: days
    description: Consulting services
"#;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    #[serde(flatten)]
    pub party: Party,
    pub currency: Option<String>,
    #[serde(with = "num::option")]
    pub tax_rate: Option<Decimal>,
    #[serde(with = "num::option")]
    pub rate: Option<Decimal>,
    pub unit: Option<String>,
    pub description: Option<String>,
    /// Label language for this client's invoices (overrides the config).
    pub language: Option<String>,
    /// Tax mention for this client's invoices (overrides the config), e.g.
    /// `reverse_charge`. See `Config::tax_note`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub invoices_dir: PathBuf,
    pub number_prefix: String,
    pub currency: String,
    #[serde(with = "num")]
    pub tax_rate: Decimal,
    pub due_days: i64,
    pub from: Party,
    pub payment: Payment,
    pub clients: BTreeMap<String, ClientConfig>,
    /// Label language: en, es, de, fr, it, pt or nl.
    pub language: String,
    /// Overrides for single labels, e.g. `bill_to: Customer`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// Custom typst template replacing the built-in one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<PathBuf>,
    /// Default legal tax mention under the totals: a preset
    /// (`reverse_charge`, `exempt`, `outside_scope`) or free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_note: Option<String>,
    /// Directory of the loaded config file; relative `from.logo` and
    /// `template` paths resolve against it. Not part of the file.
    #[serde(skip)]
    pub dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            invoices_dir: expand_tilde(Path::new("~/Documents/Invoices")),
            number_prefix: "EINV".into(),
            currency: "EUR".into(),
            tax_rate: Decimal::ZERO,
            due_days: 14,
            from: Party::default(),
            payment: Payment::default(),
            clients: BTreeMap::new(),
            language: "en".into(),
            labels: BTreeMap::new(),
            template: None,
            tax_note: None,
            dir: None,
        }
    }
}

impl Config {
    /// ~/.config/invgen/config.yaml
    ///
    /// Inside a Flatpak sandbox `XDG_CONFIG_HOME` points at the app's private
    /// `~/.var/app/<id>/config`, so the real `~/.config` is used instead (the
    /// manifest grants access to `xdg-config/invgen`). That way the native and
    /// the Flatpak builds share one config.
    pub fn path() -> PathBuf {
        let base = if std::env::var_os("FLATPAK_ID").is_some() {
            home().join(".config")
        } else {
            dirs::config_dir().unwrap_or_else(|| home().join(".config"))
        };
        base.join("invgen").join("config.yaml")
    }

    /// Load from `path()`; returns Default if the file does not exist. `~` in invoices_dir is expanded.
    pub fn load() -> anyhow::Result<Config> {
        let path = Self::path();
        if path.exists() {
            Self::load_from(&path)
        } else {
            Ok(Config {
                dir: path.parent().map(Path::to_path_buf),
                ..Config::default()
            })
        }
    }

    pub fn load_from(path: &Path) -> anyhow::Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let mut cfg: Config = if text.trim().is_empty() {
            Config::default()
        } else {
            serde_yaml_ng::from_str(&text)
                .with_context(|| format!("parsing config {}", path.display()))?
        };
        cfg.invoices_dir = expand_tilde(&cfg.invoices_dir);
        cfg.dir = std::path::absolute(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .parent()
            .map(Path::to_path_buf);
        cfg.check_languages()
            .with_context(|| format!("in config {}", path.display()))?;
        Ok(cfg)
    }

    /// Reject unknown `language` values (global and per client).
    pub fn check_languages(&self) -> anyhow::Result<()> {
        crate::render::i18n::normalize_language(&self.language)
            .map_err(|e| anyhow::anyhow!("`language`: {e}"))?;
        for (key, client) in &self.clients {
            if let Some(lang) = &client.language {
                crate::render::i18n::normalize_language(lang)
                    .map_err(|e| anyhow::anyhow!("client `{key}`: `language`: {e}"))?;
            }
        }
        Ok(())
    }

    /// Resolve a user-written path: expand `~`, then make a relative path
    /// relative to the config file's directory (when known).
    pub fn resolve_path(&self, path: &Path) -> PathBuf {
        let p = expand_tilde(path);
        match &self.dir {
            Some(dir) if p.is_relative() => dir.join(p),
            _ => p,
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&Self::path())
    }

    /// Write the config to `path`, creating parent directories. The home
    /// directory in `invoices_dir` is written back as `~`.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let mut out = self.clone();
        out.invoices_dir = collapse_tilde(&out.invoices_dir);
        let yaml = serde_yaml_ng::to_string(&out)?;
        std::fs::write(path, yaml).with_context(|| format!("writing config {}", path.display()))
    }

    /// Next invoice number by scanning invoices_dir for `<prefix>-<year>-<seq>.(md|pdf)`
    /// (also accepts unpadded seq like EINV-2025-1). Seq is global across years.
    pub fn next_number(&self, year: i32) -> String {
        let max = crate::library::discover_with_extensions(&self.invoices_dir, &["md", "pdf"])
            .iter()
            .filter_map(|p| p.file_stem()?.to_str())
            .filter_map(|s| sequence_of(s, &self.number_prefix))
            .max()
            .unwrap_or(0);
        format!(
            "{}-{}-{:02}",
            self.number_prefix,
            year,
            max.saturating_add(1)
        )
    }

    /// Markdown skeleton for a new invoice for `client` (or the first client), dated today,
    /// with one item prefilled from client defaults and "<Month YYYY> period" detail for the previous month.
    pub fn new_invoice_markdown(&self, client: Option<&str>) -> String {
        self.invoice_template(client, Local::now().date_naive())
    }

    /// Write the next invoice for `client` (see `new_invoice_markdown`) into
    /// `invoices_dir` as `<number>.md` and return its path. Fails if that file
    /// already exists. Used by `invgen new` and the editor.
    pub fn write_new_invoice(&self, client: Option<&str>) -> anyhow::Result<PathBuf> {
        let md = self.new_invoice_markdown(client);
        let number = crate::parse::parse_invoice(&md, self)
            .map(|inv| inv.number)
            .unwrap_or_else(|_| self.next_number(Local::now().year()));
        std::fs::create_dir_all(&self.invoices_dir)
            .with_context(|| format!("creating {}", self.invoices_dir.display()))?;
        let path = self
            .invoices_dir
            .join(format!("{}.md", crate::model::file_stem(&number)));
        if path.exists() {
            anyhow::bail!("{} already exists", path.display());
        }
        std::fs::write(&path, md).with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }

    /// `new_invoice_markdown` with an explicit "today", for deterministic output.
    pub fn invoice_template(&self, client: Option<&str>, today: NaiveDate) -> String {
        let key = client
            .map(str::to_string)
            .or_else(|| self.clients.keys().next().cloned());
        let defaults = key.as_deref().and_then(|k| self.clients.get(k));

        let prev = today
            .with_day(1)
            .and_then(|d| d.checked_sub_months(Months::new(1)))
            .unwrap_or(today);
        let period = prev.format("%B %Y").to_string();
        let qty = weekdays_in_month(prev.year(), prev.month());
        let description = defaults
            .and_then(|c| c.description.clone())
            .unwrap_or_else(|| "Professional services".into());
        let unit = defaults
            .and_then(|c| c.unit.clone())
            .unwrap_or_else(|| "days".into());
        let rate = round_cents(defaults.and_then(|c| c.rate).unwrap_or_default());
        let currency = defaults
            .and_then(|c| c.currency.clone())
            .unwrap_or_else(|| self.currency.clone());
        let tax_rate = defaults.and_then(|c| c.tax_rate).unwrap_or(self.tax_rate);

        let mut s = String::new();
        s.push_str("---\n");
        s.push_str(&format!(
            "number: {}   # leave out to auto-number from {}\n",
            self.next_number(today.year()),
            collapse_tilde(&self.invoices_dir).display()
        ));
        s.push_str(&format!("date: {}\n", today.format("%Y-%m-%d")));
        s.push_str(&format!(
            "due_days: {}                # or an explicit `due: YYYY-MM-DD`\n",
            self.due_days
        ));
        s.push_str("status: pending             # pending | paid | overdue | draft\n");
        match &key {
            Some(k) => s.push_str(&format!("client: {k}\n")),
            None => s.push_str(
                "to:                         # no clients in the config yet: inline the client\n  \
                 name: Client Name\n  \
                 address: [Street 1, City, \"00000\", Country]\n  \
                 tax: [\"VAT ID: XX000000\"]\n",
            ),
        }
        s.push_str(&format!(
            "# currency: {currency}               # defaults to the client, then the config\n"
        ));
        s.push_str(&format!(
            "# tax_rate: {tax_rate}                 # percent\n"
        ));
        s.push_str("# signature: false           # hide the signature, or set a custom text\n");
        s.push_str("---\n\n");
        s.push_str(&format!("# {}\n{} period\n\n", description, period));
        s.push_str("| Description | Qty | Unit | Rate |\n");
        s.push_str("|-------------|-----|------|------|\n");
        s.push_str(&format!(
            "| {} | {} | {} | {:.2} |\n\n",
            description.replace('|', "\\|"),
            qty,
            unit,
            rate
        ));
        s.push_str(
            "<!-- Paragraphs below the table become notes on the invoice, e.g.\n     \
             \"Payment within 14 days. Thank you!\" -->\n",
        );
        s
    }
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Expand a leading `~` or `~/` to the home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => home().join(rest),
        Err(_) => path.to_path_buf(),
    }
}

fn collapse_tilde(path: &Path) -> PathBuf {
    match path.strip_prefix(home()) {
        Ok(rest) if home() != Path::new("/") => Path::new("~").join(rest),
        _ => path.to_path_buf(),
    }
}

/// `EINV-2025-02` -> Some(2). Trailing text after the digits is allowed
/// (`EINV-2025-02-acme`), but the year must be exactly four digits.
fn sequence_of(stem: &str, prefix: &str) -> Option<u64> {
    let rest = stem.strip_prefix(prefix)?.strip_prefix('-')?;
    let (year, rest) = rest.split_at_checked(4)?;
    if !year.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let rest = rest.strip_prefix('-')?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn weekdays_in_month(year: i32, month: u32) -> u32 {
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return 0;
    };
    first
        .iter_days()
        .take_while(|d| d.month() == month)
        .filter(|d| !matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_in(dir: &Path) -> Config {
        Config {
            invoices_dir: dir.to_path_buf(),
            ..Config::default()
        }
    }

    #[test]
    fn defaults() {
        let c = Config::default();
        assert_eq!(c.number_prefix, "EINV");
        assert_eq!(c.currency, "EUR");
        assert_eq!(c.tax_rate, Decimal::ZERO);
        assert_eq!(c.due_days, 14);
        assert!(c.invoices_dir.ends_with("Documents/Invoices"));
        assert!(!c.invoices_dir.starts_with("~"));
        assert!(c.clients.is_empty());
    }

    /// A `config: invoices_dir:` override can point the parser at any folder,
    /// so a file named with the largest sequence must not overflow.
    #[test]
    fn next_number_does_not_overflow() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(format!("EINV-2026-{}.md", u64::MAX)), "").unwrap();
        let next = cfg_in(dir.path()).next_number(2026);
        assert_eq!(next, format!("EINV-2026-{}", u64::MAX));
    }

    #[test]
    fn next_number_empty_or_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(cfg_in(dir.path()).next_number(2026), "EINV-2026-01");
        let missing = dir.path().join("nope");
        assert_eq!(cfg_in(&missing).next_number(2026), "EINV-2026-01");
    }

    #[test]
    fn next_number_real_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut names = vec!["EINV-2025-1.pdf".to_string()];
        names.extend((2..=13).map(|n| format!("EINV-2025-{n:02}.pdf")));
        names.extend((14..=22).map(|n| format!("EINV-2026-{n}.md")));
        names.extend((14..=22).map(|n| format!("EINV-2026-{n}.pdf")));
        names.push("EINV-2026-99.txt".into()); // wrong extension
        names.push("OTHER-2026-50.pdf".into()); // wrong prefix
        names.push("EINV-26-40.pdf".into()); // bad year
        for n in names {
            std::fs::write(dir.path().join(n), "").unwrap();
        }
        let cfg = cfg_in(dir.path());
        assert_eq!(cfg.next_number(2026), "EINV-2026-23");
        assert_eq!(cfg.next_number(2027), "EINV-2027-23");
    }

    #[test]
    fn next_number_across_years_and_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("2025")).unwrap();
        std::fs::write(dir.path().join("2025/EINV-2025-13.pdf"), "").unwrap();
        std::fs::write(dir.path().join("EINV-2024-5.md"), "").unwrap();
        assert_eq!(cfg_in(dir.path()).next_number(2026), "EINV-2026-14");
        std::fs::write(dir.path().join("EINV-2026-120-acme.pdf"), "").unwrap();
        assert_eq!(cfg_in(dir.path()).next_number(2026), "EINV-2026-121");
    }

    #[test]
    fn sequence_parsing() {
        assert_eq!(sequence_of("EINV-2025-1", "EINV"), Some(1));
        assert_eq!(sequence_of("EINV-2025-02", "EINV"), Some(2));
        assert_eq!(sequence_of("EINV-2025-", "EINV"), None);
        assert_eq!(sequence_of("EINVX-2025-3", "EINV"), None);
    }

    #[test]
    fn load_save_roundtrip_and_tilde() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/config.yaml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "invoices_dir: ~/Inv\nnumber_prefix: INV\nclients:\n  acme:\n    name: ACME\n    rate: 100\n    unknown_key: 1\n",
        )
        .unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.invoices_dir, home().join("Inv"));
        assert_eq!(cfg.number_prefix, "INV");
        assert_eq!(cfg.currency, "EUR");
        assert_eq!(cfg.clients["acme"].party.name, "ACME");
        assert_eq!(cfg.clients["acme"].rate, Some(Decimal::from(100)));

        let out = dir.path().join("fresh/sub/config.yaml");
        cfg.save_to(&out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("invoices_dir: ~/Inv"), "{text}");
        let back = Config::load_from(&out).unwrap();
        assert_eq!(back.invoices_dir, cfg.invoices_dir);
        assert_eq!(back.clients["acme"].party.name, "ACME");
        assert_eq!(back.clients["acme"].rate, Some(Decimal::from(100)));
        assert_eq!(back.tax_rate, Decimal::ZERO);
    }

    #[test]
    fn rates_accept_numbers_and_percent_strings() {
        let cfg: Config = serde_yaml_ng::from_str(
            "tax_rate: \"21%\"\nclients:\n  a:\n    tax_rate: 7.7\n    rate: \"1,234.50\"\n  b:\n    rate: null\n",
        )
        .unwrap();
        assert_eq!(cfg.tax_rate, Decimal::from(21));
        assert_eq!(cfg.clients["a"].tax_rate, Some(Decimal::new(77, 1)));
        assert_eq!(cfg.clients["a"].rate, Some(Decimal::new(123450, 2)));
        assert_eq!(cfg.clients["b"].rate, None);
        assert!(serde_yaml_ng::from_str::<Config>("tax_rate: lots").is_err());
    }

    #[test]
    fn empty_config_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "\n").unwrap();
        assert_eq!(Config::load_from(&path).unwrap().due_days, 14);
    }

    #[test]
    fn branding_keys_load_and_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            "language: de\nlabels:\n  bill_to: Kunde\ntemplate: t.typ\nfrom:\n  name: Me\n  logo: logo.svg\nclients:\n  acme:\n    name: ACME\n    language: fr\n",
        )
        .unwrap();
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.language, "de");
        assert_eq!(cfg.labels["bill_to"], "Kunde");
        assert_eq!(cfg.clients["acme"].language.as_deref(), Some("fr"));
        assert_eq!(cfg.dir.as_deref(), Some(dir.path()));
        assert_eq!(
            cfg.resolve_path(cfg.from.logo.as_deref().unwrap()),
            dir.path().join("logo.svg")
        );
        assert_eq!(
            cfg.resolve_path(Path::new("/abs.typ")),
            Path::new("/abs.typ")
        );
        assert_eq!(cfg.resolve_path(Path::new("~/x")), home().join("x"));

        // Defaults stay out of a saved config.
        let out = dir.path().join("saved.yaml");
        Config::default().save_to(&out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(!text.contains("labels") && !text.contains("template") && !text.contains("logo"));
        assert!(text.contains("language: en"), "{text}");
    }

    #[test]
    fn unknown_language_in_config_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "language: xx\n").unwrap();
        let err = format!("{:#}", Config::load_from(&path).unwrap_err());
        assert!(err.contains("`xx`") && err.contains("en, es, de"), "{err}");
        std::fs::write(&path, "clients:\n  acme:\n    language: yy\n").unwrap();
        let err = format!("{:#}", Config::load_from(&path).unwrap_err());
        assert!(err.contains("acme") && err.contains("`yy`"), "{err}");
    }

    #[test]
    fn weekdays() {
        assert_eq!(weekdays_in_month(2026, 10), 22);
        assert_eq!(weekdays_in_month(2026, 2), 20);
    }

    #[test]
    fn template_parses_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = cfg_in(dir.path());
        cfg.from.name = "Me".into();
        cfg.clients.insert(
            "acme".into(),
            ClientConfig {
                party: Party {
                    name: "ACME".into(),
                    ..Party::default()
                },
                rate: Some(Decimal::new(45050, 2)),
                unit: Some("days".into()),
                description: Some("Senior Software Engineer".into()),
                ..ClientConfig::default()
            },
        );
        let today = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let md = cfg.invoice_template(None, today);
        assert!(md.contains("client: acme"), "{md}");
        assert!(md.contains("number: EINV-2026-01"), "{md}");
        let inv = crate::parse::parse_invoice(&md, &cfg).unwrap();
        assert_eq!(inv.number, "EINV-2026-01");
        assert_eq!(inv.to.name, "ACME");
        assert_eq!(inv.items.len(), 1);
        let item = &inv.items[0];
        assert_eq!(item.description, "Senior Software Engineer");
        assert_eq!(item.quantity, Decimal::from(22));
        assert_eq!(item.unit, "days");
        assert_eq!(item.rate, Decimal::new(45050, 2));
        assert_eq!(item.detail.as_deref(), Some("October 2026 period"));
        assert!(inv.notes.is_empty(), "{:?}", inv.notes);
        assert_eq!(inv.due, NaiveDate::from_ymd_opt(2026, 11, 16));

        // January wraps to December of the previous year.
        let md = cfg.invoice_template(Some("acme"), NaiveDate::from_ymd_opt(2027, 1, 5).unwrap());
        assert!(md.contains("December 2026 period"), "{md}");
    }

    #[test]
    fn template_without_clients_parses() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = cfg_in(dir.path());
        let md = cfg.invoice_template(None, NaiveDate::from_ymd_opt(2026, 11, 2).unwrap());
        let inv = crate::parse::parse_invoice(&md, &cfg).unwrap();
        assert_eq!(inv.to.name, "Client Name");
        assert_eq!(inv.items[0].description, "Professional services");
    }
}
