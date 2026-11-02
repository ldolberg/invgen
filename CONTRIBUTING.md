# Contributing to invgen

Thanks for helping. This file covers how to build, test and release invgen, and
the rules every change follows. User documentation lives in the
[user guide](https://ldolberg.github.io/invgen/guide/) (sources in `guide/`).

## Layout

| Path | What it is |
|---|---|
| `src/main.rs` | The `invgen` CLI (clap) |
| `src/parse.rs` | Markdown + front matter to `Invoice` |
| `src/model.rs` | Shared data model |
| `src/config.rs` | `config.yaml`, numbering, the new-invoice template |
| `src/render/` | Typst renderer, bundled fonts, template data |
| `src/gui/` | The egui editor (live preview, library, worker thread) |
| `assets/invoice.typ` | The invoice template |
| `docs/FORMAT.md` | The invoice format spec (also included in the guide) |
| `guide/` | The mdBook user guide |
| `examples/` | Example config and invoices (fictional data) |
| `packaging/` | Desktop entry, metainfo, icon, Flatpak manifest and publish script |

## Build and run

You need Rust 1.85 or newer (edition 2024).

```sh
cargo build
cargo run -- --help
cargo run -- edit examples/invoices/EINV-2026-22.md
```

Never run invgen against your real config while developing. Point it at a
throwaway file instead:

```sh
export INVGEN_CONFIG=$(mktemp --suffix=.yaml)
cp examples/config.yaml "$INVGEN_CONFIG"
```

`make build` builds the release binary and `make install` installs it, the
desktop entry and the icon into `~/.local`.

## Test and lint

Run all three before every commit; CI runs the tests on release.

```sh
cargo fmt --all                                   # make fmt
cargo clippy --all-targets -- -D warnings         # make lint
cargo test                                        # make test
```

Every behavior change comes with a test. Parser and config tests live next to
the code (`#[cfg(test)]` modules); renderer tests are in `src/render/tests.rs`.
Tests that touch the filesystem use `tempfile` and never the real config or
invoices directory.

There is no fuzzing setup yet.

## The guide

```sh
cargo install mdbook --locked
mdbook serve guide        # http://localhost:3000, reloads on save
mdbook build guide        # output in guide/book/
```

`guide/src/format.md` includes `docs/FORMAT.md`, so edit the format spec there.
Pushes to `main` that touch `guide/` or `docs/` publish the book to
<https://ldolberg.github.io/invgen/guide/> (`.github/workflows/guide.yml`).
Describe only features that exist on `main`.

## Pull requests

- Branch from `main`, keep the change focused, and open a pull request against
  `main`. The PR template lists what to check.
- Update `docs/FORMAT.md` for any change to the invoice or config format, and
  the README and the guide for user-facing changes.
- If you add or update dependencies, `Cargo.lock` changes; also run
  `make flatpak-sources` so the offline Flatpak build keeps working (or say in
  the PR that it still needs to be done).
- Reference the issue in the commit message (`Closes #N`).

## Rules

These apply to code, comments, docs, commit messages and PR descriptions.

- **English only.**
- **No em dash and no en dash characters.** Use commas, parentheses, periods or
  a plain hyphen (`-`). For ranges write "from X to Y" or `X-Y`.
- **No personal data.** The repository is public. Examples, tests, screenshots
  and bug reports use fictional data only, such as Alex Morgan, Northwind Labs,
  `example.com` addresses and well-known test IBANs like
  `DE89 3704 0044 0532 0130 00`. Never commit a real config, real invoices or
  real client or bank details.
- **Never touch the real config in tests or manual runs**: set `INVGEN_CONFIG`
  to a temporary file.
- Prefer well-known, maintained crates, and do not reformat or refactor code
  unrelated to your change.

## Release process

Releases are cut by the maintainer from `main`:

1. Bump `version` in `Cargo.toml` (and let `Cargo.lock` follow with `cargo build`).
2. Add a `<release version="X.Y.Z" date="YYYY-MM-DD">` entry with a description
   to `packaging/io.github.ldolberg.invgen.metainfo.xml`. Its text becomes the
   GitHub release notes.
3. Run `make flatpak-sources` if `Cargo.lock` changed.
4. Commit, then push a matching tag:

   ```sh
   git tag vX.Y.Z && git push origin vX.Y.Z
   ```

The Release workflow (`.github/workflows/release.yml`) checks that the tag
matches `Cargo.toml` and the metainfo, runs the tests, builds the x86_64
tarball and the Flatpak, signs and pushes the Flatpak repo to the `gh-pages`
branch, and creates the GitHub release with both artifacts and checksums.
Running the workflow by hand from the Actions tab is a dry run.

The `gh-pages` branch serves both the Flatpak repo (site root) and the user
guide (`guide/`). Never force push it or delete files there by hand.

## License

By contributing you agree that your contributions are licensed under the MIT
license of this project.
