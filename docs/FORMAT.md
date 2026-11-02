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
accent: "#111827"             # optional
language: en                  # optional: en, es, de, fr, it, pt, nl
template: my-invoice.typ      # optional: custom typst template (docs/TEMPLATES.md)
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
4. **Total** = `subtotal + tax`.

So the line totals shown on the invoice always add up to the subtotal, and subtotal
plus tax always equals the total, to the cent. Rates and quantities with more than two
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
  The PDF always shows the status as written. `invgen list` and the editor sidebar
  show an effective status instead: a `pending` invoice whose due date has passed
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
  language changes every label on the invoice (headings, column names, status);
  dates and amounts keep their format.
* `template`: front matter, then the config; none means the built-in template.
* `issued_hash`: written by invgen when the invoice is first exported; see
  [Issued invoices](#issued-invoices-issued_hash). Do not write it by hand.
* `sent_on`: `YYYY-MM-DD`, written by `invgen send --mark-sent`. Informational only.

## Issued invoices (`issued_hash`)

Once an invoice is sent it should not change. When a non-draft invoice is exported
to PDF for the first time (`invgen build`, `invgen send`, or Export PDF / Send in the
editor), invgen adds one line to its front matter:

```yaml
issued_hash: "3f2a9c4e1b7d6a05"
```

It is the first 16 hex digits of a SHA-256 over the fully resolved invoice: the JSON
of the invoice model after config defaults, the client entry and every override are
applied (sender, client, payment details, items, tax, dates, language, labels,
template path...). The written `status`, `paid_on` and `sent_on` are left out, so
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

## Sending (`invgen send`)

`invgen send FILE|NUMBER` builds the PDF when it is missing or older than the invoice
file or the config (which also locks the invoice, see above), then opens the desktop
mail client with:

* **To**: the client's `email` (`clients.<key>.email`, or `to: email:` inline).
  Without one, the mail client asks for the recipient.
* **Subject** and **body** from the config's `email:` block, else built-in defaults in
  the invoice language (`en`, `es`, `de`, `fr`, `it`, `pt`, `nl`).
* **Attachment**: the PDF (`<number>.pdf` next to the invoice file).

Placeholders in `email.subject` and `email.body`: `{number}`, `{total}` (e.g.
`EUR 9,450.00`), `{due}` (`YYYY-MM-DD`, empty without a due date), `{client}` (client
name) and `{from}` (your name). Unknown `{...}` text is kept as written. The subject
is one line; line breaks in it become spaces.

```yaml
email:
  subject: "Invoice {number} from {from}"
  body: |
    Hello,

    please find attached invoice {number} for {total}, due {due}.

    Best regards,
    {from}
```

How the mail client is opened:

* **Linux** (and other freedesktop systems): `xdg-email --subject ... --body ...
  --attach <pdf> <to>`, which opens the default mail client with everything filled in.
  If `xdg-email` is missing, a `mailto:` link is opened with `xdg-open` and the PDF
  path is printed to attach by hand.
* **macOS**: Mail, through `osascript`, with every field and the attachment. If that
  fails, a `mailto:` link is opened with `open` and the PDF path is printed.
* **Windows**: a `mailto:` link (opened through `explorer.exe`); mailto cannot carry
  attachments, so the PDF path is printed.

invgen never sends mail itself and never asks for or stores SMTP credentials: the
message is only handed to your mail client, and you press Send there.
`--dry-run` prints the message and the command instead of running anything;
`--mark-sent` records `sent_on: <today>` in the front matter after the mail client
opened. The editor's **Send** button exports the PDF and then does the same.

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
language: en                         # optional: en, es, de, fr, it, pt, nl
labels:                              # optional: override single labels
  bill_to: Customer
template: ~/invgen/my-invoice.typ    # optional: custom typst template
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
clients:
  northwind:
    name: Northwind Labs SL
    address: ["Calle Mayor, 12", Madrid, "28001", Spain]
    tax: ["Tax ID: B12345678"]
    email: billing@example.com   # optional: recipient for `invgen send`
    currency: EUR      # optional per-client defaults
    tax_rate: 0
    language: es       # optional: invoices for this client in Spanish
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
