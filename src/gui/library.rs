//! Invoice library (sidebar) scanning and file watching.

use super::{App, Pending, ToastKind};
use crate::config::Config;
use crate::model::{Status, format_money};
use crate::parse::parse_invoice;
use chrono::NaiveDate;
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
    /// Effective status (pending past due shows as overdue).
    pub status: Option<Status>,
    pub days_overdue: Option<i64>,
    pub date: Option<chrono::NaiveDate>,
    pub paid_on: Option<chrono::NaiveDate>,
}

/// Library entries for every invoice [`crate::library::discover`] finds in
/// `dir`, newest first. Status is the effective one as of `today`.
pub fn scan(dir: &Path, cfg: &Config, today: NaiveDate) -> Vec<Entry> {
    let mut out = Vec::new();
    for path in crate::library::discover(dir) {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let entry = match parse_invoice(&text, cfg) {
            Ok(inv) => Entry {
                path: path.clone(),
                number: inv.number.clone(),
                client: inv.to.name.clone(),
                total: format_money(&inv.currency, inv.signed_total()),
                status: Some(inv.effective_status(today)),
                days_overdue: inv.days_overdue(today),
                date: Some(inv.date),
                paid_on: inv.paid_on,
            },
            Err(_) => Entry {
                path: path.clone(),
                number: stem,
                client: "could not parse".into(),
                total: String::new(),
                status: None,
                days_overdue: None,
                date: None,
                paid_on: None,
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

/// An action from the sidebar context menu.
#[derive(Clone, Copy)]
pub enum Action {
    /// Write this status (`paid` also records `paid_on` as today).
    SetStatus(Status),
    /// Copy the invoice as the next one and open it.
    Duplicate,
}

impl App {
    /// Run a sidebar context menu `action` on the invoice at `path`.
    pub fn library_action(&mut self, path: PathBuf, action: Action) {
        let today = chrono::Local::now().date_naive();
        match action {
            Action::SetStatus(status) => self.write_status(&path, status, today),
            Action::Duplicate => match crate::duplicate::duplicate_file(&self.cfg, &path, today) {
                Ok(new) => {
                    self.library_due = Some(std::time::Instant::now());
                    self.toast(
                        ToastKind::Success,
                        format!("Created {}", super::file_name(&new)),
                    );
                    self.request(Pending::Open(new));
                }
                Err(e) => self.toast(ToastKind::Error, format!("Duplicate failed: {e:#}")),
            },
        }
    }

    fn write_status(&mut self, path: &Path, status: Status, today: NaiveDate) {
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let is_open = self
            .path
            .as_deref()
            .is_some_and(|p| canon(p) == canon(path));
        if is_open && self.dirty() {
            self.toast(
                ToastKind::Error,
                format!(
                    "{} has unsaved changes. Save first, then set the status.",
                    self.display_name()
                ),
            );
            return;
        }
        let result = std::fs::read_to_string(path)
            .map_err(anyhow::Error::from)
            .and_then(|src| crate::frontmatter::set_status(&src, status, None, today))
            .and_then(|text| {
                std::fs::write(path, &text)?;
                Ok(text)
            });
        match result {
            Ok(text) => {
                if is_open {
                    // Show the new front matter right away; the watcher then
                    // sees nothing new on disk.
                    self.text = text.clone();
                    self.saved_text = text;
                    self.text_changed(true);
                }
                self.library_due = Some(std::time::Instant::now());
                self.toast(
                    ToastKind::Success,
                    format!("{} marked {}", super::file_name(path), status.label()),
                );
            }
            Err(e) => self.toast(
                ToastKind::Error,
                format!("Could not update {}: {e:#}", super::file_name(path)),
            ),
        }
    }
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
                    let today = chrono::Local::now().date_naive();
                    if out_tx.send(scan(&dir, &cfg, today)).is_err() {
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

/// Watches directories (each recursively or not) and reports changed paths.
pub struct FsWatch {
    watcher: Option<RecommendedWatcher>,
    rx: Receiver<PathBuf>,
    watched: HashSet<(PathBuf, bool)>,
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

    /// Make sure exactly `dirs` are watched; `true` watches recursively.
    /// A path listed both ways is watched recursively.
    pub fn set_dirs(&mut self, dirs: &[(PathBuf, bool)]) {
        let Some(w) = self.watcher.as_mut() else {
            return;
        };
        let mut want: HashSet<(PathBuf, bool)> = HashSet::new();
        for (d, recursive) in dirs.iter().filter(|(d, _)| d.is_dir()) {
            let recursive = *recursive || dirs.iter().any(|(o, r)| *r && o == d);
            want.insert((d.clone(), recursive));
        }
        if want == self.watched {
            return;
        }
        // Rare (only when the open file or the invoices dir changes), so start
        // over instead of diffing: overlapping watches share OS handles and
        // removing one could silently drop another.
        for (old, _) in &self.watched {
            let _ = w.unwatch(old);
        }
        for (new, recursive) in &want {
            let mode = if *recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            let _ = w.watch(new, mode);
        }
        self.watched = want;
    }

    pub fn poll(&self) -> Vec<PathBuf> {
        self.rx.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a minimal invoice numbered after the file stem.
    fn write(root: &Path, rel: &str, due: &str, status: &str) {
        let p = root.join(rel);
        let number = p.file_stem().unwrap().to_string_lossy().into_owned();
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let md = format!(
            "---\nnumber: {number}\ndate: 2026-10-01\ndue: {due}\nstatus: {status}\n\
             to:\n  name: Northwind Labs\n---\n\n\
             | Description | Qty | Unit | Rate |\n|---|---|---|---|\n| Consulting | 1 | days | 100 |\n"
        );
        std::fs::write(p, md).unwrap();
    }

    #[test]
    fn scan_includes_subfolders_and_effective_status() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(root, "EINV-2026-01.md", "2026-10-21", "pending");
        write(root, "2026/EINV-2026-02.md", "2026-12-01", "pending");
        write(root, "a/b/EINV-2026-03.md", "2026-10-01", "paid");
        write(root, "a/b/EINV-2026-04.md", "false", "pending");
        write(root, "a/b/c/EINV-2026-05.md", "2026-10-01", "paid");
        let cfg = Config {
            invoices_dir: root.to_path_buf(),
            ..Config::default()
        };
        let today = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let entries = scan(root, &cfg, today);
        let got: Vec<_> = entries
            .iter()
            .map(|e| (e.number.as_str(), e.status, e.days_overdue))
            .collect();
        // Newest first; too-deep folders are not part of the library.
        assert_eq!(
            got,
            [
                ("EINV-2026-04", Some(Status::Pending), None),
                ("EINV-2026-03", Some(Status::Paid), None),
                ("EINV-2026-02", Some(Status::Pending), None),
                ("EINV-2026-01", Some(Status::Overdue), Some(12)),
            ]
        );
    }
}
