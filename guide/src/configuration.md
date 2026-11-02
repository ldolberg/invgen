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
language: en                         # labels: en, es, de, fr, it, pt, nl
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
    language: es                     # optional: this client's invoices in Spanish
    rate: 450                        # used by the new-invoice template
    unit: days
    description: Senior Software Engineer
```

| Key | Default | Meaning |
|---|---|---|
| `invoices_dir` | `~/Documents/Invoices` | Where `invgen new` and the editor create invoices, where `invgen list` and the editor sidebar look, and the folder scanned for the next number. |
| `number_prefix` | `EINV` | Prefix of generated numbers. The starter file written by `invgen config` sets `INV`. |
| `currency` | `EUR` | Currency code printed next to amounts. Any text works; invgen does not convert between currencies. |
| `tax_rate` | `0` | Percent applied to the subtotal. |
| `due_days` | `14` | Days from the invoice date to the due date. |
| `language` | `en` | Language of every label on the invoice: `en`, `es`, `de`, `fr`, `it`, `pt`, `nl`. An unknown value is an error. Dates and amounts keep their format. |
| `labels` | none | Map of label key to text, replacing single labels in any language (keys in [TEMPLATES.md](https://github.com/ldolberg/invgen/blob/main/docs/TEMPLATES.md#labels)). |
| `template` | built-in | Path of a custom typst template. See [TEMPLATES.md](https://github.com/ldolberg/invgen/blob/main/docs/TEMPLATES.md). |
| `from` | empty | Sender: `name`, `address` (list of lines), `tax` (list of lines), `email`, `phone`, `logo` (SVG or PNG shown top-left; a missing file is an error). |
| `payment` | empty | `holder`, `iban`, `bic`, `notes` (list of lines). |
| `clients` | none | Map of client key to a party (`name`, `address`, `tax`, `email`, `phone`) plus optional `currency`, `tax_rate`, `language`, `rate`, `unit`, `description`. The client's `email` is the recipient of [`invgen send`](cli.md#invgen-send-filenumber---dry-run---mark-sent). |
| `email` | per language | `subject` and `body` of the message `invgen send` prepares, with the placeholders `{number}`, `{total}`, `{due}`, `{client}`, `{from}`. |

Unknown keys are ignored, so a typo in a key name does not raise an error. If a
value does not show up on the invoice, check the spelling first.

Paths (`from.logo`, `template`) may start with `~`; relative paths are resolved
against the folder of the config file.

## Precedence

For each invoice, `currency`, `tax_rate` and `language` are taken from the first place that
sets them:

1. the invoice front matter,
2. the client entry,
3. the top-level config.

`due_days` comes from the front matter, then the config. `rate`, `unit` and
`description` on a client are only used to prefill new invoices (`invgen new` and
**Ctrl+N** in the editor); they never change an existing invoice.

An invoice can also override any config key for itself with a `config:` block.
See [Per-invoice overrides](recipes/overrides.md).

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
