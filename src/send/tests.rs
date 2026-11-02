use super::*;
use crate::config::Config;
use crate::parse::parse_invoice;
use chrono::NaiveDate;

fn invoice(extra: &str) -> Invoice {
    let src = format!(
        "---\nnumber: EINV-2026-07\ndate: 2026-11-02\ndue: 2026-11-16\n{extra}from:\n  name: Alex Morgan\nto:\n  name: Northwind Labs\n  email: billing@example.com\n---\n- Consulting: 2 days @ 450.50\n"
    );
    parse_invoice(&src, &Config::default()).unwrap()
}

fn message() -> Message {
    compose(
        &invoice(""),
        &EmailConfig::default(),
        Path::new("/tmp/out dir/EINV-2026-07.pdf"),
    )
}

#[test]
fn expands_placeholders() {
    let inv = invoice("");
    assert_eq!(
        expand("{number} {total} {due} {client} {from}", &inv),
        "EINV-2026-07 EUR 901.00 2026-11-16 Northwind Labs Alex Morgan"
    );
    // Unknown or unclosed placeholders stay as written; spaces inside are allowed.
    assert_eq!(
        expand("{nope} { number } {number", &inv),
        "{nope} EINV-2026-07 {number"
    );
    assert_eq!(expand("{{number}}", &inv), "{EINV-2026-07}");
    let mut no_due = invoice("");
    no_due.due = None;
    assert_eq!(expand("due {due}.", &no_due), "due .");
}

#[test]
fn default_templates_follow_the_invoice_language() {
    let msg = message();
    assert_eq!(msg.to.as_deref(), Some("billing@example.com"));
    assert_eq!(msg.subject, "Invoice EINV-2026-07");
    assert!(
        msg.body
            .contains("invoice EINV-2026-07 for EUR 901.00, due 2026-11-16"),
        "{}",
        msg.body
    );
    assert!(msg.body.ends_with("Alex Morgan"));
    let es = compose(
        &invoice("language: es\n"),
        &EmailConfig::default(),
        Path::new("x.pdf"),
    );
    assert_eq!(es.subject, "Factura EINV-2026-07");
    let de = compose(
        &invoice("language: de\n"),
        &EmailConfig::default(),
        Path::new("x.pdf"),
    );
    assert_eq!(de.subject, "Rechnung EINV-2026-07");
}

#[test]
fn config_templates_win() {
    let email = EmailConfig {
        subject: Some("{from}: invoice\n{number}".into()),
        body: Some("Total {total}".into()),
    };
    let msg = compose(&invoice(""), &email, Path::new("x.pdf"));
    assert_eq!(msg.subject, "Alex Morgan: invoice EINV-2026-07");
    assert_eq!(msg.body, "Total EUR 901.00");
}

#[test]
fn no_recipient_without_client_email() {
    let src = "---\nnumber: X-1\nto:\n  name: Northwind Labs\n  email: \"  \"\n---\n- A: 1 @ 1\n";
    let inv = parse_invoice(src, &Config::default()).unwrap();
    let msg = compose(&inv, &EmailConfig::default(), Path::new("x.pdf"));
    assert_eq!(msg.to, None);
    let cmds = commands(&msg, Platform::Unix);
    assert_eq!(cmds[0].args.last().map(String::as_str), Some("x.pdf"));
    assert!(mailto(&msg).starts_with("mailto:?subject="));
}

#[test]
fn unix_uses_xdg_email_with_attachment() {
    let msg = message();
    let cmds = commands(&msg, Platform::Unix);
    assert_eq!(cmds.len(), 2);
    let x = &cmds[0];
    assert_eq!(x.program, "xdg-email");
    assert!(x.attaches);
    assert_eq!(
        x.args,
        [
            "--utf8",
            "--subject",
            "Invoice EINV-2026-07",
            "--body",
            msg.body.as_str(),
            "--attach",
            "/tmp/out dir/EINV-2026-07.pdf",
            "billing@example.com",
        ]
    );
    assert_eq!(cmds[1].program, "xdg-open");
    assert!(!cmds[1].attaches);
    assert!(
        x.display()
            .starts_with("xdg-email --utf8 --subject 'Invoice EINV-2026-07' --body 'Hello,")
    );
}

#[test]
fn mailto_is_percent_encoded() {
    let msg = Message {
        to: Some("billing@example.com".into()),
        subject: "Invoice A&B #1".into(),
        body: "Line 1\nLine 2 = 100%".into(),
        attachment: PathBuf::from("x.pdf"),
    };
    assert_eq!(
        mailto(&msg),
        "mailto:billing@example.com?subject=Invoice%20A%26B%20%231&body=Line%201%0D%0ALine%202%20%3D%20100%25"
    );
    // Non-ASCII is encoded as UTF-8 bytes.
    let msg = Message {
        subject: "Grüße".into(),
        ..msg
    };
    assert!(mailto(&msg).contains("subject=Gr%C3%BC%C3%9Fe"));
}

#[test]
fn macos_uses_mail_then_mailto() {
    let msg = Message {
        subject: "Say \"hi\"".into(),
        body: "a\nb \\ c".into(),
        ..message()
    };
    let cmds = commands(&msg, Platform::MacOs);
    assert_eq!(cmds[0].program, "osascript");
    assert!(cmds[0].attaches);
    let script = &cmds[0].args[1];
    assert!(script.contains("subject:\"Say \\\"hi\\\"\""), "{script}");
    assert!(
        script.contains("\"a\" & linefeed & \"b \\\\ c\""),
        "{script}"
    );
    assert!(
        script.contains("address:\"billing@example.com\""),
        "{script}"
    );
    assert!(
        script.contains("POSIX file \"/tmp/out dir/EINV-2026-07.pdf\""),
        "{script}"
    );
    assert_eq!(cmds[1].program, "open");
    assert!(cmds[1].args[0].starts_with("mailto:billing@example.com?"));
    assert!(!cmds[1].attaches);
}

#[test]
fn windows_uses_mailto() {
    let cmds = commands(&message(), Platform::Windows);
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].program, "explorer.exe");
    assert!(!cmds[0].attaches);
    assert!(cmds[0].args[0].starts_with("mailto:"));
}

#[test]
fn staleness_compares_modification_times() {
    let tmp = tempfile::tempdir().unwrap();
    let md = tmp.path().join("a.md");
    let pdf = tmp.path().join("a.pdf");
    let cfg = tmp.path().join("config.yaml");
    std::fs::write(&md, "x").unwrap();
    std::fs::write(&cfg, "x").unwrap();
    assert!(is_stale(&pdf, &md, Some(&cfg)));
    std::fs::write(&pdf, "x").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    let set = |p: &Path, t| {
        std::fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(t)
            .unwrap()
    };
    set(&md, old);
    set(&cfg, old);
    assert!(!is_stale(&pdf, &md, Some(&cfg)));
    set(&pdf, old - std::time::Duration::from_secs(60));
    assert!(is_stale(&pdf, &md, None));
}

#[test]
fn mark_sent_records_the_date() {
    let src = "---\nnumber: X-1\nstatus: pending # note\n---\nbody\n";
    let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    let out = mark_sent(src, day).unwrap();
    assert_eq!(
        out,
        "---\nnumber: X-1\nstatus: pending # note\nsent_on: 2026-11-02\n---\nbody\n"
    );
    // Sending again updates the date in place.
    let later = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
    assert_eq!(
        mark_sent(&out, later).unwrap(),
        out.replace("2026-11-02", "2026-11-09")
    );
}
