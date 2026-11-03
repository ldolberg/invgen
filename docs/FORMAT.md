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
paid_on: 2026-11-20           # optional: when it was paid (`invgen paid` writes it)
client: northwind       # key into config `clients:`  ...or an inline `to:` block
currency: EUR                 # optional: falls back to client currency, then config default
tax_rate: 0                   # percent; optional, falls back to client, then config
tax_label: VAT                # optional: defaults to the translated "VAT" label
tax_note: reverse_charge      # optional: legal mention under the totals (presets or free text)
accent: "#111827"             # optional
language: en                  # optional: en, es, de, fr, it, pt, nl
locale: de-CH                 # optional: number and date format, default follows language
date_format: "%d.%m.%Y"       # optional: strftime pattern for dates on the PDF
number_format:                # optional: override single parts of the locale
  thousands: "'"
template: my-invoice.typ      # optional: custom typst template (docs/TEMPLATES.md)
buyer_reference: PO-4711      # optional: e-invoicing BT-10, required by Peppol (docs/E-INVOICING.md)
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
  optional `detail`, optional `tax`/`vat` (also `tax rate`, `vat %`...). A `Days` or `Hours` header is a quantity column that also sets
  the unit. Other columns (e.g. `Total`) are ignored; totals are always computed.
  Description, quantity and rate columns are required. Fully empty rows are skipped.
  `Qty` may be written `21 days` (the unit is taken from it if there is no unit column
  or the unit cell is empty). Later tables are ignored.
* **Per-line tax rate**: a `Tax` (or `VAT`) column gives each row its own rate in
  percent (`21`, `21%`, `10 %`, `0%`); an empty cell uses the invoice `tax_rate`.
  In a list item, add `, tax 10%` (or `, vat 10%`, `; tax: 10%`) at the end of the
  first line: `- Training: 2 days @ 600, tax 10%`. Rates must not be negative.

  ```markdown
  | Description | Qty | Rate | Tax |
  |-------------|-----|------|-----|
  | Consulting  | 21  | 450  |     |
  | Training    | 2   | 600  | 10% |
  | Books       | 3   | 40   | 0%  |
  ```
* **Numbers** accept `1,234.50` (`,` is only a thousands separator in groups of three,
  `.` is the decimal point; `1.234,50` is an error). A leading or trailing currency
  symbol/code is ignored: `EUR 450`, `€450`, `450 EUR`. Negative rates are allowed (discounts).
  Numbers are kept as exact decimals (no binary floating point), so `0.1` is exactly
  `0.1`. See [Totals and rounding](#totals-and-rounding).
* **List items** are an alternative to a table, used only when the document has no table:
  `- Senior Software Engineer: 21 days @ 450` (`@` or ` x `). A list is an items list
  when its first entry has that shape; then every entry must have it. The `detail` is the
  item's continuation lines (indented text under it) and/or its nested list entries
  (joined with `; `). Lists whose first entry is not item-shaped, and lists under a
  `Notes` heading, become notes (one note per entry).
  Lists may nest at most 64 levels deep; deeper nesting is an error.
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

## Totals and rounding

All amounts are computed with exact decimal arithmetic, and there is one rounding
rule: **round to cents, half away from zero** (`1.005 -> 1.01`, `-1.005 -> -1.01`,
`1.004 -> 1.00`). It is applied in this order:

1. **Line total** = `quantity x rate`, rounded to cents.
   `7.5 hours @ 133.33` is `999.975`, shown as `999.98`.
2. **Subtotal** = the exact sum of the rounded line totals (no further rounding).
3. **Tax** = `subtotal x tax_rate / 100`, rounded to cents.
   `1,000.99` at `7.7%` is `77.07623`, shown as `77.08`.
   With per-line rates, the lines are grouped by rate: each rate's **base** is the
   exact sum of its rounded line totals, its tax is `base x rate / 100` rounded once
   to cents, and the invoice **tax** is the sum of the per-rate taxes. With a single
   rate this is exactly the rule above.
4. **Total** = `subtotal + tax`.

So the line totals shown on the invoice always add up to the subtotal, the per-rate
bases add up to the subtotal, and subtotal plus tax always equals the total, to the
cent. When an invoice uses more than one rate, the PDF shows the rate under each
line's price and one totals row per rate (`VAT 21%`, with its taxable amount) instead
of the single `VAT (21%)` row. Rates and quantities with more than two
decimals are used at full precision in step 1 and only displayed rounded (quantities
show at most two decimals, trailing zeros dropped). An invoice whose amounts exceed
about 7.9e28 is an error.

## Front matter resolution

* No front matter at all is fine: everything takes its default. Unknown keys are ignored.
* `currency`, `tax_rate`: front matter, then the client, then the config.
  `tax_rate` is a percent (`21`, `7.7` or `"21%"`); the same forms work for
  `tax_rate` and `rate` in the config.
* `client:` must be a key of `clients:` in the config (the error lists the known ones).
  Without `client:` and without `to:`, the recipient is empty.
* `number`: when absent, the next number from the invoices dir (see below). Prefer
  writing it explicitly (`invgen new` does): an auto number for a file that already
  lives in the invoices dir would count the file itself (or its PDF) and skip a number.
* `date`: `YYYY-MM-DD`, default today. `due`: explicit date, else `date + due_days`
  (front matter `due_days`, else config `due_days`). `due: false` (or `due: none`)
  leaves the due date off the invoice.
* `status`: `pending` (default), `paid`, `overdue`, `draft`; case-insensitive.
  The PDF always shows the status as written. `invgen list` shows an
  effective status instead: a `pending` invoice whose due date has passed
  shows as `overdue` with the days late (`overdue 12d`). Any other written status
  wins, and an invoice without a due date (`due: false`) is never overdue automatically.
* `paid_on`: `YYYY-MM-DD`, the day the invoice was paid. Optional and not printed on
  the PDF; `invgen list` shows it next to a `paid` status (`paid 2026-11-20`).
  `invgen paid` sets `status: paid` and `paid_on` for you, and `invgen status`
  with any other status removes it. Both change only those front matter lines.
* `signature`: defaults to `from.name`; `false` hides it, a string replaces it.
* `tax_label` defaults to the `tax` label of the invoice language (`VAT`, `IVA`,
  `USt.`, `TVA`, `Btw`...); `accent` defaults to `#111827` (`#rgb` or `#rrggbb`).
* `language`: front matter, then the client, then the config, default `en`. Supported:
  `en`, `es`, `de`, `fr`, `it`, `pt`, `nl` (case-insensitive; a region suffix such as
  `fr-BE` is accepted and ignored). Any other value is an error listing these. The
  language changes every label on the invoice (headings, column names, status)
  and, unless `locale` says otherwise, how dates and amounts are written on the PDF
  (see [Number and date formatting](#number-and-date-formatting)).
* `locale`, `number_format`, `date_format`: front matter, then the client, then the
  config. `number_format` merges field by field (a client `decimal` and a config
  `thousands` both apply).
* `template`: front matter, then the config; none means the built-in template.
* `tax_note`: front matter, then the client, then the config. A legal mention printed
  under the totals. Presets are translated into the invoice language:
  `reverse_charge` (reverse charge, Article 196 of Council Directive 2006/112/EC),
  `exempt` (exempt from VAT) and `outside_scope` (outside the scope of VAT); any other
  text is printed as written. `tax_note: false` on an invoice drops a client or config
  note. The preset texts are labels (`tax_note_reverse_charge`, `tax_note_exempt`,
  `tax_note_outside_scope`), so `labels:` in the config can reword them. They are
  standard mentions, not legal advice: check what your jurisdiction requires.
  `invgen build` (and the editor, on export) warns when an invoice charges no tax,
  sets no `tax_note` and the client has `tax` lines (such as a VAT ID).
* `issued_hash`: written by invgen when the invoice is first exported; see
  [Issued invoices](#issued-invoices-issued_hash). Do not write it by hand.
* `kind`: `invoice` (default) or `credit_note`; see [Credit notes](#credit-notes).
  `refers_to` and `refers_to_date` are only allowed on credit notes.

## Issued invoices (`issued_hash`)

Once an invoice is sent it should not change. When a non-draft invoice is exported
to PDF for the first time (`invgen build`, or Export PDF in the editor), invgen adds one line to its front matter:

```yaml
issued_hash: "3f2a9c4e1b7d6a05"
```

It is the first 16 hex digits of a SHA-256 over the fully resolved invoice: the JSON
of the invoice model after config defaults, the client entry and every override are
applied (sender, client, payment details, items, tax, dates, language, labels,
template path...). The written `status` and `paid_on` are left out, so
marking an invoice as paid does not break the lock. If `number:` or `date:` is
missing it is written too, since both would otherwise be resolved again on every
build. Only that line (and those keys) is added; the rest of the file stays byte for
byte. Drafts (`status: draft`) are never locked.

On every later build the hash is computed again. When it differs, the invoice was
issued and what a rebuild produces is no longer what was sent, whether the change
came from the file or from the config (a new address, a new bank account):

* `invgen build` prints a warning naming the invoice and suggesting a credit note,
  and still builds. `invgen build --strict` refuses instead (exit status 1).
* `invgen check` reports it as an error.
* The editor shows a banner on locked invoices ("Issued invoice, edits change a
  sent document"), in red when the content changed. Its **Unlock** button removes
  `issued_hash` (save to keep that); the next export locks the invoice again.

The first export is the one that counts: building a locked invoice again never
updates the hash. `invgen watch` warns like `build` but never adds a lock, since it
rebuilds while the invoice is still being written. To accept a change on purpose,
delete the `issued_hash` line (or use Unlock) and export again.

## Credit notes

An issued invoice should not be edited or deleted; a correction goes through a
credit note that references it and reverses all or part of it.

```markdown
---
kind: credit_note
number: CN-2026-01            # optional: auto = next number in the credit_prefix sequence
date: 2026-11-02
refers_to: EINV-2026-20       # required: the invoice this credit note reverses
refers_to_date: 2026-10-01    # optional: looked up from the library when omitted
status: pending
client: northwind
---

| Description              | Qty | Unit | Rate   |
|--------------------------|-----|------|--------|
| Senior Software Engineer | 2   | days | 450.00 |
```

* **Amounts are written positive.** Write quantities and rates as on the invoice
  (`2 days @ 450.00`); `kind: credit_note` makes the whole document count
  negatively. Totals and rounding work exactly as for invoices (see
  [Totals and rounding](#totals-and-rounding)), and the
  result is then negated: the PDF shows each line total, the subtotal, the tax and
  the total as negative amounts (`EUR -900.00`) while quantities and rates stay as
  written. `invgen list` and its CSV/JSON export subtract the credit note from the
  totals, so an invoice and its full credit note net to zero.
* **Numbering**: credit notes have their own sequence with `credit_prefix` (config,
  default `CN`): `CN-2026-01`, `CN-2026-02`... Invoices and credit notes never consume
  each other's numbers. Keep the two prefixes different.
* **Reference**: `refers_to` is required. The PDF is titled "Credit note" (translated
  like every label) and shows "Refers to invoice EINV-2026-20 of October 1, 2026"
  under the title. The date is `refers_to_date` when written, else the date of the
  invoice with that number in the invoice library (matched against the front matter
  `number:`, else the file name; its `date:` must be written). When neither is
  found the line leaves the date out.
* **Due date**: a credit note has none unless `due` or `due_days` is written in its
  front matter (the config `due_days` does not apply), so it is never overdue.
* `invgen check` reports a credit note whose `refers_to` is not an invoice in the
  library.
* `invgen new --credit EINV-2026-20` creates the next credit note in `invoices_dir`,
  prefilled from that invoice: its front matter is copied except `number`, `date`,
  `due`, `due_days`, `status`, `paid_on`, `issued_hash`, `kind` and
  `refers_to*`, which are replaced or removed, and its items and notes are copied as
  written. Delete or reduce lines to credit only part of the invoice. Like an
  invoice, a credit note is locked (`issued_hash`) when it is first exported.

## Branding and languages

* **Logo**: `from.logo` (SVG, PNG, JPEG, GIF or WebP) is drawn top-left in the header,
  scaled to fit 14 mm high by 60 mm wide with its aspect ratio kept. Set it in the
  config (`from:` then `logo: logo.svg`) or per invoice with `from:` / `config: from:`
  in the front matter; `logo: null` removes it for one invoice. A missing or unreadable
  file, or one that is not the image type its extension says, is an error naming the path.
* **Paths** (`from.logo`, `template`) may start with `~`. Relative paths are resolved
  against the directory of the config file, wherever they are written (config or front
  matter). In the Flatpak build, keep these files in the config directory
  (`~/.config/invgen`) or under `~/Documents`, which the sandbox can read.
* **Labels**: `labels:` in the config replaces single labels whatever the language,
  e.g. `labels: {bill_to: Customer, tax: Sales tax}`. The keys are listed in
  [TEMPLATES.md](TEMPLATES.md#labels); a key a language does not define falls back
  to English. Use `config: labels:` in the front matter to change one invoice only.
* **Templates**: see [TEMPLATES.md](TEMPLATES.md) for writing your own and the data
  it receives. `invgen template --export FILE.typ` writes the built-in one.

## Number and date formatting

Amounts, quantities, percentages and dates on the PDF follow the invoice
`language`. The invoice markdown itself is always written the same way (the body
rules above), and the CLI (`invgen build`, `invgen list`) always prints
`EUR 9,450.00`.

| `language` | Amount          | Date                     | Quantity, rate |
|------------|-----------------|--------------------------|----------------|
| `en`       | `EUR 9,450.00`  | `November 2, 2026`       | `7.5`, `21%`   |
| `de`       | `9.450,00 €`    | `2. November 2026`       | `7,5`, `21 %`  |
| `es`       | `9.450,00 €`    | `2 de noviembre de 2026` | `7,5`, `21 %`  |
| `fr`       | `9 450,00 €`    | `2 novembre 2026`        | `7,5`, `21 %`  |
| `it`       | `9.450,00 €`    | `2 novembre 2026`        | `7,5`, `21%`   |
| `pt`       | `9 450,00 €`    | `2 de novembro de 2026`  | `7,5`, `21%`   |
| `nl`       | `€ 9.450,00`    | `2 november 2026`        | `7,5`, `21%`   |

The spaces inside amounts are no-break spaces (French thousands use the narrow
no-break space U+202F), so an amount never wraps. Month and weekday names come from
built-in tables, never from the system locale. Quantities keep at most two decimals
and no thousands separator.

* **`locale`**: a language code with an optional region, written `de-CH`, `de_CH` or
  `de-ch`. It changes only numbers and dates (labels follow `language`), so an English
  invoice can use Swiss formatting. The language part must be one of the supported
  languages; an unknown region uses that language's defaults. Regions with their own
  conventions:

  | `locale`                     | Amount                              | Date               |
  |------------------------------|-------------------------------------|--------------------|
  | `en-US`, `en-CA`, `en-PH`    | `$9,450.00`, `€9,450.00`, `CHF 9,450.00` | `November 2, 2026` |
  | `en-GB`, `en-IE`, `en-AU`, `en-NZ`, `en-MT` | `£9,450.00`          | `2 November 2026`  |
  | `de-CH`, `de-LI`, `it-CH`    | `CHF 9’450.00`                      | `2. November 2026` |
  | `de-AT`                      | `€ 9 450,00`                        | `2. Jänner 2026`   |
  | `es-MX`, `es-US` (and other dollar countries) | `$9,450.00`        | `2 de noviembre de 2026` |
  | `fr-CA`                      | `9 450,00 $`                        | `2 novembre 2026`  |
  | `pt-BR`                      | `R$ 9.450,00`                       | `2 de novembro de 2026` |
  | `nl-BE`                      | `9.450,00 €`                        | `2 november 2026`  |

* **Currency symbols**: common ISO codes are written as their symbol (`EUR` €, `USD` $,
  `GBP` £, `JPY` ¥, `CNY` CN¥, `INR` ₹, `BRL` R$, `CAD` CA$, `AUD` A$, `PLN` zł,
  `SEK`/`NOK`/`DKK` kr, ...). `CHF` and any other code stay as written. A region uses
  `$` for its own dollar (`CAD` in `en-CA`) and `US$` for the US dollar there. A
  symbol before the amount touches it in English (`$9,450.00`) unless it ends in a
  letter (`CHF 9,450.00`); other locales put a space.
* **`number_format`** overrides single parts of the locale:

  ```yaml
  number_format:
    thousands: "'"            # thousands separator; "" for none
    decimal: "."              # decimal separator
    currency: symbol          # symbol (€) or code (EUR)
    currency_position: before # before or after the amount
  ```

  An unknown key, an empty decimal separator, a separator with digits or signs, or the
  same separator for thousands and decimals is an error at the `number_format:` line.
* **`date_format`**: a strftime pattern used for the invoice and due dates, e.g.
  `"%d.%m.%Y"` (`02.11.2026`), `"%Y-%m-%d"`, `"%-d %b %Y"` (`2 Nov 2026`) or
  `"%A, %-d. %B %Y"` (`Montag, 2. November 2026`). `%B`/`%b` (month) and `%A`/`%a`
  (weekday) are translated into the locale's language; `%d`, `%-d`, `%e`, `%m`, `%-m`,
  `%Y`, `%y` and the other chrono date specifiers work as usual, and `%%` is a literal
  `%`. Time specifiers (`%H`...) and unknown ones are an error. Quote the pattern in
  YAML (it starts with `%`).

## Payment QR code

`payment.qr: true` (in the config, or per invoice with `payment:` / `config: payment:`
in the front matter) prints an EPC QR code ("GiroCode", EPC069-12) under the payment
details. European banking apps scan it to prefill a SEPA credit transfer with:

* beneficiary: `payment.holder`, else `from.name` (at most 70 characters);
* IBAN (spaces removed) and BIC (optional; left out when malformed);
* amount: the invoice total in EUR, e.g. `EUR9450` or `EUR123.45` (left for the payer
  to fill in above 999,999,999.99);
* remittance text: the invoice number (at most 140 characters).

The code uses version 002, UTF-8 and error correction level M. It is skipped when the
currency is not EUR, the IBAN is missing or fails its checksum, or the total is not
positive: `invgen build` then prints a warning on stderr and still writes the PDF; the
editor preview simply shows no code. `qr: false` turns it off for one invoice.
Credit notes never show the code (there is nothing for the client to pay), without
a warning.

## E-invoices (Factur-X, UBL)

`invgen build --facturx` writes the PDF as a Factur-X / ZUGFeRD hybrid (PDF/A-3b with
the EN 16931 XML embedded); `invgen build --ubl` also writes `<number>.xml` (UBL,
Peppol BIS Billing 3.0) next to the PDF. `e_invoice: facturx`, `ubl` or
`[facturx, ubl]` in the config does the same for every build and editor export;
`--no-e-invoice` turns it off for one build. Before anything is written, the invoice
is checked and every missing mandatory field is listed. These optional party fields
exist for e-invoicing only and are never printed:

```yaml
from:                             # or a client, or `to:` in an invoice
  vat_id: DE123456789             # VAT ID with country prefix; default: read from `tax:`
  legal_id: HRB 12345             # company registration number
  country: DE                     # ISO code; default: guessed from the last address line
  endpoint: "0208:0123456789"     # Peppol address; default: from the VAT ID
  buyer_reference: PO-4711        # clients only: reference to quote (also front matter)
```

See [E-INVOICING.md](E-INVOICING.md) for the mapping, the heuristics and the rules.

## Front matter overrides

Any of `from:`, `to:`, `payment:` may be given inline using the same shape as the
config (`name`, `address: [..]`, `tax: [..]`, `email`, `phone`, `vat_id`, `legal_id`,
`country`, `endpoint`, `buyer_reference` / `holder`, `iban`, `bic`, `notes`, `qr`).
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
credit_prefix: CN                    # optional: credit notes, CN-<year>-<seq> (default CN)
currency: EUR
tax_rate: 0
due_days: 14
tax_note: exempt                     # optional: default legal tax mention
language: en                         # optional: en, es, de, fr, it, pt, nl
locale: en-GB                        # optional: number and date format (default: language)
date_format: "%-d %B %Y"             # optional: strftime pattern for dates on the PDF
number_format:                       # optional: thousands, decimal, currency, currency_position
  currency: symbol
labels:                              # optional: override single labels
  bill_to: Customer
template: ~/invgen/my-invoice.typ    # optional: custom typst template
e_invoice: facturx                   # optional: facturx, ubl or [facturx, ubl] (E-INVOICING.md)
email:                               # optional: `invgen send` message (see Sending)
  subject: "Invoice {number}"
  body: "Hello,\n\nplease find attached invoice {number} for {total}.\n\n{from}"
from:
  name: Alex Morgan
  address: [Rue de la Gare, Luxembourg, "1616", Luxembourg]
  tax: ["VAT ID: LU12345678", "Tax ID: R.C.S. Luxembourg B000000 Business Permit 00000000/0"]
  email: alex@example.com
  phone: "+352 600 000 000"
  logo: logo.svg                     # optional: relative to this file
payment:
  holder: ALEX MORGAN
  iban: DE89 3704 0044 0532 0130 00
  bic: COBADEFFXXX
  qr: true                           # optional: EPC payment QR code (EUR only)
clients:
  northwind:
    name: Northwind Labs SL
    address: ["Calle Mayor, 12", Madrid, "28001", Spain]
    tax: ["Tax ID: B12345678"]
    email: billing@example.com   # optional: recipient for `invgen send`
    vat_id: ESB12345678          # optional: e-invoicing only (E-INVOICING.md)
    buyer_reference: NW-PO-2026  # optional: e-invoicing BT-10, required by Peppol
    currency: EUR      # optional per-client defaults
    tax_rate: 0
    tax_note: reverse_charge   # optional: EU B2B services without VAT
    language: es       # optional: invoices for this client in Spanish
    locale: es-MX      # optional: also number_format, date_format per client
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
Each prefix has its own sequence: credit notes count only `<credit_prefix>-...` files,
so `CN-2026-03.md` does not advance the invoice numbers (and vice versa).

The invoice library is `invoices_dir` plus up to two levels of subfolders
(`Invoices/2026/`, `Invoices/clients/acme/`); hidden folders such as `.git` are
skipped. `invgen list`, the editor sidebar and next-number scanning all use it.

The editor's first-run setup and its Clients window write to this file in place:
only the lines of the keys they change are rewritten (a key's trailing `# comment`
is kept), new keys go at the end of their block, and everything else stays byte for
byte. They write values in the same style as above (plain scalars, quoted when
YAML would read them as something else, lists as `[a, b]`). Client keys must be
plain YAML keys: letters, digits, `-`, `_` and `.`, starting with a letter or digit.
Before writing, the result is parsed again and checked; the previous file is kept as
`config.yaml.bak`. Files the editor cannot change safely (flow mappings like
`clients: {acme: {...}}`, anchors and aliases, tags, several documents) are never
rewritten; edit those by hand.
