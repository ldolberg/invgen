//! End-to-end tests of the `invgen` binary.
//!
//! Every command runs with `INVGEN_CONFIG` pointing at a temp config and `HOME`
//! (plus the XDG dirs) pointing at a temp dir, so nothing touches real files.
//! Assertions use stable substrings, not exact money formatting or column widths.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A sandbox with its own HOME, config file and invoices directory.
struct Env {
    tmp: TempDir,
}

impl Env {
    /// Empty sandbox: no config file yet.
    fn empty() -> Env {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("home")).unwrap();
        Env { tmp }
    }

    /// Sandbox with a config whose invoices_dir is inside the temp dir.
    fn with_config() -> Env {
        let env = Env::empty();
        let config = format!(
            r#"invoices_dir: {}
number_prefix: INV
currency: EUR
tax_rate: 0
due_days: 14

from:
  name: Alex Morgan
  address: [Rue de la Gare, Luxembourg, "1616", Luxembourg]
  email: alex@example.com

payment:
  holder: ALEX MORGAN
  iban: DE89 3704 0044 0532 0130 00
  bic: COBADEFFXXX

clients:
  northwind:
    name: Northwind Labs SL
    address: ["Calle Mayor, 12", Madrid, "28001", Spain]
    rate: 450
    unit: days
    description: Senior Software Engineer
  contoso:
    name: Contoso Ltd
    rate: 80
    unit: hours
    description: Support
"#,
            env.invoices().display()
        );
        fs::write(env.config(), config).unwrap();
        env
    }

    fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    fn config(&self) -> PathBuf {
        self.tmp.path().join("config.yaml")
    }

    fn invoices(&self) -> PathBuf {
        self.tmp.path().join("invoices")
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }

    /// `invgen` with a sandboxed environment and INVGEN_CONFIG set.
    fn cmd(&self) -> Command {
        let mut cmd = self.bare_cmd();
        cmd.env("INVGEN_CONFIG", self.config());
        cmd
    }

    /// `invgen` with a sandboxed environment but no INVGEN_CONFIG.
    fn bare_cmd(&self) -> Command {
        let mut cmd = assert_cmd::cargo_bin_cmd!("invgen");
        cmd.env_remove("INVGEN_CONFIG")
            .env("HOME", self.home())
            .env("XDG_CONFIG_HOME", self.home().join(".config"))
            .env("XDG_DATA_HOME", self.home().join(".local/share"))
            .env("NO_COLOR", "1")
            .env_remove("VISUAL")
            .env_remove("EDITOR")
            .current_dir(self.tmp.path());
        cmd
    }

    /// Write `name` with `body` into the invoices dir and return its path.
    fn invoice(&self, name: &str, body: &str) -> PathBuf {
        fs::create_dir_all(self.invoices()).unwrap();
        let path = self.invoices().join(name);
        fs::write(&path, body).unwrap();
        path
    }
}

const INVOICE: &str = "---
number: INV-2025-7
date: 2025-03-01
status: paid
client: northwind
---

| Description | Qty | Unit | Rate |
|---|---:|---|---:|
| Senior Software Engineer | 20 | days | 450.00 |
";

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn is_pdf(path: &Path) -> bool {
    fs::read(path).is_ok_and(|b| b.starts_with(b"%PDF"))
}

fn is_png(path: &Path) -> bool {
    fs::read(path).is_ok_and(|b| b.starts_with(b"\x89PNG"))
}

/// Trailing sequence number of an invoice file name like `INV-2026-3.md`.
fn seq_of(path: &str) -> u64 {
    let stem = Path::new(path.trim())
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap();
    stem.rsplit('-').next().unwrap().parse().unwrap()
}

// ---------------------------------------------------------------- config

#[test]
fn config_creates_default_file() {
    let env = Env::empty();
    assert!(!env.config().exists());
    env.cmd()
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains(env.config().to_str().unwrap()))
        .stderr(predicate::str::contains("created"));
    let text = fs::read_to_string(env.config()).unwrap();
    assert!(text.contains("invoices_dir:"), "{text}");
    assert!(text.contains("clients:"), "{text}");

    // A second run leaves the existing file alone.
    fs::write(env.config(), "# mine\n").unwrap();
    env.cmd()
        .arg("config")
        .assert()
        .success()
        .stderr(predicate::str::contains("created").not());
    assert_eq!(fs::read_to_string(env.config()).unwrap(), "# mine\n");
}

#[test]
fn config_creates_parent_directories() {
    let env = Env::empty();
    let nested = env.path("a/b/config.yaml");
    env.bare_cmd()
        .env("INVGEN_CONFIG", &nested)
        .arg("config")
        .assert()
        .success();
    assert!(nested.is_file());
}

// ---------------------------------------------------------------- new

#[test]
fn new_numbers_sequentially() {
    let env = Env::with_config();
    let first = env
        .cmd()
        .args(["new", "--client", "northwind"])
        .output()
        .unwrap();
    assert!(first.status.success(), "{first:?}");
    let first = stdout_of(&first);
    let first_path = PathBuf::from(first.trim());
    assert!(first_path.starts_with(env.invoices()), "{first}");
    let md = fs::read_to_string(&first_path).unwrap();
    assert!(md.contains("client: northwind"), "{md}");

    let second = env
        .cmd()
        .args(["new", "--client", "contoso"])
        .output()
        .unwrap();
    assert!(second.status.success(), "{second:?}");
    let second = stdout_of(&second);
    assert_eq!(seq_of(&second), seq_of(&first) + 1, "{first} then {second}");
    let md = fs::read_to_string(second.trim()).unwrap();
    assert!(md.contains("client: contoso"), "{md}");
}

#[test]
fn new_continues_after_highest_existing_number() {
    let env = Env::with_config();
    let year = chrono_year();
    env.invoice(&format!("INV-{year}-41.md"), INVOICE);
    let out = env
        .cmd()
        .args(["new", "--client", "northwind"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(seq_of(&stdout_of(&out)), 42);
}

#[test]
fn new_refuses_to_overwrite() {
    let env = Env::with_config();
    // The number scan only counts files, so a directory named like the next
    // invoice makes `new` pick a path that already exists.
    let year = chrono_year();
    let taken = env.invoices().join(format!("INV-{year}-01.md"));
    fs::create_dir_all(&taken).unwrap();
    fs::write(taken.join("keep.txt"), "keep me").unwrap();
    env.cmd()
        .args(["new", "--client", "northwind"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"))
        .stderr(predicate::str::contains(format!("INV-{year}-01.md")));
    assert!(taken.is_dir());
    assert_eq!(
        fs::read_to_string(taken.join("keep.txt")).unwrap(),
        "keep me"
    );
}

#[test]
fn new_rejects_unknown_client_listing_known_ones() {
    let env = Env::with_config();
    env.cmd()
        .args(["new", "--client", "nobody"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown client"))
        .stderr(predicate::str::contains("nobody"))
        .stderr(predicate::str::contains("northwind"))
        .stderr(predicate::str::contains("contoso"));
    assert!(!env.invoices().exists() || fs::read_dir(env.invoices()).unwrap().next().is_none());
}

// ---------------------------------------------------------------- build

#[test]
fn build_writes_pdf_next_to_markdown() {
    let env = Env::with_config();
    let md = env.invoice("march.md", INVOICE);
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stdout(predicate::str::contains("built"))
        .stdout(predicate::str::contains("INV-2025-7"))
        .stdout(predicate::str::contains("Northwind Labs SL"));
    assert!(is_pdf(&env.invoices().join("INV-2025-7.pdf")));
}

#[test]
fn build_warns_about_zero_tax_without_tax_note() {
    let env = Env::with_config();
    let with_vat_id = |extra: &str| {
        format!(
            "---\nnumber: INV-2025-8\ndate: 2025-03-01\nclient: northwind\n{extra}to:\n  tax: [\"VAT ID: ESB12345678\"]\n---\n\n- Consulting: 2 days @ 450\n"
        )
    };

    // 0% tax, client with a VAT ID, no tax_note: warning, but the build succeeds.
    let md = env.invoice("a.md", &with_vat_id(""));
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stdout(predicate::str::contains("built"))
        .stderr(predicate::str::contains("warning:"))
        .stderr(predicate::str::contains("tax_note"))
        .stderr(predicate::str::contains("a.md"));
    assert!(is_pdf(&env.invoices().join("INV-2025-8.pdf")));

    // A tax note, a non-zero rate, or a client without tax lines: no warning.
    for (name, body) in [
        ("b.md", with_vat_id("tax_note: reverse_charge\n")),
        ("c.md", with_vat_id("tax_note: Not subject to VAT\n")),
        ("d.md", with_vat_id("tax_rate: 21\n")),
        ("e.md", INVOICE.to_string()),
    ] {
        let md = env.invoice(name, &body);
        env.cmd()
            .arg("build")
            .arg(&md)
            .assert()
            .success()
            .stderr(predicate::str::contains("warning").not());
    }
}

#[test]
fn build_mixed_tax_rates_totals() {
    let env = Env::with_config();
    let md = env.invoice(
        "mixed.md",
        "---\nnumber: INV-2025-9\ndate: 2025-03-01\nclient: northwind\ntax_rate: 21\n---\n\n\
         - Consulting: 2 days @ 450\n- Training: 1 days @ 600, tax 10%\n- Books: 2 @ 20, vat 0%\n",
    );
    // 900 + 600 + 40 = 1540; 21% of 900 = 189, 10% of 600 = 60 -> 1789.
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stdout(predicate::str::contains("1,789.00"));
}

#[test]
fn build_output_flag() {
    let env = Env::with_config();
    let md = env.invoice("march.md", INVOICE);
    let out = env.path("out/sub/custom.pdf");
    env.cmd()
        .arg("build")
        .arg(&md)
        .arg("-o")
        .arg(&out)
        .assert()
        .success()
        .stdout(predicate::str::contains("custom.pdf"));
    assert!(is_pdf(&out));
    assert!(!env.invoices().join("INV-2025-7.pdf").exists());
}

#[test]
fn build_png_flag() {
    let env = Env::with_config();
    let md = env.invoice("march.md", INVOICE);
    let png_dir = env.path("png");
    env.cmd()
        .arg("build")
        .arg(&md)
        .arg("--png")
        .arg(&png_dir)
        .assert()
        .success();
    assert!(is_pdf(&env.invoices().join("INV-2025-7.pdf")));
    let pngs: Vec<PathBuf> = fs::read_dir(&png_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(!pngs.is_empty());
    for p in &pngs {
        let name = p.file_name().unwrap().to_str().unwrap();
        assert!(
            name.starts_with("INV-2025-7") && name.ends_with(".png"),
            "{name}"
        );
        assert!(is_png(p), "{name}");
    }
}

#[test]
fn build_bad_input_reports_file_and_line() {
    let env = Env::with_config();
    let md = env.invoice(
        "bad.md",
        "---\nnumber: INV-2025-8\nclient: northwind\n---\n\n| Description | Qty | Rate |\n|---|---|---|\n| Work | 1 | 1.234,50 |\n",
    );
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "{}:8: error:",
            md.display()
        )));
    assert!(!env.invoices().join("INV-2025-8.pdf").exists());
}

#[test]
fn build_unknown_client_in_front_matter_fails() {
    let env = Env::with_config();
    let md = env.invoice(
        "bad.md",
        &INVOICE.replace("client: northwind", "client: nobody"),
    );
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!("{}:", md.display())))
        .stderr(predicate::str::contains("error:"))
        .stderr(predicate::str::contains("nobody"));
}

#[test]
fn build_missing_file_fails() {
    let env = Env::with_config();
    env.cmd()
        .args(["build", "does-not-exist.md"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"))
        .stderr(predicate::str::contains("does-not-exist.md"));
}

// ---------------------------------------------------------------- list

#[test]
fn list_prints_rows_and_yearly_totals() {
    let env = Env::with_config();
    env.invoice("INV-2025-7.md", INVOICE);
    env.invoice(
        "INV-2026-8.md",
        &INVOICE
            .replace("INV-2025-7", "INV-2026-8")
            .replace("2025-03-01", "2026-01-15")
            .replace("status: paid", "status: pending"),
    );
    env.cmd()
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("NUMBER"))
        .stdout(predicate::str::contains("INV-2025-7"))
        .stdout(predicate::str::contains("INV-2026-8"))
        .stdout(predicate::str::contains("Northwind Labs SL"))
        .stdout(predicate::str::contains("2025 total"))
        .stdout(predicate::str::contains("2026 total"));
}

#[test]
fn list_fails_when_a_file_is_skipped() {
    let env = Env::with_config();
    env.invoice("INV-2025-7.md", INVOICE);
    let bad = env.invoice("broken.md", "---\nnumber: INV-2025-9\n---\n\njust a note\n");
    env.cmd()
        .arg("list")
        .assert()
        .failure()
        .stdout(predicate::str::contains("INV-2025-7"))
        .stdout(predicate::str::contains("2025 total"))
        .stderr(predicate::str::contains(bad.display().to_string()))
        .stderr(predicate::str::contains("skipped"));
}

#[test]
fn list_empty_dir() {
    let env = Env::with_config();
    fs::create_dir_all(env.invoices()).unwrap();
    env.cmd()
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("no invoices"));
}

#[test]
fn list_missing_dir_fails() {
    let env = Env::with_config();
    env.cmd()
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

// ---------------------------------------------------------------- --config

#[test]
fn config_flag_matches_env_var() {
    let env = Env::with_config();
    env.invoice("INV-2025-7.md", INVOICE);

    let via_env = env.cmd().arg("list").output().unwrap();
    let via_flag = env
        .bare_cmd()
        .arg("--config")
        .arg(env.config())
        .arg("list")
        .output()
        .unwrap();
    assert!(via_env.status.success(), "{via_env:?}");
    assert!(via_flag.status.success(), "{via_flag:?}");
    assert_eq!(via_env.stdout, via_flag.stdout);
    assert!(stdout_of(&via_flag).contains("INV-2025-7"));

    // The flag is global: it works after the subcommand too.
    env.bare_cmd()
        .arg("list")
        .arg("--config")
        .arg(env.config())
        .assert()
        .success()
        .stdout(predicate::str::contains("INV-2025-7"));
}

#[test]
fn config_flag_overrides_env_var() {
    let env = Env::with_config();
    let other = env.path("other/config.yaml");
    env.cmd()
        .arg("--config")
        .arg(&other)
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains(other.to_str().unwrap()));
    assert!(other.is_file());
}

#[test]
fn config_flag_used_by_new() {
    let env = Env::with_config();
    env.bare_cmd()
        .arg("--config")
        .arg(env.config())
        .args(["new", "--client", "northwind"])
        .assert()
        .success()
        .stdout(predicate::str::contains(env.invoices().to_str().unwrap()));
    assert_eq!(fs::read_dir(env.invoices()).unwrap().count(), 1);
}

// ---------------------------------------------------------------- template

#[test]
fn template_export_to_stdout_and_file() {
    let env = Env::with_config();
    let out = env.cmd().args(["template", "--export"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let builtin = stdout_of(&out);
    assert!(builtin.contains("json(\"/invoice.json\")"), "{builtin}");

    let path = env.path("tpl/my-invoice.typ");
    env.cmd()
        .args(["template", "--export"])
        .arg(&path)
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&path).unwrap(), builtin);

    // Refuses to overwrite without --force.
    fs::write(&path, "edited").unwrap();
    env.cmd()
        .args(["template", "--export"])
        .arg(&path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--force"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "edited");
    env.cmd()
        .args(["template", "--export"])
        .arg(&path)
        .arg("--force")
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&path).unwrap(), builtin);

    // The exported template builds an invoice when used as a custom template.
    let md = env.invoice(
        "custom.md",
        &INVOICE.replace(
            "client: northwind\n",
            &format!("client: northwind\ntemplate: {}\n", path.display()),
        ),
    );
    env.cmd().arg("build").arg(&md).assert().success();
    assert!(is_pdf(&env.invoices().join("INV-2025-7.pdf")));

    // Without --export the subcommand says what it needs.
    env.cmd()
        .arg("template")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--export"));
}

fn chrono_year() -> i32 {
    use chrono::Datelike;
    chrono::Local::now().year()
}
