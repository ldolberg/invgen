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

## `invgen config [--edit]`

Prints the path of the config file in use. If the file does not exist, it is
created first with commented placeholders.

| Option | Meaning |
|---|---|
| `--edit` | Open the file in `$VISUAL`, else `$EDITOR`, else `nano`. When the editor exits, the file is validated and any error is shown as a warning (exit status 1). |

## Exit status

`0` on success, `1` on any error (bad arguments, unreadable files, a parse error,
a skipped file in `list`, problems found by `check`, an invalid config after
`config --edit`).
