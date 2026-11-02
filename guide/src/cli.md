# Command line

```text
invgen [--config PATH] [COMMAND]
```

With no command, invgen opens the editor. `invgen --help` and
`invgen COMMAND --help` print the built-in help, `invgen --version` the version.

## Global options

| Option | Meaning |
|---|---|
| `--config PATH` | Use this config file instead of `~/.config/invgen/config.yaml`. Accepted before or after the command name. |
| `INVGEN_CONFIG=PATH` | Environment variable with the same effect. `--config` wins when both are set. |
| `NO_COLOR` | Set to any value to turn off colored output. Colors are also off when the output is not a terminal. |

Errors in an invoice are reported compiler style, so editors and terminals can
jump to them:

```text
invoices/EINV-2026-23.md:9: error: unknown status `sent` (use pending, paid, overdue or draft)
```

## `invgen edit [FILE]`

Opens the [editor](editor.md). `invgen` alone does the same.

- With `FILE`, opens that invoice. If `FILE` does not exist yet, the editor starts
  a new invoice that will be saved to that path.
- Without `FILE`, reopens the last file you worked on, or starts a new invoice.

## `invgen new [--client KEY] [--open]`

Creates the next invoice in `invoices_dir` and prints its path.

| Option | Meaning |
|---|---|
| `--client KEY` | Client key from the config. Default: the first client (in alphabetical order of keys). An unknown key is an error that lists the known ones. |
| `--open` | Open the new file in the editor afterwards. |

The file is named `<number>.md`, where the number is the
[next free number](configuration.md#invoice-numbers). invgen refuses to overwrite
an existing file. The template contains:

- front matter with `number`, today's `date`, `due_days`, `status: pending` and
  `client`, plus commented hints for `currency`, `tax_rate` and `signature`;
- a heading with the client's `description` and the previous month as the
  period, for example `October 2026 period`;
- one table row with the client's `description`, `unit` and `rate`, and the
  number of weekdays of the previous month as the quantity.

With no clients in the config, the template has an inline `to:` block to fill in
instead.

## `invgen build FILE.md [-o OUT.pdf] [--png DIR]`

Renders one invoice.

| Option | Meaning |
|---|---|
| `-o`, `--output OUT.pdf` | Where to write the PDF. Default: `<number>.pdf` in the same folder as `FILE.md`. Missing parent folders are created. |
| `--png DIR` | Also write a PNG of every page into `DIR` (144 dpi): `<number>.png`, or `<number>-1.png`, `<number>-2.png`, ... for multi-page invoices. |

On success it prints one line with the number, client, total and output path.
On a parse error it prints the error and exits with status 1.

## `invgen watch FILE.md [-o OUT.pdf]`

Builds the PDF once, then again every time `FILE.md` is saved, until you press
Ctrl+C. The config is read again on every rebuild, so config edits are picked
up too. A failed build prints the error and keeps watching. `-o` works as in
`build`. See the [watch mode recipe](recipes/watch.md).

## `invgen list`

Prints a table of all invoices in `invoices_dir` (and up to two levels of
subfolders), in issue order:

```text
NUMBER        DATE        CLIENT                     TOTAL  STATUS
EINV-2026-20  2026-07-01  Northwind Labs SL  EUR 10,350.00  pending
EINV-2026-21  2026-08-01  Northwind Labs SL   EUR 9,450.00  pending
EINV-2026-22  2026-10-01  Northwind Labs SL  EUR 12,450.90  paid
2026 total                                   EUR 32,250.90
```

After each year it prints that year's total, one line per currency. Files that
fail to parse are reported as `skipped:` with their error, and the command then
exits with status 1 (after printing the table), which makes it usable as a check
in scripts.

## `invgen config [--edit]`

Prints the path of the config file in use. If the file does not exist, it is
created first with commented placeholders.

| Option | Meaning |
|---|---|
| `--edit` | Open the file in `$VISUAL`, else `$EDITOR`, else `nano`. When the editor exits, the file is validated and any error is shown as a warning (exit status 1). |

## Exit status

`0` on success, `1` on any error (bad arguments, unreadable files, a parse error,
a skipped file in `list`, an invalid config after `config --edit`).
