# Quickstart

This walks through a first invoice. The names and numbers are examples; use
your own.

## 1. Create the config

```sh
invgen config --edit
```

This creates `~/.config/invgen/config.yaml` with commented placeholders and opens
it in `$VISUAL` or `$EDITOR` (`nano` when neither is set). Fill in your name,
address, bank details and at least one client:

```yaml
invoices_dir: ~/Documents/Invoices
number_prefix: INV
currency: EUR
tax_rate: 0
due_days: 14

from:
  name: Alex Morgan
  address: [Rue de la Gare, Luxembourg, "1616", Luxembourg]
  tax: ["VAT ID: LU12345678"]
  email: alex@example.com

payment:
  holder: ALEX MORGAN
  iban: DE89 3704 0044 0532 0130 00
  bic: COBADEFFXXX

clients:
  northwind:
    name: Northwind Labs SL
    address: ["Calle Mayor, 12", Madrid, "28001", Spain]
    tax: ["Tax ID: B12345678"]
    rate: 450
    unit: days
    description: Senior Software Engineer
```

When you close your text editor, invgen loads the file again and warns about
mistakes. See [Configuration](configuration.md) for every key.

## 2. Create an invoice

```sh
invgen new --client northwind
```

invgen prints the path of the new file, for example
`~/Documents/Invoices/INV-2026-01.md`. The number is the next free one in your
invoices directory. The file is prefilled for last month: one line item using
the client's `description`, `unit` and `rate`, and a quantity equal to the
number of weekdays (Monday to Friday) in that month.

## 3. Edit it with a live preview

```sh
invgen edit ~/Documents/Invoices/INV-2026-01.md
```

Change the quantity, add rows, write a note. The preview on the right updates as
you type, and mistakes are shown with their line number.
`invgen new --client northwind --open` does steps 2 and 3 in one go.

## 4. Build the PDF

Press **Ctrl+E** in the editor, or from a terminal:

```sh
invgen build ~/Documents/Invoices/INV-2026-01.md
```

The PDF is written next to the markdown file and named after the invoice
number: `INV-2026-01.pdf`.

## 5. Keep track

```sh
invgen list
```

shows every invoice in the invoices directory with its date, client, total and
status, plus a total per year. When a client pays, change `status: pending` to
`status: paid` in the file.

## Next steps

- Learn the full [invoice format](format.md): tables or lists, notes, due dates,
  tax.
- Set up a [monthly routine](recipes/monthly-billing.md).
