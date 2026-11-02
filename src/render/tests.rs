use super::*;
use crate::model::{LineItem, Party, Payment, Status};
use chrono::NaiveDate;
use std::time::Instant;

fn sample() -> Invoice {
    Invoice {
        number: "EINV-2026-23".into(),
        date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        due: NaiveDate::from_ymd_opt(2026, 11, 15),
        status: Status::Pending,
        currency: "EUR".into(),
        from: Party {
            name: "Alex Morgan".into(),
            address: vec!["Rue de la Gare".into(), "1725 Luxembourg".into()],
            tax: vec!["VAT ID: LU12345678".into()],
            email: Some("alex@example.com".into()),
            phone: Some("+352 600 000 000".into()),
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
            quantity: 21.0,
            unit: "days".into(),
            rate: 450.0,
        }],
        tax_rate: 0.0,
        tax_label: "VAT".into(),
        notes: vec!["Reverse charge.".into()],
        signature: Some("Alex Morgan".into()),
        accent: "#111827".into(),
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
    inv2.items[0].quantity = 22.0;
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
    assert_eq!(data::format_qty(21.0), "21");
    assert_eq!(data::format_qty(7.5), "7.5");
    assert_eq!(data::format_qty(0.0), "0");
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
    let c = r.compile(&inv).unwrap();
    assert_eq!(c.page_count(), 1);
    assert!(c.pdf().unwrap().starts_with(b"%PDF"));
}
