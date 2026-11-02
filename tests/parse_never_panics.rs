//! `parse_invoice` runs on every keystroke in the editor, so it must return a
//! `ParseError` (never panic) on any input. This is the stable-toolchain
//! counterpart of the `fuzz/` target: it builds markdown-like documents from
//! invoice-shaped fragments so it reaches deep into the parser quickly.

use invgen::config::Config;
use invgen::parse::parse_invoice;
use proptest::prelude::*;
use std::sync::OnceLock;

/// The fictional example config, never the user's real one.
fn config() -> &'static Config {
    static CFG: OnceLock<Config> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut cfg: Config = serde_yaml_ng::from_str(include_str!("../examples/config.yaml"))
            .expect("examples/config.yaml parses");
        cfg.invoices_dir = std::env::temp_dir().join("invgen-proptest-nonexistent-dir");
        cfg
    })
}

/// Numbers as people type them, plus the awkward ones.
fn number() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<i64>().prop_map(|n| n.to_string()),
        any::<f64>().prop_map(|n| n.to_string()),
        any::<u128>().prop_map(|n| n.to_string()),
        "[-+]?[0-9]{0,4}(,[0-9]{0,4}){0,3}(\\.[0-9]{0,3})?",
        "[0-9]{30,400}(\\.[0-9]{0,40})?",
        "(€|\\$|£|¥|EUR |USD )?[-+]?[0-9,.]{0,12}( EUR| CHF| %)?",
        Just("1e308".to_string()),
        Just("-1e309".to_string()),
        Just(".inf".to_string()),
        Just(".nan".to_string()),
        Just("NaN".to_string()),
        Just("0x1F".to_string()),
        Just("".to_string()),
        Just("-".to_string()),
        Just(".".to_string()),
        Just(",".to_string()),
    ]
}

/// Text with a bias towards characters that are special to the parser.
fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z ]{0,12}",
        "[:@x|#\\-*`\\[\\]()%,.0-9 ]{0,12}",
        "\\PC{0,8}",
        Just("é€😀\u{feff}\u{200b}\u{0301}".to_string()),
    ]
}

fn yaml_scalar() -> impl Strategy<Value = String> {
    prop_oneof![
        number(),
        text(),
        Just("null".to_string()),
        Just("~".to_string()),
        Just("true".to_string()),
        Just("false".to_string()),
        Just("none".to_string()),
        Just("[]".to_string()),
        Just("{}".to_string()),
        Just("[1, [2, [3]]]".to_string()),
        Just("\"#0af\"".to_string()),
        Just("2026-02-30".to_string()),
        Just("262143-12-31".to_string()),
        Just("-9999-01-01".to_string()),
        Just("&a [*a]".to_string()),
        Just("*nope".to_string()),
        Just("!!binary aGk=".to_string()),
        Just("|\n  block".to_string()),
    ]
}

const KEYS: &[&str] = &[
    "number",
    "date",
    "due",
    "due_days",
    "status",
    "client",
    "currency",
    "tax_rate",
    "tax_label",
    "accent",
    "signature",
    "from",
    "to",
    "payment",
    "config",
    "name",
    "address",
    "clients",
    "northwind",
    "invoices_dir",
    "number_prefix",
    "rate",
    "<<",
    "? ",
];

fn yaml_line() -> impl Strategy<Value = String> {
    (
        0usize..4,
        prop::sample::select(KEYS),
        prop::bool::ANY,
        yaml_scalar(),
    )
        .prop_map(|(indent, key, nested, value)| {
            let pad = "  ".repeat(indent);
            if nested {
                format!("{pad}{key}:")
            } else {
                format!("{pad}{key}: {value}")
            }
        })
}

fn front_matter() -> impl Strategy<Value = String> {
    let fence = prop_oneof![
        Just("---"),
        Just("..."),
        Just("--- "),
        Just("----"),
        Just(""),
    ];
    (
        prop::bool::ANY,
        prop::collection::vec(yaml_line(), 0..12),
        fence,
    )
        .prop_map(|(open, lines, close)| {
            let mut s = String::new();
            if open {
                s.push_str("---\n");
            }
            for l in lines {
                s.push_str(&l);
                s.push('\n');
            }
            s.push_str(close);
            s.push('\n');
            s
        })
}

fn table() -> impl Strategy<Value = String> {
    let header = prop::sample::select(
        &[
            "Description",
            "Item",
            "Qty",
            "Days",
            "Hours",
            "Unit",
            "Rate (EUR)",
            "Price",
            "Detail",
            "",
            "(",
        ][..],
    );
    (
        prop::collection::vec(header, 0..7),
        prop::collection::vec(
            prop::collection::vec(prop_oneof![number(), text()], 0..8),
            0..6,
        ),
    )
        .prop_map(|(header, rows)| {
            let mut s = format!("| {} |\n", header.join(" | "));
            s.push_str(&format!("|{}\n", "---|".repeat(header.len().max(1))));
            for row in rows {
                s.push_str(&format!("| {} |\n", row.join(" | ")));
            }
            s
        })
}

fn list_item() -> impl Strategy<Value = String> {
    (
        text(),
        number(),
        text(),
        prop_oneof![Just(" @ "), Just(" x "), Just("@"), Just(" X ")],
        number(),
    )
        .prop_map(|(desc, qty, unit, sep, rate)| format!("{desc}: {qty} {unit}{sep}{rate}"))
}

fn list() -> impl Strategy<Value = String> {
    (
        prop::collection::vec((0usize..6, list_item()), 1..8),
        prop_oneof![Just("- "), Just("* "), Just("1. "), Just("+ ")],
    )
        .prop_map(|(items, bullet)| {
            items
                .into_iter()
                .map(|(depth, item)| format!("{}{bullet}{item}\n", "  ".repeat(depth)))
                .collect()
        })
}

fn block() -> impl Strategy<Value = String> {
    prop_oneof![
        table(),
        list(),
        text().prop_map(|t| format!("{t}\n")),
        text().prop_map(|t| format!("# {t}\n")),
        Just("## Notes\n".to_string()),
        Just("---\n".to_string()),
        Just("<!-- comment -->\n".to_string()),
        Just("```\ncode\n".to_string()),
        Just("> quote\n> - A: 1 @ 1\n".to_string()),
        (1usize..200).prop_map(|n| format!("{}- A: 1 @ 1\n", "> ".repeat(n))),
        (1usize..100).prop_map(|n| format!("{}A: 1 @ 1\n", "- ".repeat(n))),
        (1usize..100).prop_map(|n| format!("{}A: 1 @ 1{}\n", "[".repeat(n), "]".repeat(n))),
    ]
}

fn document() -> impl Strategy<Value = String> {
    (
        prop::bool::ANY,
        front_matter(),
        prop::collection::vec(block(), 0..8),
        prop_oneof![Just("\n"), Just("\r\n"), Just("\r")],
    )
        .prop_map(|(bom, fm, blocks, eol)| {
            let mut s = String::new();
            if bom {
                s.push('\u{feff}');
            }
            s.push_str(&fm);
            for b in blocks {
                s.push('\n');
                s.push_str(&b);
            }
            s.replace('\n', eol)
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, ..ProptestConfig::default() })]

    #[test]
    fn markdown_like_input_never_panics(src in document()) {
        let _ = parse_invoice(&src, config());
    }

    #[test]
    fn arbitrary_input_never_panics(src in "\\PC*") {
        let _ = parse_invoice(&src, config());
    }

    #[test]
    fn mutated_examples_never_panic(
        which in 0usize..3,
        at in any::<prop::sample::Index>(),
        insert in "\\PC{0,16}",
        cut in 0usize..16,
    ) {
        let seeds = [
            include_str!("../examples/invoices/EINV-2026-20.md"),
            include_str!("../examples/invoices/EINV-2026-21.md"),
            include_str!("../examples/invoices/EINV-2026-22.md"),
        ];
        let seed = seeds[which];
        let mut i = at.index(seed.len() + 1);
        while !seed.is_char_boundary(i) {
            i -= 1;
        }
        let mut j = (i + cut).min(seed.len());
        while !seed.is_char_boundary(j) {
            j -= 1;
        }
        let src = format!("{}{}{}", &seed[..i], insert, &seed[j..]);
        let _ = parse_invoice(&src, config());
    }
}
