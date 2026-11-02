# Custom templates

invgen lays out invoices with a [Typst](https://typst.app/docs/) template. The
built-in one lives in `assets/invoice.typ`; you can replace it with your own
without forking the project.

```sh
invgen template --export my-invoice.typ   # start from the built-in template
invgen template --export                  # or print it to stdout
```

Then point invgen at it, globally in the config or for one invoice in the front
matter:

```yaml
template: my-invoice.typ        # config.yaml, or an invoice's front matter
```

Paths may start with `~`. A relative path is resolved against the directory of
the config file (also when written in an invoice's front matter). The template
is read again on every render, so edits show up on the next build or preview
refresh.

## How a template gets its data

The template never receives typst markup from the invoice. All data is a JSON
document served as the virtual file `/invoice.json`:

```typst
#let d = json("/invoice.json")
#let t = d.labels
```

Every value is a string (already formatted), `none` (JSON `null`) or a list.
Text from the invoice is plain text: placing it with `#d.number` or
`text(d.to.name)` never evaluates it as markup.

### Files a template can read

* `/invoice.json`: the data described below.
* The logo, at the path given in `d.logo` (for example `/logo.svg`), when the
  invoice has one.
* The EPC payment QR code, an SVG at the path given in `d.payment.qr` (for
  example `/epc-qr.svg`), when the invoice has one.
* Any file in the template's own directory and its subdirectories, by a path
  relative to the template (`#import "parts.typ": *`, `image("bg.png")`). Paths
  cannot leave that directory, and nothing else on disk is reachable.

Typst packages (`@preview/...`) are not available: rendering is offline and
hermetic. The bundled fonts are Inter (regular, italic, medium, semibold, bold)
and Mrs Saint Delafield (used for the signature), plus Typst's default fonts.

### Factur-X

With `--facturx` (or `e_invoice: facturx`) invgen appends one
`#pdf.attach("/factur-x.xml", ...)` line to the end of the template source and
exports PDF/A-3b, so a custom template becomes a Factur-X PDF without changes. The
template must then stay within PDF/A-3b (fonts are always embedded; the export
reports anything else, such as an image format PDF/A forbids). See
[E-INVOICING.md](E-INVOICING.md).

### Errors

A template that fails to compile reports the template file and the line, for
example:

```
error: rendering invoice.md: template /home/me/my-invoice.typ failed to compile:
  /home/me/my-invoice.typ:12: dictionary does not contain key "totl"
```

## Data contract

`contract_version` is currently **1**. It is bumped only when a field is
renamed or removed or its meaning changes; new fields can appear without a
bump. A template can guard against a future breaking change with:

```typst
#assert(d.contract_version == 1, message: "this template expects contract version 1")
```

| Field | Type | Description |
|-------|------|-------------|
| `contract_version` | integer | Version of this contract (`1`). |
| `language` | string | Label language code: `en`, `es`, `de`, `fr`, `it`, `pt` or `nl`. |
| `locale` | string | Formatting locale of the amounts and dates, e.g. `en`, `de` or `de-CH` (the `locale:` key, else `language`). |
| `labels` | dict | Translated labels for `language`, with the config `labels:` overrides applied (see below). |
| `logo` | string or none | Virtual path of the sender's logo, for `image(d.logo)`. |
| `number` | string | Invoice number, e.g. `EINV-2026-23`. |
| `status` | string | `pending`, `paid`, `overdue` or `draft` (stable keys, not translated). |
| `status_label` | string | The status in `language`, e.g. `Pagada`. |
| `date` | string | Invoice date, formatted for `locale`, e.g. `November 1, 2026` or `1. November 2026`. |
| `due` | string or none | Due date, formatted; none when hidden. |
| `currency` | string | ISO code, e.g. `EUR`. |
| `accent` | string | Accent color as `#rrggbb` (always valid, for `rgb(d.accent)`). |
| `from`, `to` | party | Sender and recipient (see below). |
| `payment` | dict | `holder`, `iban`, `bic` (string or none), `notes` (list of strings) and `qr` (see below). |
| `items` | list of items | Line items (see below). |
| `subtotal` | string | Formatted money, e.g. `EUR 9,450.00` or `9.450,00 €`. |
| `tax_label` | string | Tax row label (front matter `tax_label`, else the translated `tax` label). |
| `tax_rate` | string | Tax rate with percent sign, e.g. `21%` or `7,5 %`. When every line uses the same rate, that rate (even if it differs from the invoice default); with several rates, the invoice default. |
| `tax` | string | Formatted tax amount (the sum of the per-rate taxes). |
| `tax_breakdown` | list of tax groups | One entry per distinct tax rate, highest rate first; empty when there are no items. More than one entry means the invoice mixes rates (see below). |
| `tax_note` | string or none | Legal tax mention to print near the totals: a `tax_note` preset already translated into `language` (from the `tax_note_*` labels), or the user's free text. |
| `total` | string | Formatted total. |
| `notes` | list of strings | Paragraphs shown under the items. |
| `signature` | string or none | Signature text; none hides it. |
| `kind` | string | `invoice` or `credit_note` (stable keys). Added without a version bump; a template that must also run on older invgen releases can read it with `d.at("kind", default: "invoice")`. |
| `refers_to` | dict or none | For a credit note: `number` (string) and `date` (formatted string, or none when unknown) of the invoice it reverses. None for invoices. |

For a credit note the amounts `subtotal`, `tax`, `total` and each item's
`total` are negative (e.g. `EUR -900.00`), while `qty` and `rate` stay as
written (positive). Use `labels.credit_note` as the title and
`labels.total_credited` instead of `labels.total_due`.

A **party** has `name` (string), `address` (list of lines), `tax` (list of
lines), `email` and `phone` (string or none). Empty lines are removed.

`payment.qr` is the virtual path of the EPC (SEPA credit transfer) QR code
image, for `image(d.payment.qr)`, or none. It is set only when the config or
front matter has `payment.qr: true` and the invoice qualifies (EUR, a valid
IBAN, a positive total); see [FORMAT.md](FORMAT.md#payment-qr-code). The SVG has
no quiet zone, so keep at least 2 mm of blank space around it. Templates
written before this field existed keep working (contract version 1).

An **item** has `description` (string), `detail` (string or none), `qty`
(formatted number, e.g. `21` or `7.5`), `unit` (string, may be empty), `rate`
(formatted amount without currency, e.g. `450.00`), `total` (formatted money)
and `tax_rate` (the rate that applies to the line, e.g. `21%`).

A **tax group** has `rate` (e.g. `10%`), `base` (formatted money: the sum of the
line totals at that rate) and `tax` (formatted money). The bases add up to
`subtotal` and the taxes to `tax`. The built-in template shows a single
`tax_label (tax_rate)` row when `tax_breakdown` has at most one entry, and one
row per group otherwise. These fields were added without a version bump; a
template that ignores them keeps working and shows the combined `tax`.

All formatted strings follow `locale` (see the number and date formatting section
of [FORMAT.md](FORMAT.md#number-and-date-formatting)): `7,5`, `450,00`,
`9.450,00 €`. Amounts may contain no-break spaces (U+00A0, U+202F). The shape of
the data does not change with the locale, so a template needs no changes.

### Labels

`labels` always contains every built-in key below. A key missing from a
language's table falls back to English, and the config's `labels:` mapping
replaces single keys. Keys under `labels:` that are not built in are passed
through too, so a custom template can define its own translatable strings.

| Key | English |
|-----|---------|
| `invoice` | Invoice |
| `status` | Status |
| `invoice_date` | Invoice date |
| `due_date` | Due date |
| `bill_to` | Bill to |
| `bill_from` | Bill from |
| `total_amount` | Total amount |
| `tax_details` | Tax details |
| `payment_method` | Payment method |
| `iban` | IBAN |
| `bic` | BIC |
| `scan_to_pay` | Scan to pay |
| `description` | Description |
| `qty_unit` | Qty (unit) |
| `rate` | Rate |
| `total` | Total |
| `subtotal` | Subtotal |
| `tax` | VAT |
| `tax_base` | Taxable amount (per-rate base in the tax breakdown) |
| `total_due` | Total due |
| `notes` | Notes |
| `contact` | Contact |
| `page_of` | Page {page} of {total} (the template replaces `{page}` and `{total}`) |
| `status_pending` | Pending |
| `status_paid` | Paid |
| `status_overdue` | Overdue |
| `status_draft` | Draft |
| `tax_note_reverse_charge` | Reverse charge: VAT to be accounted for by the recipient (Article 196 of Council Directive 2006/112/EC). |
| `tax_note_exempt` | Exempt from VAT. |
| `tax_note_outside_scope` | Outside the scope of VAT. |
| `credit_note` | Credit note |
| `refers_to` | Refers to invoice {number} of {date} (the template replaces `{number}` and `{date}`) |
| `refers_to_undated` | Refers to invoice {number} (used when the invoice date is unknown) |
| `total_credited` | Total credited |

### Example

A minimal template:

```typst
#let d = json("/invoice.json")
#let t = d.labels
#set page(paper: "a4")
#set text(font: "Inter", lang: d.language)

#if d.logo != none { image(d.logo, height: 12mm) }
= #t.invoice #d.number
#t.bill_to: #d.to.name \
#t.invoice_date: #d.date

#table(
  columns: (1fr, auto, auto),
  [*#t.description*], [*#t.qty_unit*], [*#t.total*],
  ..d.items.map(it => (it.description, it.qty + " " + it.unit, it.total)).flatten(),
)

*#t.total_due: #d.total*
```
