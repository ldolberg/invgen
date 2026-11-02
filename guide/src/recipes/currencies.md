# Multiple currencies

invgen prints amounts in whatever currency an invoice uses. It does not convert
between currencies; each invoice is in exactly one currency.

## A default per client

Set the global default once and override it for the clients that pay in
something else:

```yaml
currency: EUR

clients:
  northwind:
    name: Northwind Labs SL
    address: ["Calle Mayor, 12", Madrid, "28001", Spain]
  contoso:
    name: Contoso Ltd
    address: [1 Example Street, London, "EC1A 1AA", United Kingdom]
    currency: GBP
    rate: 380
    unit: days
  fabrikam:
    name: Fabrikam AG
    address: [Bahnhofstrasse 1, "8001 Zurich", Switzerland]
    currency: CHF
    tax_rate: 8.1
```

Invoices for `contoso` are in GBP, those for `fabrikam` in CHF with 8.1% tax,
everything else in EUR. `invgen new --client contoso` already writes the GBP
default as a hint in the front matter.

## One invoice in another currency

Set `currency` in the front matter; it beats the client and the config:

```yaml
---
client: northwind
currency: USD
---
```

The currency is printed as a code in front of every amount (`USD 9,450.00`), so
any code or text works.

## Writing amounts

Rates can carry a currency symbol or code, which is ignored when reading the
number: `450`, `EUR 450`, `€450` and `450 EUR` all mean 450. The currency of the
invoice still comes from the front matter, client or config, never from the
table.

## Totals across currencies

`invgen list` never adds different currencies together. The yearly summary has
one total line per currency:

```text
2026 total                                   CHF 4,860.00
2026 total                                   EUR 32,250.90
2026 total                                   GBP 7,600.00
```

The outstanding balance below the table also has one line per currency.
