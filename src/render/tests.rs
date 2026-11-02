use super::*;
use crate::model::{Decimal, Kind, LineItem, Party, Payment, Status};
use chrono::NaiveDate;
use std::time::Instant;

fn sample() -> Invoice {
    Invoice {
        number: "EINV-2026-23".into(),
        date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        due: NaiveDate::from_ymd_opt(2026, 11, 15),
        status: Status::Pending,
        paid_on: None,
        currency: "EUR".into(),
        from: Party {
            name: "Alex Morgan".into(),
            address: vec!["Rue de la Gare".into(), "1725 Luxembourg".into()],
            tax: vec!["VAT ID: LU12345678".into()],
            email: Some("alex@example.com".into()),
            phone: Some("+352 600 000 000".into()),
            logo: None,
        },
        to: Party {
            name: "Northwind Labs SL".into(),
            address: vec!["Calle Mayor, 12".into(), "28001 Madrid".into()],
            tax: vec!["Tax ID: B12345678".into()],
            ..Default::default()
        },
        payment: Payment {
            holder: Some("ALEX MORGAN".into()),
            iban: Some("DE89 3704 0044 0532 0130 00".into()),
            bic: Some("COBADEFFXXX".into()),
            notes: vec![],
            qr: false,
        },
        items: vec![LineItem {
            description: "Senior Software Engineer #[panic()] $x$ *bold*".into(),
            detail: Some("October 2026 period".into()),
            quantity: Decimal::from(21),
            unit: "days".into(),
            rate: Decimal::from(450),
            tax_rate: None,
        }],
        tax_rate: Decimal::ZERO,
        tax_label: "VAT".into(),
        tax_note: None,
        notes: vec!["Reverse charge.".into()],
        signature: Some("Alex Morgan".into()),
        accent: "#111827".into(),
        language: "en".into(),
        labels: Default::default(),
        template: None,
        format: Default::default(),
        kind: Kind::Invoice,
        refers_to: None,
        refers_to_date: None,
    }
}

fn credit_note() -> Invoice {
    Invoice {
        number: "CN-2026-01".into(),
        kind: Kind::CreditNote,
        refers_to: Some("EINV-2026-20".into()),
        refers_to_date: NaiveDate::from_ymd_opt(2026, 10, 1),
        due: None,
        ..sample()
    }
}

#[test]
fn credit_note_is_titled_and_negated() {
    let r = Renderer::new().unwrap();
    let text = doc_text(&r.compile(&credit_note()).unwrap());
    assert!(text.contains("CREDIT NOTE"), "{text}");
    assert!(
        text.contains("Refers to invoice EINV-2026-20 of October 1, 2026"),
        "{text}"
    );
    assert!(text.contains("Total credited"), "{text}");
    assert!(text.contains("EUR -9,450.00"), "{text}");
    assert!(
        !text.contains("Total due") && !text.contains("INVOICE"),
        "{text}"
    );
    // Rates stay as written.
    assert!(text.contains("450.00"), "{text}");

    // Without a known date, and in another language.
    let mut inv = credit_note();
    inv.refers_to_date = None;
    inv.language = "de".into();
    let text = doc_text(&r.compile(&inv).unwrap());
    assert!(text.contains("GUTSCHRIFT"), "{text}");
    assert!(
        text.contains("Bezieht sich auf Rechnung EINV-2026-20"),
        "{text}"
    );
    assert!(!text.contains(" vom "), "{text}");

    // Every language has its credit note labels.
    for lang in i18n::SUPPORTED {
        let mut inv = credit_note();
        inv.language = lang.to_string();
        let text = doc_text(&r.compile(&inv).unwrap());
        // Text runs can split a label into words with extra spaces.
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let labels = i18n::labels(lang, &Default::default());
        assert!(
            text.contains(&labels["credit_note"].to_uppercase()),
            "{lang}: {text}"
        );
        assert!(text.contains(&labels["total_credited"]), "{lang}: {text}");
    }
}

#[test]
fn invoices_render_no_credit_note_text() {
    let r = Renderer::new().unwrap();
    let mut inv = sample();
    // `refers_to` is ignored for a plain invoice.
    inv.refers_to = Some("EINV-2026-01".into());
    let text = doc_text(&r.compile(&inv).unwrap());
    assert!(
        !text.contains("Refers to") && !text.contains("CREDIT"),
        "{text}"
    );
    assert!(
        text.contains("EUR 9,450.00") && !text.contains("-9,450"),
        "{text}"
    );
}

#[test]
fn compiles_one_page_pdf() {
    let r = Renderer::new().unwrap();
    let inv = sample();
    let t = Instant::now();
    let c = r.compile(&inv).unwrap();
    let t_compile = t.elapsed();
    assert_eq!(c.page_count(), 1);
    let pdf = c.pdf().unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let t = Instant::now();
    let pages = c.render_pages(2.0);
    let t_render = t.elapsed();
    assert_eq!(pages.len(), 1);
    assert_eq!(
        pages[0].rgba.len() as u32,
        pages[0].width * pages[0].height * 4
    );

    // Warm recompile with a small edit (what the live preview does).
    let mut inv2 = inv.clone();
    inv2.items[0].quantity = Decimal::from(22);
    let t = Instant::now();
    let c2 = r.compile(&inv2).unwrap();
    let t_warm = t.elapsed();
    let t = Instant::now();
    c2.render_pages(2.0);
    let t_render2 = t.elapsed();
    println!(
        "cold compile {t_compile:?}, render {t_render:?}; warm compile {t_warm:?}, render {t_render2:?}"
    );
}

#[test]
fn qty_and_accent_formatting() {
    let d = |s: &str| s.parse::<Decimal>().unwrap();
    assert_eq!(data::format_qty(d("21")), "21");
    assert_eq!(data::format_qty(d("21.00")), "21");
    assert_eq!(data::format_qty(d("7.5")), "7.5");
    assert_eq!(data::format_qty(d("0")), "0");
    assert_eq!(data::format_qty(d("-0.001")), "0");
    assert_eq!(data::format_qty(d("0.125")), "0.13");
    assert_eq!(data::format_qty(d("1234.5")), "1234.5");
    assert_eq!(data::format_qty(d("-2.5")), "-2.5");
    assert_eq!(data::format_qty(d("100")), "100");
    assert_eq!(data::normalize_accent("#2563EB"), "#2563eb");
    assert_eq!(data::normalize_accent("red\"); panic()"), "#111827");
}

#[test]
fn minimal_and_hostile_inputs_compile() {
    let r = Renderer::new().unwrap();
    let mut inv = sample();
    inv.items.clear();
    inv.from = Party {
        name: "A \"quoted\" <name> \\ #let x = 1".into(),
        ..Default::default()
    };
    inv.to = Party::default();
    inv.payment = Payment::default();
    inv.notes = vec!["  ".into()];
    inv.signature = None;
    inv.due = None;
    inv.status = Status::Draft;
    inv.tax_label = String::new();
    inv.accent = "not a color".into();
    inv.language = "unknown".into();
    let c = r.compile(&inv).unwrap();
    assert_eq!(c.page_count(), 1);
    assert!(c.pdf().unwrap().starts_with(b"%PDF"));
}

/// All text placed on the pages, items joined with spaces.
fn doc_text(c: &Compiled) -> String {
    fn walk(frame: &typst::layout::Frame, out: &mut String) {
        for (_, item) in frame.items() {
            match item {
                typst::layout::FrameItem::Text(t) => {
                    out.push_str(&t.text);
                    out.push(' ');
                }
                typst::layout::FrameItem::Group(g) => walk(&g.frame, out),
                _ => {}
            }
        }
    }
    let mut out = String::new();
    for page in c.doc.pages() {
        walk(&page.frame, &mut out);
    }
    out
}

fn image_count(c: &Compiled) -> usize {
    fn walk(frame: &typst::layout::Frame) -> usize {
        frame
            .items()
            .map(|(_, item)| match item {
                typst::layout::FrameItem::Image(..) => 1,
                typst::layout::FrameItem::Group(g) => walk(&g.frame),
                _ => 0,
            })
            .sum()
    }
    c.doc.pages().iter().map(|p| walk(&p.frame)).sum()
}

#[test]
fn every_language_renders_its_labels() {
    let r = Renderer::new().unwrap();
    for lang in i18n::SUPPORTED {
        let mut inv = sample();
        inv.language = lang.to_string();
        inv.status = Status::Overdue;
        let c = r.compile(&inv).unwrap();
        // A label may wrap next to a long localized date: join the lines
        // (only plain spaces, the no-break spaces of amounts stay).
        let text = doc_text(&c)
            .split(' ')
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let labels = i18n::labels(lang, &Default::default());
        for key in ["invoice_date", "due_date", "subtotal", "total_due"] {
            assert!(
                text.contains(&labels[key]),
                "{lang}: {key} missing in {text}"
            );
        }
        // Section labels and the status pill are upper-cased by the template.
        for key in ["bill_to", "payment_method", "status_overdue", "contact"] {
            let up = labels[key].to_uppercase();
            assert!(text.contains(&up), "{lang}: {key} ({up}) missing in {text}");
        }
        // Dates and amounts follow the language.
        let (date, total) = match *lang {
            "en" => ("November 1, 2026", "EUR 9,450.00"),
            "es" => ("1 de noviembre de 2026", "9.450,00\u{a0}€"),
            "de" => ("1. November 2026", "9.450,00\u{a0}€"),
            "fr" => ("1 novembre 2026", "9\u{202f}450,00\u{a0}€"),
            "it" => ("1 novembre 2026", "9.450,00\u{a0}€"),
            "pt" => ("1 de novembro de 2026", "9\u{a0}450,00\u{a0}€"),
            "nl" => ("1 november 2026", "€\u{a0}9.450,00"),
            other => panic!("no expectation for {other}"),
        };
        assert!(text.contains(date), "{lang}: {date} missing in {text}");
        assert!(text.contains(total), "{lang}: {total} missing in {text}");
    }
}

/// The view model for a default (English) invoice uses exactly the strings
/// of the fixed CLI format, so English PDFs are unchanged by locales.
#[test]
fn english_data_is_the_historical_format() {
    use crate::model::{format_amount, format_date, format_money};
    let mut inv = sample();
    inv.tax_rate = "7.5".parse().unwrap();
    inv.items[0].quantity = "1234.5".parse().unwrap();
    let v = serde_json::to_value(data::View::new(&inv, None, None)).unwrap();
    assert_eq!(v["locale"], "en");
    assert_eq!(v["date"], format_date(inv.date));
    assert_eq!(v["due"], format_date(inv.due.unwrap()));
    assert_eq!(v["date"], "November 1, 2026");
    assert_eq!(v["total"], format_money("EUR", inv.total()));
    assert_eq!(v["subtotal"], format_money("EUR", inv.subtotal()));
    assert_eq!(v["tax"], format_money("EUR", inv.tax()));
    assert_eq!(v["tax_rate"], "7.5%");
    assert_eq!(v["items"][0]["qty"], "1234.5");
    assert_eq!(v["items"][0]["rate"], format_amount(inv.items[0].rate));
    assert_eq!(v["items"][0]["total"], "EUR 555,525.00");
}

#[test]
fn locale_overrides_reach_the_pdf() {
    use crate::model::{CurrencyPosition, CurrencyStyle, NumberFormat};
    let r = Renderer::new().unwrap();
    let mut inv = sample();
    inv.language = "en".into();
    inv.currency = "CHF".into();
    inv.tax_rate = "7.7".parse().unwrap();
    inv.items[0].quantity = "20.5".parse().unwrap();
    inv.format.locale = Some("de-CH".into());
    let text = doc_text(&r.compile(&inv).unwrap());
    // English labels, Swiss numbers and German month names.
    assert!(text.contains("Total due"), "{text}");
    assert!(text.contains("CHF\u{a0}9\u{2019}225.00"), "{text}");
    assert!(text.contains("20.5"), "{text}");
    assert!(text.contains("1. November 2026"), "{text}");

    inv.format.number_format = NumberFormat {
        thousands: Some(" ".into()),
        decimal: Some(",".into()),
        currency: Some(CurrencyStyle::Code),
        currency_position: Some(CurrencyPosition::After),
    };
    inv.format.date_format = Some("%d.%m.%Y".into());
    let text = doc_text(&r.compile(&inv).unwrap());
    assert!(text.contains("9 225,00\u{a0}CHF"), "{text}");
    assert!(text.contains("20,5"), "{text}");
    assert!(text.contains("7,7%"), "{text}");
    assert!(
        text.contains("01.11.2026") && text.contains("15.11.2026"),
        "{text}"
    );
}

#[test]
fn spanish_invoice_has_no_english_labels() {
    let r = Renderer::new().unwrap();
    let en = doc_text(&r.compile(&sample()).unwrap());
    assert!(en.contains("BILL TO") && en.contains("Total due"), "{en}");
    let mut inv = sample();
    inv.language = "es".into();
    let es = doc_text(&r.compile(&inv).unwrap());
    for english in ["BILL TO", "Total due", "Invoice date", "PENDING"] {
        assert!(
            !es.contains(english),
            "`{english}` in spanish invoice: {es}"
        );
    }
    assert!(
        es.contains("FACTURAR A") && es.contains("PENDIENTE"),
        "{es}"
    );

    // The page footer of a multi-page invoice is translated too.
    let item = inv.items[0].clone();
    inv.items = vec![item; 40];
    let c = r.compile(&inv).unwrap();
    assert!(c.page_count() > 1);
    let text = doc_text(&c);
    assert!(
        text.contains(&format!("Página 2 de {}", c.page_count())),
        "{text}"
    );
}

#[test]
fn label_overrides_apply() {
    let r = Renderer::new().unwrap();
    let mut inv = sample();
    inv.language = "de".into();
    inv.labels
        .insert("invoice_date".into(), "Leistungsdatum".into());
    let text = doc_text(&r.compile(&inv).unwrap());
    assert!(text.contains("Leistungsdatum"), "{text}");
    assert!(!text.contains("Rechnungsdatum"), "{text}");
    assert!(text.contains("Zu zahlen"), "{text}");
}

#[test]
fn view_carries_contract_version_language_and_labels() {
    let mut inv = sample();
    inv.language = "fr".into();
    inv.tax_label = String::new();
    let v = serde_json::to_value(data::View::new(&inv, None, None)).unwrap();
    assert_eq!(v["contract_version"], CONTRACT_VERSION);
    assert_eq!(v["language"], "fr");
    assert_eq!(v["labels"]["bill_to"], "Facturé à");
    assert_eq!(v["status"], "pending");
    assert_eq!(v["status_label"], "En attente");
    assert_eq!(v["tax_label"], "TVA");
    assert!(v["logo"].is_null());
    assert_eq!(v["kind"], "invoice");
    assert!(v["refers_to"].is_null());
    assert_eq!(v["total"], "EUR 9,450.00");

    let v = serde_json::to_value(data::View::new(&credit_note(), None)).unwrap();
    assert_eq!(v["kind"], "credit_note");
    assert_eq!(v["refers_to"]["number"], "EINV-2026-20");
    assert_eq!(v["refers_to"]["date"], "October 1, 2026");
    assert_eq!(v["labels"]["credit_note"], "Credit note");
    assert_eq!(v["items"][0]["rate"], "450.00");
    assert_eq!(v["items"][0]["total"], "EUR -9,450.00");
    assert_eq!(v["subtotal"], "EUR -9,450.00");
    assert_eq!(v["tax"], "EUR 0.00");
    assert_eq!(v["total"], "EUR -9,450.00");
    assert_eq!(v["tax_breakdown"][0]["base"], "EUR -9,450.00");
}

fn write_png(path: &std::path::Path, w: u32, h: u32) {
    let rgba: Vec<u8> = (0..w * h).flat_map(|_| [15, 118, 110, 255]).collect();
    image::save_buffer(path, &rgba, w, h, image::ExtendedColorType::Rgba8).unwrap();
}

#[test]
fn logo_svg_and_png_render() {
    let r = Renderer::new().unwrap();
    let base = r.compile(&sample()).unwrap();
    assert_eq!(image_count(&base), 0);

    let mut inv = sample();
    inv.from.logo = Some(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/logo.svg").into());
    let c = r.compile(&inv).unwrap();
    assert_eq!(c.page_count(), 1);
    assert_eq!(image_count(&c), 1);

    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("Logo.PNG");
    write_png(&png, 400, 100);
    inv.from.logo = Some(png);
    let c = r.compile(&inv).unwrap();
    assert_eq!(image_count(&c), 1);
    assert!(c.pdf().unwrap().starts_with(b"%PDF"));
}

#[test]
fn logo_errors_name_the_path() {
    let r = Renderer::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut inv = sample();

    let missing = dir.path().join("nope.svg");
    inv.from.logo = Some(missing.clone());
    let err = format!("{:#}", r.compile(&inv).err().unwrap());
    assert!(err.contains(&missing.display().to_string()), "{err}");

    let bmp = dir.path().join("logo.bmp");
    std::fs::write(&bmp, b"BM").unwrap();
    inv.from.logo = Some(bmp);
    let err = format!("{:#}", r.compile(&inv).err().unwrap());
    assert!(
        err.contains("logo.bmp") && err.contains("unsupported"),
        "{err}"
    );

    let fake = dir.path().join("logo.png");
    std::fs::write(&fake, b"<svg></svg>").unwrap();
    inv.from.logo = Some(fake.clone());
    let err = format!("{:#}", r.compile(&inv).err().unwrap());
    assert!(
        err.contains(&fake.display().to_string()) && err.contains("PNG"),
        "{err}"
    );
}

#[test]
fn custom_template_gets_the_data_and_its_own_files() {
    let r = Renderer::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("part.typ"), "#let greet(x) = [Hello #x]\n").unwrap();
    let logo = dir.path().join("logo.png");
    write_png(&logo, 10, 10);
    let tpl = dir.path().join("mine.typ");
    std::fs::write(
        &tpl,
        "#import \"part.typ\": greet\n\
         #let d = json(\"/invoice.json\")\n\
         #greet(d.to.name) v#d.contract_version #d.labels.bill_to #d.total #d.locale\n\
         #if d.logo != none { image(d.logo, height: 1cm) }\n",
    )
    .unwrap();
    let mut inv = sample();
    inv.template = Some(tpl);
    inv.from.logo = Some(logo);
    inv.language = "nl".into();
    let c = r.compile(&inv).unwrap();
    let text = doc_text(&c);
    assert!(text.contains("Hello"), "{text}");
    assert!(text.contains("Northwind Labs SL"), "{text}");
    assert!(text.contains(&format!("v{CONTRACT_VERSION}")), "{text}");
    assert!(text.contains("Factuur aan"), "{text}");
    assert!(text.contains("€\u{a0}9.450,00 nl"), "{text}");
    assert!(!text.contains("FACTUUR AAN"), "{text}");
    assert_eq!(image_count(&c), 1);
}

#[test]
fn exported_builtin_template_works_as_custom_template() {
    let r = Renderer::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("invoice.typ");
    std::fs::write(&tpl, TEMPLATE).unwrap();
    let mut inv = sample();
    inv.template = Some(tpl);
    let custom = doc_text(&r.compile(&inv).unwrap());
    let builtin = doc_text(&r.compile(&sample()).unwrap());
    assert_eq!(custom, builtin);
}

#[test]
fn custom_template_errors_point_at_file_and_line() {
    let r = Renderer::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("broken.typ");
    std::fs::write(
        &tpl,
        "#let d = json(\"/invoice.json\")\n\n#d.no_such_field\n",
    )
    .unwrap();
    let mut inv = sample();
    inv.template = Some(tpl.clone());
    let err = format!("{:#}", r.compile(&inv).err().unwrap());
    assert!(err.contains(&format!("{}:3:", tpl.display())), "{err}");
    assert!(err.contains("no_such_field"), "{err}");

    // Errors in an imported file name that file.
    let part = dir.path().join("part.typ");
    std::fs::write(&part, "#let x = (1 + \"a\")\n").unwrap();
    std::fs::write(&tpl, "#import \"part.typ\": x\n#x\n").unwrap();
    let err = format!("{:#}", r.compile(&inv).err().unwrap());
    assert!(err.contains(&format!("{}:1:", part.display())), "{err}");

    // A missing template is a clear error.
    inv.template = Some(dir.path().join("missing.typ"));
    let err = format!("{:#}", r.compile(&inv).err().unwrap());
    assert!(err.contains("missing.typ"), "{err}");
}

#[test]
fn custom_template_cannot_escape_its_directory() {
    let r = Renderer::new().unwrap();
    let outer = tempfile::tempdir().unwrap();
    std::fs::write(outer.path().join("secret.txt"), "top secret").unwrap();
    let inner = outer.path().join("tpl");
    std::fs::create_dir(&inner).unwrap();
    let tpl = inner.join("t.typ");
    std::fs::write(&tpl, "#read(\"../secret.txt\")\n").unwrap();
    let mut inv = sample();
    inv.template = Some(tpl);
    match r.compile(&inv) {
        Ok(c) => assert!(!doc_text(&c).contains("top secret")),
        Err(e) => assert!(!format!("{e:#}").contains("top secret")),
    }
}

#[test]
fn example_invoices_render_with_example_config() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let cfg = crate::config::Config::load_from(&root.join("config.yaml")).unwrap();
    let r = Renderer::new().unwrap();
    let mut seen = 0;
    for entry in std::fs::read_dir(root.join("invoices")).unwrap() {
        let path = entry.unwrap().path();
        let src = std::fs::read_to_string(&path).unwrap();
        let inv = crate::parse::parse_invoice(&src, &cfg).unwrap();
        assert_eq!(
            inv.from.logo.as_deref(),
            Some(root.join("logo.svg").as_path())
        );
        let c = r.compile(&inv).unwrap();
        assert_eq!(image_count(&c), 1, "{}", path.display());
        seen += 1;
    }
    assert!(seen >= 3);
}

/// Decode every QR code found on the rendered first page.
fn decode_qr_codes(c: &Compiled) -> Vec<String> {
    let page = &c.render_pages(3.0)[0];
    let (w, h) = (page.width as usize, page.height as usize);
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| {
        let i = (y * w + x) * 4;
        let px = &page.rgba[i..i + 4];
        // Transparent background counts as white paper.
        let a = u32::from(px[3]);
        let grey = (u32::from(px[0]) + u32::from(px[1]) + u32::from(px[2])) / 3;
        (grey * a / 255 + (255 - a)) as u8
    });
    img.detect_grids()
        .iter()
        .filter_map(|g| g.decode().ok().map(|(_, text)| text))
        .collect()
}

#[test]
fn epc_qr_code_renders_and_scans_back_to_the_payload() {
    let r = Renderer::new().unwrap();
    let mut inv = sample();
    inv.payment.qr = true;
    let payload = epc::check(&inv).unwrap().unwrap();
    assert!(
        payload.ends_with("\nEUR9450\n\n\nEINV-2026-23"),
        "{payload}"
    );

    let c = r.compile(&inv).unwrap();
    assert_eq!(c.page_count(), 1);
    assert!(c.pdf().unwrap().starts_with(b"%PDF"));
    assert_eq!(image_count(&c), 1);
    assert!(doc_text(&c).contains("Scan to pay"));
    assert_eq!(decode_qr_codes(&c), vec![payload]);

    let v = serde_json::to_value(data::View::new(&inv, None, Some(QR_PATH.into()))).unwrap();
    assert_eq!(v["payment"]["qr"], QR_PATH);
    assert_eq!(v["labels"]["scan_to_pay"], "Scan to pay");

    // Translated caption.
    inv.language = "de".into();
    let c = r.compile(&inv).unwrap();
    assert!(doc_text(&c).contains("Zum Bezahlen scannen"));
}

#[test]
fn epc_qr_code_is_skipped_silently_when_not_applicable() {
    let r = Renderer::new().unwrap();
    let mut inv = sample();
    // Off by default.
    let c = r.compile(&inv).unwrap();
    assert_eq!(image_count(&c), 0);
    assert!(!doc_text(&c).contains("Scan to pay"));
    let v = serde_json::to_value(data::View::new(&inv, None, None)).unwrap();
    assert!(v["payment"]["qr"].is_null());

    inv.payment.qr = true;
    let breakers: [fn(&mut Invoice); 4] = [
        |i| i.currency = "USD".into(),
        |i| i.payment.iban = None,
        |i| i.payment.iban = Some("DE00 0000 0000 0000 0000 00".into()),
        |i| i.items[0].rate = Decimal::ZERO,
    ];
    for broken in breakers {
        let mut inv = inv.clone();
        broken(&mut inv);
        let c = r.compile(&inv).unwrap();
        assert_eq!(image_count(&c), 0);
        assert!(!doc_text(&c).contains("Scan to pay"));
        assert!(decode_qr_codes(&c).is_empty());
    }
}

fn multi_rate(lang: &str) -> Invoice {
    let mut inv = sample();
    inv.language = lang.into();
    inv.tax_label = String::new();
    inv.tax_rate = Decimal::from(21);
    let mut training = inv.items[0].clone();
    training.description = "Training".into();
    training.quantity = Decimal::from(2);
    training.rate = Decimal::from(600);
    training.tax_rate = Some(Decimal::from(10));
    inv.items.push(training);
    inv
}

#[test]
fn single_rate_view_is_unchanged() {
    let mut inv = sample();
    inv.tax_rate = Decimal::from(21);
    let v = serde_json::to_value(data::View::new(&inv, None, None)).unwrap();
    assert_eq!(v["tax_rate"], "21%");
    assert_eq!(v["tax"], "EUR 1,984.50");
    assert_eq!(v["total"], "EUR 11,434.50");
    assert_eq!(v["items"][0]["tax_rate"], "21%");
    assert_eq!(v["tax_breakdown"].as_array().unwrap().len(), 1);
    assert!(v["tax_note"].is_null());
    let text = doc_text(&Renderer::new().unwrap().compile(&inv).unwrap());
    assert!(text.contains("VAT (21%)"), "{text}");
    assert!(!text.contains("Taxable amount"), "{text}");
}

#[test]
fn multi_rate_breakdown_renders_in_two_languages() {
    let r = Renderer::new().unwrap();
    let inv = multi_rate("en");
    let v = serde_json::to_value(data::View::new(&inv, None, None)).unwrap();
    // 9,450 at 21% = 1,984.50; 1,200 at 10% = 120.00.
    let groups = v["tax_breakdown"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["rate"], "21%");
    assert_eq!(groups[0]["base"], "EUR 9,450.00");
    assert_eq!(groups[0]["tax"], "EUR 1,984.50");
    assert_eq!(groups[1]["rate"], "10%");
    assert_eq!(groups[1]["base"], "EUR 1,200.00");
    assert_eq!(groups[1]["tax"], "EUR 120.00");
    assert_eq!(v["tax"], "EUR 2,104.50");
    assert_eq!(v["total"], "EUR 12,754.50");
    assert_eq!(v["items"][1]["tax_rate"], "10%");

    let en = doc_text(&r.compile(&inv).unwrap());
    for s in [
        "VAT 21%",
        "VAT 10%",
        "Taxable amount EUR 9,450.00",
        "EUR 1,984.50",
        "EUR 120.00",
        "EUR 12,754.50",
    ] {
        assert!(en.contains(s), "`{s}` missing in {en}");
    }
    assert!(!en.contains("VAT (21%)"), "{en}");

    let es = doc_text(&r.compile(&multi_rate("es")).unwrap());
    // Spanish formatting: decimal comma, `€` after the amount, `21 %`.
    for s in [
        "IVA 21\u{a0}%",
        "IVA 10\u{a0}%",
        "Base imponible 1.200,00\u{a0}€",
    ] {
        assert!(es.contains(s), "`{s}` missing in {es}");
    }
}

#[test]
fn tax_note_presets_render_translated() {
    let r = Renderer::new().unwrap();
    for lang in i18n::SUPPORTED {
        for preset in crate::model::TAX_NOTE_PRESETS {
            let mut inv = sample();
            inv.language = lang.to_string();
            inv.tax_note = Some(preset.to_string());
            let key = format!("tax_note_{preset}");
            let expected = i18n::label(lang, &key, &Default::default());
            assert_ne!(expected, key, "{lang}: {key} has no text");
            let v = serde_json::to_value(data::View::new(&inv, None, None)).unwrap();
            assert_eq!(v["tax_note"], expected.as_str(), "{lang} {preset}");
            if *preset == "reverse_charge" {
                assert!(
                    expected.contains("196") && expected.contains("2006/112/"),
                    "{lang}: {expected}"
                );
            }
        }
    }
    // The text reaches the PDF (two languages are enough for the layout).
    for (lang, needle) in [("en", "Reverse charge"), ("de", "Steuerschuldnerschaft")] {
        let mut inv = sample();
        inv.language = lang.into();
        inv.tax_note = Some("reverse_charge".into());
        let text = doc_text(&r.compile(&inv).unwrap());
        assert!(text.contains(needle), "{lang}: {text}");
    }
    // Free text is printed as written; label overrides reword presets.
    let mut inv = sample();
    inv.tax_note = Some("Not subject to VAT, section 4 of the local VAT act.".into());
    let text = doc_text(&r.compile(&inv).unwrap());
    assert!(text.contains("section 4 of the local VAT act"), "{text}");
    inv.tax_note = Some("exempt".into());
    inv.labels
        .insert("tax_note_exempt".into(), "Custom exemption text".into());
    let v = serde_json::to_value(data::View::new(&inv, None, None)).unwrap();
    assert_eq!(v["tax_note"], "Custom exemption text");
}
