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

## `invgen new --from NUMBER|last [--client KEY] [--open]`

Duplicates an existing invoice for recurring work: the copy keeps the items
table or list, the notes, the front matter overrides and your formatting, and
gets the next free number. It is written to `invoices_dir` like `invgen new`
and its path is printed.

| Argument | Meaning |
|---|---|
| `--from NUMBER` | The invoice to copy: its number (`EINV-2026-23`), the bare sequence number (`23`), or a file path. |
| `--from last` | The highest-numbered invoice in the library, or the highest one with `client: KEY` when `--client KEY` is given. `--client` only works together with `--from last`. |

The copy changes only what belongs to one invoice:

- `number` is the next free number and `date` is today;
- an explicit `due:` date is removed, so the due date is computed again from
  `due_days` (`due: false` and `due_days` are kept);
- `status` becomes `pending` and `paid_on` is removed;
- in the body, a month with its year (`October 2026`, `Oct 2026`) or a
  `YYYY-MM` period (`2026-10`) that refers to the source invoice's month or the
  month before it moves forward by the number of months since the source date
  (at least one): `October 2026 period` becomes `November 2026 period`, and
  `December 2026` becomes `January 2027`. Full dates such as `2026-10-01` and
  other months are left alone.

Quantities are copied as they are, so check the number of days.

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

## `invgen paid FILE|NUMBER [--on YYYY-MM-DD]`

Marks an invoice as paid: sets `status: paid` and `paid_on:` in its front
matter. Only those lines change; comments, key order and the body stay byte for
byte as they were (a missing front matter block is created).

`FILE|NUMBER` is a path, or an invoice number looked up in the invoice library:
the full number (`EINV-2026-23`, matched against the front matter `number` and
the file name) or the bare sequence number (`23`). No match, or more than one,
is an error.

| Option | Meaning |
|---|---|
| `--on YYYY-MM-DD` | The payment date. Default: the `paid_on` already in the file, else today. |

## `invgen status FILE|NUMBER STATUS`

Sets the written status to `pending`, `paid`, `overdue` or `draft`, editing only
that line. `paid` works like `invgen paid` without `--on`; any other status
removes `paid_on`.

## `invgen config [--edit]`

Prints the path of the config file in use. If the file does not exist, it is
created first with commented placeholders.

| Option | Meaning |
|---|---|
| `--edit` | Open the file in `$VISUAL`, else `$EDITOR`, else `nano`. When the editor exits, the file is validated and any error is shown as a warning (exit status 1). |

## Exit status

`0` on success, `1` on any error (bad arguments, unreadable files, a parse error,
a skipped file in `list`, an invalid config after `config --edit`).
