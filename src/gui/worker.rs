//! Background thread: parse + compile + rasterize, and PDF export.
//! The UI only ever sends jobs and polls results, so it never blocks.

use crate::config::Config;
use crate::einvoice;
use crate::lint::{self, Diagnostic};
use crate::model::{Invoice, file_stem};
use crate::parse::{ParseError, parse_invoice};
use crate::render::{Compiled, PageImage, Renderer};
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

pub enum Job {
    Preview {
        generation: u64,
        text: String,
        pixels_per_pt: f32,
    },
    /// Export `text` to `path`, or to `<number>.pdf` in `dir` when `path` is None.
    Export {
        text: String,
        path: Option<PathBuf>,
        dir: PathBuf,
    },
    /// The config changed (clients edited, first-run setup): use this one from now on.
    Config(Arc<Config>),
}

pub enum Outcome {
    /// Text failed to parse or compile; keep showing the last good pages.
    Failed {
        generation: u64,
        line: usize,
        message: String,
        /// Everything the live check found (the failure included).
        diagnostics: Vec<Diagnostic>,
    },
    Preview {
        generation: u64,
        invoice: Box<Invoice>,
        pixels_per_pt: f32,
        pages: Vec<PageImage>,
        diagnostics: Vec<Diagnostic>,
    },
    /// The written PDF and the invoice it shows.
    Exported(Result<(PathBuf, Box<Invoice>), String>),
    /// A preview job was dropped in favor of a newer one (no other outcome follows).
    Skipped,
}

pub struct Worker {
    tx: Sender<Job>,
    rx: Receiver<Outcome>,
}

impl Worker {
    pub fn spawn(cfg: Arc<Config>, ctx: egui::Context) -> Worker {
        let (job_tx, job_rx) = channel::<Job>();
        let (out_tx, out_rx) = channel::<Outcome>();
        std::thread::Builder::new()
            .name("invgen-render".into())
            .spawn(move || run(cfg, ctx, job_rx, out_tx))
            .expect("spawn render thread");
        Worker {
            tx: job_tx,
            rx: out_rx,
        }
    }

    pub fn send(&self, job: Job) {
        let _ = self.tx.send(job);
    }

    pub fn poll(&self) -> Vec<Outcome> {
        self.rx.try_iter().collect()
    }
}

struct Cache {
    text: String,
    invoice: Invoice,
    compiled: Compiled,
    diagnostics: Vec<Diagnostic>,
}

/// The live check for the editor; a bug in it must not stop the preview.
fn check(cfg: &Config, text: &str, parsed: Result<&Invoice, &ParseError>) -> Vec<Diagnostic> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        lint::check_parsed(text, cfg, parsed)
    }))
    .unwrap_or_default()
}

fn compile(
    renderer: &Option<Renderer>,
    cfg: &Config,
    text: &str,
) -> Result<(Invoice, Compiled), ParseError> {
    // A bug in the parser or renderer must not take the preview down for good.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_inner(renderer, cfg, text)
    }))
    .unwrap_or_else(|_| {
        Err(ParseError {
            line: 0,
            message: "internal error while rendering (see terminal)".into(),
        })
    })
}

fn compile_inner(
    renderer: &Option<Renderer>,
    cfg: &Config,
    text: &str,
) -> Result<(Invoice, Compiled), ParseError> {
    let invoice = parse_invoice(text, cfg)?;
    let renderer = renderer.as_ref().ok_or_else(|| ParseError {
        line: 0,
        message: "renderer failed to initialize".into(),
    })?;
    let compiled = renderer.compile(&invoice).map_err(|e| ParseError {
        line: 0,
        message: format!("render: {e:#}"),
    })?;
    Ok((invoice, compiled))
}

fn run(cfg: Arc<Config>, ctx: egui::Context, rx: Receiver<Job>, tx: Sender<Outcome>) {
    let renderer = match std::panic::catch_unwind(Renderer::new)
        .unwrap_or_else(|_| Err(anyhow::anyhow!("panicked during initialization")))
    {
        Ok(r) => Some(r),
        Err(e) => {
            let _ = tx.send(Outcome::Failed {
                generation: 0,
                line: 0,
                message: format!("renderer: {e:#}"),
                diagnostics: Vec::new(),
            });
            None
        }
    };
    let mut cache: Option<Cache> = None;
    let mut cfg = cfg;

    while let Ok(first) = rx.recv() {
        // Coalesce: run every export, but only the newest preview.
        let mut jobs = vec![first];
        loop {
            match rx.try_recv() {
                Ok(j) => jobs.push(j),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let last_preview = jobs.iter().rposition(|j| matches!(j, Job::Preview { .. }));
        for (i, job) in jobs.into_iter().enumerate() {
            match job {
                Job::Config(new) => {
                    cfg = new;
                    cache = None;
                    continue;
                }
                Job::Export { text, path, dir } => {
                    let res = (|| -> Result<(PathBuf, Box<Invoice>), String> {
                        let fresh;
                        let (invoice, compiled) = match &cache {
                            Some(c) if c.text == text => (&c.invoice, &c.compiled),
                            _ => {
                                fresh = compile(&renderer, &cfg, &text)
                                    .map_err(|e| format!("line {}: {}", e.line, e.message))?;
                                (&fresh.0, &fresh.1)
                            }
                        };
                        // `e_invoice` in the config also applies to exports
                        // from the editor (see docs/E-INVOICING.md).
                        let facturx = cfg.e_invoice.contains(&einvoice::Format::FacturX);
                        let ubl = if cfg.e_invoice.contains(&einvoice::Format::Ubl) {
                            Some(einvoice::ubl_xml(invoice).map_err(|e| e.to_string())?)
                        } else {
                            None
                        };
                        let bytes = match (facturx, &renderer) {
                            (true, Some(r)) => einvoice::facturx_pdf(r, invoice),
                            _ => compiled.pdf(),
                        }
                        .map_err(|e| format!("{e:#}"))?;
                        let path = path.unwrap_or_else(|| {
                            dir.join(format!("{}.pdf", file_stem(&invoice.number)))
                        });
                        if let Some(dir) = path.parent() {
                            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                        }
                        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
                        if let Some(xml) = ubl {
                            std::fs::write(path.with_extension("xml"), xml)
                                .map_err(|e| e.to_string())?;
                        }
                        Ok((path, Box::new(invoice.clone())))
                    })();
                    let _ = tx.send(Outcome::Exported(res));
                }
                Job::Preview {
                    generation,
                    text,
                    pixels_per_pt,
                } => {
                    if Some(i) != last_preview {
                        let _ = tx.send(Outcome::Skipped);
                        continue;
                    }
                    let hit = matches!(&cache, Some(c) if c.text == text);
                    if !hit {
                        match compile(&renderer, &cfg, &text) {
                            Ok((invoice, compiled)) => {
                                let diagnostics = check(&cfg, &text, Ok(&invoice));
                                cache = Some(Cache {
                                    text,
                                    invoice,
                                    compiled,
                                    diagnostics,
                                })
                            }
                            Err(e) => {
                                let diagnostics = check(&cfg, &text, Err(&e));
                                let _ = tx.send(Outcome::Failed {
                                    generation,
                                    line: e.line,
                                    message: e.message,
                                    diagnostics,
                                });
                                ctx.request_repaint();
                                continue;
                            }
                        }
                    }
                    let c = cache.as_ref().expect("cache filled above");
                    let pages = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        c.compiled.render_pages(pixels_per_pt)
                    }))
                    .unwrap_or_default();
                    let _ = tx.send(Outcome::Preview {
                        generation,
                        invoice: Box::new(c.invoice.clone()),
                        pixels_per_pt,
                        pages,
                        diagnostics: c.diagnostics.clone(),
                    });
                }
            }
            ctx.request_repaint();
        }
    }
}
