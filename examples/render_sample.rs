//! Visual check for the renderer.
//!
//! cargo run --release --example render_sample -- <out-dir>
//!
//! Writes, for each scenario (basic, long, paid): <name>.pdf and
//! <name>-<page>.png (at 2 px/pt), and prints timings.

use std::path::PathBuf;
use std::time::Instant;

use chrono::NaiveDate;
use invgen::model::{Invoice, LineItem, Party, Payment, Status};
use invgen::render::Renderer;

fn base() -> Invoice {
    Invoice {
        number: "EINV-2026-23".into(),
        date: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        due: NaiveDate::from_ymd_opt(2026, 11, 15),
        status: Status::Pending,
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
        },
        payment: Payment {
            holder: Some("ALEX MORGAN".into()),
            iban: Some("DE89 3704 0044 0532 0130 00".into()),
            bic: Some("COBADEFFXXX".into()),
            notes: vec![],
        },
        items: vec![LineItem {
            description: "Senior Software Engineer".into(),
            detail: Some("October 2026 period".into()),
            quantity: 21.0,
            unit: "days".into(),
            rate: 450.0,
        }],
        tax_rate: 0.0,
        tax_label: "VAT".into(),
        notes: vec![
            "Reverse charge: VAT to be accounted for by the recipient (Art. 196, Directive 2006/112/EC).".into(),
            "Please include the invoice number as the transfer reference.".into(),
        ],
        signature: Some("Alex Morgan".into()),
        accent: "#111827".into(),
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
            quantity: (i % 5 + 1) as f64 * 1.5,
            unit: "hours".into(),
            rate: 85.0 + i as f64,
        })
        .collect();
    inv
}

fn paid() -> Invoice {
    let mut inv = base();
    inv.number = "EINV-2026-25".into();
    inv.status = Status::Paid;
    inv.tax_rate = 21.0;
    inv.tax_label = "IVA".into();
    inv.accent = "#0f766e".into();
    inv.due = None;
    inv.notes = vec![];
    inv.items.push(LineItem {
        description: "Architecture review workshop".into(),
        detail: None,
        quantity: 2.0,
        unit: "days".into(),
        rate: 600.0,
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
    for q in [22.0, 23.0, 24.0] {
        inv.items[0].quantity = q;
        let t = Instant::now();
        let c = r.compile(&inv)?;
        let tc = t.elapsed();
        let t = Instant::now();
        c.render_pages(2.0);
        println!("warm edit: compile {tc:?}, render {:?}", t.elapsed());
    }
    Ok(())
}
