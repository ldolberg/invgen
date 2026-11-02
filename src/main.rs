//! `invgen` command line. With no arguments (or `invgen edit FILE`) it opens the GUI.

use anyhow::{Context, Result, bail};
use chrono::{Datelike, Local};
use clap::{Parser, Subcommand};
use invgen::config::{Config, DEFAULT_CONFIG};
use invgen::model::{Decimal, Invoice, Status, format_money};
use invgen::parse::parse_invoice;
use invgen::render::Renderer;
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
    },
    /// Create the next invoice in the invoices directory.
    New {
        /// Client key from the config (default: the first client).
        #[arg(long, value_name = "KEY")]
        client: Option<String>,
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
    /// List the invoices in the invoices directory with yearly totals.
    List,
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
        Some(Cmd::Build { file, output, png }) => {
            let cfg = load_config(config_path.as_deref())?;
            let renderer = Renderer::new()?;
            match build(&cfg, &renderer, &file, output.as_deref(), png.as_deref())? {
                Some(_) => Ok(ExitCode::SUCCESS),
                None => Ok(ExitCode::FAILURE),
            }
        }
        Some(Cmd::New { client, open }) => {
            let cfg = load_config(config_path.as_deref())?;
            cmd_new(&cfg, client.as_deref(), open, config_path)
        }
        Some(Cmd::Watch { file, output }) => {
            cmd_watch(config_path.as_deref(), &file, output.as_deref())
        }
        Some(Cmd::List) => {
            let cfg = load_config(config_path.as_deref())?;
            cmd_list(&cfg)
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
fn parse_file(cfg: &Config, file: &Path) -> Result<Option<Invoice>> {
    let src =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    match parse_invoice(&src, cfg) {
        Ok(inv) => Ok(Some(inv)),
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

/// Parse + render one invoice. Returns the written PDF path, or None on a parse error.
fn build(
    cfg: &Config,
    renderer: &Renderer,
    file: &Path,
    output: Option<&Path>,
    png: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let Some(inv) = parse_file(cfg, file)? else {
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
    let compiled = renderer
        .compile(&inv)
        .with_context(|| format!("rendering {}", file.display()))?;
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
        st.bold(&format_money(&inv.currency, inv.total())),
        st.dim(&out.display().to_string())
    );
    Ok(Some(out))
}

fn cmd_new(
    cfg: &Config,
    client: Option<&str>,
    open: bool,
    config_path: Option<PathBuf>,
) -> Result<ExitCode> {
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
    let path = cfg.write_new_invoice(client)?;
    println!("{}", path.display());
    if open {
        invgen::gui::run(Some(path), config_path)?;
    }
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
        let result =
            load_config(config_path).and_then(|cfg| build(&cfg, &renderer, &file, output, None));
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

fn cmd_list(cfg: &Config) -> Result<ExitCode> {
    let st = Style::stdout();
    let dir = &cfg.invoices_dir;
    if !dir.is_dir() {
        bail!(
            "invoices directory {} does not exist (create one with `invgen new`)",
            dir.display()
        );
    }
    let files = invgen::library::discover(dir);
    let today = Local::now().date_naive();
    let mut rows = Vec::new();
    let mut failed = false;
    for f in &files {
        let Ok(src) = std::fs::read_to_string(f) else {
            continue;
        };
        match parse_invoice(&src, cfg) {
            Ok(inv) => rows.push(inv),
            Err(e) => {
                failed = true;
                let loc = if e.line > 0 {
                    format!("{}:{}:", f.display(), e.line)
                } else {
                    format!("{}:", f.display())
                };
                eprintln!(
                    "{} {} {}",
                    Style::stderr().bold(&loc),
                    Style::stderr().yellow("skipped:"),
                    e.message
                );
            }
        }
    }
    if rows.is_empty() {
        println!("no invoices in {}", dir.display());
        return Ok(ExitCode::SUCCESS);
    }
    // Sequence numbers are global and monotonic, so they give the issue order.
    rows.sort_by(|a, b| {
        (seq_of(&a.number), a.date, &a.number).cmp(&(seq_of(&b.number), b.date, &b.number))
    });
    // year -> currency -> sum
    let mut per_year: BTreeMap<i32, BTreeMap<String, Decimal>> = BTreeMap::new();
    for inv in &rows {
        let sum = per_year
            .entry(inv.date.year())
            .or_default()
            .entry(inv.currency.clone())
            .or_default();
        *sum = sum.saturating_add(inv.total());
    }

    let w_num = rows
        .iter()
        .map(|r| r.number.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let w_client = rows
        .iter()
        .map(|r| r.to.name.chars().count())
        .max()
        .unwrap_or(6)
        .clamp(6, 32);
    let totals: Vec<String> = rows
        .iter()
        .map(|r| format_money(&r.currency, r.total()))
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
    for (inv, total) in rows.iter().zip(&totals) {
        let year = inv.date.year();
        if last_year.is_some_and(|y| y != year) {
            print_year_total(&st, last_year.unwrap(), &per_year, w_num, w_client, w_total);
        }
        last_year = Some(year);
        let client: String = inv.to.name.chars().take(w_client).collect();
        let status = inv.status_display(today);
        let status = match inv.effective_status(today) {
            Status::Paid => st.green(&status),
            Status::Overdue => st.red_bold(&status),
            Status::Draft => st.dim(&status),
            Status::Pending => st.yellow(&status),
        };
        println!(
            "{:<w_num$}  {}  {:<w_client$}  {:>w_total$}  {}",
            inv.number,
            inv.date.format("%Y-%m-%d"),
            client,
            total,
            status
        );
    }
    if let Some(y) = last_year {
        print_year_total(&st, y, &per_year, w_num, w_client, w_total);
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
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

fn seq_of(number: &str) -> u64 {
    number
        .rsplit('-')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
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
