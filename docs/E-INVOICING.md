# E-invoicing (Factur-X / ZUGFeRD and UBL / Peppol)

invgen can write the same invoice as a structured e-invoice following the European
standard EN 16931, in two syntaxes:

| Format | Command | Output |
|---|---|---|
| Factur-X 1.0 / ZUGFeRD 2.x, profile EN 16931 | `invgen build --facturx` | The PDF itself, as PDF/A-3b with the CII XML embedded as `factur-x.xml` |
| UBL 2.1, Peppol BIS Billing 3.0 | `invgen build --ubl` | `<number>.xml` next to the PDF (`-o out/x.pdf` gives `out/x.xml`) |

Both flags can be combined. `e_invoice: facturx`, `e_invoice: ubl` or
`e_invoice: [facturx, ubl]` in `config.yaml` turns them on for every `invgen build`,
`invgen watch` and
**Export PDF** in the editor; `invgen build --no-e-invoice` writes a plain PDF once.
The setting is read from the config file: a `config:` block in an invoice does not
change it.

Credit notes (`kind: credit_note`) become type code 381 (UBL `CreditNote`) with the
invoice they reverse (`refers_to`, `refers_to_date`) as the preceding invoice reference.
Amounts stay positive, as EN 16931 expects.

## What becomes mandatory

Before anything is written, the invoice is mapped and checked. Every problem is
listed at once, with the business term (BT) and the key that fixes it, and nothing
is written (exit status 1):

```text
INV-2026-07.md: error: cannot write a UBL (Peppol BIS 3.0) e-invoice, 2 fields are missing or invalid:
  BT-10: Peppol needs a buyer reference: set `buyer_reference:` in the invoice or on the client ...
  BT-48: reverse charge needs the client's VAT ID: set `vat_id:` on the client ...
```

| Term | Needed | Where it comes from |
|---|---|---|
| BT-1 invoice number | always | `number` |
| BT-5 currency | always, ISO 4217 | `currency` |
| BT-9 due date | when something is due (not `status: paid`) | `due` / `due_days` |
| BT-10 buyer reference | UBL (Peppol rule R003) | front matter `buyer_reference`, else the client's `buyer_reference` |
| BT-25 preceding invoice | credit notes | `refers_to` |
| BT-27 / BT-44 seller / buyer name | always | `from.name`, client `name` |
| BT-40 / BT-55 country | always | `country`, else guessed (see below) |
| BT-31 seller VAT ID | categories S, Z, E (or a tax number, BT-32) and AE | `vat_id`, else `tax:` lines |
| BT-30 seller legal ID | when no seller VAT ID is shown | `legal_id` |
| BT-48 buyer VAT ID | reverse charge (AE) | client `vat_id`, else its `tax:` lines |
| BT-34 / BT-49 electronic address | UBL (Peppol rules R010, R020) | `endpoint`, else derived from the VAT ID |
| BT-84 IBAN | payment by transfer | `payment.iban` |

Factur-X needs neither a buyer reference nor electronic addresses; when there is no
`endpoint`, the email is used there with scheme `EM`.

## New optional fields

These live on `from:`, on a client, or inline in `to:`. They are only used for
e-invoices and are never printed on the PDF (keep the human readable version in
`tax:` and `address:`).

| Key | Term | Notes |
|---|---|---|
| `vat_id` | BT-31 / BT-48 | With the country prefix: `DE123456789`, `ESB12345678`. Spaces, dots and dashes are removed. |
| `legal_id` | BT-30 / BT-47 | Company registration number, e.g. `HRB 12345`, `B123456`. |
| `country` | BT-40 / BT-55 | ISO 3166-1 alpha-2 code: `DE`, `LU`. |
| `endpoint` | BT-34 / BT-49 | Peppol participant ID as `<scheme>:<id>` with an EAS scheme code, e.g. `0208:0123456789` (Belgian company number) or `0088:5790000435951` (GLN). |
| `buyer_reference` | BT-10 | Clients only: the reference the client asked you to quote (purchase order, German Leitweg-ID). The front matter key `buyer_reference:` overrides it per invoice. |

## Heuristics

So that an existing config usually works unchanged, the structured data is read
from the free-form lines when the explicit fields are not set:

* **VAT ID**: the first `tax:` line whose value (after `Label:`) is a known VAT prefix
  (EU member states with `EL` for Greece and `XI` for Northern Ireland, plus `GB`,
  `CH`, `NO`) followed by letters and digits with at least two digits:
  `VAT ID: LU12345678`, `USt-IdNr.: DE 123 456 789`. Failing that, the first line
  whose label names VAT in a common language (`vat`, `ust`, `tva`, `iva`, `btw`,
  `mwst`, `moms`, `alv`, `dph`, `dic`, `nip`, `pvm`, `afm`) is used, prefixed with
  the country code when it has none: `IVA: B12345678` in Spain gives `ESB12345678`.
  A plain `Tax ID: B12345678` is **not** taken as a VAT ID: set `vat_id`.
* **Seller tax number** (BT-32): the first other `tax:` line of `from`, as written.
* **Country**: `country`, else the last address line when it is a country name
  (English or native, e.g. `Germany`, `Deutschland`, `España`) or a two-letter code,
  else the prefix of the VAT ID found above.
* **Address**: the first line is always a street line. Among the others,
  `10115 Berlin` or `L-1616 Luxembourg` (a token with digits, then a name) gives
  postcode and city, as does `Madrid 28001`; a line that is only a postcode
  (`28001`) takes the closest line without digits before it (else after it) as the
  city. Everything else stays a street line (BT-35, BT-36, BT-162; extra lines are
  joined into the third).
* **Electronic address**: `endpoint`, else for UBL the VAT ID under its country's
  Peppol VAT scheme (`9930:DE123456789`, `9920:ESB12345678`, `9957:FR...`, `0211`
  for Italy; Nordic countries have no VAT scheme and need `endpoint`), else for
  Factur-X the email address (`EM`).
* **Unit codes** (BT-130, UN/ECE Recommendation 20): days `DAY`, hours `HUR`,
  minutes `MIN`, weeks `WEE`, months `MON`, years `ANN`, pieces `H87`, `km` `KMT`,
  `kg` `KGM`, litres `LTR`, `kWh` `KWH`, pages `QA`, sets `SET`, in the languages
  invgen speaks (`días`, `Stunden`, `jours`...). Anything else, including no unit,
  is `C62` ("one").

## VAT categories

| Line | `tax_note` | Category (BT-118/151) | Exemption reason (BT-120 / BT-121) |
|---|---|---|---|
| rate above 0 | any | `S` standard | none |
| 0% | `reverse_charge` | `AE` reverse charge | translated preset text / `VATEX-EU-AE` |
| 0% | `exempt` | `E` exempt | translated preset text |
| 0% | `outside_scope` | `O` not subject to VAT | translated preset text / `VATEX-EU-O` |
| 0% | free text | `E` exempt | the text |
| 0% | none | `Z` zero rated | none |

Lines at different rates (per-line `Tax` column) give one VAT breakdown row each,
with exactly the bases and taxes printed on the PDF. Category `O` carries no rate,
cannot be combined with taxed lines and leaves out all VAT IDs (EN 16931 rules
BR-O-*), so the seller then needs `legal_id`.

## Mapping

| Term | Meaning | Source |
|---|---|---|
| BT-1 | Invoice number | `number` |
| BT-2 | Issue date | `date` |
| BT-3 | Type code | `380`, or `381` for `kind: credit_note` |
| BT-5 | Currency | `currency` |
| BT-9 | Due date | `due` (UBL credit notes: `PaymentMeans/PaymentDueDate`) |
| BT-10 | Buyer reference | `buyer_reference` |
| BT-22 | Notes | invoice notes (paragraphs) |
| BT-23 | Business process | UBL: `urn:fdc:peppol.eu:2017:poacc:billing:01:1.0` |
| BT-24 | Specification | CII: `urn:cen.eu:en16931:2017`; UBL: Peppol BIS 3.0 |
| BT-25 / BT-26 | Preceding invoice and date | `refers_to`, `refers_to_date` |
| BT-27 / BT-44 | Seller / buyer name (also UBL registration name) | `from.name`, client `name` |
| BT-30 / BT-47 | Legal registration ID | `legal_id` |
| BT-31 / BT-48 | VAT ID | `vat_id` or `tax:` (heuristic) |
| BT-32 | Seller tax registration | other `from.tax` line |
| BT-34 / BT-49 | Electronic address | `endpoint` / VAT ID / email |
| BT-35..BT-40, BT-50..BT-55 | Postal address | `address`, `country` (heuristic) |
| BT-41 | Seller contact point | `from.name` |
| BT-42 / BT-43, BT-57 / BT-58 | Phone, email | `phone`, `email` |
| BT-81 | Payment means | `58` SEPA credit transfer (IBAN, EUR), `30` credit transfer (IBAN, other currency), `1` without IBAN |
| BT-83 | Remittance information | invoice number |
| BT-84 / BT-85 / BT-86 | IBAN, account name, BIC | `payment.iban`, `payment.holder`, `payment.bic` |
| BT-106, BT-109 | Sum of lines, total without VAT | subtotal |
| BT-110 | Total VAT | tax |
| BT-112 | Total with VAT | total |
| BT-113 | Paid amount | the total when `status: paid`, else none |
| BT-115 | Amount due | total minus paid amount |
| BT-116..BT-121 | VAT breakdown | one row per rate (see VAT categories) |
| BT-126 | Line ID | 1, 2, 3... |
| BT-129 / BT-130 | Quantity, unit code | `Qty`, `Unit` (mapped) |
| BT-131 | Line net amount | quantity x rate, rounded to cents |
| BT-146 | Net price | `Rate` (not rounded) |
| BT-151 / BT-152 | Line VAT category and rate | line tax rate |
| BT-153 / BT-154 | Item name and description | description, detail line |

Not produced: allowances and charges, invoice period, delivery information, purchase
order and contract references, attachments other than the XML.

## The Factur-X PDF

Typst does most of the work. The CII XML is attached with `pdf.attach` (file name
`factur-x.xml`, MIME type `text/xml`, `AFRelationship` `Alternative`), appended to
the template's source so custom templates get it too, and the document is exported
as PDF/A-3b: all fonts embedded, an sRGB output intent, the `/AF` array and the PDF/A
identification in the XMP metadata. Typst 0.15 has no API for custom XMP, and
Factur-X requires the `fx:` properties (`DocumentType INVOICE`, `DocumentFileName`,
`Version 1.0`, `ConformanceLevel EN 16931`) declared through a PDF/A extension
schema. invgen therefore patches the finished PDF once (`einvoice::patch_xmp`): it
adds the schema to the existing `pdfaExtension:schemas` bag and the properties as a
new description, then fixes the stream length and the cross-reference offsets. That
is a few dozen lines instead of a PDF library such as lopdf; the PDFs Typst writes
(a single revision, a classic xref table and an uncompressed metadata stream) are
the only input, and anything else is refused with an error.

## Validation status

The golden files in `tests/golden/einvoice/` (the four example invoices plus one
invoice per VAT category: mixed German rates, exempt, outside scope in USD, zero
rated) and the Factur-X PDFs built from the examples were checked with:

* **Mustang CLI 2.26.0** (`--action validate`): PDF/A-3b by veraPDF, Factur-X XMP,
  CII XSD and the Factur-X EN 16931 schematron for the PDFs and the CII files; the
  UBL files with its EN 16931 UBL checks. All valid. The only message is a
  warning (PEPPOL-EN16931-R008, empty element) for `ApplicableHeaderTradeDelivery`,
  which the CII schema requires but which invgen has nothing to put in.
* **CEN EN 16931 UBL schematron and Peppol BIS Billing 3.0 schematron** (from
  OpenPEPPOL/peppol-bis-invoice-3, compiled with SchXslt and run with Saxon HE
  12.5): no failed assertions, including the German national rules for the
  domestic German case.

Not validated: XRechnung (CIUS rules beyond EN 16931, such as mandatory seller
contact phone and email and the Leitweg-ID), Chorus Pro specifics, actual delivery
through a Peppol access point, and the other Factur-X profiles (MINIMUM, BASIC,
EXTENDED). `cargo test` compares against the golden files and checks the PDF
structure; it does not run the Java validators. To regenerate the golden files
after an intended change: `INVGEN_UPDATE_GOLDEN=1 cargo test einvoice`, then
review the diff and revalidate.
