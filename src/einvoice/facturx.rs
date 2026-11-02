//! Factur-X / ZUGFeRD hybrid PDFs.
//!
//! Typst does most of the work: the CII XML is attached with `pdf.attach`
//! (relationship `Alternative`, MIME type `text/xml`) and the PDF is
//! exported as PDF/A-3b, which embeds all fonts, writes the `/AF` array and
//! the PDF/A identification in the XMP metadata.
//!
//! What typst (0.15, through krilla) cannot do is add custom XMP: Factur-X
//! requires the `fx:` properties (document type, file name, version,
//! conformance level) plus a PDF/A extension schema that declares them. So
//! the finished PDF is patched once: [`patch_xmp`] inserts those properties
//! into the metadata stream and shifts the cross-reference offsets. krilla
//! writes a single revision with a classic xref table and an unfiltered
//! metadata stream, which keeps the patch a few dozen lines with no PDF
//! library; anything else is refused with an error rather than guessed at.

use anyhow::{Context, anyhow, bail};

use crate::model::Invoice;
use crate::render::{Attachment, Compiled, Renderer};

/// Name of the embedded XML, fixed by the Factur-X specification.
pub const FACTURX_FILE_NAME: &str = "factur-x.xml";

const PDFA_EXTENSION_NS: &str = "xmlns:pdfaExtension=\"http://www.aiim.org/pdfa/ns/extension/\" \
     xmlns:pdfaSchema=\"http://www.aiim.org/pdfa/ns/schema#\" \
     xmlns:pdfaProperty=\"http://www.aiim.org/pdfa/ns/property#\"";

/// The PDF/A extension schema entry (`rdf:li`) declaring the Factur-X 1.0
/// `fx:` properties.
fn facturx_schema() -> String {
    let prop = |name: &str, desc: &str| {
        format!(
            "<rdf:li rdf:parseType=\"Resource\">\
             <pdfaProperty:name>{name}</pdfaProperty:name>\
             <pdfaProperty:valueType>Text</pdfaProperty:valueType>\
             <pdfaProperty:category>external</pdfaProperty:category>\
             <pdfaProperty:description>{desc}</pdfaProperty:description>\
             </rdf:li>"
        )
    };
    format!(
        "<rdf:li rdf:parseType=\"Resource\">\
         <pdfaSchema:schema>Factur-X PDFA Extension Schema</pdfaSchema:schema>\
         <pdfaSchema:namespaceURI>urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#</pdfaSchema:namespaceURI>\
         <pdfaSchema:prefix>fx</pdfaSchema:prefix>\
         <pdfaSchema:property><rdf:Seq>{}{}{}{}</rdf:Seq></pdfaSchema:property>\
         </rdf:li>",
        prop("DocumentFileName", "The name of the embedded XML document"),
        prop(
            "DocumentType",
            "The type of the hybrid document in capital letters, e.g. INVOICE or ORDER"
        ),
        prop(
            "Version",
            "The actual version of the standard applying to the embedded XML document"
        ),
        prop(
            "ConformanceLevel",
            "The conformance level of the embedded XML document"
        ),
    )
}

/// The Factur-X 1.0 properties, profile EN 16931.
fn facturx_properties() -> String {
    format!(
        "<rdf:Description rdf:about=\"\" \
         xmlns:fx=\"urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#\">\
         <fx:DocumentType>INVOICE</fx:DocumentType>\
         <fx:DocumentFileName>{FACTURX_FILE_NAME}</fx:DocumentFileName>\
         <fx:Version>1.0</fx:Version>\
         <fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>\
         </rdf:Description>"
    )
}

/// Add the Factur-X schema and properties to an XMP packet. The schema
/// joins the existing `pdfaExtension:schemas` bag when there is one (a
/// property may appear only once per resource), else it gets its own
/// description.
pub(super) fn add_facturx_xmp(packet: &str) -> anyhow::Result<String> {
    let mut s = packet.to_string();
    let close = |s: &str| {
        s.rfind("</rdf:RDF>")
            .ok_or_else(|| anyhow!("the XMP packet has no </rdf:RDF>"))
    };
    let schema = facturx_schema();
    match s.find("</rdf:Bag></pdfaExtension:schemas>") {
        Some(i) => s.insert_str(i, &schema),
        None => {
            let i = close(&s)?;
            s.insert_str(
                i,
                &format!(
                    "<rdf:Description rdf:about=\"\" {PDFA_EXTENSION_NS}>\
                     <pdfaExtension:schemas><rdf:Bag>{schema}</rdf:Bag></pdfaExtension:schemas>\
                     </rdf:Description>"
                ),
            );
        }
    }
    let i = close(&s)?;
    s.insert_str(i, &facturx_properties());
    Ok(s)
}

/// Render `inv` as a Factur-X PDF: PDF/A-3b with `factur-x.xml` embedded.
/// Fails with the list of missing fields when the invoice cannot be a
/// valid EN 16931 invoice.
pub fn facturx_pdf(renderer: &Renderer, inv: &Invoice) -> anyhow::Result<Vec<u8>> {
    let attachment = facturx_attachment(inv)?;
    let compiled = renderer.compile_with(inv, Some(&attachment))?;
    facturx_export(&compiled, inv)
}

/// The `factur-x.xml` attachment for [`Renderer::compile_with`].
pub fn facturx_attachment(inv: &Invoice) -> Result<Attachment, super::ValidationError> {
    Ok(Attachment {
        name: FACTURX_FILE_NAME,
        data: super::cii_xml(inv)?.into_bytes(),
        mime_type: "text/xml",
        description: "Factur-X / ZUGFeRD invoice (EN 16931)",
        relationship: "alternative",
    })
}

/// Export a document compiled with [`facturx_attachment`] as a Factur-X
/// PDF: PDF/A-3b plus the Factur-X XMP properties.
pub fn facturx_export(compiled: &Compiled, inv: &Invoice) -> anyhow::Result<Vec<u8>> {
    let pdf = compiled.pdf_a3b(inv.date)?;
    patch_xmp(&pdf, add_facturx_xmp).context("adding the Factur-X XMP metadata")
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

fn parse_uint(b: &[u8]) -> Option<(usize, usize)> {
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    let n = std::str::from_utf8(&b[..digits]).ok()?.parse().ok()?;
    Some((n, digits))
}

/// Replace the XMP packet of a single-revision PDF with a classic xref
/// table by `edit(packet)`, fixing the stream `/Length`, the xref offsets
/// and `startxref`.
pub fn patch_xmp(
    pdf: &[u8],
    edit: impl FnOnce(&str) -> anyhow::Result<String>,
) -> anyhow::Result<Vec<u8>> {
    let begin =
        find(pdf, b"<?xpacket begin=", 0).ok_or_else(|| anyhow!("the PDF has no XMP packet"))?;
    if find(pdf, b"<?xpacket begin=", begin + 1).is_some() {
        bail!("the PDF has more than one XMP packet");
    }
    let endstream =
        find(pdf, b"endstream", begin).ok_or_else(|| anyhow!("unterminated XMP stream"))?;
    if !(pdf[..begin].ends_with(b"stream\n") || pdf[..begin].ends_with(b"stream\r\n")) {
        bail!("the XMP packet is not a plain stream");
    }

    // The object holding the packet and its /Length.
    let obj = rfind(&pdf[..begin], b" obj").ok_or_else(|| anyhow!("no object for the XMP"))?;
    let obj_start = pdf[..obj]
        .iter()
        .rposition(|&c| c == b'\n' || c == b'\r')
        .map_or(0, |p| p + 1);
    let dict = &pdf[obj..begin];
    if find(dict, b"/Filter", 0).is_some() {
        bail!("the XMP stream is compressed");
    }
    let len_key = find(dict, b"/Length", 0).ok_or_else(|| anyhow!("XMP stream without /Length"))?;
    let mut len_at = obj + len_key + b"/Length".len();
    while pdf[len_at] == b' ' {
        len_at += 1;
    }
    let (old_len, len_digits) =
        parse_uint(&pdf[len_at..]).ok_or_else(|| anyhow!("indirect XMP /Length"))?;
    let data_end = begin + old_len;
    if data_end > endstream || !pdf[data_end..endstream].iter().all(u8::is_ascii_whitespace) {
        bail!("XMP /Length does not match the stream");
    }
    let packet = std::str::from_utf8(&pdf[begin..data_end]).context("XMP is not UTF-8")?;
    let new_packet = edit(packet)?;

    let new_len = new_packet.len().to_string();
    let mut out = Vec::with_capacity(pdf.len() + new_packet.len());
    out.extend_from_slice(&pdf[..len_at]);
    out.extend_from_slice(new_len.as_bytes());
    out.extend_from_slice(&pdf[len_at + len_digits..begin]);
    out.extend_from_slice(new_packet.as_bytes());
    out.extend_from_slice(&pdf[data_end..]);
    // Offsets after the metadata object move by this much (may be negative).
    let delta = out.len() as i64 - pdf.len() as i64;
    let shift = |off: usize| (off as i64 + delta) as usize;

    // startxref, at the very end.
    let sx = rfind(&out, b"startxref").ok_or_else(|| anyhow!("no startxref"))?;
    let mut num_at = sx + b"startxref".len();
    while out[num_at].is_ascii_whitespace() {
        num_at += 1;
    }
    let (old_xref, xref_digits) =
        parse_uint(&out[num_at..]).ok_or_else(|| anyhow!("bad startxref"))?;
    if old_xref < obj_start {
        bail!("unexpected xref position");
    }
    let xref = shift(old_xref);
    if !out[xref..].starts_with(b"xref") {
        bail!("the PDF uses an xref stream; cannot patch its XMP");
    }
    if find(&out[xref..], b"/Prev", 0).is_some() {
        bail!("the PDF has several revisions; cannot patch its XMP");
    }

    // Classic xref table: subsection headers, then 20-byte entries.
    let mut at = xref + 4;
    loop {
        while out[at].is_ascii_whitespace() {
            at += 1;
        }
        if out[at..].starts_with(b"trailer") {
            break;
        }
        let (_first, d1) = parse_uint(&out[at..]).ok_or_else(|| anyhow!("bad xref"))?;
        at += d1 + 1;
        let (count, d2) = parse_uint(&out[at..]).ok_or_else(|| anyhow!("bad xref"))?;
        at += d2;
        while out[at].is_ascii_whitespace() {
            at += 1;
        }
        for _ in 0..count {
            let entry = &out[at..at + 20];
            if entry[10] != b' ' || entry[16] != b' ' {
                bail!("bad xref entry");
            }
            let (off, d) = parse_uint(&entry[..10]).ok_or_else(|| anyhow!("bad xref entry"))?;
            if d != 10 {
                bail!("bad xref entry");
            }
            if entry[17] == b'n' && off > obj_start {
                out[at..at + 10].copy_from_slice(format!("{:010}", shift(off)).as_bytes());
            }
            at += 20;
        }
    }

    let mut fixed = out[..num_at].to_vec();
    fixed.extend_from_slice(xref.to_string().as_bytes());
    fixed.extend_from_slice(&out[num_at + xref_digits..]);
    Ok(fixed)
}

/// Check that every in-use xref entry points at its object (`N G obj`).
/// For tests and debugging.
pub fn xref_is_consistent(pdf: &[u8]) -> bool {
    let Some(sx) = rfind(pdf, b"startxref") else {
        return false;
    };
    let mut at = sx + 9;
    while pdf[at].is_ascii_whitespace() {
        at += 1;
    }
    let Some((xref, _)) = parse_uint(&pdf[at..]) else {
        return false;
    };
    if !pdf[xref..].starts_with(b"xref") {
        return false;
    }
    let mut at = xref + 4;
    loop {
        while pdf[at].is_ascii_whitespace() {
            at += 1;
        }
        if pdf[at..].starts_with(b"trailer") {
            return true;
        }
        let Some((first, d1)) = parse_uint(&pdf[at..]) else {
            return false;
        };
        at += d1 + 1;
        let Some((count, d2)) = parse_uint(&pdf[at..]) else {
            return false;
        };
        at += d2;
        while pdf[at].is_ascii_whitespace() {
            at += 1;
        }
        for i in 0..count {
            let entry = &pdf[at..at + 20];
            if entry[17] == b'n' {
                let off: usize = std::str::from_utf8(&entry[..10])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(usize::MAX);
                let head = format!("{} 0 obj", first + i);
                if !pdf
                    .get(off..)
                    .is_some_and(|p| p.starts_with(head.as_bytes()))
                {
                    return false;
                }
            }
            at += 20;
        }
    }
}
