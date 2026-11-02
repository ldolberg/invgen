# Invoice markdown format

An invoice is a markdown file with YAML front matter. Static data (who you
are, your bank, your clients) lives once in `~/.config/invgen/config.yaml`; the
invoice file only says what changes month to month.

```markdown
---
number: EINV-2026-23          # optional: auto = next number after the highest in the invoices dir
date: 2026-11-02              # optional: defaults to today
due: 2026-11-16               # optional: or `due_days: 14`, or `false` to hide it
status: pending               # pending | paid | overdue | draft
client: northwind       # key into config `clients:`  ...or an inline `to:` block
currency: EUR                 # optional: falls back to client currency, then config default
tax_rate: 0                   # percent; optional, falls back to client, then config
tax_label: VAT                # optional
accent: "#111827"             # optional
---

# Senior Software Engineer
October 2026 period

| Description              | Qty | Unit | Rate   |
|--------------------------|-----|------|--------|
| Senior Software Engineer | 21  | days | 450.00 |

Any other paragraph becomes a note under the table.
```

## Body rules

* **Items table**: the first markdown table. Columns are matched by header name
  (case-insensitive, anything after `(` ignored, so `Rate (EUR)` works):
  `description`/`item`/`service`, `qty`/`quantity`, `unit`, `rate`/`price`/`unit price`,
  optional `detail`. A `Days` or `Hours` header is a quantity column that also sets
  the unit. Other columns (e.g. `Total`) are ignored; totals are always computed.
  Description, quantity and rate columns are required. Fully empty rows are skipped.
  `Qty` may be written `21 days` (the unit is taken from it if there is no unit column
  or the unit cell is empty). Later tables are ignored.
* **Numbers** accept `1,234.50` (`,` is only a thousands separator in groups of three,
  `.` is the decimal point; `1.234,50` is an error). A leading or trailing currency
  symbol/code is ignored: `EUR 450`, `€450`, `450 EUR`. Negative rates are allowed (discounts).
* **List items** are an alternative to a table, used only when the document has no table:
  `- Senior Software Engineer: 21 days @ 450` (`@` or ` x `). A list is an items list
  when its first entry has that shape; then every entry must have it. The `detail` is the
  item's continuation lines (indented text under it) and/or its nested list entries
  (joined with `; `). Lists whose first entry is not item-shaped, and lists under a
  `Notes` heading, become notes (one note per entry).
* **Headings** are ignored for data (they are for you), except a heading named
  `Notes` whose following paragraphs and lists become notes (until the next heading).
* **Lead paragraph**: a paragraph directly after a (non-`Notes`) heading and before the
  items becomes the `detail` of the item when the invoice has exactly one item and that
  item has no detail of its own. In every other case it is a regular note. This is what
  turns `October 2026 period` in the example above into the item detail.
* **Paragraphs** outside the table/list become `notes`, in order (inline markdown
  stripped to text, line breaks joined with spaces). HTML comments, code blocks, quotes
  and rules are ignored, so `<!-- ... -->` is a safe place for remarks.
* An invoice without items is an error. Errors carry the 1-based line in the whole
  file (front matter included).

## Front matter resolution

* No front matter at all is fine: everything takes its default. Unknown keys are ignored.
* `currency`, `tax_rate`: front matter, then the client, then the config.
  `tax_rate` is a percent (`21` or `"21%"`).
* `client:` must be a key of `clients:` in the config (the error lists the known ones).
  Without `client:` and without `to:`, the recipient is empty.
* `number`: when absent, the next number from the invoices dir (see below). Prefer
  writing it explicitly (`invgen new` does): an auto number for a file that already
  lives in the invoices dir would count the file itself (or its PDF) and skip a number.
* `date`: `YYYY-MM-DD`, default today. `due`: explicit date, else `date + due_days`
  (front matter `due_days`, else config `due_days`). `due: false` (or `due: none`)
  leaves the due date off the invoice.
* `status`: `pending` (default), `paid`, `overdue`, `draft`; case-insensitive.
  The PDF always shows the status as written. `invgen list` and the editor sidebar
  show an effective status instead: a `pending` invoice whose due date has passed
  shows as `overdue` with the days late (`overdue 12d`). Any other written status
  wins, and an invoice without a due date (`due: false`) is never overdue automatically.
* `signature`: defaults to `from.name`; `false` hides it, a string replaces it.
* `tax_label` defaults to `VAT`; `accent` defaults to `#111827` (`#rgb` or `#rrggbb`).

## Front matter overrides

Any of `from:`, `to:`, `payment:` may be given inline using the same shape as the
config (`name`, `address: [..]`, `tax: [..]`, `email`, `phone` / `holder`, `iban`, `bic`, `notes`).
Inline values replace the config ones field by field.

### Overriding any config key: `config:`

A `config:` block takes the exact shape of `config.yaml` and is deep-merged over
it for this invoice only. Mappings merge key by key, while scalars and lists replace.
Set a value to `null` to remove an optional field (`phone`, `email`, a client's
`tax_rate`...).

```yaml
---
client: northwind
config:
  number_prefix: INV        # auto-numbering for this invoice uses INV-...
  due_days: 30
  from:
    name: Morgan Studio     # other `from` fields (address, tax...) are kept
    phone: null             # hide the phone on this invoice
  payment:
    iban: CH93 0076 2011 6238 5295 7   # bic and holder are kept
  clients:
    northwind:
      currency: CHF
      address: [Bahnhofstrasse 1, "8001 Zurich", Switzerland]
---
```

The block edits the config itself, so the usual precedence then applies to the
merged result: top-level front matter keys (`currency`, `tax_rate`, `due_days`,
`from:`, `to:`, `payment:`) win, then the client entry, then the global keys. So a
global `tax_rate` in the block does not beat a client's own `tax_rate`; override it
under `clients:` (or set it to `null` there) or use the top-level `tax_rate:` key.
An invalid override is reported at the `config:` line.

## Config (`~/.config/invgen/config.yaml`)

```yaml
invoices_dir: ~/Documents/Invoices   # where `invgen new` puts .md and exports .pdf
number_prefix: EINV                  # numbers look like EINV-<year>-<seq>
currency: EUR
tax_rate: 0
due_days: 14
from:
  name: Alex Morgan
  address: [Rue de la Gare, Luxembourg, "1616", Luxembourg]
  tax: ["VAT ID: LU12345678", "Tax ID: R.C.S. Luxembourg B000000 Business Permit 00000000/0"]
  email: alex@example.com
  phone: "+352 600 000 000"
payment:
  holder: ALEX MORGAN
  iban: DE89 3704 0044 0532 0130 00
  bic: COBADEFFXXX
clients:
  northwind:
    name: Northwind Labs SL
    address: ["Calle Mayor, 12", Madrid, "28001", Spain]
    tax: ["Tax ID: B12345678"]
    currency: EUR      # optional per-client defaults
    tax_rate: 0
    rate: 450          # default rate used by templates
    unit: days
    description: Senior Software Engineer
```

Sequence numbers are global and monotonically increasing across years
(EINV-2025-13 is followed by EINV-2026-14). The next number scans the invoice
library (see below) for `<prefix>-YYYY-N*.md` / `.pdf` files, with N
padded or not (`EINV-2025-1`, `EINV-2025-02`), takes the highest N of any year plus one
and formats it with at least two digits: `EINV-2026-01` for an empty directory.
`~` in `invoices_dir` is expanded; the default is `~/Documents/Invoices`.

The invoice library is `invoices_dir` plus up to two levels of subfolders
(`Invoices/2026/`, `Invoices/clients/acme/`); hidden folders such as `.git` are
skipped. `invgen list`, the editor sidebar and next-number scanning all use it.
