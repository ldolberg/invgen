# Per-invoice overrides

Most invoices only need `client:` and a table. When one invoice has to differ
from the config, override just that one; the config stays untouched.

## Top-level front matter keys

The common cases have their own keys:

```yaml
---
client: northwind
currency: USD            # instead of the client or config currency
tax_rate: 21             # percent, also "21%"
tax_label: IVA           # instead of VAT
due: 2026-12-31          # explicit due date; or due_days: 30; or due: false to hide it
accent: "#0f766e"        # accent color of the PDF
signature: false         # hide the signature, or a string to replace it
---
```

## Replacing the sender, recipient or bank

`from:`, `to:` and `payment:` take the same fields as in the config and replace
them field by field:

```yaml
---
client: northwind
to:
  address: [Paseo de la Castellana 1, Madrid, "28046", Spain]   # new billing address, name kept
payment:
  iban: CH93 0076 2011 6238 5295 7                              # holder and bic kept
---
```

A one-off client that is not in the config at all can be written inline with
`to:` and no `client:`:

```yaml
---
to:
  name: Tailspin Toys
  address: [10 Example Road, Dublin, Ireland]
  tax: ["VAT ID: IE1234567X"]
---
```

## Overriding any config key: `config:`

The `config:` block takes the shape of `config.yaml` and is merged over it for
this invoice only. Mappings merge key by key, scalars and lists replace, and
`null` removes an optional value:

```yaml
---
client: northwind
config:
  number_prefix: INV
  from:
    name: Morgan Studio      # trading name for this client; address and tax kept
    phone: null              # no phone on this invoice
  clients:
    northwind:
      currency: CHF
---
```

Precedence still applies after the merge: top-level front matter keys win over
the client entry, which wins over global keys. So to change the tax rate for a
client that has its own `tax_rate`, override it under `clients:` or use the
top-level `tax_rate:` key, not a global `tax_rate` inside `config:`.

The full rules are in [Invoice format](../format.md#front-matter-overrides).
