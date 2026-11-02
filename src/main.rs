//! `invgen` command line. With no arguments (or `invgen edit FILE`) it opens the GUI.

use anyhow::{Context, Result, bail};
use chrono::{Datelike, Local, NaiveDate};
use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Parser, Subcommand};
use invgen::config::{Config, DEFAULT_CONFIG};
use invgen::model::{Decimal, Invoice, Kind, Status, format_amount, format_money};
use invgen::parse::parse_invoice;
use invgen::render::Renderer;
use invgen::report::{self, Aging, Filter, Library, Severity};
use invgen::{lock, lookup, send};
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "invgen",
    version,
    about = "Markdown invoices to beautiful PDFs"
)]
struct Cli {
    /// Use this config file instead of ~/.config/invgen/config.yaml
    /// (the INVGEN_CONFIG environment variable works too).
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open the editor with live preview (the default when no command is given).
    Edit {
        /// Invoice markdown file to open.
        file: Option<PathBuf>,
    },
    /// Render an invoice to PDF.
    Build {
        /// Invoice markdown file.
        file: PathBuf,
        /// Output PDF (default: <number>.pdf next to the markdown file).
        #[arg(short, long, value_name = "OUT.pdf")]
        output: Option<PathBuf>,
        /// Also write PNG previews of every page into this directory.
        #[arg(long, value_name = "DIR")]
        png: Option<PathBuf>,
        /// Fail instead of warning when an issued invoice changed since it
        /// was first exported (see `issued_hash` in docs/FORMAT.md).
        #[arg(long)]
        strict: bool,
    },
    /// Create the next invoice in the invoices directory.
    New {
        /// Client key from the config (default: the first client). With
        /// `--from last`, duplicate that client's latest invoice.
        #[arg(long, value_name = "KEY")]
        client: Option<String>,
        /// Duplicate an existing invoice (a number, a file, or `last`) instead
        /// of starting from the template.
        #[arg(long, value_name = "NUMBER|last")]
        from: Option<String>,
        /// Create a credit note reversing this invoice instead, prefilled
        /// with its items (numbered with `credit_prefix`).
        #[arg(long, value_name = "NUMBER", conflicts_with_all = ["client", "from"])]
        credit: Option<String>,
        /// Open the new invoice in the editor.
        #[arg(long)]
        open: bool,
    },
    /// Rebuild the PDF every time the markdown file is saved.
    Watch {
        /// Invoice markdown file.
        file: PathBuf,
        /// Output PDF (default: <number>.pdf next to the markdown file).
        #[arg(short, long, value_name = "OUT.pdf")]
        output: Option<PathBuf>,
    },
    /// List the invoices in the invoices directory with yearly totals and
    /// the outstanding balance. Filters combine (AND).
    List {
        /// Only invoices dated in this year.
        #[arg(long, value_name = "YYYY")]
        year: Option<i32>,
        /// Only this client: a config key, or part of the client name
        /// (case-insensitive).
        #[arg(long, value_name = "KEY|NAME")]
        client: Option<String>,
        /// Only invoices with this effective status (pending, paid, overdue,
        /// draft). Repeat to allow several.
        #[arg(long, value_name = "STATUS", value_parser = invgen::report::parse_status)]
        status: Vec<Status>,
        /// Only unpaid invoices (effective status pending or overdue).
        #[arg(long)]
        unpaid: bool,
        /// Output format. csv and json are for scripts and spreadsheets.
        #[arg(long, value_enum, default_value_t = ListFormat::Table)]
        format: ListFormat,
    },
    /// Check the library: duplicate numbers, gaps, dates out of order,
    /// files that fail to parse, missing client name or sender tax lines,
    /// issued invoices that changed since they were exported.
    Check {
        /// Exit with status 1 on warnings too, not only on errors.
        #[arg(long)]
        strict: bool,
    },
    /// Mark an invoice as paid: sets `status: paid` and `paid_on`.
    Paid {
        /// Invoice file, or its number in the invoices directory (EINV-2026-23 or 23).
        invoice: String,
        /// Payment date (default: the date already recorded, else today).
        #[arg(long, value_name = "YYYY-MM-DD")]
        on: Option<NaiveDate>,
    },
    /// Set the written status of an invoice (any status but paid removes `paid_on`).
    Status {
        /// Invoice file, or its number in the invoices directory (EINV-2026-23 or 23).
        invoice: String,
        #[arg(value_parser = PossibleValuesParser::new(["pending", "paid", "overdue", "draft"])
            .map(|s| s.parse::<Status>().unwrap_or_default()))]
        status: Status,
    },
    /// Email an invoice: build the PDF if it is missing or out of date, then
    /// open the desktop mail client with the client's address, a subject,
    /// a body and the PDF attached. invgen never sends mail itself.
    Send {
        /// Invoice markdown file, or an invoice number in the invoices
        /// directory (`EINV-2026-07`, or just `7`).
        #[arg(value_name = "FILE|NUMBER")]
        invoice: String,
        /// Print the message and the command instead of running anything.
        #[arg(long)]
        dry_run: bool,
        /// Record `sent_on: <today>` in the invoice once the mail client opened.
        #[arg(long)]
        mark_sent: bool,
    },
    /// Show the config path, creating a commented default if it is missing.
    Config {
        /// Open the config in $VISUAL / $EDITOR.
        #[arg(long)]
        edit: bool,
    },
    /// Work with typst templates (see docs/TEMPLATES.md).
    Template {
        /// Write the built-in template to PATH (stdout when PATH is omitted),
        /// as a starting point for a custom `template:`.
        #[arg(long, value_name = "PATH", num_args = 0..=1, required = true)]
        export: Option<Option<PathBuf>>,
        /// Overwrite PATH if it already exists.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum ListFormat {
    Table,
    Csv,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            let st = Style::stderr();
            eprintln!("{} {:#}", st.red_bold("error:"), e);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let config_path = cli
        .config
        .or_else(|| std::env::var_os("INVGEN_CONFIG").map(PathBuf::from));
    match cli.command {
        None => invgen::gui::run(None, config_path).map(|_| ExitCode::SUCCESS),
        Some(Cmd::Edit { file }) => invgen::gui::run(file, config_path).map(|_| ExitCode::SUCCESS),
        Some(Cmd::Build {
            file,
            output,
            png,
            strict,
        }) => {
            let cfg = load_config(config_path.as_deref())?;
            let renderer = Renderer::new()?;
            let opts = BuildOpts { lock: true, strict };
            match build(
                &cfg,
                &renderer,
                &file,
                output.as_deref(),
                png.as_deref(),
                opts,
            )? {
                Some(_) => Ok(ExitCode::SUCCESS),
                None => Ok(ExitCode::FAILURE),
            }
        }
        Some(Cmd::New {
            client,
            from,
            credit,
            open,
        }) => {
            let cfg = load_config(config_path.as_deref())?;
            match (from, credit) {
                (_, Some(number)) => cmd_new_credit(&cfg, &number, open, config_path),
                (Some(from), None) => {
                    cmd_new_from(&cfg, &from, client.as_deref(), open, config_path)
                }
                (None, None) => cmd_new(&cfg, client.as_deref(), open, config_path),
            }
        }
        Some(Cmd::Watch { file, output }) => {
            cmd_watch(config_path.as_deref(), &file, output.as_deref())
        }
        Some(Cmd::List {
            year,
            client,
            status,
            unpaid,
            format,
        }) => {
            let cfg = load_config(config_path.as_deref())?;
            let filter = Filter {
                year,
                client,
                statuses: status,
                unpaid,
            };
            cmd_list(&cfg, &filter, format)
        }
        Some(Cmd::Check { strict }) => {
            let cfg = load_config(config_path.as_deref())?;
            cmd_check(&cfg, strict)
        }
        Some(Cmd::Paid { invoice, on }) => {
            let cfg = load_config(config_path.as_deref())?;
            cmd_set_status(&cfg, &invoice, Status::Paid, on)
        }
        Some(Cmd::Status { invoice, status }) => {
            let cfg = load_config(config_path.as_deref())?;
            cmd_set_status(&cfg, &invoice, status, None)
        }
        Some(Cmd::Send {
            invoice,
            dry_run,
            mark_sent,
        }) => {
            let cfg = load_config(config_path.as_deref())?;
            cmd_send(&cfg, config_path.as_deref(), &invoice, dry_run, mark_sent)
        }
        Some(Cmd::Config { edit }) => cmd_config(config_path.as_deref(), edit),
        Some(Cmd::Template { export, force }) => cmd_template(export.flatten().as_deref(), force),
    }
}

fn load_config(path: Option<&Path>) -> Result<Config> {
    match path {
        Some(p) => Config::load_from(p),
        None => Config::load(),
    }
}

/// Parse `file` and print compiler-style errors. `None` means a parse error was reported.
/// The source text comes along for front matter edits (the issued lock).
fn parse_file(cfg: &Config, file: &Path) -> Result<Option<(String, Invoice)>> {
    let src =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    match parse_invoice(&src, cfg) {
        Ok(inv) => Ok(Some((src, inv))),
        Err(e) => {
            let st = Style::stderr();
            let loc = if e.line > 0 {
                format!("{}:{}:", file.display(), e.line)
            } else {
                format!("{}:", file.display())
            };
            eprintln!("{} {} {}", st.bold(&loc), st.red_bold("error:"), e.message);
            Ok(None)
        }
    }
}

fn default_output(file: &Path, inv: &Invoice) -> PathBuf {
    let dir = file.parent().unwrap_or(Path::new("."));
    dir.join(format!("{}.pdf", sanitize(&inv.number)))
}

fn sanitize(s: &str) -> String {
    invgen::model::file_stem(s)
}

#[derive(Clone, Copy)]
struct BuildOpts {
    /// Record `issued_hash` when a non-draft invoice is built for the first
    /// time (`watch` does not: it rebuilds while the invoice is being written).
    lock: bool,
    /// Refuse to build an issued invoice whose content changed.
    strict: bool,
}

/// Parse + render one invoice. Returns the written PDF path, or None on a
/// parse error (or, with `strict`, a changed issued invoice).
fn build(
    cfg: &Config,
    renderer: &Renderer,
    file: &Path,
    output: Option<&Path>,
    png: Option<&Path>,
    opts: BuildOpts,
) -> Result<Option<PathBuf>> {
    let Some((src, inv)) = parse_file(cfg, file)? else {
        return Ok(None);
    };
    if let Some(w) = inv.tax_note_warning() {
        let st = Style::stderr();
        eprintln!(
            "{} {} {}",
            st.bold(&format!("{}:", file.display())),
            st.yellow("warning:"),
            w
        );
    }
    let state = lock::state(&src, &inv);
    if let lock::State::Changed { recorded, current } = &state {
        warn_changed(file, &inv, recorded, current, opts.strict);
        if opts.strict {
            return Ok(None);
        }
    }
    let compiled = renderer
        .compile(&inv)
        .with_context(|| format!("rendering {}", file.display()))?;
    if let Some(w) = invgen::render::epc::warning(&inv) {
        let st = Style::stderr();
        let loc = format!("{}:", file.display());
        eprintln!("{} {} {}", st.bold(&loc), st.yellow("warning:"), w);
    }
    // Lock before writing the PDF, so the PDF stays newer than the markdown
    // (`invgen send` rebuilds a PDF older than its markdown).
    let mut locked = false;
    if opts.lock
        && state == lock::State::Open
        && let Some(text) = lock::record(&src, &inv)?
    {
        std::fs::write(file, text).with_context(|| format!("writing {}", file.display()))?;
        locked = true;
    }
    let out = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_output(file, &inv));
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let pdf = compiled.pdf()?;
    std::fs::write(&out, pdf).with_context(|| format!("writing {}", out.display()))?;

    if let Some(dir) = png {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let pages = compiled.render_pages(2.0);
        let many = pages.len() > 1;
        for (i, page) in pages.iter().enumerate() {
            let name = if many {
                format!("{}-{}.png", sanitize(&inv.number), i + 1)
            } else {
                format!("{}.png", sanitize(&inv.number))
            };
            let path = dir.join(name);
            image::save_buffer(
                &path,
                &page.rgba,
                page.width,
                page.height,
                image::ExtendedColorType::Rgba8,
            )
            .with_context(|| format!("writing {}", path.display()))?;
        }
    }

    let st = Style::stdout();
    println!(
        "{} {}  {}  {}  {}",
        st.green_bold("built"),
        st.bold(&inv.number),
        inv.to.name,
        st.bold(&format_money(&inv.currency, inv.signed_total())),
        st.dim(&out.display().to_string())
    );
    if locked {
        println!(
            "{} {} as issued ({} recorded in {})",
            st.dim("locked"),
            inv.number,
            lock::KEY,
            file.display()
        );
    }
    Ok(Some(out))
}

/// Report an issued invoice whose resolved content changed.
fn warn_changed(file: &Path, inv: &Invoice, recorded: &str, current: &str, error: bool) {
    let st = Style::stderr();
    let label = if error {
        st.red_bold("error:")
    } else {
        st.yellow("warning:")
    };
    eprintln!(
        "{} {} {}",
        st.bold(&format!("{}:", file.display())),
        label,
        lock::changed_message(&inv.number)
    );
    eprintln!(
        "  {} recorded {recorded}, now {current}{}",
        st.dim(&format!("{}:", lock::KEY)),
        if error {
            ""
        } else {
            " (building anyway; --strict refuses)"
        }
    );
}

fn cmd_send(
    cfg: &Config,
    config_path: Option<&Path>,
    arg: &str,
    dry_run: bool,
    mark_sent: bool,
) -> Result<ExitCode> {
    let file = lookup::resolve(cfg, arg)?;
    let Some((src, inv)) = parse_file(cfg, &file)? else {
        return Ok(ExitCode::FAILURE);
    };
    let pdf = default_output(&file, &inv);
    let config_file = config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(Config::path);
    let stale = send::is_stale(&pdf, &file, Some(&config_file));
    let st = Style::stdout();
    let est = Style::stderr();

    if stale && !dry_run {
        let renderer = Renderer::new()?;
        let opts = BuildOpts {
            lock: true,
            strict: false,
        };
        if build(cfg, &renderer, &file, None, None, opts)?.is_none() {
            return Ok(ExitCode::FAILURE);
        }
    } else if let lock::State::Changed { recorded, current } = lock::state(&src, &inv) {
        warn_changed(&file, &inv, &recorded, &current, false);
    }

    let msg = send::compose(&inv, &cfg.email, &pdf);
    if msg.to.is_none() {
        let who = if inv.to.name.is_empty() {
            "the client"
        } else {
            inv.to.name.as_str()
        };
        eprintln!(
            "{} {who} has no `email`; the mail client will ask for the recipient",
            est.yellow("warning:"),
        );
    }
    if dry_run {
        if stale {
            println!("would build {}", pdf.display());
        }
        println!("To: {}", msg.to.as_deref().unwrap_or(""));
        println!("Subject: {}", msg.subject);
        println!("Attachment: {}", msg.attachment.display());
        println!();
        println!("{}", msg.body);
        println!();
        for (i, l) in send::commands(&msg, send::Platform::current())
            .iter()
            .enumerate()
        {
            let what = if i == 0 { "would run" } else { "fallback" };
            println!("{what}: {}", l.display());
        }
        return Ok(ExitCode::SUCCESS);
    }

    let used = send::launch(&msg)?;
    println!(
        "{} {} to {}",
        st.green_bold("opened"),
        st.bold(&inv.number),
        msg.to.as_deref().unwrap_or("(no recipient)")
    );
    if !used.attaches {
        println!(
            "attach the PDF by hand: {}",
            st.bold(&msg.attachment.display().to_string())
        );
    }
    if mark_sent {
        let src = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        let text = send::mark_sent(&src, Local::now().date_naive())?;
        std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
        println!("{} sent_on in {}", st.dim("recorded"), file.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_new(
    cfg: &Config,
    client: Option<&str>,
    open: bool,
    config_path: Option<PathBuf>,
) -> Result<ExitCode> {
    check_client(cfg, client)?;
    let path = cfg.write_new_invoice(client)?;
    println!("{}", path.display());
    if open {
        invgen::gui::run(Some(path), config_path)?;
    }
    Ok(ExitCode::SUCCESS)
}

fn check_client(cfg: &Config, client: Option<&str>) -> Result<()> {
    if let Some(key) = client
        && !cfg.clients.contains_key(key)
    {
        let known: Vec<&str> = cfg.clients.keys().map(String::as_str).collect();
        if known.is_empty() {
            bail!(
                "unknown client `{key}` (no clients in the config yet, see `invgen config --edit`)"
            );
        }
        bail!("unknown client `{key}` (known: {})", known.join(", "));
    }
    Ok(())
}

/// `invgen new --from <NUMBER|FILE|last>`: see [`invgen::duplicate`].
fn cmd_new_from(
    cfg: &Config,
    from: &str,
    client: Option<&str>,
    open: bool,
    config_path: Option<PathBuf>,
) -> Result<ExitCode> {
    check_client(cfg, client)?;
    let source = if from == "last" {
        invgen::lookup::last(cfg, client)?
    } else if client.is_some() {
        bail!("--client only applies to `--from last`")
    } else {
        invgen::lookup::resolve(cfg, from)?
    };
    let today = Local::now().date_naive();
    let path = invgen::duplicate::duplicate_file(cfg, &source, today)?;
    println!("{}", path.display());
    if open {
        invgen::gui::run(Some(path), config_path)?;
    }
    Ok(ExitCode::SUCCESS)
}

/// `invgen new --credit <NUMBER|FILE>`: see [`invgen::credit`].
fn cmd_new_credit(
    cfg: &Config,
    invoice: &str,
    open: bool,
    config_path: Option<PathBuf>,
) -> Result<ExitCode> {
    let today = Local::now().date_naive();
    let path = invgen::credit::write_credit_note(cfg, invoice, today)?;
    println!("{}", path.display());
    if open {
        invgen::gui::run(Some(path), config_path)?;
    }
    Ok(ExitCode::SUCCESS)
}

/// `invgen paid` / `invgen status`: rewrite only the front matter lines.
fn cmd_set_status(
    cfg: &Config,
    invoice: &str,
    status: Status,
    on: Option<NaiveDate>,
) -> Result<ExitCode> {
    let path = invgen::lookup::resolve(cfg, invoice)?;
    let src =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let today = Local::now().date_naive();
    let out = invgen::frontmatter::set_status(&src, status, on, today)
        .with_context(|| format!("editing {}", path.display()))?;
    if out != src {
        std::fs::write(&path, &out).with_context(|| format!("writing {}", path.display()))?;
    }
    let st = Style::stdout();
    let paid_on = invgen::frontmatter::get(&out, "paid_on")?
        .filter(|_| status == Status::Paid)
        .map(|d| format!(" on {d}"))
        .unwrap_or_default();
    println!(
        "{}{}  {}",
        st.green_bold(status.label()),
        paid_on,
        st.dim(&path.display().to_string())
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_watch(config_path: Option<&Path>, file: &Path, output: Option<&Path>) -> Result<ExitCode> {
    use notify::{RecursiveMode, Watcher};

    let file =
        std::fs::canonicalize(file).with_context(|| format!("opening {}", file.display()))?;
    let dir = file.parent().unwrap_or(Path::new("/")).to_path_buf();
    let name = file.file_name().map(|n| n.to_os_string());
    let renderer = Renderer::new()?;
    let st = Style::stdout();

    let rebuild = || {
        let stamp = Local::now().format("%H:%M:%S").to_string();
        print!("{} ", st.dim(&stamp));
        let result = load_config(config_path).and_then(|cfg| {
            let opts = BuildOpts {
                lock: false,
                strict: false,
            };
            build(&cfg, &renderer, &file, output, None, opts)
        });
        match result {
            Ok(Some(_)) => {}
            Ok(None) => println!("{}", st.red_bold("failed")),
            Err(e) => {
                println!("{}", st.red_bold("failed"));
                eprintln!("{} {:#}", Style::stderr().red_bold("error:"), e);
            }
        }
    };

    rebuild();

    // Watch the directory: editors often save by writing a temp file and renaming it.
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            let _ = tx.send(ev);
        }
    })?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    println!(
        "{} {} {}",
        st.dim("watching"),
        file.display(),
        st.dim("(Ctrl+C to stop)")
    );

    let relevant = |ev: &notify::Event| {
        !ev.kind.is_access() && ev.paths.iter().any(|p| p.file_name() == name.as_deref())
    };
    while let Ok(ev) = rx.recv() {
        if !relevant(&ev) {
            continue;
        }
        // Debounce: swallow the burst of events a single save produces.
        while rx.recv_timeout(Duration::from_millis(150)).is_ok() {}
        if file.exists() {
            rebuild();
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_list(cfg: &Config, filter: &Filter, format: ListFormat) -> Result<ExitCode> {
    let st = Style::stdout();
    let dir = &cfg.invoices_dir;
    if !dir.is_dir() {
        bail!(
            "invoices directory {} does not exist (create one with `invgen new`)",
            dir.display()
        );
    }
    let lib = Library::load(cfg);
    let est = Style::stderr();
    for e in &lib.errors {
        let loc = if e.line > 0 {
            format!("{}:{}:", e.path.display(), e.line)
        } else {
            format!("{}:", e.path.display())
        };
        eprintln!(
            "{} {} {}",
            est.bold(&loc),
            est.yellow("skipped:"),
            e.message
        );
    }
    let code = if lib.errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    };
    let today = Local::now().date_naive();
    let rows = filter.apply(&lib.entries, today);
    match format {
        ListFormat::Csv => {
            print!("{}", report::to_csv(&rows, today));
            return Ok(code);
        }
        ListFormat::Json => {
            print!("{}", report::to_json(&rows, today));
            return Ok(code);
        }
        ListFormat::Table => {}
    }
    if rows.is_empty() {
        if lib.entries.is_empty() {
            println!("no invoices in {}", dir.display());
        } else {
            println!("no invoices match the filters");
        }
        return Ok(code);
    }
    let per_year = report::year_totals(&rows);

    let w_num = rows
        .iter()
        .map(|r| r.invoice.number.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let w_client = rows
        .iter()
        .map(|r| r.invoice.to.name.chars().count())
        .max()
        .unwrap_or(6)
        .clamp(6, 32);
    let totals: Vec<String> = rows
        .iter()
        .map(|r| format_money(&r.invoice.currency, r.invoice.signed_total()))
        .collect();
    let w_total = totals
        .iter()
        .map(String::len)
        .chain(
            per_year
                .values()
                .flat_map(|m| m.iter().map(|(c, v)| format_money(c, *v).len())),
        )
        .max()
        .unwrap_or(5)
        .max(5);

    println!(
        "{}",
        st.bold(&format!(
            "{:<w_num$}  {:<10}  {:<w_client$}  {:>w_total$}  {}",
            "NUMBER", "DATE", "CLIENT", "TOTAL", "STATUS"
        ))
    );
    let mut last_year = None;
    for (entry, total) in rows.iter().zip(&totals) {
        let inv = &entry.invoice;
        let year = inv.date.year();
        if last_year.is_some_and(|y| y != year) {
            print_year_total(&st, last_year.unwrap(), &per_year, w_num, w_client, w_total);
        }
        last_year = Some(year);
        let client: String = inv.to.name.chars().take(w_client).collect();
        let status = inv.status_detail(today);
        let status = match inv.effective_status(today) {
            Status::Paid => st.green(&status),
            Status::Overdue => st.red_bold(&status),
            Status::Draft => st.dim(&status),
            Status::Pending => st.yellow(&status),
        };
        // Credit notes are marked after the status (their totals are negative).
        let credit = match (inv.kind, &inv.refers_to) {
            (Kind::CreditNote, Some(r)) => {
                format!("  {}", st.dim(&format!("credit note for {r}")))
            }
            _ => String::new(),
        };
        println!(
            "{:<w_num$}  {}  {:<w_client$}  {:>w_total$}  {}{}",
            inv.number,
            inv.date.format("%Y-%m-%d"),
            client,
            total,
            status,
            credit
        );
    }
    if let Some(y) = last_year {
        print_year_total(&st, y, &per_year, w_num, w_client, w_total);
    }
    print_outstanding(&st, &report::outstanding(&rows, today));
    Ok(code)
}

fn print_year_total(
    st: &Style,
    year: i32,
    per_year: &BTreeMap<i32, BTreeMap<String, Decimal>>,
    w_num: usize,
    w_client: usize,
    w_total: usize,
) {
    let Some(sums) = per_year.get(&year) else {
        return;
    };
    for (currency, sum) in sums {
        let label = format!("{year} total");
        let line = format!(
            "{:<w$}  {:>w_total$}",
            label,
            format_money(currency, *sum),
            w = w_num + 2 + 10 + 2 + w_client
        );
        println!("{}", st.bold(&line));
    }
    println!();
}

/// Outstanding balance per currency with aging buckets (days past due).
fn print_outstanding(st: &Style, outstanding: &BTreeMap<String, Aging>) {
    if outstanding.is_empty() {
        return;
    }
    let header = [
        "OUTSTANDING",
        "NOT DUE YET",
        "1-30 DAYS",
        "31-60 DAYS",
        "61+ DAYS",
        "TOTAL",
    ];
    let rows: Vec<[String; 6]> = outstanding
        .iter()
        .map(|(cur, a)| {
            [
                cur.clone(),
                format_amount(a.not_due),
                format_amount(a.days_1_30),
                format_amount(a.days_31_60),
                format_amount(a.days_61_plus),
                format_amount(a.total()),
            ]
        })
        .collect();
    let widths: Vec<usize> = (0..header.len())
        .map(|i| {
            rows.iter()
                .map(|r| r[i].len())
                .chain([header[i].len()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: [&str; 6]| {
        let mut s = format!("{:<w$}", cells[0], w = widths[0]);
        for (i, c) in cells.iter().enumerate().skip(1) {
            s.push_str(&format!("  {:>w$}", c, w = widths[i]));
        }
        s
    };
    println!("{}", st.bold(&line(header)));
    for r in &rows {
        println!("{}", line(r.each_ref().map(String::as_str)));
    }
}

fn cmd_check(cfg: &Config, strict: bool) -> Result<ExitCode> {
    let dir = &cfg.invoices_dir;
    if !dir.is_dir() {
        bail!(
            "invoices directory {} does not exist (create one with `invgen new`)",
            dir.display()
        );
    }
    let lib = Library::load(cfg);
    let diags = report::check(&lib, &report::pdf_stems(cfg));
    let st = Style::stdout();
    for d in &diags {
        let sev = match d.severity {
            Severity::Error => st.red_bold("error:"),
            Severity::Warning => st.yellow("warning:"),
        };
        println!("{} {} {}", st.bold(&d.location()), sev, d.message);
    }
    let errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diags.len() - errors;
    let checked = lib.entries.len() + lib.errors.len();
    if diags.is_empty() {
        println!(
            "{} {checked} invoice(s) checked, no problems",
            st.green_bold("ok:")
        );
    } else {
        println!("{checked} invoice(s) checked: {errors} error(s), {warnings} warning(s)");
    }
    Ok(if errors > 0 || (strict && warnings > 0) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn cmd_template(path: Option<&Path>, force: bool) -> Result<ExitCode> {
    let Some(path) = path else {
        print!("{}", invgen::render::TEMPLATE);
        return Ok(ExitCode::SUCCESS);
    };
    if path.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            path.display()
        );
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(path, invgen::render::TEMPLATE)
        .with_context(|| format!("writing {}", path.display()))?;
    println!("{}", path.display());
    Ok(ExitCode::SUCCESS)
}

/// Editor for `invgen config --edit` when neither VISUAL nor EDITOR is set.
const DEFAULT_EDITOR: &str = if cfg!(windows) { "notepad" } else { "nano" };

fn cmd_config(config_path: Option<&Path>, edit: bool) -> Result<ExitCode> {
    let path = config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(Config::path);
    let st = Style::stdout();
    if !path.exists() {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&path, DEFAULT_CONFIG)
            .with_context(|| format!("writing {}", path.display()))?;
        eprintln!("{} {}", st.green_bold("created"), path.display());
    }
    println!("{}", path.display());
    if edit {
        let editor = std::env::var("VISUAL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("EDITOR")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_EDITOR.into());
        let mut parts = editor.split_whitespace();
        let prog = parts.next().unwrap_or(DEFAULT_EDITOR);
        let status = std::process::Command::new(prog)
            .args(parts)
            .arg(&path)
            .status()
            .with_context(|| format!("launching editor `{editor}`"))?;
        if !status.success() {
            bail!("editor `{editor}` exited with {status}");
        }
        // Validate what was saved so mistakes surface right away.
        if let Err(e) = Config::load_from(&path) {
            eprintln!("{} {:#}", Style::stderr().yellow("warning:"), e);
            return Ok(ExitCode::FAILURE);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// ANSI styling, enabled only when the stream is a terminal and NO_COLOR is unset.
struct Style {
    on: bool,
}

impl Style {
    fn new(tty: bool) -> Style {
        Style {
            on: tty && std::env::var_os("NO_COLOR").is_none(),
        }
    }
    fn stdout() -> Style {
        Style::new(std::io::stdout().is_terminal())
    }
    fn stderr() -> Style {
        Style::new(std::io::stderr().is_terminal())
    }
    fn paint(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
    fn dim(&self, s: &str) -> String {
        self.paint("2", s)
    }
    fn green(&self, s: &str) -> String {
        self.paint("32", s)
    }
    fn green_bold(&self, s: &str) -> String {
        self.paint("1;32", s)
    }
    fn yellow(&self, s: &str) -> String {
        self.paint("33", s)
    }
    fn red_bold(&self, s: &str) -> String {
        self.paint("1;31", s)
    }
}
