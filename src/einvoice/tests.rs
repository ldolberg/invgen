//! E-invoice tests: golden XML for the examples and for one invoice per VAT
//! category, validation messages, and the Factur-X PDF structure.
//!
//! The golden files live in `tests/golden/einvoice/`. After an intended
//! change, regenerate them with
//! `INVGEN_UPDATE_GOLDEN=1 cargo test einvoice` and review the diff. They
//! were checked with Mustang (Factur-X / EN 16931 schematron, veraPDF) and
//! the official CEN and Peppol BIS 3.0 UBL schematron; see
//! docs/E-INVOICING.md.

use std::path::PathBuf;

use super::*;
use crate::config::Config;
use crate::parse::parse_invoice;
use crate::render::Renderer;

fn config() -> Config {
    let mut cfg: Config = serde_yaml_ng::from_str(include_str!("../../examples/config.yaml"))
        .expect("examples/config.yaml parses");
    cfg.invoices_dir = std::env::temp_dir().join("invgen-einvoice-test-nonexistent-dir");
    cfg.dir = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples"));
    cfg
}

fn invoice(md: &str) -> Invoice {
    parse_invoice(md, &config()).expect("test invoice parses")
}

fn example(name: &str) -> Invoice {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/invoices")
        .join(format!("{name}.md"));
    invoice(&std::fs::read_to_string(path).unwrap())
}

/// A German seller billing a German client at two rates.
const MIXED_RATES: &str = "---
number: EINV-2026-30
date: 2026-11-02
due: 2026-11-30
tax_rate: 19
tax_note: false
buyer_reference: 04011000-12345-34
from:
  name: Alex Morgan
  address: [Hauptstr. 5, 10115 Berlin, Germany]
  tax: [\"USt-IdNr.: DE 123 456 789\", \"Steuernummer: 12/345/67890\"]
  email: alex@example.com
  legal_id: HRB 12345
to:
  name: Contoso GmbH
  address: [Beispielweg 1, Gebaeude B, 80331 Muenchen, Deutschland]
  tax: [\"VAT: DE987654321\"]
  email: billing@example.com
payment:
  holder: ALEX MORGAN
  iban: DE89 3704 0044 0532 0130 00
  bic: COBADEFFXXX
---

| Description | Qty | Unit | Rate | VAT |
|---|---:|---|---:|---:|
| Consulting | 10.5 | hours | 95.00 | 19% |
| Book & guide <print> | 2 | pcs | 24.99 | 7% |
";

/// Exempt (E) with the preset reason, to a client without VAT ID.
const EXEMPT: &str = "---
number: EINV-2026-31
date: 2026-11-02
tax_rate: 0
tax_note: exempt
to:
  name: City Library
  address: [1 Main Street, L-1616 Luxembourg]
  country: LU
  endpoint: \"0088:5790000435951\"
buyer_reference: LIB-7
---

- Workshop: 1 @ 600
";

/// Outside the scope of VAT (O): no VAT IDs, the seller needs a legal ID.
const OUTSIDE_SCOPE: &str = "---
number: EINV-2026-32
date: 2026-11-02
currency: USD
tax_rate: 0
tax_note: outside_scope
from:
  legal_id: B000000
to:
  name: Fabrikam Inc.
  address: [500 Market St, San Francisco CA 94105, USA]
  email: ap@example.com
  endpoint: \"0060:123456789\"
buyer_reference: PO-99
---

- Remote advisory: 3 days @ 450
";

/// Zero rated (Z): rate 0 and no tax note.
const ZERO_RATED: &str = "---
number: EINV-2026-33
date: 2026-11-02
tax_rate: 0
tax_note: false
client: northwind
---

- Printed manuals: 100 pcs @ 1.255
";

fn cases() -> Vec<(&'static str, Invoice)> {
    vec![
        ("EINV-2026-20", example("EINV-2026-20")),
        ("EINV-2026-21", example("EINV-2026-21")),
        ("EINV-2026-22", example("EINV-2026-22")),
        ("CN-2026-01", example("CN-2026-01")),
        ("mixed-rates", invoice(MIXED_RATES)),
        ("exempt", invoice(EXEMPT)),
        ("outside-scope", invoice(OUTSIDE_SCOPE)),
        ("zero-rated", invoice(ZERO_RATED)),
    ]
}

fn golden(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/einvoice")
        .join(name);
    if std::env::var_os("INVGEN_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {}; run INVGEN_UPDATE_GOLDEN=1 cargo test einvoice",
            path.display()
        )
    });
    assert!(
        expected == actual,
        "{} differs from the generated XML; run INVGEN_UPDATE_GOLDEN=1 cargo test \
         einvoice and review the diff\n--- generated ---\n{actual}",
        path.display()
    );
}

#[test]
fn golden_cii_and_ubl() {
    for (name, inv) in cases() {
        let cii = cii_xml(&inv).unwrap_or_else(|e| panic!("{name}: {e}"));
        golden(&format!("{name}.cii.xml"), &cii);
        let ubl = ubl_xml(&inv).unwrap_or_else(|e| panic!("{name}: {e}"));
        golden(&format!("{name}.ubl.xml"), &ubl);
    }
}

#[test]
fn categories_follow_rate_and_tax_note() {
    let cat = |md: &str| Document::from_invoice(&invoice(md)).categories();
    assert_eq!(
        Document::from_invoice(&example("EINV-2026-21")).categories(),
        [VatCategory::ReverseCharge]
    );
    assert_eq!(
        Document::from_invoice(&example("EINV-2026-22")).categories(),
        [VatCategory::Standard]
    );
    assert_eq!(cat(EXEMPT), [VatCategory::Exempt]);
    assert_eq!(cat(OUTSIDE_SCOPE), [VatCategory::OutsideScope]);
    assert_eq!(cat(ZERO_RATED), [VatCategory::ZeroRated]);
    // Free text at 0% is an exemption with that text as the reason.
    let free = ZERO_RATED.replace("tax_note: false", "tax_note: Art. 44 VAT Directive");
    let doc = Document::from_invoice(&invoice(&free));
    assert_eq!(doc.categories(), [VatCategory::Exempt]);
    assert_eq!(
        doc.vat[0].exemption_reason.as_deref(),
        Some("Art. 44 VAT Directive")
    );
}

#[test]
fn mapping_of_the_reverse_charge_example() {
    let doc = Document::from_invoice(&example("EINV-2026-21"));
    assert_eq!(doc.kind.type_code(), "380");
    assert_eq!(doc.seller.vat_id.as_deref(), Some("LU12345678"));
    assert_eq!(doc.seller.address.country.as_deref(), Some("LU"));
    assert_eq!(doc.seller.address.city.as_deref(), Some("Luxembourg"));
    assert_eq!(doc.seller.address.postcode.as_deref(), Some("1616"));
    assert_eq!(doc.buyer.vat_id.as_deref(), Some("ESB12345678"));
    assert_eq!(doc.buyer.address.country.as_deref(), Some("ES"));
    let ep = doc.buyer.endpoint.as_ref().unwrap();
    assert_eq!(
        (ep.scheme.as_str(), ep.id.as_str()),
        ("9920", "ESB12345678")
    );
    assert_eq!(doc.payment.code, "58");
    assert_eq!(doc.payment.iban.as_deref(), Some("DE89370400440532013000"));
    assert_eq!(doc.lines[0].unit_code, "DAY");
    assert_eq!(doc.vat[0].exemption_code, Some("VATEX-EU-AE"));
    assert_eq!(doc.totals.payable, doc.totals.tax_inclusive);
}

#[test]
fn credit_note_maps_to_381_with_the_invoice_it_corrects() {
    let doc = Document::from_invoice(&example("CN-2026-01"));
    assert_eq!(doc.kind.type_code(), "381");
    assert_eq!(doc.kind.preceding_invoice(), Some("EINV-2026-21"));
    assert_eq!(
        doc.kind.preceding_invoice_date(),
        chrono::NaiveDate::from_ymd_opt(2026, 8, 1)
    );
    // Amounts stay positive in a credit note.
    assert!(doc.totals.tax_inclusive > crate::model::Decimal::ZERO);
    let ubl = ubl_xml(&example("CN-2026-01")).unwrap();
    assert!(ubl.contains(
        "<CreditNote xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2\""
    ));
    assert!(ubl.contains("<cbc:CreditNoteTypeCode>381</cbc:CreditNoteTypeCode>"));
    assert!(ubl.contains("<cbc:CreditedQuantity unitCode=\"DAY\">2</cbc:CreditedQuantity>"));
}

#[test]
fn paid_invoice_has_nothing_due() {
    let doc = Document::from_invoice(&example("EINV-2026-22"));
    assert_eq!(doc.totals.prepaid, doc.totals.tax_inclusive);
    assert!(doc.totals.payable.is_zero());
}

#[test]
fn payment_means_depend_on_iban_and_currency() {
    let mut inv = example("EINV-2026-21");
    assert_eq!(Document::from_invoice(&inv).payment.code, "58");
    inv.currency = "USD".into();
    assert_eq!(Document::from_invoice(&inv).payment.code, "30");
    inv.payment.iban = None;
    assert_eq!(Document::from_invoice(&inv).payment.code, "1");
}

#[test]
fn validation_lists_every_missing_field() {
    let md = "---
number: X-1
date: 2026-11-02
due: false
currency: euro
tax_rate: 0
tax_note: reverse_charge
from:
  name: Alex Morgan
  address: [Somewhere]
  tax: []
  email: null
to:
  name: \"\"
  address: []
  tax: []
---

- Work: 1 @ 100
";
    let mut cfg = config();
    cfg.clients.clear();
    let inv = parse_invoice(md, &cfg).unwrap();
    let err = ubl_xml(&inv).unwrap_err();
    let terms: Vec<&str> = err.issues.iter().map(|i| i.term).collect();
    for t in [
        "BT-5", "BT-9", "BT-10", "BT-40", "BT-44", "BT-55", "BT-48", "BT-31", "BT-30", "BT-34",
        "BT-49",
    ] {
        assert!(terms.contains(&t), "{t} missing from {terms:?}");
    }
    let text = err.to_string();
    assert!(text.starts_with("cannot write a UBL (Peppol BIS 3.0) e-invoice"));
    assert!(text.contains("BT-55: buyer country is unknown"));
    assert!(text.contains("`buyer_reference:`"));

    // Factur-X does not need the Peppol-only terms.
    let terms: Vec<&str> = cii_xml(&inv)
        .unwrap_err()
        .issues
        .iter()
        .map(|i| i.term)
        .collect();
    assert!(!terms.contains(&"BT-10") && !terms.contains(&"BT-34"));
    assert!(terms.contains(&"BT-48"));
}

#[test]
fn outside_scope_cannot_mix_with_taxed_lines() {
    let md = OUTSIDE_SCOPE.replace(
        "- Remote advisory: 3 days @ 450",
        "- Remote advisory: 3 days @ 450\n- Local work: 1 day @ 450, VAT 17%",
    );
    let err = cii_xml(&invoice(&md)).unwrap_err();
    assert!(err.issues.iter().any(|i| i.term == "BT-151"), "{err}");
}

#[test]
fn bad_endpoint_and_country_are_reported() {
    let md = EXEMPT
        .replace("country: LU", "country: Luxembourg")
        .replace("\"0088:5790000435951\"", "nonsense");
    let err = ubl_xml(&invoice(&md)).unwrap_err();
    let text = err.to_string();
    assert!(text.contains("BT-55: buyer country `LUXEMBOURG`"), "{text}");
    assert!(text.contains("BT-49: buyer `endpoint: nonsense`"), "{text}");
}

#[test]
fn format_config_values() {
    let parse = |y: &str| -> Result<Config, _> { serde_yaml_ng::from_str::<Config>(y) };
    assert_eq!(
        parse("e_invoice: facturx").unwrap().e_invoice,
        [Format::FacturX]
    );
    assert_eq!(
        parse("e_invoice: [ubl, Factur-X, ubl]").unwrap().e_invoice,
        [Format::FacturX, Format::Ubl]
    );
    assert!(parse("e_invoice: none").unwrap().e_invoice.is_empty());
    assert!(parse("e_invoice: false").unwrap().e_invoice.is_empty());
    assert!(parse("{}").unwrap().e_invoice.is_empty());
    assert!(parse("e_invoice: pdf").is_err());
    // Round trip: a single format is written as a plain string.
    let cfg = parse("e_invoice: ubl").unwrap();
    assert!(
        serde_yaml_ng::to_string(&cfg)
            .unwrap()
            .contains("e_invoice: ubl")
    );
}

fn count(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

#[test]
fn facturx_pdf_is_pdfa3_with_the_xml_attached() {
    let inv = example("EINV-2026-21");
    let pdf = facturx_pdf(&Renderer::new().unwrap(), &inv).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.7"));
    assert!(xref_is_consistent(&pdf), "xref offsets are broken");
    // PDF/A-3b identification and the Factur-X XMP properties.
    assert_eq!(count(&pdf, b"<pdfaid:part>3</pdfaid:part>"), 1);
    assert_eq!(
        count(&pdf, b"<pdfaid:conformance>B</pdfaid:conformance>"),
        1
    );
    assert_eq!(
        count(&pdf, b"<fx:DocumentType>INVOICE</fx:DocumentType>"),
        1
    );
    assert_eq!(
        count(
            &pdf,
            b"<fx:DocumentFileName>factur-x.xml</fx:DocumentFileName>"
        ),
        1
    );
    assert_eq!(
        count(&pdf, b"<fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>"),
        1
    );
    // One extension schema bag, holding the Factur-X schema.
    assert_eq!(count(&pdf, b"<pdfaExtension:schemas>"), 1);
    assert_eq!(
        count(
            &pdf,
            b"urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#</pdfaSchema:namespaceURI>"
        ),
        1
    );
    // The associated file.
    assert_eq!(count(&pdf, b"/UF(factur-x.xml)"), 1);
    assert_eq!(count(&pdf, b"/AFRelationship/Alternative"), 1);
    assert_eq!(count(&pdf, b"/Subtype/text#2Fxml"), 1);
    assert!(count(&pdf, b"/AF[") >= 1);
    assert_eq!(count(&pdf, b"/EmbeddedFiles"), 1);
}

#[test]
fn facturx_refuses_an_incomplete_invoice() {
    let mut inv = example("EINV-2026-21");
    inv.to.vat_id = None;
    let err = facturx_pdf(&Renderer::new().unwrap(), &inv).unwrap_err();
    assert!(format!("{err:#}").contains("BT-48"), "{err:#}");
}

#[test]
fn facturx_works_with_a_custom_template() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("mine.typ");
    std::fs::write(
        &tpl,
        "#let d = json(\"/invoice.json\")\n#set page(paper: \"a5\")\n#set text(font: \"Inter\")\n= #d.number\n",
    )
    .unwrap();
    let mut inv = example("EINV-2026-21");
    inv.template = Some(tpl);
    let pdf = facturx_pdf(&Renderer::new().unwrap(), &inv).unwrap();
    assert!(xref_is_consistent(&pdf));
    assert_eq!(count(&pdf, b"/UF(factur-x.xml)"), 1);
    // The attachment adds no page.
    let compiled = Renderer::new().unwrap().compile(&inv).unwrap();
    assert_eq!(compiled.page_count(), 1);
}

#[test]
fn patch_xmp_keeps_xref_offsets_valid() {
    let inv = example("EINV-2026-22");
    let compiled = Renderer::new().unwrap().compile(&inv).unwrap();
    let pdf = compiled.pdf_a3b(inv.date).unwrap();
    assert!(xref_is_consistent(&pdf));
    let patched = patch_xmp(
        &pdf,
        |p| Ok(p.replace("</rdf:RDF>", "<!-- x --></rdf:RDF>")),
    )
    .unwrap();
    assert!(xref_is_consistent(&patched));
    assert_eq!(patched.len(), pdf.len() + "<!-- x -->".len());
    // Shrinking works too.
    let shrunk = patch_xmp(&patched, |p| Ok(p.replace("<!-- x -->", ""))).unwrap();
    assert_eq!(shrunk, pdf);
    // A PDF without XMP is refused, not corrupted.
    assert!(patch_xmp(b"%PDF-1.7\n%%EOF", |p| Ok(p.to_string())).is_err());
}
