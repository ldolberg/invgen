use super::*;
use crate::config::DEFAULT_CONFIG;

const EXAMPLE: &str = include_str!("../../examples/config.yaml");

/// Lines of `before` that are not in `after`, and the other way round.
fn diff(before: &str, after: &str) -> (Vec<String>, Vec<String>) {
    let a: Vec<&str> = before.split('\n').collect();
    let b: Vec<&str> = after.split('\n').collect();
    let removed = a
        .iter()
        .filter(|l| !b.contains(l))
        .map(|s| s.to_string())
        .collect();
    let added = b
        .iter()
        .filter(|l| !a.contains(l))
        .map(|s| s.to_string())
        .collect();
    (removed, added)
}

fn client(name: &str) -> ClientConfig {
    ClientConfig {
        party: Party {
            name: name.into(),
            ..Party::default()
        },
        ..ClientConfig::default()
    }
}

fn cfg(text: &str) -> Config {
    parse_config(text).unwrap()
}

#[test]
fn no_edits_is_byte_identical() {
    for text in [
        DEFAULT_CONFIG,
        EXAMPLE,
        "",
        "\n",
        "a: 1",
        "# only a comment\n",
    ] {
        assert_eq!(apply(text, &[]).unwrap(), text);
    }
}

#[test]
fn set_scalar_keeps_comment_column_and_other_lines() {
    let out = apply(
        DEFAULT_CONFIG,
        &[Edit::set(&["number_prefix"], Field::Str("EINV".into()))],
    )
    .unwrap();
    let (removed, added) = diff(DEFAULT_CONFIG, &out);
    assert_eq!(
        removed,
        vec!["number_prefix: INV                   # numbers look like INV-<year>-<seq>"]
    );
    assert_eq!(
        added,
        vec!["number_prefix: EINV                  # numbers look like INV-<year>-<seq>"]
    );
    assert_eq!(out.lines().count(), DEFAULT_CONFIG.lines().count());
}

#[test]
fn long_value_pushes_the_comment_one_space() {
    let text = "tax_rate: 0  # percent\n";
    let out = apply(text, &[Edit::set(&["tax_rate"], Field::Num(16.875))]).unwrap();
    assert_eq!(out, "tax_rate: 16.875 # percent\n");
}

#[test]
fn set_nested_values_in_example() {
    let out = apply(
        EXAMPLE,
        &[
            Edit::set(&["from", "email"], Field::Str("alex@example.org".into())),
            Edit::set(&["clients", "northwind", "rate"], Field::Num(500.0)),
            Edit::set(&["payment", "bic"], Field::Str("DEUTDEFFXXX".into())),
        ],
    )
    .unwrap();
    let (removed, added) = diff(EXAMPLE, &out);
    assert_eq!(removed.len(), 3, "{removed:?}");
    assert_eq!(
        added,
        vec![
            "  email: alex@example.org",
            "  bic: DEUTDEFFXXX",
            "    rate: 500",
        ]
    );
    let c = cfg(&out);
    assert_eq!(c.clients["northwind"].rate, Some(500.0));
    assert_eq!(c.from.email.as_deref(), Some("alex@example.org"));
}

#[test]
fn replacing_a_block_sequence_takes_all_its_lines() {
    // examples/config.yaml writes `from.tax` as a block sequence.
    let out = apply(
        EXAMPLE,
        &[Edit::set(
            &["from", "tax"],
            Field::List(vec!["VAT ID: LU99999999".into()]),
        )],
    )
    .unwrap();
    assert!(
        out.contains("  tax: [\"VAT ID: LU99999999\"]\n  email:"),
        "{out}"
    );
    assert!(!out.contains("R.C.S."), "{out}");
    assert_eq!(cfg(&out).from.tax, vec!["VAT ID: LU99999999"]);
}

#[test]
fn block_sequence_at_the_same_indent_is_part_of_the_key() {
    let text = "from:\n  name: A\n  tax:\n  - one\n  - two\n  email: a@example.com\n";
    let out = apply(
        text,
        &[Edit::set(&["from", "tax"], Field::List(vec!["x".into()]))],
    )
    .unwrap();
    assert_eq!(
        out,
        "from:\n  name: A\n  tax: [x]\n  email: a@example.com\n"
    );
}

#[test]
fn insert_missing_key_at_end_of_block() {
    let out = apply(
        EXAMPLE,
        &[Edit::set(
            &["clients", "northwind", "email"],
            Field::Str("billing@example.com".into()),
        )],
    )
    .unwrap();
    assert!(
        out.ends_with(
            "    description: Senior Software Engineer\n    email: billing@example.com\n"
        ),
        "{out}"
    );
    let (removed, _) = diff(EXAMPLE, &out);
    assert!(removed.is_empty());
}

#[test]
fn insert_goes_after_trailing_comments_of_the_block() {
    // DEFAULT_CONFIG's `from:` ends with a commented `# phone:` line.
    let out = apply(
        DEFAULT_CONFIG,
        &[Edit::set(&["from", "phone"], Field::Str("+00 111".into()))],
    )
    .unwrap();
    assert!(
        out.contains("  # phone: \"+00 000 000 000\"\n  phone: +00 111\n\npayment:"),
        "{out}"
    );
}

#[test]
fn insert_new_client_with_all_fields() {
    let mut c = client("Contoso GmbH");
    c.party.address = vec!["Hauptstrasse 1".into(), "10115 Berlin".into()];
    c.party.tax = vec!["USt-IdNr.: DE123456789".into()];
    c.party.email = Some("ap@example.com".into());
    c.currency = Some("EUR".into());
    c.tax_rate = Some(19.0);
    c.rate = Some(95.5);
    c.unit = Some("hours".into());
    c.description = Some("Consulting: backend".into());
    let out = upsert_client(EXAMPLE, None, "contoso", &c).unwrap();
    assert!(out.starts_with(EXAMPLE), "existing text untouched:\n{out}");
    let tail = &out[EXAMPLE.len()..];
    assert_eq!(
        tail,
        "  contoso:\n    name: Contoso GmbH\n    address: [Hauptstrasse 1, 10115 Berlin]\n    \
         tax: [\"USt-IdNr.: DE123456789\"]\n    email: ap@example.com\n    currency: EUR\n    \
         tax_rate: 19\n    rate: 95.5\n    unit: hours\n    description: \"Consulting: backend\"\n"
    );
    let parsed = cfg(&out);
    let back = &parsed.clients["contoso"];
    assert_eq!(back.party, c.party);
    assert_eq!(back.rate, Some(95.5));
    assert_eq!(back.description, c.description);
    // Round trip: writing the same values again changes nothing.
    assert_eq!(
        upsert_client(&out, Some("contoso"), "contoso", &c).unwrap(),
        out
    );
}

#[test]
fn unchanged_fields_are_not_rewritten() {
    // `rate: 450` stays as written even though the form would say 450.0.
    let current = cfg(EXAMPLE).clients["northwind"].clone();
    let mut c = current.clone();
    c.unit = Some("hours".into());
    let out = upsert_client(EXAMPLE, Some("northwind"), "northwind", &c).unwrap();
    let (removed, added) = diff(EXAMPLE, &out);
    assert_eq!(removed, vec!["    unit: days"]);
    assert_eq!(added, vec!["    unit: hours"]);
    // Same values: byte identical.
    assert_eq!(
        upsert_client(EXAMPLE, Some("northwind"), "northwind", &current).unwrap(),
        EXAMPLE
    );
}

#[test]
fn clearing_optional_fields_removes_their_lines() {
    let mut c = cfg(DEFAULT_CONFIG).clients["acme"].clone();
    c.rate = None;
    c.party.tax.clear();
    let out = upsert_client(DEFAULT_CONFIG, Some("acme"), "acme", &c).unwrap();
    let (removed, added) = diff(DEFAULT_CONFIG, &out);
    assert!(added.is_empty(), "{added:?}");
    assert_eq!(
        removed,
        vec![
            "    tax: [\"Tax ID: X00000000\"]",
            "    rate: 450            # default rate used by `invgen new`",
        ]
    );
    // The commented examples inside the block stay.
    assert!(out.contains("    # currency: EUR      # optional per-client defaults"));
}

#[test]
fn rename_client_in_place() {
    let c = cfg(DEFAULT_CONFIG).clients["acme"].clone();
    let out = upsert_client(DEFAULT_CONFIG, Some("acme"), "acme-corp", &c).unwrap();
    let (removed, added) = diff(DEFAULT_CONFIG, &out);
    assert_eq!(removed, vec!["  acme:"]);
    assert_eq!(added, vec!["  acme-corp:"]);
    let parsed = cfg(&out);
    assert!(parsed.clients.contains_key("acme-corp"));
    assert!(!parsed.clients.contains_key("acme"));
}

#[test]
fn rename_onto_existing_key_is_refused() {
    let text = upsert_client(DEFAULT_CONFIG, None, "other", &client("Other")).unwrap();
    let c = cfg(&text).clients["acme"].clone();
    let err = upsert_client(&text, Some("acme"), "other", &c).unwrap_err();
    assert!(matches!(err, EditError::Invalid(_)), "{err}");
    let err = upsert_client(&text, None, "acme", &client("Again")).unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
}

#[test]
fn delete_client_removes_its_block_only() {
    let two = upsert_client(EXAMPLE, None, "contoso", &client("Contoso")).unwrap();
    let out = delete_client(&two, "northwind").unwrap();
    assert!(!out.contains("Northwind"), "{out}");
    assert!(
        out.contains("clients:\n  contoso:\n    name: Contoso\n"),
        "{out}"
    );
    // Everything above `clients:` is byte identical.
    let head = &EXAMPLE[..EXAMPLE.find("clients:").unwrap()];
    assert!(out.starts_with(head));
    // Deleting the last client leaves an empty mapping, not a null.
    let none = delete_client(&out, "contoso").unwrap();
    assert!(none.ends_with("clients: {}\n"), "{none}");
    assert!(cfg(&none).clients.is_empty());
    // And adding one to `clients: {}` turns it back into a block.
    let again = upsert_client(&none, None, "acme", &client("ACME")).unwrap();
    assert!(
        again.ends_with("clients:\n  acme:\n    name: ACME\n"),
        "{again}"
    );
}

#[test]
fn delete_takes_comments_inside_the_block() {
    let text = "clients:\n  a:\n    name: A\n    # note about a\n    rate: 1\n    # trailing note\n  b:\n    name: B\n# footer\n";
    let out = delete_client(text, "a").unwrap();
    assert_eq!(out, "clients:\n  b:\n    name: B\n# footer\n");
}

#[test]
fn comments_between_entries_do_not_end_a_block() {
    let text = "from:\n  name: A\n# a comment at column 0 inside from\n  email: a@example.com\n";
    let out = apply(
        text,
        &[Edit::set(
            &["from", "email"],
            Field::Str("b@example.com".into()),
        )],
    )
    .unwrap();
    assert_eq!(
        out,
        "from:\n  name: A\n# a comment at column 0 inside from\n  email: b@example.com\n"
    );
}

#[test]
fn creates_missing_sections() {
    let out = apply(
        "",
        &[Edit::set(
            &["clients", "acme", "name"],
            Field::Str("ACME".into()),
        )],
    )
    .unwrap();
    assert_eq!(out, "clients:\n  acme:\n    name: ACME\n");
    let text = "# my config\ncurrency: USD\n";
    let out = upsert_client(text, None, "acme", &client("ACME")).unwrap();
    assert_eq!(
        out,
        "# my config\ncurrency: USD\n\nclients:\n  acme:\n    name: ACME\n"
    );
    // No trailing newline in the input.
    let out = apply(
        "currency: USD",
        &[Edit::set(&["from", "name"], Field::Str("Me".into()))],
    )
    .unwrap();
    assert_eq!(out, "currency: USD\n\nfrom:\n  name: Me");
}

#[test]
fn new_top_level_scalar_goes_before_the_blocks() {
    let text = "currency: EUR\n\nfrom:\n  name: A\n";
    let out = apply(text, &[Edit::set(&["due_days"], Field::Num(30.0))]).unwrap();
    assert_eq!(out, "currency: EUR\ndue_days: 30\n\nfrom:\n  name: A\n");
}

#[test]
fn follows_the_documents_indentation() {
    let text = "clients:\n    acme:\n        name: ACME\n";
    let out = upsert_client(text, None, "beta", &client("Beta")).unwrap();
    assert_eq!(out, format!("{text}    beta:\n        name: Beta\n"));
    let mut c = client("ACME");
    c.rate = Some(1.0);
    let out = upsert_client(text, Some("acme"), "acme", &c).unwrap();
    assert_eq!(out, format!("{text}        rate: 1\n"));
}

#[test]
fn crlf_line_endings_are_kept() {
    let text = "currency: EUR\r\nclients:\r\n  acme:\r\n    name: ACME\r\n";
    let mut c = client("ACME");
    c.rate = Some(10.0);
    let out = upsert_client(text, Some("acme"), "acme", &c).unwrap();
    assert_eq!(out, format!("{text}    rate: 10\r\n"));
}

#[test]
fn quoted_keys_and_values_are_understood() {
    let text = "\"clients\":\n  'acme':\n    name: 'ACME # not a comment'   # a comment\n";
    let mut c = client("ACME Two");
    c.rate = None;
    let out = upsert_client(text, Some("acme"), "acme", &c).unwrap();
    assert_eq!(
        out,
        "\"clients\":\n  'acme':\n    name: ACME Two                 # a comment\n"
    );
    let out = upsert_client(text, Some("acme"), "acme2", &cfg(text).clients["acme"]).unwrap();
    assert!(out.contains("  acme2:\n"), "{out}");
}

#[test]
fn values_are_quoted_when_needed() {
    let cases = [
        ("plain text", "plain text"),
        ("00000", "\"00000\""),
        ("true", "\"true\""),
        ("null", "\"null\""),
        ("12.5", "\"12.5\""),
        ("a: b", "\"a: b\""),
        ("x #y", "\"x #y\""),
        ("#tag", "\"#tag\""),
        ("- dash", "\"- dash\""),
        ("say \"hi\"", "say \"hi\""),
        ("back\\slash", "back\\slash"),
        ("  padded", "\"  padded\""),
        ("O'Brien", "O'Brien"),
        ("", "\"\""),
        ("2026-01-01", "\"2026-01-01\""),
        ("yes", "\"yes\""),
        ("Off", "\"Off\""),
        ("0x1F", "\"0x1F\""),
        ("12 Main St", "12 Main St"),
    ];
    for (input, want) in cases {
        let out = apply("k: 0\n", &[Edit::set(&["k"], Field::Str(input.into()))]).unwrap();
        assert_eq!(out, format!("k: {want}\n"), "{input:?}");
        let back: Value = serde_yaml_ng::from_str(&out).unwrap();
        assert_eq!(back["k"], Value::String(input.into()), "{input:?}");
    }
    let list = vec![
        "Calle Mayor, 12".to_string(),
        "[x]".into(),
        "plain".into(),
        "00000".into(),
        "line\nbreak".into(),
    ];
    let out = apply("k: 0\n", &[Edit::set(&["k"], Field::List(list.clone()))]).unwrap();
    assert_eq!(
        out,
        "k: [\"Calle Mayor, 12\", \"[x]\", plain, \"00000\", \"line\\nbreak\"]\n"
    );
    let back: Vec<String> =
        serde_yaml_ng::from_value(serde_yaml_ng::from_str::<Value>(&out).unwrap()["k"].clone())
            .unwrap();
    assert_eq!(back, list);
}

#[test]
fn numbers_render_compactly() {
    for (n, want) in [
        (450.0, "450"),
        (0.0, "0"),
        (12.5, "12.5"),
        (-3.0, "-3"),
        (0.1, "0.1"),
    ] {
        let out = apply("k: 1\n", &[Edit::set(&["k"], Field::Num(n))]).unwrap();
        assert_eq!(out, format!("k: {want}\n"));
    }
    assert!(apply("k: 1\n", &[Edit::set(&["k"], Field::Num(f64::NAN))]).is_err());
}

#[test]
fn flow_mapping_target_is_unsupported() {
    let text = "from: {name: A, email: a@example.com}  # inline\nclients: {acme: {name: ACME}}\n";
    let err = set_from(
        text,
        &Party {
            name: "B".into(),
            email: Some("a@example.com".into()),
            ..Party::default()
        },
    )
    .unwrap_err();
    assert!(matches!(err, EditError::Unsupported(_)), "{err}");
    assert!(err.to_string().contains("flow mapping"), "{err}");
    let err = upsert_client(text, None, "beta", &client("Beta")).unwrap_err();
    assert!(matches!(err, EditError::Unsupported(_)), "{err}");
}

#[test]
fn empty_flow_mapping_and_null_become_blocks() {
    let out = upsert_client("clients: {}   # none yet\n", None, "a", &client("A")).unwrap();
    assert_eq!(out, "clients:      # none yet\n  a:\n    name: A\n");
    // `clients:` with nothing under it is null, which Config itself rejects,
    // but the line editor still turns it into a block.
    let set = [Edit::set(&["clients", "a", "name"], Field::Str("A".into()))];
    for text in ["clients:\n", "clients: ~\n", "clients: null\n"] {
        assert_eq!(apply(text, &set).unwrap(), "clients:\n  a:\n    name: A\n");
    }
}

#[test]
fn anchors_and_merge_keys_are_unsupported() {
    let text = "base: &base\n  currency: EUR\nclients:\n  acme:\n    <<: *base\n    name: ACME\n";
    let mut c = client("ACME 2");
    c.currency = Some("EUR".into());
    let err = upsert_client(text, Some("acme"), "acme", &c).unwrap_err();
    assert!(matches!(err, EditError::Unsupported(_)), "{err}");

    let text = "clients: &c\n  acme:\n    name: ACME\nother: *c\n";
    let err = upsert_client(text, None, "beta", &client("Beta")).unwrap_err();
    assert!(matches!(err, EditError::Unsupported(_)), "{err}");
}

#[test]
fn odd_documents_are_unsupported_not_mangled() {
    for text in [
        "\tfrom:\n\t  name: A\n",
        "%YAML 1.2\n---\nfrom:\n  name: A\n",
        "from:\n  name: A\n---\nfrom:\n  name: B\n",
    ] {
        let res = set_from(
            text,
            &Party {
                name: "Z".into(),
                ..Party::default()
            },
        );
        assert!(res.is_err(), "{text:?} gave {res:?}");
    }
    // An indented document and a leading `---` are fine.
    let out = set_from(
        "  from:\n    name: A\n",
        &Party {
            name: "Z".into(),
            ..Party::default()
        },
    )
    .unwrap();
    assert_eq!(out, "  from:\n    name: Z\n");
    let out = set_from(
        "---\nfrom:\n  name: A\n",
        &Party {
            name: "Z".into(),
            ..Party::default()
        },
    )
    .unwrap();
    assert_eq!(out, "---\nfrom:\n  name: Z\n");
}

#[test]
fn multiline_values_are_replaced_whole() {
    let text = "from:\n  name: A\n  address: [Street 1,\n    City]\n  email: a@example.com\n";
    let out = apply(
        text,
        &[Edit::set(
            &["from", "address"],
            Field::List(vec!["X".into()]),
        )],
    )
    .unwrap();
    assert_eq!(
        out,
        "from:\n  name: A\n  address: [X]\n  email: a@example.com\n"
    );
}

#[test]
fn block_scalar_content_is_not_mistaken_for_keys() {
    let text =
        "from:\n  name: A\n  notes: |\n    email: not a key\n    more\n  email: a@example.com\n";
    let out = apply(
        text,
        &[Edit::set(
            &["from", "email"],
            Field::Str("b@example.com".into()),
        )],
    )
    .unwrap();
    assert_eq!(out, text.replace("a@example.com", "b@example.com"));
}

#[test]
fn unknown_keys_are_kept() {
    let text = "clients:\n  acme:\n    name: ACME\n    po_number: 42  # used by my scripts\n";
    let mut c = client("ACME Inc");
    c.rate = Some(1.0);
    let out = upsert_client(text, Some("acme"), "acme", &c).unwrap();
    assert_eq!(
        out,
        "clients:\n  acme:\n    name: ACME Inc\n    po_number: 42  # used by my scripts\n    rate: 1\n"
    );
}

#[test]
fn invalid_result_is_refused() {
    // tax_rate must be a number for Config.
    let err = apply(
        "k: 1\n",
        &[Edit::set(&["tax_rate"], Field::Str("abc".into()))],
    )
    .unwrap_err();
    assert!(matches!(err, EditError::Invalid(_)), "{err}");
    // A broken file is reported, not edited.
    let err = upsert_client("clients: [\n", None, "a", &client("A")).unwrap_err();
    assert!(matches!(err, EditError::Invalid(_)), "{err}");
}

#[test]
fn request_validation() {
    assert!(upsert_client(DEFAULT_CONFIG, None, "bad key", &client("X")).is_err());
    assert!(upsert_client(DEFAULT_CONFIG, None, "new", &client("  ")).is_err());
    assert!(delete_client(DEFAULT_CONFIG, "missing").is_err());
    assert!(upsert_client(DEFAULT_CONFIG, Some("missing"), "x", &client("X")).is_err());
}

#[test]
fn client_keys() {
    for ok in ["acme", "northwind-labs", "a_b", "x.y", "Acme2", "2acme"] {
        assert!(valid_client_key(ok).is_ok(), "{ok}");
    }
    for bad in [
        "", "-a", "a b", "a:b", "true", "null", "123", "1.5", "~", "ñandú",
    ] {
        assert!(valid_client_key(bad).is_err(), "{bad}");
    }
    assert_eq!(suggest_client_key("Northwind Labs SL"), "northwind-labs-sl");
    assert_eq!(suggest_client_key("  ACME, Inc. "), "acme-inc");
    assert_eq!(suggest_client_key(""), "client");
    assert_eq!(suggest_client_key("123"), "client-123");
    assert_eq!(suggest_client_key("Zoë & Co"), "zo-co");
}

#[test]
fn round_trip_every_client_field_through_default_config() {
    // Start from the template, rewrite the sample client with every field,
    // then back: the text returns to the original byte for byte.
    let orig = cfg(DEFAULT_CONFIG).clients["acme"].clone();
    let mut c = orig.clone();
    c.party.name = "Changed".into();
    c.party.address = vec!["A".into()];
    c.party.email = Some("x@example.com".into());
    c.unit = Some("hours".into());
    let changed = upsert_client(DEFAULT_CONFIG, Some("acme"), "acme", &c).unwrap();
    assert_ne!(changed, DEFAULT_CONFIG);
    let mut back = c.clone();
    back.party = orig.party.clone();
    back.unit = orig.unit.clone();
    let restored = upsert_client(&changed, Some("acme"), "acme", &back).unwrap();
    // `email` was added at the end and removed again; the rest is in place.
    assert_eq!(restored, DEFAULT_CONFIG);
}

#[test]
fn set_payment_and_from() {
    let p = Payment {
        holder: Some("ALEX MORGAN".into()),
        iban: Some("DE89 3704 0044 0532 0130 00".into()),
        bic: None,
        notes: vec!["Please quote the invoice number".into()],
    };
    let out = set_payment(DEFAULT_CONFIG, &p).unwrap();
    assert!(
        out.contains(
            "payment:\n  holder: ALEX MORGAN\n  iban: DE89 3704 0044 0532 0130 00\n  \
             # notes: [\"Bank name\", \"Please quote the invoice number\"]\n  \
             notes: [Please quote the invoice number]\n"
        ),
        "{out}"
    );
    assert_eq!(cfg(&out).payment, p);
}

#[test]
fn atomic_write_keeps_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/config.yaml");
    write_atomic(&path, "a: 1\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a: 1\n");
    assert!(!backup_path(&path).exists());
    write_atomic(&path, "a: 2\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a: 2\n");
    assert_eq!(
        std::fs::read_to_string(backup_path(&path)).unwrap(),
        "a: 1\n"
    );
    // No temp files left behind.
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn every_single_field_edit_touches_one_line() {
    // For each existing scalar or list in the example and the template,
    // setting a new value rewrites exactly that line and nothing else.
    let fields: [(&[&str], Field); 13] = [
        (&["invoices_dir"], Field::Str("~/Invoices".into())),
        (&["number_prefix"], Field::Str("ACME".into())),
        (&["currency"], Field::Str("USD".into())),
        (&["tax_rate"], Field::Num(21.0)),
        (&["due_days"], Field::Num(30.0)),
        (&["from", "name"], Field::Str("Sam Doe".into())),
        (&["from", "address"], Field::List(vec!["Main St 1".into()])),
        (&["from", "email"], Field::Str("sam@example.com".into())),
        (&["payment", "holder"], Field::Str("SAM DOE".into())),
        (
            &["payment", "iban"],
            Field::Str("GB82 WEST 1234 5698 7654 32".into()),
        ),
        (&["payment", "bic"], Field::Str("WESTGB2L".into())),
        (&["clients", "CLIENT", "rate"], Field::Num(99.5)),
        (
            &["clients", "CLIENT", "description"],
            Field::Str("Audit".into()),
        ),
    ];
    for (text, client) in [(EXAMPLE, "northwind"), (DEFAULT_CONFIG, "acme")] {
        for (path, field) in &fields {
            let path: Vec<&str> = path
                .iter()
                .map(|p| if *p == "CLIENT" { client } else { p })
                .collect();
            let out = apply(text, &[Edit::set(&path, field.clone())]).unwrap();
            let (removed, added) = diff(text, &out);
            assert_eq!((removed.len(), added.len()), (1, 1), "{path:?}\n{out}");
            assert_eq!(out.lines().count(), text.lines().count(), "{path:?}");
            // The comment of that line, if any, is still there.
            if let Some(i) = removed[0].find(" #") {
                let comment = removed[0][i..].trim_start();
                assert!(added[0].ends_with(comment), "{added:?}");
            }
            assert_ne!(parse_value(text).unwrap(), parse_value(&out).unwrap());
        }
    }
}
