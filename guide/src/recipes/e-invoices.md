# E-invoices: Factur-X and Peppol

More and more clients (and, in France, Germany, Belgium and elsewhere, the law) want
invoices in a structured format that accounting software reads without retyping.
invgen writes the European standard EN 16931 in its two common forms:

* **Factur-X / ZUGFeRD**: your usual PDF, saved as PDF/A-3b with the invoice data
  embedded as `factur-x.xml`. People see the PDF; software reads the XML. Send it
  by email like any invoice.
* **UBL (Peppol BIS Billing 3.0)**: a separate `.xml` file, the format Peppol
  access points and many public-sector portals accept.

## Try it

```sh
invgen build ~/Documents/Invoices/EINV-2026-21.md --facturx
invgen build ~/Documents/Invoices/EINV-2026-21.md --ubl     # also EINV-2026-21.xml
```

If something the standard requires is missing, nothing is written and every gap is
listed with the key to set:

```text
EINV-2026-21.md: error: cannot write a Factur-X e-invoice, 1 field is missing or invalid:
  BT-48: reverse charge needs the client's VAT ID: set `vat_id:` on the client (with its country prefix, like `ESB12345678`) or add a `tax:` line like "VAT ID: ESB12345678"
```

## Fill in what is missing

invgen reads VAT IDs, countries, postcodes and cities from your `tax:` and
`address:` lines when it can (`VAT ID: LU12345678`, `[..., "28001", Spain]`). When it
cannot, add the explicit fields. They are only used for e-invoices and never printed:

```yaml
clients:
  northwind:
    name: Northwind Labs SL
    address: ["Calle Mayor, 12", Madrid, "28001", Spain]
    tax: ["Tax ID: B12345678"]       # printed as is
    vat_id: ESB12345678              # the same number as a VAT ID, with prefix
    buyer_reference: NW-PO-2026      # the reference the client asked you to quote
```

For Peppol (`--ubl`) every invoice also needs a buyer reference (on the client, or
`buyer_reference:` in the invoice for a purchase order number) and an electronic
address for both sides. The address defaults to the VAT ID for most EU countries;
otherwise set `endpoint: "<scheme>:<id>"` with the Peppol participant ID your client
gave you.

## Make it the default

```yaml
# config.yaml
e_invoice: facturx          # or ubl, or [facturx, ubl]
```

Now every `invgen build`, `invgen watch` and **Export PDF** in the editor writes
Factur-X. `invgen build --no-e-invoice` makes a plain PDF once.

## Good to know

* Credit notes become EN 16931 credit notes (type 381) that point at the invoice
  they correct.
* Reverse charge, exempt, outside-scope and zero-rated invoices get the right VAT
  category from `tax_rate` and `tax_note`.
* Custom templates work: the XML is attached whatever the layout.

The full mapping, the heuristics and what was validated (Mustang, veraPDF, the
official EN 16931 and Peppol rules) are in
[E-INVOICING.md](https://github.com/ldolberg/invgen/blob/main/docs/E-INVOICING.md).
