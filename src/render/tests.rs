use super::*;
use crate::model::{Decimal, LineItem, Party, Payment, Status};
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
        },
        items: vec![LineItem {
            description: "Senior Software Engineer #[panic()] $x$ *bold*".into(),
            detail: Some("October 2026 period".into()),
            quantity: Decimal::from(21),
            unit: "days".into(),
            rate: Decimal::from(450),
        }],
        tax_rate: Decimal::ZERO,
        tax_label: "VAT".into(),
        notes: vec!["Reverse charge.".into()],
        signature: Some("Alex Morgan".into()),
        accent: "#111827".into(),
        language: "en".into(),
        labels: Default::default(),
        template: None,
    }
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
        let text = doc_text(&c);
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
        // Dates and money keep their current format in every language.
        assert!(text.contains("November 1, 2026"), "{lang}: {text}");
        assert!(text.contains("EUR 9,450.00"), "{lang}: {text}");
    }
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
    let v = serde_json::to_value(data::View::new(&inv, None)).unwrap();
    assert_eq!(v["contract_version"], CONTRACT_VERSION);
    assert_eq!(v["language"], "fr");
    assert_eq!(v["labels"]["bill_to"], "Facturé à");
    assert_eq!(v["status"], "pending");
    assert_eq!(v["status_label"], "En attente");
    assert_eq!(v["tax_label"], "TVA");
    assert!(v["logo"].is_null());
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
         #greet(d.to.name) v#d.contract_version #d.labels.bill_to #d.total\n\
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
    assert!(text.contains("EUR 9,450.00"), "{text}");
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
