//! User configuration (~/.config/invgen/config.yaml). See docs/FORMAT.md.

use crate::model::{Party, Payment};
use anyhow::Context;
use chrono::{Datelike, Local, Months, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    #[serde(flatten)]
    pub party: Party,
    pub currency: Option<String>,
    pub tax_rate: Option<f64>,
    pub rate: Option<f64>,
    pub unit: Option<String>,
    pub description: Option<String>,
    /// Label language for this client's invoices (overrides the config).
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub invoices_dir: PathBuf,
    pub number_prefix: String,
    pub currency: String,
    pub tax_rate: f64,
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
            tax_rate: 0.0,
            due_days: 14,
            from: Party::default(),
            payment: Payment::default(),
            clients: BTreeMap::new(),
            language: "en".into(),
            labels: BTreeMap::new(),
            template: None,
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
        let mut max = 0u64;
        scan_sequences(&self.invoices_dir, &self.number_prefix, 2, &mut max);
        format!("{}-{}-{:02}", self.number_prefix, year, max + 1)
    }

    /// Markdown skeleton for a new invoice for `client` (or the first client), dated today,
    /// with one item prefilled from client defaults and "<Month YYYY> period" detail for the previous month.
    pub fn new_invoice_markdown(&self, client: Option<&str>) -> String {
        self.invoice_template(client, Local::now().date_naive())
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
        let rate = defaults.and_then(|c| c.rate).unwrap_or(0.0);
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

/// Collect the highest sequence number of `<prefix>-YYYY-N[...].(md|pdf)` files
/// in `dir`, descending `depth` levels into subdirectories (e.g. per-year folders).
fn scan_sequences(dir: &Path, prefix: &str, depth: u32, max: &mut u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                scan_sequences(&path, prefix, depth - 1, max);
            }
            continue;
        }
        let ext_ok = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("pdf"));
        if !ext_ok {
            continue;
        }
        if let Some(seq) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| sequence_of(s, prefix))
        {
            *max = (*max).max(seq);
        }
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
        assert_eq!(c.tax_rate, 0.0);
        assert_eq!(c.due_days, 14);
        assert!(c.invoices_dir.ends_with("Documents/Invoices"));
        assert!(!c.invoices_dir.starts_with("~"));
        assert!(c.clients.is_empty());
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
        assert_eq!(cfg.clients["acme"].rate, Some(100.0));

        let out = dir.path().join("fresh/sub/config.yaml");
        cfg.save_to(&out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("invoices_dir: ~/Inv"), "{text}");
        let back = Config::load_from(&out).unwrap();
        assert_eq!(back.invoices_dir, cfg.invoices_dir);
        assert_eq!(back.clients["acme"].party.name, "ACME");
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
                rate: Some(450.0),
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
        assert_eq!(item.quantity, 22.0);
        assert_eq!(item.unit, "days");
        assert_eq!(item.rate, 450.0);
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
