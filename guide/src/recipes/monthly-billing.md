# Monthly billing for one client

A contractor bills one client once a month for the days worked in the previous
month. Set the client up once, then each month takes a minute.

## Set up the client once

Give the client the defaults the template should use:

```yaml
# ~/.config/invgen/config.yaml
due_days: 14

clients:
  northwind:
    name: Northwind Labs SL
    address: ["Calle Mayor, 12", Madrid, "28001", Spain]
    tax: ["Tax ID: B12345678"]
    rate: 450
    unit: days
    description: Senior Software Engineer
```

## Every month

At the start of the month:

```sh
invgen new --client northwind --open
```

On 2 November 2026 this creates a file like:

```markdown
---
number: EINV-2026-23   # leave out to auto-number from ~/Documents/Invoices
date: 2026-11-02
due_days: 14                # or an explicit `due: YYYY-MM-DD`
status: pending             # pending | paid | overdue | draft
client: northwind
# currency: EUR               # defaults to the client, then the config
# tax_rate: 0                 # percent
# signature: false           # hide the signature, or set a custom text
---

# Senior Software Engineer
October 2026 period

| Description | Qty | Unit | Rate |
|-------------|-----|------|------|
| Senior Software Engineer | 22 | days | 450.00 |

<!-- Paragraphs below the table become notes on the invoice, e.g.
     "Payment within 14 days. Thank you!" -->
```

The quantity is the number of weekdays in October. Adjust it for holidays and
days off, add rows for anything extra (on-call hours, expenses) and press
**Ctrl+E** to export `EINV-2026-23.pdf`. Because the invoice has a single item,
the line `October 2026 period` under the heading is printed as that item's
detail.

Prefer the terminal? Edit the file in any editor and run
`invgen build ~/Documents/Invoices/EINV-2026-23.md`.

## When the client pays

Change the status in the front matter:

```yaml
status: paid
```

`invgen list` then shows the invoice as paid. invgen never changes the status on
its own; mark late invoices `overdue` yourself if you want them to stand out in
the list and on the PDF.

## A standing note

For a note that every invoice to this client should carry (a purchase order
number, a payment reference), add it to the payment notes in the config:

```yaml
payment:
  holder: ALEX MORGAN
  iban: DE89 3704 0044 0532 0130 00
  bic: COBADEFFXXX
  notes: ["Please quote the invoice number as the transfer reference"]
```

For a note on a single invoice, write a paragraph below the table.
