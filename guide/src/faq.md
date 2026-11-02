# FAQ and troubleshooting

## Where is my config?

Run `invgen config`. It prints the path in use (and creates a starter file
there if none exists). In order, invgen uses:

1. `--config PATH` on the command line,
2. the `INVGEN_CONFIG` environment variable,
3. `$XDG_CONFIG_HOME/invgen/config.yaml`, which is
   `~/.config/invgen/config.yaml` on most systems.

The Flatpak always uses `~/.config/invgen/config.yaml` on the host, not the
private `~/.var/app/io.github.ldolberg.invgen/config` folder, so the native build
and the Flatpak share one config.

## I changed the config but the editor does not show it

The editor reads the config once at startup. Close and reopen it. The CLI reads
the config on every command, and `invgen watch` on every rebuild.

## A config key seems to have no effect

Unknown keys are ignored silently, so check the spelling against
[Configuration](configuration.md). Also check precedence: a value in the invoice
front matter or in the client entry beats the global one.

## Flatpak file access

The Flatpak sandbox can read and write:

- `~/Documents` (the default `invoices_dir` is `~/Documents/Invoices`),
- `~/.config/invgen` (the config).

Files anywhere else can still be opened and saved from the editor through the
file chooser (Ctrl+O, Shift+Ctrl+S, Shift+Ctrl+E), which grants access to the
chosen file. What does not work inside the sandbox:

- an `invoices_dir` outside `~/Documents`: `invgen new`, `invgen list`, the
  sidebar and automatic numbering cannot see it;
- passing a path outside `~/Documents` on the command line, for example
  `flatpak run io.github.ldolberg.invgen build ~/work/invoice.md`.

To allow another folder, grant it with an override, for example:

```sh
flatpak override --user --filesystem=~/Invoices io.github.ldolberg.invgen
```

Or keep invoices under `~/Documents`, or use the tarball or a source build,
which have no sandbox.

## Which fonts are used? Can I change them?

The PDF uses fonts embedded in invgen: Inter for all text and Mrs Saint
Delafield for the signature. System fonts are never used, so a PDF looks the
same on every machine and nothing needs to be installed. The fonts cannot be
changed from the config today.

Inter covers Latin, Greek and Cyrillic scripts. Characters outside the bundled
fonts (for example Chinese, Japanese, Arabic or emoji) will not render
correctly.

## The signature shows my name. How do I hide or change it?

The signature defaults to `from.name`. Set `signature: false` in the front
matter to hide it, or `signature: Alex M.` to print other text.

## Can invgen mark invoices overdue automatically?

No. The status is whatever the front matter says (`pending` by default). Change
it to `paid` or `overdue` yourself.

## The invoice number skipped one

If an invoice has no `number:` and lives in `invoices_dir`, the automatic number
counts the file itself (or its PDF) and comes out one too high. Write the number
explicitly; `invgen new` and the editor template do this for you.

## `invgen list` prints "skipped" and exits with 1

Those files failed to parse. The message shows the file, line and problem.
Fix them (or move unrelated `.md` files out of `invoices_dir`) and run it again.

## The editor does not start

The editor needs a graphical session (Wayland or X11). Over SSH or in a plain
terminal, use the CLI commands `new`, `build`, `watch` and `list` instead; they
do not need a display.

## Where do I report a bug or ask for a feature?

Open an issue at <https://github.com/ldolberg/invgen/issues>. Use dummy data in
examples, never real client or bank details.
