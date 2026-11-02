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
  tax: ["VAT ID: LU12345678"]
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
fn build_payment_qr_code_and_skip_warning() {
    let env = Env::with_config();
    let with_qr = INVOICE.replace(
        "client: northwind\n",
        "client: northwind\npayment:\n  qr: true\n",
    );
    let md = env.invoice("qr.md", &with_qr);
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stderr(predicate::str::contains("QR").not());
    assert!(is_pdf(&env.invoices().join("INV-2025-7.pdf")));

    // Not EUR: still builds, with a warning naming the file and the reason.
    let usd = with_qr.replace("client: northwind\n", "client: northwind\ncurrency: USD\n");
    let md = env.invoice("usd.md", &usd.replace("INV-2025-7", "INV-2025-8"));
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stderr(predicate::str::contains("usd.md"))
        .stderr(predicate::str::contains(
            "warning: payment QR code skipped: currency is USD",
        ));
    assert!(is_pdf(&env.invoices().join("INV-2025-8.pdf")));

    // `config: payment:` turns it on too; a bad IBAN is reported.
    let bad = INVOICE.replace(
        "client: northwind\n",
        "client: northwind\nconfig:\n  payment:\n    qr: true\n    iban: DE00 1234\n",
    );
    let md = env.invoice("bad.md", &bad.replace("INV-2025-7", "INV-2025-9"));
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stderr(predicate::str::contains("IBAN `DE00 1234` is not valid"));
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

/// An invoice for `client` with the given number, date, written status and due date.
fn invoice_md(number: &str, date: &str, status: &str, due: &str, client: &str) -> String {
    format!(
        "---\nnumber: {number}\ndate: {date}\ndue: {due}\nstatus: {status}\nclient: {client}\n---\n\n\
         | Description | Qty | Unit | Rate |\n|---|---:|---|---:|\n| Work, \"special\" | 2 | days | 100.50 |\n"
    )
}

/// A library with paid, long overdue, not yet due and draft invoices for two clients.
fn filter_env() -> Env {
    let env = Env::with_config();
    env.invoice(
        "INV-2025-1.md",
        &invoice_md(
            "INV-2025-1",
            "2025-03-01",
            "paid",
            "2025-03-15",
            "northwind",
        ),
    );
    env.invoice(
        "INV-2025-2.md",
        &invoice_md(
            "INV-2025-2",
            "2025-04-01",
            "pending",
            "2025-04-15",
            "contoso",
        ),
    );
    env.invoice(
        "INV-2026-3.md",
        &invoice_md(
            "INV-2026-3",
            "2026-01-10",
            "pending",
            "2099-01-01",
            "northwind",
        ),
    );
    env.invoice(
        "INV-2026-4.md",
        &invoice_md(
            "INV-2026-4",
            "2026-02-10",
            "draft",
            "2026-02-24",
            "northwind",
        ),
    );
    env
}

#[test]
fn list_filters_combine() {
    let env = filter_env();
    let run = |args: &[&str]| {
        let out = env.cmd().arg("list").args(args).output().unwrap();
        assert!(out.status.success(), "{out:?}");
        stdout_of(&out)
    };
    let has = |out: &str, n: &str| out.lines().any(|l| l.starts_with(&format!("{n} ")));

    let out = run(&["--year", "2026"]);
    assert!(!has(&out, "INV-2025-1") && has(&out, "INV-2026-3") && has(&out, "INV-2026-4"));
    assert!(!out.contains("2025 total"), "{out}");

    let out = run(&["--client", "CONTOSO"]);
    assert!(has(&out, "INV-2025-2") && !has(&out, "INV-2025-1"), "{out}");
    let out = run(&["--client", "northwind labs"]);
    assert!(has(&out, "INV-2025-1") && !has(&out, "INV-2025-2"), "{out}");

    let out = run(&["--unpaid"]);
    assert!(has(&out, "INV-2025-2") && has(&out, "INV-2026-3"), "{out}");
    assert!(
        !has(&out, "INV-2025-1") && !has(&out, "INV-2026-4"),
        "{out}"
    );

    let out = run(&["--status", "overdue", "--status", "draft"]);
    assert!(has(&out, "INV-2025-2") && has(&out, "INV-2026-4"), "{out}");
    assert!(!has(&out, "INV-2026-3"), "{out}");

    let out = run(&["--year", "2026", "--unpaid", "--client", "northwind"]);
    assert!(has(&out, "INV-2026-3") && !has(&out, "INV-2026-4"), "{out}");
    // Totals respect the filters: one invoice of EUR 201.00.
    assert!(
        out.contains("2026 total") && out.contains("EUR 201.00"),
        "{out}"
    );
    assert!(!out.contains("EUR 402.00"), "{out}");

    let out = run(&["--year", "2024"]);
    assert!(out.contains("no invoices match"), "{out}");

    env.cmd()
        .args(["list", "--status", "sent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown status"));
}

#[test]
fn list_prints_outstanding_with_aging() {
    let env = filter_env();
    let out = env.cmd().arg("list").output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let out = stdout_of(&out);
    let header = out
        .lines()
        .find(|l| l.starts_with("OUTSTANDING"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(header.contains("NOT DUE YET") && header.contains("61+ DAYS"));
    let eur: Vec<&str> = out
        .lines()
        .find(|l| l.starts_with("EUR "))
        .unwrap()
        .split_whitespace()
        .collect();
    // not due: INV-2026-3, 61+: INV-2025-2, total of both.
    assert_eq!(eur, ["EUR", "201.00", "0.00", "0.00", "201.00", "402.00"]);

    // Nothing unpaid: no outstanding section.
    let paid = env
        .cmd()
        .args(["list", "--status", "paid"])
        .output()
        .unwrap();
    assert!(!stdout_of(&paid).contains("OUTSTANDING"));
}

#[test]
fn list_exports_csv() {
    let env = filter_env();
    let out = env
        .cmd()
        .args(["list", "--format", "csv", "--client", "northwind"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty());
    let csv = stdout_of(&out);
    let lines: Vec<&str> = csv.split_terminator("\r\n").collect();
    assert_eq!(
        lines[0],
        "number,date,due,status,paid_on,client_key,client,currency,subtotal,tax,total,file"
    );
    assert_eq!(lines.len(), 4, "{csv}");
    let file = env.invoices().join("INV-2025-1.md");
    assert_eq!(
        lines[1],
        format!(
            "INV-2025-1,2025-03-01,2025-03-15,paid,,northwind,Northwind Labs SL,EUR,201.00,0.00,201.00,{}",
            file.display()
        )
    );
}

#[test]
fn list_exports_json() {
    let env = filter_env();
    let out = env
        .cmd()
        .args(["list", "--format", "json", "--unpaid"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = json.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["number"], "INV-2025-2");
    assert_eq!(rows[0]["status"], "overdue");
    assert_eq!(rows[0]["client_key"], "contoso");
    assert_eq!(rows[0]["total"], "201.00");
    assert_eq!(rows[0]["items"][0]["description"], "Work, \"special\"");
    assert_eq!(rows[0]["items"][0]["rate"], "100.5");
    assert_eq!(rows[1]["due"], "2099-01-01");

    let none = env
        .cmd()
        .args(["list", "--format", "json", "--year", "1999"])
        .output()
        .unwrap();
    assert_eq!(stdout_of(&none).trim(), "[]");
}

#[test]
fn list_export_keeps_stdout_clean_on_parse_errors() {
    let env = Env::with_config();
    env.invoice("INV-2025-7.md", INVOICE);
    env.invoice("broken.md", "---\nnumber: INV-2025-9\n---\n\njust a note\n");
    let out = env
        .cmd()
        .args(["list", "--format", "json"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert!(String::from_utf8_lossy(&out.stderr).contains("skipped"));
}

// ---------------------------------------------------------------- check

#[test]
fn check_clean_library() {
    let env = Env::with_config();
    env.invoice("INV-2025-7.md", INVOICE);
    env.invoice(
        "INV-2026-8.md",
        &INVOICE
            .replace("INV-2025-7", "INV-2026-8")
            .replace("2025-03-01", "2026-01-15"),
    );
    env.cmd()
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "2 invoice(s) checked, no problems",
        ));
}

#[test]
fn check_reports_errors_with_file_and_line() {
    let env = Env::with_config();
    let a = env.invoice("INV-2025-7.md", INVOICE);
    let dup = env.invoice("copy.md", INVOICE);
    let late = env.invoice(
        "INV-2025-8.md",
        &INVOICE
            .replace("INV-2025-7", "INV-2025-8")
            .replace("2025-03-01", "2025-01-01"),
    );
    let bad = env.invoice("broken.md", "---\nnumber: INV-2025-9\nstatus: sent\n---\n");
    env.cmd()
        .arg("check")
        .assert()
        .code(1)
        .stdout(predicate::str::contains(format!(
            "{}:2: error: duplicate number INV-2025-7",
            a.display()
        )))
        .stdout(predicate::str::contains(format!(
            "{}:2: error: duplicate number INV-2025-7",
            dup.display()
        )))
        .stdout(predicate::str::contains(format!(
            "{}:3: error: INV-2025-8 is dated 2025-01-01, before the lower number INV-2025-7",
            late.display()
        )))
        .stdout(predicate::str::contains(format!(
            "{}:3: error:",
            bad.display()
        )));
}

#[test]
fn check_warnings_fail_only_with_strict() {
    let env = Env::with_config();
    env.invoice("INV-2025-7.md", INVOICE);
    let gap = env.invoice(
        "INV-2025-10.md",
        &INVOICE.replace("INV-2025-7", "INV-2025-10"),
    );
    let expected = format!(
        "{}:2: warning: gap in the INV sequence before INV-2025-10: 8-9 missing",
        gap.display()
    );
    env.cmd()
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains(expected.as_str()))
        .stdout(predicate::str::contains("0 error(s), 1 warning(s)"));
    env.cmd()
        .args(["check", "--strict"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(expected.as_str()));

    // A PDF in the library fills the gap.
    fs::write(env.invoices().join("INV-2025-8.pdf"), "").unwrap();
    fs::write(env.invoices().join("INV-2025-09.pdf"), "").unwrap();
    let out = env.cmd().arg("check").output().unwrap();
    assert!(!stdout_of(&out).contains("gap"), "{out:?}");
}

#[test]
fn check_missing_dir_fails() {
    let env = Env::with_config();
    env.cmd()
        .arg("check")
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

// ---------------------------------------------------------------- paid / status

const PENDING: &str = "---
number: INV-2026-3   # keep this comment
date: 2026-10-01
due: 2026-10-15
status: pending      # pending | paid
client: northwind
---

# Senior Software Engineer
September 2026 period

| Description | Qty | Unit | Rate |
|---|---:|---|---:|
| Senior Software Engineer | 20 | days | 450.00 |
";

#[test]
fn paid_by_number_changes_only_front_matter_lines() {
    let env = Env::with_config();
    let path = env.invoice("INV-2026-3.md", PENDING);
    env.cmd()
        .args(["paid", "INV-2026-3", "--on", "2026-10-20"])
        .assert()
        .success()
        .stdout(predicate::str::contains("paid on 2026-10-20"));
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(
        text,
        PENDING.replace(
            "status: pending      # pending | paid\n",
            "status: paid         # pending | paid\npaid_on: 2026-10-20\n"
        )
    );
    // `list` shows the payment date.
    env.cmd()
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("paid 2026-10-20"));

    // Back to pending by bare sequence number: the file is as before.
    env.cmd()
        .args(["status", "3", "pending"])
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&path).unwrap(), PENDING);
}

#[test]
fn paid_by_path_defaults_to_today() {
    let env = Env::with_config();
    let path = env.invoice("INV-2026-3.md", PENDING);
    env.cmd()
        .args(["paid", path.to_str().unwrap()])
        .assert()
        .success();
    let today = chrono::Local::now().date_naive().format("%Y-%m-%d");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(&format!("paid_on: {today}\n")), "{text}");
}

#[test]
fn status_errors() {
    let env = Env::with_config();
    env.invoice("INV-2026-3.md", PENDING);
    env.cmd()
        .args(["status", "INV-2026-3", "sent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sent"));
    env.cmd()
        .args(["paid", "INV-2026-9"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no invoice `INV-2026-9`"));
    env.cmd()
        .args(["paid", "INV-2026-3", "--on", "yesterday"])
        .assert()
        .failure();
    // The same number twice is ambiguous.
    fs::create_dir_all(env.invoices().join("copy")).unwrap();
    fs::write(env.invoices().join("copy/INV-2026-3.md"), PENDING).unwrap();
    env.cmd()
        .args(["paid", "INV-2026-3"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("matches 2 invoices"));
}

// ---------------------------------------------------------------- new --from

#[test]
fn new_from_number_copies_and_resets() {
    let env = Env::with_config();
    let paid = PENDING
        .replace("status: pending ", "status: paid    ")
        .replace(
            "client: northwind",
            "paid_on: 2026-10-20\nclient: northwind",
        );
    env.invoice("INV-2026-3.md", &paid);
    let out = env
        .cmd()
        .args(["new", "--from", "INV-2026-3"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let path = PathBuf::from(stdout_of(&out).trim());
    let year = chrono_year();
    assert_eq!(path, env.invoices().join(format!("INV-{year}-04.md")));
    let text = fs::read_to_string(&path).unwrap();
    let today = chrono::Local::now().date_naive();
    assert!(
        text.starts_with(&format!(
            "---\nnumber: INV-{year}-04  # keep this comment\n"
        )),
        "{text}"
    );
    // The source is dated October, so its September period moves forward.
    assert!(text.contains("\nOctober 2026 period\n"), "{text}");
    assert!(
        text.contains(&format!("date: {}\n", today.format("%Y-%m-%d"))),
        "{text}"
    );
    assert!(
        text.contains("status: pending      # pending | paid\n"),
        "{text}"
    );
    assert!(!text.contains("due:"), "{text}");
    assert!(!text.contains("paid_on"), "{text}");
    assert!(
        text.contains("| Senior Software Engineer | 20 | days | 450.00 |"),
        "{text}"
    );
    // The source is left alone and the copy builds.
    assert_eq!(
        fs::read_to_string(env.invoices().join("INV-2026-3.md")).unwrap(),
        paid
    );
    env.cmd()
        .args(["build", path.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn new_from_last_picks_the_highest_number_for_the_client() {
    let env = Env::with_config();
    env.invoice("INV-2026-3.md", PENDING);
    env.invoice(
        "INV-2026-4.md",
        "---\nnumber: INV-2026-4\nclient: contoso\n---\n- Support: 3 hours @ 80\n",
    );
    let out = env
        .cmd()
        .args(["new", "--from", "last", "--client", "northwind"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = fs::read_to_string(stdout_of(&out).trim()).unwrap();
    let year = chrono_year();
    assert!(text.contains(&format!("number: INV-{year}-05")), "{text}");
    assert!(text.contains("client: northwind"), "{text}");

    // The copy (just created, northwind) is now the highest overall.
    let out = env.cmd().args(["new", "--from", "last"]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = fs::read_to_string(stdout_of(&out).trim()).unwrap();
    assert!(text.contains(&format!("number: INV-{year}-06")), "{text}");
    assert!(text.contains("client: northwind"), "{text}");

    env.cmd()
        .args(["new", "--from", "last", "--client", "nobody"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown client `nobody`"));
    env.cmd()
        .args(["new", "--from", "INV-2026-3", "--client", "contoso"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--from last"));
}

#[test]
fn new_from_last_with_empty_library_fails() {
    let env = Env::with_config();
    env.cmd()
        .args(["new", "--from", "last"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no invoices"));
}

// ---------------------------------------------------------------- issued lock

#[test]
fn build_locks_issued_invoice_and_warns_on_change() {
    let env = Env::with_config();
    let md = env.invoice("INV-2025-7.md", INVOICE);
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stdout(predicate::str::contains("locked INV-2025-7"));
    let locked = fs::read_to_string(&md).unwrap();
    assert!(locked.contains("\nissued_hash: \""), "{locked}");
    // Everything else is kept byte for byte.
    let without: String = locked
        .lines()
        .filter(|l| !l.starts_with("issued_hash:"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(without, INVOICE);

    // Rebuilding unchanged: no warning, the hash stays.
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    assert_eq!(fs::read_to_string(&md).unwrap(), locked);

    // Payment tracking does not break the lock.
    fs::write(&md, locked.replace("status: paid", "status: pending")).unwrap();
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    env.cmd().arg("check").assert().success();

    // A config change (new sender address) alters the issued invoice.
    let cfg = fs::read_to_string(env.config()).unwrap();
    fs::write(
        env.config(),
        cfg.replace("Rue de la Gare", "Avenue de la Liberte"),
    )
    .unwrap();
    env.cmd()
        .arg("build")
        .arg(&md)
        .assert()
        .success()
        .stdout(predicate::str::contains("built"))
        .stderr(predicate::str::contains("INV-2025-7 was issued"))
        .stderr(predicate::str::contains("credit note"));
    env.cmd()
        .args(["build", "--strict"])
        .arg(&md)
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
    env.cmd()
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "issued invoice INV-2025-7 changed",
        ));
}

#[test]
fn build_never_locks_drafts() {
    let env = Env::with_config();
    let draft = INVOICE.replace("status: paid", "status: draft");
    let md = env.invoice("INV-2025-7.md", &draft);
    env.cmd().arg("build").arg(&md).assert().success();
    assert_eq!(fs::read_to_string(&md).unwrap(), draft);
}

// ---------------------------------------------------------------- send

/// The INVOICE for a client with an email address.
fn mailable(env: &Env) -> PathBuf {
    env.invoice(
        "INV-2025-7.md",
        &INVOICE.replace(
            "client: northwind\n",
            "client: northwind\nto:\n  email: billing@example.com\n",
        ),
    )
}

#[test]
fn send_dry_run_prints_message_and_command() {
    let env = Env::with_config();
    let md = mailable(&env);
    let before = fs::read_to_string(&md).unwrap();
    let out = env
        .cmd()
        .args(["send", "INV-2025-7", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = stdout_of(&out);
    let pdf = env.invoices().join("INV-2025-7.pdf");
    assert!(
        stdout.contains(&format!("would build {}", pdf.display())),
        "{stdout}"
    );
    assert!(stdout.contains("To: billing@example.com"), "{stdout}");
    assert!(stdout.contains("Subject: Invoice INV-2025-7"), "{stdout}");
    assert!(
        stdout.contains("invoice INV-2025-7 for EUR 9,000.00"),
        "{stdout}"
    );
    assert!(stdout.contains("Alex Morgan"), "{stdout}");
    let launcher = if cfg!(target_os = "macos") {
        "would run: osascript"
    } else if cfg!(windows) {
        "would run: explorer.exe 'mailto:billing@example.com?subject="
    } else {
        "would run: xdg-email --utf8 --subject 'Invoice INV-2025-7'"
    };
    assert!(stdout.contains(launcher), "{stdout}");
    // A dry run builds and changes nothing.
    assert!(!pdf.exists());
    assert_eq!(fs::read_to_string(&md).unwrap(), before);

    // By path too; once built, nothing needs building.
    env.cmd().arg("build").arg(&md).assert().success();
    env.cmd()
        .arg("send")
        .arg(&md)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("would build").not())
        .stdout(predicate::str::contains("Subject: Invoice INV-2025-7"));
}

#[test]
fn send_uses_config_templates_and_warns_without_recipient() {
    let env = Env::with_config();
    let mut cfg = fs::read_to_string(env.config()).unwrap();
    cfg.push_str(
        "email:\n  subject: \"{from} / {number}\"\n  body: \"Due {due}, total {total}, for {client}.\"\n",
    );
    fs::write(env.config(), cfg).unwrap();
    env.invoice("INV-2025-7.md", INVOICE);
    env.cmd()
        .args(["send", "7", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Subject: Alex Morgan / INV-2025-7",
        ))
        .stdout(predicate::str::contains(
            "Due 2025-03-15, total EUR 9,000.00, for Northwind Labs SL.",
        ))
        .stderr(predicate::str::contains("has no `email`"));
}

#[test]
fn send_unknown_invoice_fails() {
    let env = Env::with_config();
    env.invoice("INV-2025-7.md", INVOICE);
    env.cmd()
        .args(["send", "INV-2099-99", "--dry-run"])
        .assert()
        .failure();
}
