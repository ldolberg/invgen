# Configuration

Everything that stays the same from one invoice to the next lives in one YAML
file: who you are, how to pay you, your clients and a few defaults.

## Where the file lives

| Situation | Path |
|---|---|
| Default | `~/.config/invgen/config.yaml` (or `$XDG_CONFIG_HOME/invgen/config.yaml` when that variable is set) |
| Flatpak | Always `~/.config/invgen/config.yaml`, shared with the native build |
| `--config PATH` on any command | `PATH` |
| `INVGEN_CONFIG=PATH` in the environment | `PATH` (`--config` wins when both are given) |

`invgen config` prints the path in use and creates a commented starter file there
if none exists. `invgen config --edit` also opens it in `$VISUAL` or `$EDITOR`
(falling back to `nano`) and validates the result when you close the editor.

Without a config file invgen still works, with built-in defaults and no sender,
bank or clients. An empty file behaves the same way.

## Keys

```yaml
invoices_dir: ~/Documents/Invoices   # where invoices live; `~` is expanded
number_prefix: EINV                  # numbers look like EINV-<year>-<seq>
currency: EUR                        # default currency code
tax_rate: 0                          # default tax rate, in percent
due_days: 14                         # due date = invoice date + due_days
# tax_note: exempt                   # optional: legal tax mention under the totals
language: en                         # labels: en, es, de, fr, it, pt, nl
# locale: de-CH                      # optional: number and date format (default: language)
# date_format: "%d.%m.%Y"            # optional: strftime pattern for dates
# number_format:                     # optional: override single parts of the locale
#   thousands: "'"
labels:                              # optional: replace single labels
  bill_to: Customer
# template: my-invoice.typ           # optional: your own typst template

from:                                # you, printed as the sender
  name: Alex Morgan
  address: [Rue de la Gare, Luxembourg, "1616", Luxembourg]
  tax: ["VAT ID: LU12345678"]        # any number of tax or registration lines
  email: alex@example.com
  phone: "+352 600 000 000"
  logo: logo.svg                     # optional: SVG or PNG, relative to this file

payment:                             # printed in the payment box
  holder: ALEX MORGAN
  iban: DE89 3704 0044 0532 0130 00
  bic: COBADEFFXXX
  notes: ["Please quote the invoice number"]

clients:
  northwind:                         # the key used by `client: northwind`
    name: Northwind Labs SL
    address: ["Calle Mayor, 12", Madrid, "28001", Spain]
    tax: ["Tax ID: B12345678"]
    email: billing@example.com
    currency: EUR                    # optional per-client defaults
    tax_rate: 0
    tax_note: reverse_charge         # optional: EU B2B services without VAT
    language: es                     # optional: this client's invoices in Spanish
    # locale: es-MX                  # optional: also number_format, date_format
    rate: 450                        # used by the new-invoice template
    unit: days
    description: Senior Software Engineer
```

| Key | Default | Meaning |
|---|---|---|
| `invoices_dir` | `~/Documents/Invoices` | Where `invgen new` and the editor create invoices, where `invgen list` and the editor sidebar look, and the folder scanned for the next number. |
| `number_prefix` | `EINV` | Prefix of generated numbers. The starter file written by `invgen config` sets `INV`. |
| `currency` | `EUR` | Currency code printed next to amounts. Any text works; invgen does not convert between currencies. |
| `tax_rate` | `0` | Percent applied to the subtotal. Single lines can use another rate (a `Tax` column or `, tax 10%`), see [FORMAT.md](https://github.com/ldolberg/invgen/blob/main/docs/FORMAT.md#body-rules). |
| `tax_note` | none | Legal tax mention printed under the totals: `reverse_charge`, `exempt`, `outside_scope` (translated into the invoice language) or any free text. See [Tax notes](#tax-notes). |
| `due_days` | `14` | Days from the invoice date to the due date. |
| `language` | `en` | Language of every label on the invoice: `en`, `es`, `de`, `fr`, `it`, `pt`, `nl`. An unknown value is an error. Also sets how dates and amounts are written on the PDF unless `locale` is set. |
| `locale` | `language` | Number and date conventions on the PDF: a language with an optional region, e.g. `de`, `de-CH`, `en-US`, `pt-BR`. Labels still follow `language`. See below. |
| `number_format` | locale | `thousands`, `decimal` (separators), `currency` (`symbol` or `code`), `currency_position` (`before` or `after`). Each key is optional and overrides the locale. |
| `date_format` | locale | strftime pattern for dates on the PDF, e.g. `"%d.%m.%Y"`. Month and weekday names (`%B`, `%b`, `%A`, `%a`) are translated. |
| `labels` | none | Map of label key to text, replacing single labels in any language (keys in [TEMPLATES.md](https://github.com/ldolberg/invgen/blob/main/docs/TEMPLATES.md#labels)). |
| `template` | built-in | Path of a custom typst template. See [TEMPLATES.md](https://github.com/ldolberg/invgen/blob/main/docs/TEMPLATES.md). |
| `from` | empty | Sender: `name`, `address` (list of lines), `tax` (list of lines), `email`, `phone`, `logo` (SVG or PNG shown top-left; a missing file is an error). |
| `payment` | empty | `holder`, `iban`, `bic`, `notes` (list of lines). |
| `clients` | none | Map of client key to a party (`name`, `address`, `tax`, `email`, `phone`) plus optional `currency`, `tax_rate`, `tax_note`, `language`, `locale`, `number_format`, `date_format`, `rate`, `unit`, `description`. |

Unknown keys are ignored, so a typo in a key name does not raise an error. If a
value does not show up on the invoice, check the spelling first.

Paths (`from.logo`, `template`) may start with `~`; relative paths are resolved
against the folder of the config file.

## Precedence

For each invoice, `currency`, `tax_rate`, `tax_note`, `language`, `locale` and `date_format` are
taken from the first place that sets them (`number_format` merges key by key in the same order):

1. the invoice front matter,
2. the client entry,
3. the top-level config.

`due_days` comes from the front matter, then the config. `rate`, `unit` and
`description` on a client are only used to prefill new invoices (`invgen new` and
**Ctrl+N** in the editor); they never change an existing invoice.

An invoice can also override any config key for itself with a `config:` block.
See [Per-invoice overrides](recipes/overrides.md).

## Tax notes

Invoices without VAT usually need a legal mention. `tax_note` prints one under the
totals; the presets are translated into every supported language:

| Preset | English text |
|---|---|
| `reverse_charge` | Reverse charge: VAT to be accounted for by the recipient (Article 196 of Council Directive 2006/112/EC). |
| `exempt` | Exempt from VAT. |
| `outside_scope` | Outside the scope of VAT. |

Any other value is printed as written, and `tax_note: false` in an invoice drops the
client or config note. To reword a preset, override its label (`tax_note_exempt`,
for example) under `labels:`. These are standard wordings, not legal advice; check
what your jurisdiction requires.

`invgen build` prints a warning (and the editor shows a notice after export) when an
invoice charges no tax, has no `tax_note` and the client has `tax` lines. That is the
typical cross-border B2B case that needs the reverse charge mention.

## Number and date formatting

Amounts and dates on the PDF follow the invoice language by default:

| `language` | Amount | Date |
|---|---|---|
| `en` | `EUR 9,450.00` | `November 2, 2026` |
| `de` | `9.450,00 €` | `2. November 2026` |
| `es` | `9.450,00 €` | `2 de noviembre de 2026` |
| `fr` | `9 450,00 €` | `2 novembre 2026` |
| `it` | `9.450,00 €` | `2 novembre 2026` |
| `pt` | `9 450,00 €` | `2 de novembro de 2026` |
| `nl` | `€ 9.450,00` | `2 november 2026` |

Quantities and the tax rate use the same decimal separator (`7,5`, `21 %`). A
`locale` with a region picks regional conventions: `de-CH` writes `CHF 9’450.00`,
`en-US` writes `$9,450.00`, `en-GB` writes `2 November 2026`, `pt-BR` writes
`R$ 9.450,00`. Common currency codes become symbols (€, $, £, ¥, ...); `CHF` and
unknown codes stay as written.

For anything else, override single parts:

```yaml
locale: de-CH
number_format:
  thousands: "'"            # "" for none
  decimal: "."
  currency: code            # symbol or code
  currency_position: before # before or after
date_format: "%d.%m.%Y"
```

The command line (`invgen build`, `invgen list`) always prints `EUR 9,450.00`, whatever
the locale. The full list of regions and date specifiers is in
[docs/FORMAT.md](https://github.com/ldolberg/invgen/blob/main/docs/FORMAT.md#number-and-date-formatting).

## Invoice numbers

Numbers have the shape `<prefix>-<year>-<seq>`, for example `EINV-2026-14`. The
sequence is global: it keeps counting across years (`EINV-2025-13` is followed by
`EINV-2026-14`). To find the next number invgen scans `invoices_dir` and up to two
levels of subfolders for `.md` and `.pdf` files named `<prefix>-YYYY-N...`, takes
the highest `N` and adds one. Per-year folders such as `Invoices/2025/` are
therefore fine.

## Changes take effect

The CLI reads the config on every command (and `invgen watch` on every rebuild).
The editor reads it once at startup, so restart the editor after changing the
config.

## Full example

The repository ships a complete example with fictional data:
[`examples/config.yaml`](https://github.com/ldolberg/invgen/blob/main/examples/config.yaml).
