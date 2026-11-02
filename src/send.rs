//! `invgen send` and the editor's Send button: open the desktop mail client
//! with a prefilled message and the PDF attached.
//!
//! invgen never talks to a mail server and never stores credentials: it only
//! hands the message to the user's own mail client, which sends it.
//!
//! - Linux and BSD: `xdg-email` (recipient, subject, body and attachment).
//! - macOS: Mail through `osascript` (all fields and the attachment), else a
//!   `mailto:` link (no attachment, so the PDF path is printed).
//! - Windows: a `mailto:` link (no attachment, so the PDF path is printed).

use crate::config::EmailConfig;
use crate::model::{Invoice, format_money};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A message ready to hand to the mail client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Recipient (the client's `email`), if known.
    pub to: Option<String>,
    pub subject: String,
    pub body: String,
    pub attachment: PathBuf,
}

/// Default subject and body per language. `{...}` are placeholders.
fn defaults(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "es" => (
            "Factura {number}",
            "Hola,\n\nadjunto la factura {number} por {total}, con vencimiento el {due}.\n\nSaludos,\n{from}",
        ),
        "de" => (
            "Rechnung {number}",
            "Hallo,\n\nanbei die Rechnung {number} über {total}, fällig am {due}.\n\nViele Grüße\n{from}",
        ),
        "fr" => (
            "Facture {number}",
            "Bonjour,\n\nveuillez trouver ci-joint la facture {number} d'un montant de {total}, à régler avant le {due}.\n\nCordialement,\n{from}",
        ),
        "it" => (
            "Fattura {number}",
            "Buongiorno,\n\nin allegato la fattura {number} di {total}, con scadenza il {due}.\n\nCordiali saluti,\n{from}",
        ),
        "pt" => (
            "Fatura {number}",
            "Olá,\n\nsegue em anexo a fatura {number} no valor de {total}, com vencimento em {due}.\n\nCumprimentos,\n{from}",
        ),
        "nl" => (
            "Factuur {number}",
            "Hallo,\n\nin de bijlage vindt u factuur {number} van {total}, te betalen vóór {due}.\n\nMet vriendelijke groet,\n{from}",
        ),
        _ => (
            "Invoice {number}",
            "Hello,\n\nplease find attached invoice {number} for {total}, due {due}.\n\nBest regards,\n{from}",
        ),
    }
}

/// Replace `{number}`, `{total}`, `{due}`, `{client}` and `{from}` in
/// `template`. Unknown placeholders are kept as written. `{due}` is the due
/// date as `YYYY-MM-DD`, or empty when the invoice has none.
pub fn expand(template: &str, inv: &Invoice) -> String {
    let due = inv
        .due
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let value = |name: &str| -> Option<String> {
        Some(match name {
            "number" => inv.number.clone(),
            "total" => format_money(&inv.currency, inv.total()),
            "due" => due.clone(),
            "client" => inv.to.name.clone(),
            "from" => inv.from.name.clone(),
            _ => return None,
        })
    };
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').and_then(|close| {
            let name = &after[..close];
            value(name.trim()).map(|v| (v, close))
        }) {
            Some((v, close)) => {
                out.push_str(&v);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The message for `inv` with `pdf` attached. Templates come from the
/// config's `email:` block, else the defaults in the invoice language.
pub fn compose(inv: &Invoice, email: &EmailConfig, pdf: &Path) -> Message {
    let (subject, body) = defaults(&inv.language);
    let subject = email.subject.as_deref().unwrap_or(subject);
    let body = email.body.as_deref().unwrap_or(body);
    Message {
        to: inv
            .to
            .email
            .as_deref()
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(str::to_string),
        // A subject is one line.
        subject: expand(subject, inv).lines().collect::<Vec<_>>().join(" "),
        body: expand(body, inv),
        attachment: pdf.to_path_buf(),
    }
}

/// Target platform for [`commands`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Linux, the BSDs and other freedesktop systems.
    Unix,
    MacOs,
    Windows,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Unix
        }
    }
}

/// One way to open the mail client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// Whether the PDF is attached this way. When not, the user is told to
    /// attach the file at [`Message::attachment`] by hand.
    pub attaches: bool,
}

impl Launch {
    /// The command line as one string, for `--dry-run` and messages.
    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .map(shell_quote)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The commands to try, in order, to open the mail client on `platform`.
/// The first one that starts wins.
pub fn commands(msg: &Message, platform: Platform) -> Vec<Launch> {
    let mailto = |program: &str| Launch {
        program: program.to_string(),
        args: vec![mailto(msg)],
        attaches: false,
    };
    match platform {
        Platform::Unix => {
            let mut args = vec![
                "--utf8".to_string(),
                "--subject".into(),
                msg.subject.clone(),
                "--body".into(),
                msg.body.clone(),
                "--attach".into(),
                msg.attachment.display().to_string(),
            ];
            args.extend(msg.to.clone());
            vec![
                Launch {
                    program: "xdg-email".into(),
                    args,
                    attaches: true,
                },
                mailto("xdg-open"),
            ]
        }
        Platform::MacOs => vec![
            Launch {
                program: "osascript".into(),
                args: vec!["-e".into(), apple_script(msg)],
                attaches: true,
            },
            mailto("open"),
        ],
        // explorer.exe hands the link to ShellExecute without shell quoting.
        Platform::Windows => vec![mailto("explorer.exe")],
    }
}

/// Start the first command of [`commands`] that launches. Returns it, so
/// the caller can tell the user to attach the PDF when it does not.
pub fn launch(msg: &Message) -> Result<Launch> {
    if !msg.attachment.is_file() {
        bail!("{} does not exist", msg.attachment.display());
    }
    let mut last_err = None;
    for l in commands(msg, Platform::current()) {
        let res = Command::new(&l.program)
            .args(&l.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match res {
            Ok(status) if status.success() => return Ok(l),
            Ok(status) => last_err = Some(anyhow::anyhow!("`{}` exited with {status}", l.program)),
            Err(e) => {
                last_err = Some(anyhow::Error::new(e).context(format!("running `{}`", l.program)))
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| anyhow::anyhow!("no mail command for this platform"))
        .context("opening the mail client"))
}

/// `mailto:` URL with recipient, subject and body (RFC 6068). Attachments
/// cannot be passed this way.
pub fn mailto(msg: &Message) -> String {
    let to = msg.to.as_deref().map(encode).unwrap_or_default();
    // Line breaks in a mailto body are CRLF.
    let body = msg.body.replace("\r\n", "\n").replace('\n', "\r\n");
    format!(
        "mailto:{to}?subject={}&body={}",
        encode(&msg.subject),
        encode(&body)
    )
}

/// Percent-encode everything but RFC 3986 unreserved characters (and `@`,
/// which is common in addresses and allowed in `mailto:`).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'@') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// An AppleScript string literal: quotes and backslashes escaped, line
/// breaks as `linefeed`.
fn apple_string(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    let parts: Vec<String> = escaped
        .replace("\r\n", "\n")
        .split('\n')
        .map(|p| format!("\"{p}\""))
        .collect();
    parts.join(" & linefeed & ")
}

/// AppleScript that opens a new Mail message with every field filled in.
fn apple_script(msg: &Message) -> String {
    let recipient = msg
        .to
        .as_deref()
        .map(|to| {
            format!(
                "\n    make new to recipient at end of to recipients with properties {{address:{}}}",
                apple_string(to)
            )
        })
        .unwrap_or_default();
    format!(
        "tell application \"Mail\"\n  \
         set m to make new outgoing message with properties {{subject:{}, content:{} & linefeed & linefeed, visible:true}}\n  \
         tell m{recipient}\n    \
         tell content\n      \
         make new attachment with properties {{file name:(POSIX file {})}} at after the last paragraph\n    \
         end tell\n  \
         end tell\n  \
         activate\n\
         end tell",
        apple_string(&msg.subject),
        apple_string(&msg.body),
        apple_string(&msg.attachment.display().to_string()),
    )
}

/// Quote a word for display as a POSIX shell command.
fn shell_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c));
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// The PDF of `md` is missing or older than the markdown file or the config.
pub fn is_stale(pdf: &Path, md: &Path, config: Option<&Path>) -> bool {
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let Some(built) = modified(pdf) else {
        return true;
    };
    [Some(md), config]
        .into_iter()
        .flatten()
        .filter_map(modified)
        .any(|t| t > built)
}

/// Record `sent_on: <today>` in an invoice source (byte-preserving).
pub fn mark_sent(src: &str, today: chrono::NaiveDate) -> Result<String> {
    use crate::frontmatter::{Place, set};
    set(
        src,
        "sent_on",
        &today.format("%Y-%m-%d").to_string(),
        Place::After(&["paid_on", "status"]),
    )
    .context("recording sent_on")
}

#[cfg(test)]
mod tests;
