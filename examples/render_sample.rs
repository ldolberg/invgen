//! Visual check for the renderer.
//!
//! cargo run --release --example render_sample -- <out-dir>
//!
//! Writes, for each scenario (basic, long, paid): <name>.pdf and
//! <name>-<page>.png (at 2 px/pt), and prints timings.

use std::path::PathBuf;
use std::time::Instant;

use chrono::NaiveDate;
use invgen::model::{Decimal, Invoice, Kind, LineItem, Party, Payment, Status};
use invgen::render::Renderer;

fn base() -> Invoice {
    Invoice {
        number: "EINV-2026-23".into(),
        date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        due: NaiveDate::from_ymd_opt(2026, 11, 15),
        status: Status::Pending,
        paid_on: None,
        currency: "EUR".into(),
        from: Party {
            name: "Alex Morgan".into(),
            address: vec![
                "Rue de la Gare".into(),
                "Luxembourg".into(),
                "1616".into(),
                "Luxembourg".into(),
            ],
            tax: vec![
                "VAT ID: LU12345678".into(),
                "Tax ID: R.C.S. Luxembourg B000000 Business Permit 00000000/0".into(),
            ],
            email: Some("alex@example.com".into()),
            phone: Some("+352 600 000 000".into()),
            logo: None,
            ..Party::default()
        },
        to: Party {
            name: "Northwind Labs SL".into(),
            address: vec![
                "Calle Mayor, 12".into(),
                "Madrid".into(),
                "28001".into(),
                "Spain".into(),
            ],
            tax: vec!["Tax ID: B12345678".into()],
            email: None,
            phone: None,
            logo: None,
            ..Party::default()
        },
        payment: Payment {
            holder: Some("ALEX MORGAN".into()),
            iban: Some("DE89 3704 0044 0532 0130 00".into()),
            bic: Some("COBADEFFXXX".into()),
            notes: vec![],
            qr: true,
        },
        items: vec![LineItem {
            description: "Senior Software Engineer".into(),
            detail: Some("October 2026 period".into()),
            quantity: Decimal::from(21),
            unit: "days".into(),
            rate: Decimal::from(450),
            tax_rate: None,
        }],
        tax_rate: Decimal::ZERO,
        tax_label: "VAT".into(),
        tax_note: None,
        notes: vec![
            "Reverse charge: VAT to be accounted for by the recipient (Art. 196, Directive 2006/112/EC).".into(),
            "Please include the invoice number as the transfer reference.".into(),
        ],
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

fn long() -> Invoice {
    let mut inv = base();
    inv.number = "EINV-2026-24".into();
    inv.status = Status::Overdue;
    inv.accent = "#1d4ed8".into();
    inv.items = (1..=25)
        .map(|i| LineItem {
            description: format!("Engineering work package {i}"),
            detail: (i % 3 == 0).then(|| format!("Sprint {i}: backend, reviews and on-call")),
            quantity: Decimal::from(i % 5 + 1) * Decimal::new(15, 1),
            unit: "hours".into(),
            rate: Decimal::from(85 + i),
            tax_rate: None,
        })
        .collect();
    inv
}

fn paid() -> Invoice {
    let mut inv = base();
    inv.number = "EINV-2026-25".into();
    inv.status = Status::Paid;
    inv.tax_rate = Decimal::from(21);
    inv.tax_label = "IVA".into();
    inv.language = "es".into();
    inv.from.logo = Some(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/logo.svg"
    )));
    inv.accent = "#0f766e".into();
    inv.due = None;
    inv.notes = vec![];
    inv.items.push(LineItem {
        description: "Architecture review workshop".into(),
        detail: None,
        quantity: Decimal::from(2),
        unit: "days".into(),
        rate: Decimal::from(600),
        tax_rate: None,
    });
    inv
}

fn main() -> anyhow::Result<()> {
    let out: PathBuf = std::env::args().nth(1).unwrap_or_else(|| ".".into()).into();
    std::fs::create_dir_all(&out)?;

    let t = Instant::now();
    let r = Renderer::new()?;
    println!("Renderer::new: {:?}", t.elapsed());

    for (name, inv) in [("basic", base()), ("long", long()), ("paid", paid())] {
        let t = Instant::now();
        let c = r.compile(&inv)?;
        let tc = t.elapsed();
        let t = Instant::now();
        let pages = c.render_pages(2.0);
        let tr = t.elapsed();
        let t = Instant::now();
        let pdf = c.pdf()?;
        let tp = t.elapsed();
        std::fs::write(out.join(format!("{name}.pdf")), &pdf)?;
        for (i, p) in pages.iter().enumerate() {
            let img = image::RgbaImage::from_raw(p.width, p.height, p.rgba.clone())
                .expect("buffer size matches");
            img.save(out.join(format!("{name}-{}.png", i + 1)))?;
        }
        println!(
            "{name}: {} page(s); compile {tc:?}, render {tr:?}, pdf {tp:?}",
            c.page_count()
        );
    }

    // Warm recompile after a small edit, as the live preview does.
    let mut inv = base();
    for q in [22, 23, 24] {
        inv.items[0].quantity = Decimal::from(q);
        let t = Instant::now();
        let c = r.compile(&inv)?;
        let tc = t.elapsed();
        let t = Instant::now();
        c.render_pages(2.0);
        println!("warm edit: compile {tc:?}, render {:?}", t.elapsed());
    }
    Ok(())
}
