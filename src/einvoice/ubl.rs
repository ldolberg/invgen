//! UBL 2.1 Invoice / CreditNote following Peppol BIS Billing 3.0. Element
//! order follows the UBL XSD sequences; do not reorder.

use super::document::{Document, DocumentKind, Party, VatCategory, amount, decimal};
use super::xml::Xml;

/// Specification identifier (BT-24).
pub const CUSTOMIZATION: &str =
    "urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0";
/// Business process (BT-23).
pub const PROFILE: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";

/// Element names that differ between Invoice and CreditNote.
struct Names {
    root: &'static str,
    ns: &'static str,
    type_code: &'static str,
    line: &'static str,
    quantity: &'static str,
}

fn names(kind: &DocumentKind) -> Names {
    match kind {
        DocumentKind::Invoice => Names {
            root: "Invoice",
            ns: "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
            type_code: "cbc:InvoiceTypeCode",
            line: "cac:InvoiceLine",
            quantity: "cbc:InvoicedQuantity",
        },
        DocumentKind::CreditNote { .. } => Names {
            root: "CreditNote",
            ns: "urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2",
            type_code: "cbc:CreditNoteTypeCode",
            line: "cac:CreditNoteLine",
            quantity: "cbc:CreditedQuantity",
        },
    }
}

fn money(x: &mut Xml, tag: &str, currency: &str, value: rust_decimal::Decimal) {
    x.leaf_attrs(tag, &[("currencyID", currency)], &amount(value));
}

fn tax_scheme(x: &mut Xml, id: &str) {
    x.open("cac:TaxScheme", &[]);
    x.leaf("cbc:ID", id);
    x.close();
}

fn party(x: &mut Xml, tag: &'static str, p: &Party, seller: bool, vat_ids: bool) {
    x.open(tag, &[]);
    x.open("cac:Party", &[]);
    if let Some(e) = &p.endpoint {
        x.leaf_attrs("cbc:EndpointID", &[("schemeID", &e.scheme)], &e.id);
    }
    x.open("cac:PartyName", &[]);
    x.leaf("cbc:Name", &p.name);
    x.close();
    let a = &p.address;
    x.open("cac:PostalAddress", &[]);
    let mut lines = a.lines.iter();
    x.opt("cbc:StreetName", lines.next().map(String::as_str));
    x.opt("cbc:AdditionalStreetName", lines.next().map(String::as_str));
    x.opt("cbc:CityName", a.city.as_deref());
    x.opt("cbc:PostalZone", a.postcode.as_deref());
    let rest: Vec<&str> = lines.map(String::as_str).collect();
    if !rest.is_empty() {
        x.open("cac:AddressLine", &[]);
        x.leaf("cbc:Line", &rest.join(", "));
        x.close();
    }
    if let Some(c) = &a.country {
        x.open("cac:Country", &[]);
        x.leaf("cbc:IdentificationCode", c);
        x.close();
    }
    x.close();
    if vat_ids && let Some(vat) = &p.vat_id {
        x.open("cac:PartyTaxScheme", &[]);
        x.leaf("cbc:CompanyID", vat);
        tax_scheme(x, "VAT");
        x.close();
    }
    if seller && let Some(fc) = &p.tax_registration {
        x.open("cac:PartyTaxScheme", &[]);
        x.leaf("cbc:CompanyID", fc);
        tax_scheme(x, "TAX");
        x.close();
    }
    x.open("cac:PartyLegalEntity", &[]);
    x.leaf("cbc:RegistrationName", &p.name);
    x.opt("cbc:CompanyID", p.legal_id.as_deref());
    x.close();
    // The seller's contact point (BT-41) is the seller's name: German
    // domestic invoices require it (DE-R-005).
    if seller || p.phone.is_some() || p.email.is_some() {
        x.open("cac:Contact", &[]);
        if seller {
            x.leaf("cbc:Name", &p.name);
        }
        x.opt("cbc:Telephone", p.phone.as_deref());
        x.opt("cbc:ElectronicMail", p.email.as_deref());
        x.close();
    }
    x.close();
    x.close();
}

fn tax_category(
    x: &mut Xml,
    tag: &'static str,
    category: VatCategory,
    rate: rust_decimal::Decimal,
    exemption: Option<(Option<&str>, Option<&str>)>,
) {
    x.open(tag, &[]);
    x.leaf("cbc:ID", category.code());
    if category.has_rate() {
        x.leaf("cbc:Percent", &decimal(rate));
    }
    if let Some((code, reason)) = exemption {
        x.opt("cbc:TaxExemptionReasonCode", code);
        x.opt("cbc:TaxExemptionReason", reason);
    }
    tax_scheme(x, "VAT");
    x.close();
}

/// The UBL XML for `doc` (already validated).
pub fn write(doc: &Document) -> String {
    let n = names(&doc.kind);
    let cur = doc.currency.as_str();
    let mut x = Xml::new();
    x.open(
        n.root,
        &[
            ("xmlns", n.ns),
            (
                "xmlns:cac",
                "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
            ),
            (
                "xmlns:cbc",
                "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
            ),
        ],
    );
    x.leaf("cbc:CustomizationID", CUSTOMIZATION);
    x.leaf("cbc:ProfileID", PROFILE);
    x.leaf("cbc:ID", &doc.number);
    x.leaf(
        "cbc:IssueDate",
        &doc.issue_date.format("%Y-%m-%d").to_string(),
    );
    let due = doc.due_date.map(|d| d.format("%Y-%m-%d").to_string());
    if !doc.kind.is_credit_note() {
        x.opt("cbc:DueDate", due.as_deref());
    }
    x.leaf(n.type_code, doc.kind.type_code());
    for note in &doc.notes {
        x.leaf("cbc:Note", note);
    }
    x.leaf("cbc:DocumentCurrencyCode", cur);
    x.opt("cbc:BuyerReference", doc.buyer_reference.as_deref());
    if let Some(prev) = doc.kind.preceding_invoice() {
        x.open("cac:BillingReference", &[]);
        x.open("cac:InvoiceDocumentReference", &[]);
        x.leaf("cbc:ID", prev);
        if let Some(d) = doc.kind.preceding_invoice_date() {
            x.leaf("cbc:IssueDate", &d.format("%Y-%m-%d").to_string());
        }
        x.close();
        x.close();
    }

    let vat_ids = !doc.omit_vat_ids();
    party(
        &mut x,
        "cac:AccountingSupplierParty",
        &doc.seller,
        true,
        vat_ids,
    );
    party(
        &mut x,
        "cac:AccountingCustomerParty",
        &doc.buyer,
        false,
        vat_ids,
    );

    let p = &doc.payment;
    x.open("cac:PaymentMeans", &[]);
    x.leaf("cbc:PaymentMeansCode", p.code);
    if doc.kind.is_credit_note() {
        // A credit note has no DueDate element; BT-9 goes here.
        x.opt("cbc:PaymentDueDate", due.as_deref());
    }
    x.leaf("cbc:PaymentID", &p.remittance);
    if let Some(iban) = &p.iban {
        x.open("cac:PayeeFinancialAccount", &[]);
        x.leaf("cbc:ID", iban);
        x.opt("cbc:Name", p.holder.as_deref());
        if let Some(bic) = &p.bic {
            x.open("cac:FinancialInstitutionBranch", &[]);
            x.leaf("cbc:ID", bic);
            x.close();
        }
        x.close();
    }
    x.close();

    x.open("cac:TaxTotal", &[]);
    money(&mut x, "cbc:TaxAmount", cur, doc.totals.tax);
    for v in &doc.vat {
        x.open("cac:TaxSubtotal", &[]);
        money(&mut x, "cbc:TaxableAmount", cur, v.basis);
        money(&mut x, "cbc:TaxAmount", cur, v.tax);
        tax_category(
            &mut x,
            "cac:TaxCategory",
            v.category,
            v.rate,
            Some((v.exemption_code, v.exemption_reason.as_deref())),
        );
        x.close();
    }
    x.close();

    let t = &doc.totals;
    x.open("cac:LegalMonetaryTotal", &[]);
    money(&mut x, "cbc:LineExtensionAmount", cur, t.line_total);
    money(&mut x, "cbc:TaxExclusiveAmount", cur, t.tax_exclusive);
    money(&mut x, "cbc:TaxInclusiveAmount", cur, t.tax_inclusive);
    if !t.prepaid.is_zero() {
        money(&mut x, "cbc:PrepaidAmount", cur, t.prepaid);
    }
    money(&mut x, "cbc:PayableAmount", cur, t.payable);
    x.close();

    for l in &doc.lines {
        x.open(n.line, &[]);
        x.leaf("cbc:ID", &l.id);
        x.leaf_attrs(
            n.quantity,
            &[("unitCode", l.unit_code)],
            &decimal(l.quantity),
        );
        money(&mut x, "cbc:LineExtensionAmount", cur, l.net);
        x.open("cac:Item", &[]);
        x.opt("cbc:Description", l.description.as_deref());
        x.leaf("cbc:Name", &l.name);
        tax_category(
            &mut x,
            "cac:ClassifiedTaxCategory",
            l.category,
            l.rate,
            None,
        );
        x.close();
        x.open("cac:Price", &[]);
        money_raw(&mut x, "cbc:PriceAmount", cur, l.price);
        x.close();
        x.close();
    }

    x.close();
    x.finish()
}

/// A price: not rounded to cents.
fn money_raw(x: &mut Xml, tag: &str, currency: &str, value: rust_decimal::Decimal) {
    x.leaf_attrs(tag, &[("currencyID", currency)], &decimal(value));
}
