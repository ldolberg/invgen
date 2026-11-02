# invgen

Write invoices in markdown, get clean PDFs. A native desktop editor shows a live
preview as you type; a small CLI builds, watches and lists invoices from the
terminal. Your details, bank and clients live once in a config file, so each
invoice is a few lines.

![invgen editor](docs/screenshot.png)

<img src="docs/sample-invoice.png" width="420" alt="Sample invoice PDF">

## Install

### Linux

Flatpak (any distro, auto-updates with `flatpak update`):

```sh
flatpak install --user https://ldolberg.github.io/invgen/io.github.ldolberg.invgen.flatpakref
```

The app reads the same `~/.config/invgen/config.yaml` as the native build and has
access to `~/Documents`; open files elsewhere through the file chooser. Install page:
<https://ldolberg.github.io/invgen/>.

Prebuilt (x86_64): grab `invgen-*-x86_64-linux.tar.gz` from
[Releases](https://github.com/ldolberg/invgen/releases), then

```sh
tar xzf invgen-*-x86_64-linux.tar.gz && ./invgen-*/install.sh
```

### macOS

Grab `invgen-*-macos-universal.zip` (Apple Silicon and Intel, macOS 11+) from
[Releases](https://github.com/ldolberg/invgen/releases), unzip it and put `invgen`
on your `PATH`. The binary is not notarized, so clear the quarantine flag once:

```sh
unzip invgen-*-macos-universal.zip && cd invgen-*-macos-universal
xattr -d com.apple.quarantine invgen
sudo install -m755 invgen /usr/local/bin/
```

The config lives in `~/Library/Application Support/invgen/config.yaml`.

### Windows

Grab `invgen-*-x86_64-windows.zip` from
[Releases](https://github.com/ldolberg/invgen/releases), unzip it and add the
folder to your `PATH` (or run `invgen.exe` from there). SmartScreen may warn about
an unsigned download; choose "More info" and "Run anyway". The config lives in
`%APPDATA%\invgen\config.yaml`.

### With Cargo (any platform)

```sh
cargo install invgen --locked
```

Needs Rust 1.95+ and a C compiler.

### From source

```sh
make install      # Linux: binary to ~/.local/bin, desktop entry and icon to ~/.local/share
make uninstall
```

Or just the binary: `cargo install --path . --locked`.

`invgen config` prints the config path on every platform. Paths below use the Linux
`~/.config/invgen` for brevity.

## Quickstart

```sh
invgen config --edit         # create ~/.config/invgen/config.yaml and fill in your data
invgen new --client acme     # writes ~/Documents/Invoices/INV-2026-01.md
invgen edit ~/Documents/Invoices/INV-2026-01.md   # editor with live preview
invgen build ~/Documents/Invoices/INV-2026-01.md  # -> INV-2026-01.pdf next to it
```

A full real-world config is in [`examples/config.yaml`](examples/config.yaml)
and sample invoices in [`examples/invoices/`](examples/invoices/).

| Command | What it does |
|---|---|
| `invgen` / `invgen edit [FILE]` | Open the editor |
| `invgen new [--client KEY] [--open]` | Create the next numbered invoice |
| `invgen build FILE.md [-o OUT.pdf] [--png DIR]` | Render a PDF (and optional PNG previews) |
| `invgen watch FILE.md` | Rebuild the PDF on every save |
| `invgen list` | Table of all invoices with yearly totals |
| `invgen config [--edit]` | Show (or edit) the config file |

Every command accepts `--config PATH` (or `INVGEN_CONFIG=PATH`) to use another config.
Parse errors are reported as `file:line: message`.

## Format

```markdown
---
number: EINV-2026-23
date: 2026-11-02
due_days: 14
status: pending
client: northwind
---

| Description              | Qty | Unit | Rate   |
|--------------------------|-----|------|--------|
| Senior Software Engineer | 21  | days | 450.00 |

Any other paragraph becomes a note under the table.
```

Front matter holds the metadata (`client:` points into the config), the first
table holds the line items, and loose paragraphs become notes. List items
(`- Consulting: 21 days @ 450`) work instead of a table. Full spec:
[docs/FORMAT.md](docs/FORMAT.md). A `config:` block in the front matter overrides
any config key for a single invoice.

## Keyboard shortcuts

| Keys | Action |
|---|---|
| Ctrl+N | New invoice |
| Ctrl+O | Open |
| Ctrl+S | Save |
| Ctrl+E | Export PDF |
| Ctrl+= / Ctrl+- | Zoom preview in / out |
| Ctrl+0 | Reset zoom |

## Releasing

Bump `version` in `Cargo.toml`, add a `<release>` entry to
`packaging/io.github.ldolberg.invgen.metainfo.xml` (and run `make flatpak-sources`
if `Cargo.lock` changed), commit, then push a matching tag:

```sh
git tag v0.1.3 && git push origin v0.1.3
```

The Release workflow tests the crate, builds the Linux tarball, the macOS and
Windows zips and the Flatpak (smoke testing each binary on its own OS), publishes
the signed Flatpak repo to GitHub Pages, creates the GitHub release and publishes
the crate to crates.io (secret `CARGO_REGISTRY_TOKEN`). Running it by hand from the
Actions tab is a dry run.

## License

MIT
