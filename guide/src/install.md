# Install

invgen runs on Linux. Pick one of the three options below; all of them read the
same config file (`~/.config/invgen/config.yaml`).

## Flatpak

Works on any distribution and updates with `flatpak update`:

```sh
flatpak install --user https://ldolberg.github.io/invgen/io.github.ldolberg.invgen.flatpakref
```

The repository is signed and self-hosted on GitHub Pages; the
[install page](https://ldolberg.github.io/invgen/) has the details. Launch the
editor from your desktop menu, or from a terminal:

```sh
flatpak run io.github.ldolberg.invgen                  # the editor
flatpak run io.github.ldolberg.invgen build FILE.md    # any CLI command
```

A shell alias makes the CLI feel native:

```sh
alias invgen='flatpak run io.github.ldolberg.invgen'
```

The sandbox can read and write `~/Documents` and `~/.config/invgen`. Files
elsewhere can still be opened through the file chooser; see
[Flatpak file access](faq.md#flatpak-file-access).

## Prebuilt tarball (x86_64)

Download `invgen-vX.Y.Z-x86_64-linux.tar.gz` from the
[releases page](https://github.com/ldolberg/invgen/releases), check it against the
`.sha256` file next to it, then:

```sh
tar xzf invgen-*-x86_64-linux.tar.gz && ./invgen-*/install.sh
```

The tarball also contains the format spec, an example config and example
invoices.

## From source

You need Rust 1.85 or newer.

```sh
git clone https://github.com/ldolberg/invgen
cd invgen
make install      # binary to ~/.local/bin, desktop entry and icon to ~/.local/share
```

`make uninstall` removes them again. To install only the binary:

```sh
cargo install --path . --locked
```

Make sure `~/.local/bin` (or `~/.cargo/bin`) is on your `PATH`.

## Check the install

```sh
invgen --version
invgen --help
```
