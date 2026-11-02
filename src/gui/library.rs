//! Invoice library (sidebar) scanning and file watching.

use crate::config::Config;
use crate::model::{Status, format_money};
use crate::parse::parse_invoice;
use eframe::egui;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub number: String,
    pub client: String,
    pub total: String,
    pub status: Option<Status>,
    pub date: Option<chrono::NaiveDate>,
}

pub fn scan(dir: &Path, cfg: &Config) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let parsed =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse_invoice(&text, cfg)));
        let entry = match parsed {
            Ok(Ok(inv)) => Entry {
                path: path.clone(),
                number: inv.number.clone(),
                client: inv.to.name.clone(),
                total: format_money(&inv.currency, inv.total()),
                status: Some(inv.status),
                date: Some(inv.date),
            },
            _ => Entry {
                path: path.clone(),
                number: stem,
                client: "could not parse".into(),
                total: String::new(),
                status: None,
                date: None,
            },
        };
        out.push(entry);
    }
    // Newest first: by date, then by trailing sequence number.
    out.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| seq(&b.number).cmp(&seq(&a.number)))
            .then_with(|| b.number.cmp(&a.number))
    });
    out
}

fn seq(number: &str) -> u64 {
    number
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Scans the library off the UI thread.
pub struct Scanner {
    tx: Sender<(PathBuf, Arc<Config>)>,
    rx: Receiver<Vec<Entry>>,
}

impl Scanner {
    pub fn spawn(ctx: egui::Context) -> Scanner {
        let (tx, job_rx) = channel::<(PathBuf, Arc<Config>)>();
        let (out_tx, rx) = channel();
        std::thread::Builder::new()
            .name("invgen-library".into())
            .spawn(move || {
                while let Ok(mut job) = job_rx.recv() {
                    while let Ok(newer) = job_rx.try_recv() {
                        job = newer;
                    }
                    let (dir, cfg) = job;
                    if out_tx.send(scan(&dir, &cfg)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn library thread");
        Scanner { tx, rx }
    }

    /// Scan `dir`, parsing invoices against `cfg`.
    pub fn request(&self, dir: PathBuf, cfg: Arc<Config>) {
        let _ = self.tx.send((dir, cfg));
    }

    pub fn poll(&self) -> Option<Vec<Entry>> {
        self.rx.try_iter().last()
    }
}

/// Watches directories (non-recursively) and reports changed paths.
pub struct FsWatch {
    watcher: Option<RecommendedWatcher>,
    rx: Receiver<PathBuf>,
    watched: HashSet<PathBuf>,
}

impl FsWatch {
    pub fn new(ctx: egui::Context) -> FsWatch {
        let (tx, rx) = channel();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if matches!(ev.kind, notify::EventKind::Access(_)) {
                    return;
                }
                for p in ev.paths {
                    let _ = tx.send(p);
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        })
        .ok();
        FsWatch {
            watcher,
            rx,
            watched: HashSet::new(),
        }
    }

    /// Make sure exactly `dirs` are watched.
    pub fn set_dirs(&mut self, dirs: &[PathBuf]) {
        let Some(w) = self.watcher.as_mut() else {
            return;
        };
        let want: HashSet<PathBuf> = dirs.iter().filter(|d| d.is_dir()).cloned().collect();
        for old in self.watched.difference(&want) {
            let _ = w.unwatch(old);
        }
        for new in want.difference(&self.watched) {
            let _ = w.watch(new, RecursiveMode::NonRecursive);
        }
        self.watched = want;
    }

    pub fn poll(&self) -> Vec<PathBuf> {
        self.rx.try_iter().collect()
    }
}
