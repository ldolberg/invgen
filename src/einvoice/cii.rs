//! UN/CEFACT Cross Industry Invoice (D16B), as embedded in Factur-X /
//! ZUGFeRD PDFs, profile EN 16931 ("COMFORT"). Element order follows the
//! CII XSD sequence; do not reorder.

use chrono::NaiveDate;

use super::document::{Document, Party, amount, decimal};
use super::xml::Xml;

/// Guideline (BT-24) of the Factur-X / ZUGFeRD EN 16931 profile.
pub const GUIDELINE: &str = "urn:cen.eu:en16931:2017";

fn date(x: &mut Xml, tag: &'static str, d: NaiveDate) {
    x.open(tag, &[]);
    x.leaf_attrs(
        "udt:DateTimeString",
        &[("format", "102")],
        &d.format("%Y%m%d").to_string(),
    );
    x.close();
}

fn party(x: &mut Xml, tag: &'static str, p: &Party, seller: bool, vat_ids: bool) {
    x.open(tag, &[]);
    x.leaf("ram:Name", &p.name);
    if let Some(id) = &p.legal_id {
        x.open("ram:SpecifiedLegalOrganization", &[]);
        x.leaf("ram:ID", id);
        x.close();
    }
    // The seller's contact point (BT-41) is the seller's name: German
    // domestic invoices require it (DE-R-005).
    if seller || p.phone.is_some() || p.email.is_some() {
        x.open("ram:DefinedTradeContact", &[]);
        if seller {
            x.leaf("ram:PersonName", &p.name);
        }
        if let Some(phone) = &p.phone {
            x.open("ram:TelephoneUniversalCommunication", &[]);
            x.leaf("ram:CompleteNumber", phone);
            x.close();
        }
        if let Some(email) = &p.email {
            x.open("ram:EmailURIUniversalCommunication", &[]);
            x.leaf("ram:URIID", email);
            x.close();
        }
        x.close();
    }
    let a = &p.address;
    x.open("ram:PostalTradeAddress", &[]);
    x.opt("ram:PostcodeCode", a.postcode.as_deref());
    let mut lines = a.lines.iter();
    x.opt("ram:LineOne", lines.next().map(String::as_str));
    x.opt("ram:LineTwo", lines.next().map(String::as_str));
    let rest: Vec<&str> = lines.map(String::as_str).collect();
    if !rest.is_empty() {
        x.leaf("ram:LineThree", &rest.join(", "));
    }
    x.opt("ram:CityName", a.city.as_deref());
    x.opt("ram:CountryID", a.country.as_deref());
    x.close();
    if let Some(e) = p.endpoint.as_ref().or(p.email_endpoint.as_ref()) {
        x.open("ram:URIUniversalCommunication", &[]);
        x.leaf_attrs("ram:URIID", &[("schemeID", &e.scheme)], &e.id);
        x.close();
    }
    if vat_ids && let Some(vat) = &p.vat_id {
        x.open("ram:SpecifiedTaxRegistration", &[]);
        x.leaf_attrs("ram:ID", &[("schemeID", "VA")], vat);
        x.close();
    }
    if seller && let Some(fc) = &p.tax_registration {
        x.open("ram:SpecifiedTaxRegistration", &[]);
        x.leaf_attrs("ram:ID", &[("schemeID", "FC")], fc);
        x.close();
    }
    x.close();
}

/// The CII XML for `doc` (already validated).
pub fn write(doc: &Document) -> String {
    let mut x = Xml::new();
    x.open(
        "rsm:CrossIndustryInvoice",
        &[
            (
                "xmlns:rsm",
                "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
            ),
            (
                "xmlns:qdt",
                "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
            ),
            (
                "xmlns:ram",
                "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
            ),
            ("xmlns:xs", "http://www.w3.org/2001/XMLSchema"),
            (
                "xmlns:udt",
                "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
            ),
        ],
    );

    x.open("rsm:ExchangedDocumentContext", &[]);
    x.open("ram:GuidelineSpecifiedDocumentContextParameter", &[]);
    x.leaf("ram:ID", GUIDELINE);
    x.close();
    x.close();

    x.open("rsm:ExchangedDocument", &[]);
    x.leaf("ram:ID", &doc.number);
    x.leaf("ram:TypeCode", doc.kind.type_code());
    date(&mut x, "ram:IssueDateTime", doc.issue_date);
    for n in &doc.notes {
        x.open("ram:IncludedNote", &[]);
        x.leaf("ram:Content", n);
        x.close();
    }
    x.close();

    x.open("rsm:SupplyChainTradeTransaction", &[]);
    for l in &doc.lines {
        x.open("ram:IncludedSupplyChainTradeLineItem", &[]);
        x.open("ram:AssociatedDocumentLineDocument", &[]);
        x.leaf("ram:LineID", &l.id);
        x.close();
        x.open("ram:SpecifiedTradeProduct", &[]);
        x.leaf("ram:Name", &l.name);
        x.opt("ram:Description", l.description.as_deref());
        x.close();
        x.open("ram:SpecifiedLineTradeAgreement", &[]);
        x.open("ram:NetPriceProductTradePrice", &[]);
        x.leaf("ram:ChargeAmount", &decimal(l.price));
        x.close();
        x.close();
        x.open("ram:SpecifiedLineTradeDelivery", &[]);
        x.leaf_attrs(
            "ram:BilledQuantity",
            &[("unitCode", l.unit_code)],
            &decimal(l.quantity),
        );
        x.close();
        x.open("ram:SpecifiedLineTradeSettlement", &[]);
        x.open("ram:ApplicableTradeTax", &[]);
        x.leaf("ram:TypeCode", "VAT");
        x.leaf("ram:CategoryCode", l.category.code());
        if l.category.has_rate() {
            x.leaf("ram:RateApplicablePercent", &decimal(l.rate));
        }
        x.close();
        x.open("ram:SpecifiedTradeSettlementLineMonetarySummation", &[]);
        x.leaf("ram:LineTotalAmount", &amount(l.net));
        x.close();
        x.close();
        x.close();
    }

    let vat_ids = !doc.omit_vat_ids();
    x.open("ram:ApplicableHeaderTradeAgreement", &[]);
    x.opt("ram:BuyerReference", doc.buyer_reference.as_deref());
    party(&mut x, "ram:SellerTradeParty", &doc.seller, true, vat_ids);
    party(&mut x, "ram:BuyerTradeParty", &doc.buyer, false, vat_ids);
    x.close();

    x.open("ram:ApplicableHeaderTradeDelivery", &[]);
    x.close();

    x.open("ram:ApplicableHeaderTradeSettlement", &[]);
    x.leaf("ram:PaymentReference", &doc.payment.remittance);
    x.leaf("ram:InvoiceCurrencyCode", &doc.currency);
    let p = &doc.payment;
    x.open("ram:SpecifiedTradeSettlementPaymentMeans", &[]);
    x.leaf("ram:TypeCode", p.code);
    if let Some(iban) = &p.iban {
        x.open("ram:PayeePartyCreditorFinancialAccount", &[]);
        x.leaf("ram:IBANID", iban);
        x.opt("ram:AccountName", p.holder.as_deref());
        x.close();
        if let Some(bic) = &p.bic {
            x.open("ram:PayeeSpecifiedCreditorFinancialInstitution", &[]);
            x.leaf("ram:BICID", bic);
            x.close();
        }
    }
    x.close();
    for v in &doc.vat {
        x.open("ram:ApplicableTradeTax", &[]);
        x.leaf("ram:CalculatedAmount", &amount(v.tax));
        x.leaf("ram:TypeCode", "VAT");
        x.opt("ram:ExemptionReason", v.exemption_reason.as_deref());
        x.leaf("ram:BasisAmount", &amount(v.basis));
        x.leaf("ram:CategoryCode", v.category.code());
        x.opt("ram:ExemptionReasonCode", v.exemption_code);
        if v.category.has_rate() {
            x.leaf("ram:RateApplicablePercent", &decimal(v.rate));
        }
        x.close();
    }
    if let Some(due) = doc.due_date {
        x.open("ram:SpecifiedTradePaymentTerms", &[]);
        date(&mut x, "ram:DueDateDateTime", due);
        x.close();
    }
    let t = &doc.totals;
    x.open("ram:SpecifiedTradeSettlementHeaderMonetarySummation", &[]);
    x.leaf("ram:LineTotalAmount", &amount(t.line_total));
    x.leaf("ram:TaxBasisTotalAmount", &amount(t.tax_exclusive));
    x.leaf_attrs(
        "ram:TaxTotalAmount",
        &[("currencyID", &doc.currency)],
        &amount(t.tax),
    );
    x.leaf("ram:GrandTotalAmount", &amount(t.tax_inclusive));
    if !t.prepaid.is_zero() {
        x.leaf("ram:TotalPrepaidAmount", &amount(t.prepaid));
    }
    x.leaf("ram:DuePayableAmount", &amount(t.payable));
    x.close();
    if let Some(prev) = doc.kind.preceding_invoice() {
        x.open("ram:InvoiceReferencedDocument", &[]);
        x.leaf("ram:IssuerAssignedID", prev);
        if let Some(d) = doc.kind.preceding_invoice_date() {
            x.open("ram:FormattedIssueDateTime", &[]);
            x.leaf_attrs(
                "qdt:DateTimeString",
                &[("format", "102")],
                &d.format("%Y%m%d").to_string(),
            );
            x.close();
        }
        x.close();
    }
    x.close();

    x.close();
    x.close();
    x.finish()
}
