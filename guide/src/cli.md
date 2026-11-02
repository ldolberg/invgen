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

```text
invgen list [--year YYYY] [--client KEY|NAME] [--status STATUS]... [--unpaid] [--format table|csv|json]
```

Prints a table of the invoices in `invoices_dir` (and up to two levels of
subfolders), in issue order:

```text
NUMBER        DATE        CLIENT                     TOTAL  STATUS
EINV-2026-20  2026-07-01  Northwind Labs SL  EUR 10,350.00  overdue 94d
EINV-2026-21  2026-08-01  Northwind Labs SL   EUR 9,450.00  overdue 63d
EINV-2026-22  2026-10-01  Northwind Labs SL  EUR 12,450.90  paid
2026 total                                   EUR 32,250.90

OUTSTANDING  NOT DUE YET  1-30 DAYS  31-60 DAYS   61+ DAYS      TOTAL
EUR                 0.00       0.00        0.00  19,800.00  19,800.00
```

After each year it prints that year's total, one line per currency. Below the
table comes the outstanding balance: the total of the invoices whose
[effective status](format.md) is `pending` or `overdue`, per currency, split by
days past the due date (not due yet, 1-30, 31-60, 61 or more). Invoices without
a due date count as not due yet. Drafts and paid invoices are not outstanding.

| Option | Meaning |
|---|---|
| `--year YYYY` | Only invoices dated in that year. |
| `--client KEY\|NAME` | Only one client: a client key from the config, or part of the client name. Case-insensitive. |
| `--status STATUS` | Only invoices with that effective status: `pending`, `paid`, `overdue` or `draft`. Repeat it to allow several (`--status pending --status overdue`). |
| `--unpaid` | Only unpaid invoices, the same as `--status pending --status overdue`. |
| `--format FORMAT` | `table` (default), `csv` or `json`. See [Export](#export). |

Filters combine: `invgen list --year 2026 --client northwind --unpaid` shows
what Northwind still owes for 2026. The yearly totals and the outstanding
balance only count the invoices shown.

Files that fail to parse are reported on stderr as `skipped:` with their error,
and the command then exits with status 1 (after printing the table), which
makes it usable as a check in scripts.

### Export

`--format csv` writes RFC 4180 CSV with a header row; `--format json` writes a
JSON array with the same fields plus the line items. Amounts are exact decimal
strings with two decimals (`9450.00`), never floating point numbers. Only the
data goes to stdout, so it can be piped or redirected:

```sh
invgen list --year 2026 --format csv > invoices-2026.csv
invgen list --unpaid --format json | jq -r '.[] | "\(.number) \(.total)"'
```

The fields are documented in
[docs/EXPORT.md](https://github.com/ldolberg/invgen/blob/main/docs/EXPORT.md).

## `invgen check [--strict]`

Scans the library and reports problems that matter for bookkeeping, compiler
style, one per line:

```text
invoices/EINV-2026-23.md:2: error: duplicate number EINV-2026-23 (also in invoices/2026/EINV-2026-23.md)
invoices/EINV-2026-27.md:2: warning: gap in the EINV sequence before EINV-2026-27: 25-26 missing
invoices/EINV-2026-28.md:3: error: EINV-2026-28 is dated 2026-09-01, before the lower number EINV-2026-27 (2026-10-01)
28 invoice(s) checked: 2 error(s), 1 warning(s)
```

Errors:

- a file that fails to parse;
- a duplicate number, or the same sequence number used twice under one prefix
  (sequences are global across years, so `EINV-2025-5` and `EINV-2026-05` collide);
- a higher number dated before a lower one (drafts are ignored here);
- an invoice whose client has no name.

Warnings:

- a gap in the sequence of a prefix (between its lowest and highest number); a
  number that only exists as a PDF in the library fills its place;
- an invoice without `number:` in its front matter (it gets an automatic one
  each time it is parsed, so it is left out of the sequence checks);
- a number that does not look like `PREFIX-YYYY-N`, or whose year differs from
  the invoice date;
- a sender without tax lines (`from: tax:`).

It exits with status 0 when there are no errors and 1 otherwise. Warnings alone
do not fail, unless `--strict` is given. See the
[pre-commit hook recipe](recipes/git.md#check-before-committing).

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

## `invgen template --export [PATH] [--force]`

Writes the built-in typst template to `PATH` (or prints it when `PATH` is left
out), as a starting point for a custom `template:`. An existing file is only
replaced with `--force`. The data a template receives is documented in
[TEMPLATES.md](https://github.com/ldolberg/invgen/blob/main/docs/TEMPLATES.md).

## Exit status

`0` on success, `1` on any error (bad arguments, unreadable files, a parse error,
a skipped file in `list`, problems found by `check`, an invalid config after
`config --edit`).
