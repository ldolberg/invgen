//! Invoice discovery: the one place that decides which files under
//! `invoices_dir` belong to the library. Used by `invgen list`, the GUI
//! sidebar and next-number scanning, so they always agree.

use std::path::{Path, PathBuf};

/// How many levels of subfolders below `invoices_dir` are searched
/// (e.g. `Invoices/2026/client/`).
pub const MAX_DEPTH: usize = 2;

/// All invoice markdown files (`*.md`) in `dir` and up to [`MAX_DEPTH`]
/// levels of subfolders, sorted by path. Hidden folders (`.git`, `.cache`...)
/// are skipped. A missing or unreadable `dir` yields an empty list.
pub fn discover(dir: &Path) -> Vec<PathBuf> {
    discover_with_extensions(dir, &["md"])
}

/// Like [`discover`], but matching any of `exts` (case-insensitive, without
/// the dot). Next-number scanning uses `["md", "pdf"]`.
pub fn discover_with_extensions(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, MAX_DEPTH, exts, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, depth: usize, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 && !is_hidden(&path) {
                walk(&path, depth - 1, exts, out);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
        {
            out.push(path);
        }
    }
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'))
}

/// Whether a change at `path` can affect the library rooted at `root`: it is
/// inside `root`, within [`MAX_DEPTH`] subfolders, and not under a hidden
/// folder. Used to filter events from a recursive file watcher.
pub fn affects(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    let parts: Vec<_> = rel.components().collect();
    if parts.is_empty() || parts.len() > MAX_DEPTH + 1 {
        return false;
    }
    // Folders on the way to the file must not be hidden.
    !parts[..parts.len() - 1]
        .iter()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, "").unwrap();
    }

    fn rels(root: &Path, files: &[PathBuf]) -> Vec<String> {
        files
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn finds_nested_invoices_up_to_two_levels() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        touch(root, "EINV-2025-01.md");
        touch(root, "2026/EINV-2026-02.md");
        touch(root, "a/b/EINV-2026-03.md");
        touch(root, "a/b/c/too-deep.md");
        touch(root, ".git/hidden.md");
        touch(root, "a/.trash/hidden.md");
        touch(root, "2026/EINV-2026-02.pdf");
        touch(root, "notes.txt");
        assert_eq!(
            rels(root, &discover(root)),
            [
                "2026/EINV-2026-02.md",
                "EINV-2025-01.md",
                "a/b/EINV-2026-03.md"
            ]
        );
        assert_eq!(
            rels(root, &discover_with_extensions(root, &["md", "pdf"])),
            [
                "2026/EINV-2026-02.md",
                "2026/EINV-2026-02.pdf",
                "EINV-2025-01.md",
                "a/b/EINV-2026-03.md"
            ]
        );
    }

    #[test]
    fn missing_dir_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(discover(&tmp.path().join("nope")).is_empty());
    }

    #[test]
    fn affects_matches_discovery_depth() {
        let root = Path::new("/inv");
        assert!(affects(root, Path::new("/inv/x.md")));
        assert!(affects(root, Path::new("/inv/2026")));
        assert!(affects(root, Path::new("/inv/2026/x.md")));
        assert!(affects(root, Path::new("/inv/a/b/x.md")));
        assert!(!affects(root, Path::new("/inv/a/b/c/x.md")));
        assert!(!affects(root, Path::new("/inv/.git/x.md")));
        assert!(!affects(root, Path::new("/inv")));
        assert!(!affects(root, Path::new("/other/x.md")));
    }
}
