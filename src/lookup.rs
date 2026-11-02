//! Find an invoice file in the library by path or by number.

use crate::config::Config;
use crate::frontmatter;
use crate::model::file_stem;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// An invoice file with the number and client key written in it.
#[derive(Debug, Clone)]
pub struct Found {
    pub path: PathBuf,
    /// The front matter `number`, else the file stem.
    pub number: String,
    /// The front matter `client` key, if any.
    pub client: Option<String>,
}

/// Every invoice in the library under `dir` (see [`crate::library::discover`]).
/// Only the front matter is read, so files that do not parse are included.
pub fn index(dir: &Path) -> Vec<Found> {
    crate::library::discover(dir)
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let stem = path.file_stem()?.to_string_lossy().into_owned();
            let number = frontmatter::get(&text, "number")
                .ok()
                .flatten()
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or(stem);
            let client = frontmatter::get(&text, "client").ok().flatten();
            Some(Found {
                path,
                number,
                client,
            })
        })
        .collect()
}

/// Trailing run of digits of an invoice number: `EINV-2026-23` -> 23.
pub fn sequence(number: &str) -> u64 {
    let digits = number.len() - number.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    number[number.len() - digits..].parse().unwrap_or(0)
}

/// Resolve a `FILE|NUMBER` argument. An existing file is used as is.
/// Otherwise `arg` is an invoice number in `cfg.invoices_dir` (matched against
/// the front matter number and the file name), or a bare sequence number
/// (`23` for `EINV-2026-23`). No match, or more than one, is an error.
pub fn resolve(cfg: &Config, arg: &str) -> Result<PathBuf> {
    let path = Path::new(arg);
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    let dir = &cfg.invoices_dir;
    let all = index(dir);
    let wanted = arg.trim();
    let stem = file_stem(wanted);
    let mut hits: Vec<&Found> = all
        .iter()
        .filter(|f| {
            f.number == wanted
                || f.path
                    .file_stem()
                    .is_some_and(|s| s.to_string_lossy() == stem)
        })
        .collect();
    if hits.is_empty() && !wanted.is_empty() && wanted.chars().all(|c| c.is_ascii_digit()) {
        let n: u64 = wanted.parse().unwrap_or(u64::MAX);
        hits = all.iter().filter(|f| sequence(&f.number) == n).collect();
    }
    hits.dedup_by(|a, b| a.path == b.path);
    match hits.as_slice() {
        [one] => Ok(one.path.clone()),
        [] if arg.ends_with(".md") || arg.contains(['/', '\\']) => {
            bail!("{arg}: no such file")
        }
        [] => bail!("no invoice `{arg}` in {}", dir.display()),
        many => {
            let list: Vec<String> = many.iter().map(|f| f.path.display().to_string()).collect();
            bail!(
                "`{arg}` matches {} invoices, pass a path instead: {}",
                many.len(),
                list.join(", ")
            )
        }
    }
}

/// The highest-numbered invoice in `cfg.invoices_dir`, only among those with
/// `client: <key>` when `client` is given.
pub fn last(cfg: &Config, client: Option<&str>) -> Result<PathBuf> {
    let found = index(&cfg.invoices_dir)
        .into_iter()
        .filter(|f| client.is_none_or(|c| f.client.as_deref().map(str::trim) == Some(c)))
        .max_by(|a, b| {
            (sequence(&a.number), &a.number, &a.path).cmp(&(
                sequence(&b.number),
                &b.number,
                &b.path,
            ))
        });
    match (found, client) {
        (Some(f), _) => Ok(f.path),
        (None, Some(c)) => bail!(
            "no invoice for client `{c}` in {}",
            cfg.invoices_dir.display()
        ),
        (None, None) => bail!("no invoices in {}", cfg.invoices_dir.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, front: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, format!("---\n{front}\n---\n- A: 1 @ 1\n")).unwrap();
    }

    fn cfg(root: &Path) -> Config {
        Config {
            invoices_dir: root.to_path_buf(),
            ..Config::default()
        }
    }

    #[test]
    fn sequences() {
        assert_eq!(sequence("EINV-2026-23"), 23);
        assert_eq!(sequence("EINV-2026-07"), 7);
        assert_eq!(sequence("draft"), 0);
    }

    #[test]
    fn resolve_by_path_number_and_sequence() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "EINV-2026-08.md",
            "number: EINV-2026-08\nclient: northwind",
        );
        write(
            root,
            "2026/renamed.md",
            "number: \"EINV-2026-09\"  # c\nclient: contoso",
        );
        write(root, "EINV-2026-10.md", "client: northwind");
        let cfg = cfg(root);
        let p = root.join("EINV-2026-08.md");
        assert_eq!(resolve(&cfg, p.to_str().unwrap()).unwrap(), p);
        assert_eq!(resolve(&cfg, "EINV-2026-08").unwrap(), p);
        assert_eq!(
            resolve(&cfg, "EINV-2026-09").unwrap(),
            root.join("2026/renamed.md")
        );
        // No number in the file: the file name counts.
        assert_eq!(
            resolve(&cfg, "EINV-2026-10").unwrap(),
            root.join("EINV-2026-10.md")
        );
        assert_eq!(resolve(&cfg, "9").unwrap(), root.join("2026/renamed.md"));
        assert!(
            resolve(&cfg, "EINV-2026-99")
                .unwrap_err()
                .to_string()
                .contains("no invoice `EINV-2026-99`")
        );
        assert!(
            resolve(&cfg, "missing.md")
                .unwrap_err()
                .to_string()
                .contains("no such file")
        );
        // Two files with the same number are ambiguous.
        write(root, "copy/EINV-2026-08.md", "number: EINV-2026-08");
        let err = resolve(&cfg, "EINV-2026-08").unwrap_err().to_string();
        assert!(err.contains("matches 2 invoices"), "{err}");
    }

    #[test]
    fn last_by_client() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            root,
            "EINV-2025-09.md",
            "number: EINV-2025-09\nclient: contoso",
        );
        write(
            root,
            "EINV-2026-10.md",
            "number: EINV-2026-10\nclient: northwind",
        );
        write(
            root,
            "EINV-2026-11.md",
            "number: EINV-2026-11\nclient: contoso",
        );
        write(root, "EINV-2026-12.md", "number: EINV-2026-12");
        let cfg = cfg(root);
        assert_eq!(last(&cfg, None).unwrap(), root.join("EINV-2026-12.md"));
        assert_eq!(
            last(&cfg, Some("northwind")).unwrap(),
            root.join("EINV-2026-10.md")
        );
        assert_eq!(
            last(&cfg, Some("contoso")).unwrap(),
            root.join("EINV-2026-11.md")
        );
        assert!(last(&cfg, Some("acme")).is_err());
        assert!(last(&self::cfg(&root.join("empty")), None).is_err());
    }
}
