//! Translations of every label the invoice template prints.
//!
//! The template never hardcodes text: it reads `labels.<key>` from the data
//! (see docs/TEMPLATES.md). Each language table lists the same keys as
//! `EN`; a key missing from a table falls back to English. The config can
//! override single keys with `labels:`.

use std::collections::BTreeMap;

/// Language codes with a built-in translation table.
pub const SUPPORTED: &[&str] = &["en", "es", "de", "fr", "it", "pt", "nl"];

/// The reference table. Its keys are the full set of built-in labels.
const EN: &[(&str, &str)] = &[
    ("invoice", "Invoice"),
    ("status", "Status"),
    ("invoice_date", "Invoice date"),
    ("due_date", "Due date"),
    ("bill_to", "Bill to"),
    ("bill_from", "Bill from"),
    ("total_amount", "Total amount"),
    ("tax_details", "Tax details"),
    ("payment_method", "Payment method"),
    ("iban", "IBAN"),
    ("bic", "BIC"),
    ("scan_to_pay", "Scan to pay"),
    ("description", "Description"),
    ("qty_unit", "Qty (unit)"),
    ("rate", "Rate"),
    ("total", "Total"),
    ("subtotal", "Subtotal"),
    ("tax", "VAT"),
    ("tax_base", "Taxable amount"),
    ("total_due", "Total due"),
    ("notes", "Notes"),
    ("contact", "Contact"),
    ("page_of", "Page {page} of {total}"),
    ("status_pending", "Pending"),
    ("status_paid", "Paid"),
    ("status_overdue", "Overdue"),
    ("status_draft", "Draft"),
    (
        "tax_note_reverse_charge",
        "Reverse charge: VAT to be accounted for by the recipient (Article 196 of Council Directive 2006/112/EC).",
    ),
    ("tax_note_exempt", "Exempt from VAT."),
    ("tax_note_outside_scope", "Outside the scope of VAT."),
    ("credit_note", "Credit note"),
    ("refers_to", "Refers to invoice {number} of {date}"),
    ("refers_to_undated", "Refers to invoice {number}"),
    ("total_credited", "Total credited"),
];

const ES: &[(&str, &str)] = &[
    ("invoice", "Factura"),
    ("status", "Estado"),
    ("invoice_date", "Fecha de factura"),
    ("due_date", "Fecha de vencimiento"),
    ("bill_to", "Facturar a"),
    ("bill_from", "Emisor"),
    ("total_amount", "Importe total"),
    ("tax_details", "Datos fiscales"),
    ("payment_method", "Forma de pago"),
    ("iban", "IBAN"),
    ("bic", "BIC"),
    ("scan_to_pay", "Escanee para pagar"),
    ("description", "Concepto"),
    ("qty_unit", "Cant. (unidad)"),
    ("rate", "Precio"),
    ("total", "Total"),
    ("subtotal", "Subtotal"),
    ("tax", "IVA"),
    ("tax_base", "Base imponible"),
    ("total_due", "Total a pagar"),
    ("notes", "Notas"),
    ("contact", "Contacto"),
    ("page_of", "Página {page} de {total}"),
    ("status_pending", "Pendiente"),
    ("status_paid", "Pagada"),
    ("status_overdue", "Vencida"),
    ("status_draft", "Borrador"),
    (
        "tax_note_reverse_charge",
        "Inversión del sujeto pasivo: IVA a cargo del destinatario (artículo 196 de la Directiva 2006/112/CE del Consejo).",
    ),
    ("tax_note_exempt", "Operación exenta de IVA."),
    ("tax_note_outside_scope", "Operación no sujeta a IVA."),
    ("credit_note", "Nota de crédito"),
    ("refers_to", "Rectifica la factura {number} del {date}"),
    ("refers_to_undated", "Rectifica la factura {number}"),
    ("total_credited", "Total abonado"),
];

const DE: &[(&str, &str)] = &[
    ("invoice", "Rechnung"),
    ("status", "Status"),
    ("invoice_date", "Rechnungsdatum"),
    ("due_date", "Fälligkeitsdatum"),
    ("bill_to", "Rechnung an"),
    ("bill_from", "Rechnung von"),
    ("total_amount", "Gesamtbetrag"),
    ("tax_details", "Steuerangaben"),
    ("payment_method", "Zahlungsart"),
    ("iban", "IBAN"),
    ("bic", "BIC"),
    ("scan_to_pay", "Zum Bezahlen scannen"),
    ("description", "Beschreibung"),
    ("qty_unit", "Menge (Einheit)"),
    ("rate", "Einzelpreis"),
    ("total", "Betrag"),
    ("subtotal", "Zwischensumme"),
    ("tax", "USt."),
    ("tax_base", "Bemessungsgrundlage"),
    ("total_due", "Zu zahlen"),
    ("notes", "Hinweise"),
    ("contact", "Kontakt"),
    ("page_of", "Seite {page} von {total}"),
    ("status_pending", "Offen"),
    ("status_paid", "Bezahlt"),
    ("status_overdue", "Überfällig"),
    ("status_draft", "Entwurf"),
    (
        "tax_note_reverse_charge",
        "Steuerschuldnerschaft des Leistungsempfängers (Reverse Charge, Artikel 196 der Richtlinie 2006/112/EG des Rates).",
    ),
    ("tax_note_exempt", "Steuerfreie Leistung."),
    ("tax_note_outside_scope", "Nicht steuerbare Leistung."),
    ("credit_note", "Gutschrift"),
    ("refers_to", "Bezieht sich auf Rechnung {number} vom {date}"),
    ("refers_to_undated", "Bezieht sich auf Rechnung {number}"),
    ("total_credited", "Gutschriftsbetrag"),
];

const FR: &[(&str, &str)] = &[
    ("invoice", "Facture"),
    ("status", "Statut"),
    ("invoice_date", "Date de facture"),
    ("due_date", "Date d'échéance"),
    ("bill_to", "Facturé à"),
    ("bill_from", "Émetteur"),
    ("total_amount", "Montant total"),
    ("tax_details", "Informations fiscales"),
    ("payment_method", "Moyen de paiement"),
    ("iban", "IBAN"),
    ("bic", "BIC"),
    ("scan_to_pay", "Scanner pour payer"),
    ("description", "Description"),
    ("qty_unit", "Qté (unité)"),
    ("rate", "Prix unitaire"),
    ("total", "Total"),
    ("subtotal", "Sous-total"),
    ("tax", "TVA"),
    ("tax_base", "Base d'imposition"),
    ("total_due", "Total à payer"),
    ("notes", "Notes"),
    ("contact", "Contact"),
    ("page_of", "Page {page} sur {total}"),
    ("status_pending", "En attente"),
    ("status_paid", "Payée"),
    ("status_overdue", "En retard"),
    ("status_draft", "Brouillon"),
    (
        "tax_note_reverse_charge",
        "Autoliquidation : TVA due par le preneur (article 196 de la directive 2006/112/CE du Conseil).",
    ),
    ("tax_note_exempt", "Exonération de TVA."),
    (
        "tax_note_outside_scope",
        "Opération hors du champ d'application de la TVA.",
    ),
    ("credit_note", "Avoir"),
    ("refers_to", "Se rapporte à la facture {number} du {date}"),
    ("refers_to_undated", "Se rapporte à la facture {number}"),
    ("total_credited", "Total de l'avoir"),
];

const IT: &[(&str, &str)] = &[
    ("invoice", "Fattura"),
    ("status", "Stato"),
    ("invoice_date", "Data fattura"),
    ("due_date", "Data di scadenza"),
    ("bill_to", "Destinatario"),
    ("bill_from", "Emittente"),
    ("total_amount", "Importo totale"),
    ("tax_details", "Dati fiscali"),
    ("payment_method", "Metodo di pagamento"),
    ("iban", "IBAN"),
    ("bic", "BIC"),
    ("scan_to_pay", "Scansiona per pagare"),
    ("description", "Descrizione"),
    ("qty_unit", "Qtà (unità)"),
    ("rate", "Prezzo unitario"),
    ("total", "Totale"),
    ("subtotal", "Imponibile"),
    ("tax", "IVA"),
    ("tax_base", "Base imponibile"),
    ("total_due", "Totale da pagare"),
    ("notes", "Note"),
    ("contact", "Contatti"),
    ("page_of", "Pagina {page} di {total}"),
    ("status_pending", "In attesa"),
    ("status_paid", "Pagata"),
    ("status_overdue", "Scaduta"),
    ("status_draft", "Bozza"),
    (
        "tax_note_reverse_charge",
        "Inversione contabile: IVA a carico del destinatario (articolo 196 della direttiva 2006/112/CE del Consiglio).",
    ),
    ("tax_note_exempt", "Operazione esente IVA."),
    ("tax_note_outside_scope", "Operazione fuori campo IVA."),
    ("credit_note", "Nota di credito"),
    ("refers_to", "Riferita alla fattura {number} del {date}"),
    ("refers_to_undated", "Riferita alla fattura {number}"),
    ("total_credited", "Totale accreditato"),
];

const PT: &[(&str, &str)] = &[
    ("invoice", "Fatura"),
    ("status", "Estado"),
    ("invoice_date", "Data da fatura"),
    ("due_date", "Data de vencimento"),
    ("bill_to", "Faturar a"),
    ("bill_from", "Emitente"),
    ("total_amount", "Valor total"),
    ("tax_details", "Dados fiscais"),
    ("payment_method", "Método de pagamento"),
    ("iban", "IBAN"),
    ("bic", "BIC"),
    ("scan_to_pay", "Digitalize para pagar"),
    ("description", "Descrição"),
    ("qty_unit", "Qtd. (unidade)"),
    ("rate", "Preço unitário"),
    ("total", "Total"),
    ("subtotal", "Subtotal"),
    ("tax", "IVA"),
    ("tax_base", "Base tributável"),
    ("total_due", "Total a pagar"),
    ("notes", "Notas"),
    ("contact", "Contacto"),
    ("page_of", "Página {page} de {total}"),
    ("status_pending", "Pendente"),
    ("status_paid", "Paga"),
    ("status_overdue", "Vencida"),
    ("status_draft", "Rascunho"),
    (
        "tax_note_reverse_charge",
        "Autoliquidação: IVA devido pelo adquirente (artigo 196.º da Diretiva 2006/112/CE do Conselho).",
    ),
    ("tax_note_exempt", "Isento de IVA."),
    ("tax_note_outside_scope", "Operação não sujeita a IVA."),
    ("credit_note", "Nota de crédito"),
    ("refers_to", "Refere-se à fatura {number} de {date}"),
    ("refers_to_undated", "Refere-se à fatura {number}"),
    ("total_credited", "Total creditado"),
];

const NL: &[(&str, &str)] = &[
    ("invoice", "Factuur"),
    ("status", "Status"),
    ("invoice_date", "Factuurdatum"),
    ("due_date", "Vervaldatum"),
    ("bill_to", "Factuur aan"),
    ("bill_from", "Factuur van"),
    ("total_amount", "Totaalbedrag"),
    ("tax_details", "Belastinggegevens"),
    ("payment_method", "Betaalmethode"),
    ("iban", "IBAN"),
    ("bic", "BIC"),
    ("scan_to_pay", "Scan om te betalen"),
    ("description", "Omschrijving"),
    ("qty_unit", "Aantal (eenheid)"),
    ("rate", "Tarief"),
    ("total", "Totaal"),
    ("subtotal", "Subtotaal"),
    ("tax", "Btw"),
    ("tax_base", "Grondslag"),
    ("total_due", "Te betalen"),
    ("notes", "Opmerkingen"),
    ("contact", "Contact"),
    ("page_of", "Pagina {page} van {total}"),
    ("status_pending", "Openstaand"),
    ("status_paid", "Betaald"),
    ("status_overdue", "Vervallen"),
    ("status_draft", "Concept"),
    (
        "tax_note_reverse_charge",
        "Btw verlegd: btw verschuldigd door de afnemer (artikel 196 van Richtlijn 2006/112/EG van de Raad).",
    ),
    ("tax_note_exempt", "Vrijgesteld van btw."),
    ("tax_note_outside_scope", "Buiten de reikwijdte van de btw."),
    ("credit_note", "Creditnota"),
    ("refers_to", "Betreft factuur {number} van {date}"),
    ("refers_to_undated", "Betreft factuur {number}"),
    ("total_credited", "Totaal gecrediteerd"),
];

fn table(lang: &str) -> &'static [(&'static str, &'static str)] {
    match lang {
        "es" => ES,
        "de" => DE,
        "fr" => FR,
        "it" => IT,
        "pt" => PT,
        "nl" => NL,
        _ => EN,
    }
}

/// Normalize a user-written language (`"ES"`, `" de "`, `"fr-BE"`, `"pt_PT"`)
/// to a supported code, or explain which ones exist.
pub fn normalize_language(s: &str) -> Result<&'static str, String> {
    let lower = s.trim().to_ascii_lowercase();
    let primary = lower.split(['-', '_']).next().unwrap_or("");
    SUPPORTED
        .iter()
        .find(|&&code| code == primary)
        .copied()
        .ok_or_else(|| {
            format!(
                "unknown language `{}` (supported: {})",
                s.trim(),
                SUPPORTED.join(", ")
            )
        })
}

/// The full label table for `lang` with `overrides` applied on top. Keys
/// missing from the language table fall back to English. Override keys that
/// are not built-in labels are passed through (custom templates may use them).
pub fn labels(lang: &str, overrides: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = EN
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    for (k, v) in table(lang) {
        out.insert(k.to_string(), v.to_string());
    }
    for (k, v) in overrides {
        out.insert(k.clone(), v.clone());
    }
    out
}

/// One label, with the same fallback rules as `labels`.
pub fn label(lang: &str, key: &str, overrides: &BTreeMap<String, String>) -> String {
    if let Some(v) = overrides.get(key) {
        return v.clone();
    }
    let find = |t: &'static [(&'static str, &'static str)]| {
        t.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    };
    find(table(lang))
        .or_else(|| find(EN))
        .unwrap_or(key)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_has_exactly_the_english_keys() {
        let keys: Vec<&str> = EN.iter().map(|(k, _)| *k).collect();
        for lang in SUPPORTED {
            let t: Vec<&str> = table(lang).iter().map(|(k, _)| *k).collect();
            assert_eq!(t, keys, "{lang}");
            assert!(table(lang).iter().all(|(_, v)| !v.trim().is_empty()));
        }
    }

    #[test]
    fn normalize() {
        assert_eq!(normalize_language("ES"), Ok("es"));
        assert_eq!(normalize_language(" fr-BE "), Ok("fr"));
        assert_eq!(normalize_language("pt_PT"), Ok("pt"));
        let err = normalize_language("xx").unwrap_err();
        assert!(
            err.contains("`xx`") && err.contains("en, es, de, fr"),
            "{err}"
        );
    }

    #[test]
    fn overrides_and_fallback() {
        let mut over = BTreeMap::new();
        over.insert("bill_to".to_string(), "Customer".to_string());
        over.insert("custom_key".to_string(), "Hello".to_string());
        let l = labels("de", &over);
        assert_eq!(l["bill_to"], "Customer");
        assert_eq!(l["invoice"], "Rechnung");
        assert_eq!(l["custom_key"], "Hello");
        assert_eq!(label("es", "tax", &BTreeMap::new()), "IVA");
        assert_eq!(label("es", "nope", &BTreeMap::new()), "nope");
        assert_eq!(label("es", "bill_to", &over), "Customer");
    }
}
