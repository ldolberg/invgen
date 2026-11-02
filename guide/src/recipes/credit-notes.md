# Correcting an invoice with a credit note

In the EU an invoice you have sent must not be edited or deleted. To correct it,
issue a credit note that refers to it and reverses all or part of it, then (if
needed) a new invoice with the right amounts.

## Create the credit note

```sh
invgen new --credit EINV-2026-20
# ~/Documents/Invoices/CN-2026-01.md
```

invgen finds invoice `EINV-2026-20` in your invoice library and writes the next
credit note, prefilled from it:

```markdown
---
kind: credit_note
number: CN-2026-01
date: 2026-11-02
refers_to: EINV-2026-20
refers_to_date: 2026-10-01
status: pending
client: northwind
---

| Description              | Qty | Unit | Rate   |
|--------------------------|-----|------|--------|
| Senior Software Engineer | 21  | days | 450.00 |
```

The client, currency, tax rate and any other front matter of the invoice are
kept; number, dates and status are new. Add `--open` to edit it right away.

## Credit only part of the invoice

Delete the lines you do not want to credit, or lower their quantity. For two days
billed by mistake, keep one line with `2` days. **Keep quantities and rates
positive**: the credit note itself counts as negative.

## What it looks like

The PDF is titled **Credit note** (translated with the invoice language) and says
"Refers to invoice EINV-2026-20 of October 1, 2026" under the title. Line totals,
subtotal, tax and total are shown as negative amounts, for example `EUR -900.00`.

`invgen list` shows the credit note with its negative total, marked
`credit note for EINV-2026-20`, and subtracts it from the yearly totals.

## Numbering

Credit notes have their own sequence, `CN-<year>-<seq>`. Change the prefix with
`credit_prefix` in the config. Creating a credit note never uses up an invoice
number, and the other way round.

## Writing one by hand

Any invoice file becomes a credit note with `kind: credit_note` and
`refers_to: <invoice number>`. `refers_to_date` is optional: when it is missing,
invgen looks up the date of that invoice in your library. Without `number:`, the
next credit note number is used. A credit note has no due date unless you write
`due` or `due_days`. See the [invoice format](../format.md#credit-notes).
