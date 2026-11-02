# Export format

`invgen list --format csv` and `invgen list --format json` write the invoices
of the library to stdout, for spreadsheets, accounting tools and scripts. Both
respect the `list` filters (`--year`, `--client`, `--status`, `--unpaid`) and
use the same issue order as the table.

Only the data goes to stdout. Files that fail to parse are reported on stderr
(`skipped: ...`) and make the command exit with status 1, but the output still
contains every invoice that did parse.

## Fields

| Field | Type | Meaning |
|---|---|---|
| `number` | string | Invoice number, e.g. `EINV-2026-23`. |
| `date` | string | Issue date, `YYYY-MM-DD`. |
| `due` | string or null | Due date, `YYYY-MM-DD`. Empty in CSV, `null` in JSON when the invoice has no due date (`due: false`). |
| `status` | string | Effective status: `pending`, `paid`, `overdue` or `draft`. A `pending` invoice past its due date is `overdue` (see [FORMAT.md](FORMAT.md)). |
| `paid_on` | string or null | Payment date, `YYYY-MM-DD`, from the front matter `paid_on:` (set by `invgen paid`). Empty in CSV, `null` in JSON when not recorded. |
| `client_key` | string or null | The `client:` key of the front matter. Empty in CSV, `null` in JSON for invoices with an inline `to:` block only. |
| `client` | string | Client name as printed on the invoice. |
| `currency` | string | ISO code, e.g. `EUR`. |
| `subtotal` | string | Sum of the line totals. |
| `tax` | string | Tax amount. |
| `total` | string | `subtotal + tax`. |
| `file` | string | Path of the markdown file (under `invoices_dir`). |
| `items` | array | JSON only, see below. |

Amounts (`subtotal`, `tax`, `total` and item `total`) are exact decimal strings
with exactly two decimals and a dot as decimal separator, no thousands
separator and no currency: `9450.00`, `-120.50`. They are strings in JSON too,
so no precision is lost to binary floating point. They are the same values the
PDF shows (see [Totals and rounding](FORMAT.md#totals-and-rounding)).

## CSV

RFC 4180: a header row, comma separated, CRLF line endings, UTF-8. A field is
quoted with double quotes when it contains a comma, a double quote or a line
break; a double quote inside a field is doubled.

```text
number,date,due,status,paid_on,client_key,client,currency,subtotal,tax,total,file
EINV-2026-22,2026-10-01,2026-10-15,paid,2026-10-12,northwind,Northwind Labs SL,EUR,12450.90,0.00,12450.90,/home/alex/Documents/Invoices/EINV-2026-22.md
EINV-2026-23,2026-11-02,2026-11-16,pending,,northwind,Northwind Labs SL,EUR,9450.00,0.00,9450.00,/home/alex/Documents/Invoices/EINV-2026-23.md
```

## JSON

A pretty-printed array with one object per invoice (an empty library or no
match gives `[]`). Each object has the fields above in that order, plus
`items`, the invoice lines:

| Item field | Type | Meaning |
|---|---|---|
| `description` | string | Line description. |
| `detail` | string or null | Secondary line under the description. |
| `quantity` | string | Exact decimal as written, without trailing zeros: `21`, `7.5`. |
| `unit` | string | Unit label, `""` when none. |
| `rate` | string | Exact decimal as written, without trailing zeros: `450`, `12.345`. |
| `total` | string | `quantity * rate` rounded to cents, two decimals. |

```json
[
  {
    "number": "EINV-2026-23",
    "date": "2026-11-02",
    "due": "2026-11-16",
    "status": "pending",
    "paid_on": null,
    "client_key": "northwind",
    "client": "Northwind Labs SL",
    "currency": "EUR",
    "subtotal": "9450.00",
    "tax": "0.00",
    "total": "9450.00",
    "file": "/home/alex/Documents/Invoices/EINV-2026-23.md",
    "items": [
      {
        "description": "Senior Software Engineer",
        "detail": "October 2026 period",
        "quantity": "21",
        "unit": "days",
        "rate": "450",
        "total": "9450.00"
      }
    ]
  }
]
```

New fields may be added in later versions; existing fields keep their name,
type and meaning. Scripts should ignore fields they do not know.
