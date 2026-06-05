use saneyaml::{
    BlockScalarStyle, EmitCollectionStyle, EmitOptions, KeyOrder, Node, NodeValue as Value, Number,
    ScalarQuoteStyle, Span, Tag, TaggedNode, parse_str, to_string, to_string_with_options,
};
use std::collections::BTreeMap;

fn nested_sequence(depth: usize) -> Node {
    let mut node = Node::null(Span::default());
    for _ in 0..depth {
        node = Node::new(Value::Sequence(vec![node]), Span::default());
    }
    node
}

fn nested_mapping_key(depth: usize) -> Node {
    let mut node = Node::null(Span::default());
    for _ in 0..depth {
        node = Node::new(
            Value::Mapping(vec![(node, Node::null(Span::default()))]),
            Span::default(),
        );
    }
    node
}

fn tagged_node(tag: &str, value: Node) -> Node {
    Node::new(
        Value::Tagged(Box::new(TaggedNode {
            tag: Tag::new(tag),
            tag_span: Span::default(),
            value,
        })),
        Span::default(),
    )
}

fn string_node(value: &str) -> Node {
    Node::new(Value::String(value.to_string()), Span::default())
}

fn int_node(value: i128) -> Node {
    Node::new(Value::Number(Number::Integer(value)), Span::default())
}

fn float_node(value: f64) -> Node {
    Node::new(Value::Number(Number::Float(value)), Span::default())
}

#[test]
fn emitter_options_default_to_structural_output() {
    assert_eq!(EmitOptions::default(), EmitOptions::structural());

    let node = parse_str("service:\n  image: app:v1\n  replicas: 2\n").expect("parse");
    let default = to_string(&node).expect("default emit");
    let structural =
        to_string_with_options(&node, EmitOptions::structural()).expect("structural emit");

    assert_eq!(structural, default);
    assert!(parse_str(&structural).expect("reparse").equivalent(&node));
}

#[test]
fn emitter_byte_compatible_is_supported_for_structural_trees() {
    let node = parse_str("name: api\n").expect("parse");
    let emitted =
        to_string_with_options(&node, EmitOptions::byte_compatible()).expect("byte compatible");
    assert_eq!(emitted, "name: api\n");
    assert!(parse_str(&emitted).expect("reparse").equivalent(&node));

    let sequence_value = parse_str("ports:\n  - 80\n  - 443\n").expect("parse sequence value");
    let byte_compatible = to_string_with_options(&sequence_value, EmitOptions::byte_compatible())
        .expect("byte compatible sequence value");
    assert_eq!(byte_compatible, "ports:\n- 80\n- 443\n");
    assert!(
        parse_str(&byte_compatible)
            .expect("reparse sequence value")
            .equivalent(&sequence_value)
    );
}

#[test]
fn emitter_options_sort_keys_without_changing_structural_default() {
    let node = parse_str("z: 1\na: 2\nm: 3\n").expect("parse");
    let default = to_string(&node).expect("default emit");
    assert_eq!(default, "z: 1\na: 2\nm: 3\n");

    let sorted = to_string_with_options(
        &node,
        EmitOptions::structural().with_key_order(KeyOrder::Sort),
    )
    .expect("sorted emit");

    assert_eq!(sorted, "a: 2\nm: 3\nz: 1\n");
    let reparsed = parse_str(&sorted).expect("reparse");
    assert_eq!(
        to_string_with_options(
            &reparsed,
            EmitOptions::structural().with_key_order(KeyOrder::Sort)
        )
        .expect("sorted re-emit"),
        sorted
    );
}

#[test]
fn emitter_options_control_scalar_quote_style_safely() {
    let node = parse_str(
        "plain: value\nambiguous: \"true\"\napostrophe: \"it's\"\ncontrol: \"a\\u0000b\"\n",
    )
    .expect("parse");
    let single = to_string_with_options(
        &node,
        EmitOptions::structural().with_scalar_quote_style(ScalarQuoteStyle::SingleQuoted),
    )
    .expect("single quote emit");
    assert!(single.contains("'plain': 'value'"), "{single}");
    assert!(single.contains("'ambiguous': 'true'"), "{single}");
    assert!(single.contains("'apostrophe': \"it's\""), "{single}");
    assert!(
        single.contains("'control': \"a\\0b\"") || single.contains("'control': \"a\\u0000b\""),
        "{single}"
    );
    assert!(
        parse_str(&single)
            .expect("reparse single")
            .equivalent(&node)
    );

    let double = to_string_with_options(
        &node,
        EmitOptions::structural().with_scalar_quote_style(ScalarQuoteStyle::DoubleQuoted),
    )
    .expect("double quote emit");
    assert!(double.contains("\"plain\": \"value\""), "{double}");
    assert!(double.contains("\"apostrophe\": \"it's\""), "{double}");
    assert!(
        parse_str(&double)
            .expect("reparse double")
            .equivalent(&node)
    );

    let unicode_break = Node::new(
        Value::String("before\u{2028}after".to_string()),
        Span::default(),
    );
    let emitted = to_string_with_options(
        &unicode_break,
        EmitOptions::structural().with_scalar_quote_style(ScalarQuoteStyle::SingleQuoted),
    )
    .expect("unicode line break emit");
    assert_eq!(emitted, "\"before\\u2028after\"\n");
    assert!(
        parse_str(&emitted)
            .expect("reparse unicode break")
            .equivalent(&unicode_break)
    );

    let trailing_apostrophe_key = Node::new(
        Value::Mapping(vec![(
            string_node("0{'"),
            Node::new(Value::String("a".to_string()), Span::default()),
        )]),
        Span::default(),
    );
    let emitted = to_string_with_options(
        &trailing_apostrophe_key,
        EmitOptions::structural().with_scalar_quote_style(ScalarQuoteStyle::SingleQuoted),
    )
    .expect("trailing apostrophe key emit");
    assert_eq!(emitted, "\"0{'\": 'a'\n");
    assert!(
        parse_str(&emitted)
            .expect("reparse trailing apostrophe key")
            .equivalent(&trailing_apostrophe_key)
    );
}

#[test]
fn emitter_options_control_block_scalar_style_where_representable() {
    let literal_node = parse_str("body: \"first\\nsecond\\n\"\n").expect("parse literal candidate");
    let literal = to_string_with_options(
        &literal_node,
        EmitOptions::structural().with_block_scalar_style(BlockScalarStyle::Literal),
    )
    .expect("literal emit");
    assert!(literal.contains("body: |"), "{literal}");
    assert!(
        parse_str(&literal)
            .expect("reparse literal")
            .equivalent(&literal_node)
    );

    let folded_node = parse_str("body: \"first\\n  second\\n\"\n").expect("parse folded candidate");
    let folded = to_string_with_options(
        &folded_node,
        EmitOptions::structural().with_block_scalar_style(BlockScalarStyle::Folded),
    )
    .expect("folded emit");
    assert!(folded.contains("body: >"), "{folded}");
    assert!(
        parse_str(&folded)
            .expect("reparse folded")
            .equivalent(&folded_node)
    );

    let fallback = to_string_with_options(
        &literal_node,
        EmitOptions::structural().with_block_scalar_style(BlockScalarStyle::Folded),
    )
    .expect("folded fallback emit");
    assert!(fallback.contains("body: |"), "{fallback}");
    assert!(
        parse_str(&fallback)
            .expect("reparse fallback")
            .equivalent(&literal_node)
    );
}

#[test]
fn emitter_options_control_flow_collection_style() {
    let node = parse_str("service:\n  image: app\n  ports:\n    - 80\n    - 443\n").expect("parse");
    let flow = to_string_with_options(
        &node,
        EmitOptions::structural().with_collection_style(EmitCollectionStyle::Flow),
    )
    .expect("flow emit");

    assert_eq!(flow, "{service: {image: app, ports: [80, 443]}}\n");
    assert!(parse_str(&flow).expect("reparse").equivalent(&node));
}

fn permuted_mapping_key(first_order: bool) -> Node {
    let entries = if first_order {
        vec![
            (string_node("a"), int_node(1)),
            (string_node("b"), int_node(2)),
        ]
    } else {
        vec![
            (string_node("b"), int_node(2)),
            (string_node("a"), int_node(1)),
        ]
    };
    Node::new(Value::Mapping(entries), Span::default())
}

#[test]
fn emit_parse_emit_is_stable_for_nested_config() {
    let input = include_str!("fixtures/real-world/github-actions/minimal-ci.yaml");
    let node = parse_str(input).expect("parse input");
    let emitted = to_string(&node).expect("emit");
    assert!(!emitted.starts_with("---\n"));

    let reparsed = parse_str(&emitted).expect("parse emitted YAML");
    assert!(reparsed.equivalent(&node));

    let emitted_again = to_string(&reparsed).expect("emit again");
    assert_eq!(emitted_again, emitted);
}

#[test]
fn emitter_quotes_ambiguous_yaml_1_2_scalars() {
    let input = "values: [true, \"true\", null, \"null\", \"001\", \"1e2\", \".nan\", \".inf\", \"+.INF\", \"a: b\"]";
    let node = parse_str(input).expect("parse");
    let emitted = to_string(&node).expect("emit");
    assert!(emitted.contains("\"true\""));
    assert!(emitted.contains("\"null\""));
    assert!(emitted.contains("\"001\""));
    assert!(emitted.contains("\"1e2\""));
    assert!(emitted.contains("\".nan\""));
    assert!(emitted.contains("\".inf\""));
    assert!(emitted.contains("\"+.INF\""));
    assert!(emitted.contains("\"a: b\""));
    assert!(parse_str(&emitted).expect("reparse").equivalent(&node));
}

#[test]
fn emitter_quotes_yaml_1_1_radix_looking_strings() {
    for value in ["0x10", "0xFF", "0o17", "0b101", "+0x1"] {
        let node = string_node(value);
        let emitted = to_string(&node).expect("emit radix-looking string");
        assert_eq!(emitted, format!("\"{value}\"\n"));

        let serde_value: serde_yaml::Value =
            serde_yaml::from_str(&emitted).expect("serde_yaml reparses emitted string");
        assert_eq!(serde_value, serde_yaml::Value::String(value.to_string()));
        assert!(
            parse_str(&emitted)
                .expect("saneyaml reparses")
                .equivalent(&node),
            "{emitted}"
        );
    }
}

#[test]
fn emitter_quotes_leading_bom_strings() {
    for value in ["\u{feff}", "\u{feff}abc"] {
        let node = string_node(value);
        let emitted = to_string(&node).expect("emit leading BOM string");
        assert!(emitted.starts_with('"'), "{emitted:?}");
        assert!(
            parse_str(&emitted)
                .expect("reparse leading BOM")
                .equivalent(&node),
            "{emitted:?}"
        );
    }

    let mapping = Node::new(
        Value::Mapping(vec![(string_node("\u{feff}key"), string_node("value"))]),
        Span::default(),
    );
    let emitted = to_string(&mapping).expect("emit leading BOM key");
    assert!(emitted.starts_with("\"\u{feff}key\": value"), "{emitted:?}");
    assert!(
        parse_str(&emitted)
            .expect("reparse leading BOM key")
            .equivalent(&mapping),
        "{emitted:?}"
    );
}

#[test]
fn byte_compatible_double_quotes_newline_strings() {
    let mut mapping = BTreeMap::new();
    mapping.insert("k1\nk2".to_string(), "v".to_string());
    let emitted = to_string_with_options(&mapping, EmitOptions::byte_compatible())
        .expect("byte-compatible newline key emit");
    assert_eq!(emitted, "\"k1\\nk2\": v\n");
    let reparsed: BTreeMap<String, String> =
        saneyaml::from_str(&emitted).expect("reparse byte-compatible newline key");
    assert_eq!(reparsed, mapping);

    let sequence = vec!["a\nb".to_string()];
    let flow = to_string_with_options(
        &sequence,
        EmitOptions::byte_compatible().with_collection_style(EmitCollectionStyle::Flow),
    )
    .expect("byte-compatible flow newline emit");
    assert_eq!(flow, "[\"a\\nb\"]\n");
    let reparsed: Vec<String> =
        saneyaml::from_str(&flow).expect("reparse byte-compatible flow newline");
    assert_eq!(reparsed, sequence);
}

#[test]
fn byte_compatible_quotes_document_marker_prefixed_strings() {
    for value in ["---", "---a", "----"] {
        let emitted = to_string_with_options(&value, EmitOptions::byte_compatible())
            .expect("byte-compatible marker-looking string emit");
        let reference = serde_yaml::to_string(&value).expect("serde_yaml marker-looking emit");
        assert_eq!(emitted, reference, "{value}");

        let reparsed: String = saneyaml::from_str(&emitted).expect("reparse marker-looking string");
        assert_eq!(reparsed, value);
    }

    let plain = to_string_with_options(&"--a", EmitOptions::byte_compatible())
        .expect("byte-compatible non-marker dash string emit");
    assert_eq!(plain, "--a\n");
}

#[test]
fn emitter_rejects_untagged_literal_merge_keys() {
    let node = Node::new(
        Value::Mapping(vec![(
            Node::new(Value::String("<<".to_string()), Span::default()),
            Node::new(Value::String("literal".to_string()), Span::default()),
        )]),
        Span::default(),
    );

    let error = to_string(&node).expect_err("untagged merge key is semantic YAML");
    assert!(
        error.to_string().contains("literal YAML merge keys"),
        "{error}"
    );
}

#[test]
fn emitter_rejects_overdepth_caller_built_trees_before_writing_yaml() {
    let node = nested_sequence(140);
    let error = to_string(&node).expect_err("over-depth trees are not emittable");
    let message = error.to_string();
    assert!(
        message.contains("maximum YAML nesting depth exceeded"),
        "{message}"
    );
}

#[test]
fn emitter_rejects_overdepth_mapping_keys_before_key_identity_recursion() {
    for (name, key) in [
        ("sequence key", nested_sequence(140)),
        ("mapping key", nested_mapping_key(140)),
        (
            "tagged sequence key",
            tagged_node("Thing", nested_sequence(140)),
        ),
    ] {
        let node = Node::new(
            Value::Mapping(vec![(key, Node::null(Span::default()))]),
            Span::default(),
        );
        let error = to_string(&node).expect_err("over-depth mapping keys are not emittable");
        let message = error.to_string();
        assert!(
            message.contains("maximum YAML nesting depth exceeded"),
            "{name}: {message}"
        );
    }
}

#[test]
fn emitter_handles_block_strings() {
    let input = "script: |\n  cargo test\n  cargo fmt --check\n";
    let node = parse_str(input).expect("parse");
    let emitted = to_string(&node).expect("emit");
    assert!(emitted.contains("script: |"));
    assert!(parse_str(&emitted).expect("reparse").equivalent(&node));
}

#[test]
fn emitter_quotes_multiline_strings_with_literal_controls() {
    let input = include_str!("fixtures/yaml-test-suite/data/G4RS/in.yaml");
    let node = parse_str(input).expect("parse escaped control fixture");
    let emitted = to_string(&node).expect("emit escaped control fixture");

    assert!(!emitted.contains("control: |"), "{emitted}");
    assert!(!emitted.contains("hex esc: |"), "{emitted}");
    assert!(
        emitted.contains("control: \"\\u00081998\\t1999\\t2000\\n\""),
        "{emitted}"
    );
    assert!(
        emitted.contains("hex esc: \"\\r\\n is \\r\\n\""),
        "{emitted}"
    );

    let reparsed = parse_str(&emitted).expect("parse emitted escaped control fixture");
    assert!(reparsed.equivalent(&node), "{emitted}");
    assert_eq!(
        to_string(&reparsed).expect("emit reparsed escaped control fixture"),
        emitted
    );
}

#[test]
fn emitter_round_trips_root_block_strings() {
    for input in [
        "\"line\\n\"\n",
        "\"first\\nsecond\\n\"\n",
        "\" leading\\nregular\\n\"\n",
    ] {
        let node = parse_str(input).expect("parse root multiline string");
        let emitted = to_string(&node).expect("emit root multiline string");
        let reparsed = parse_str(&emitted).expect("parse emitted root multiline string");
        assert!(
            reparsed.equivalent(&node),
            "emitted YAML did not preserve {input:?}: {emitted}"
        );
    }
}

#[test]
fn emitter_preserves_block_strings_that_start_with_space() {
    let input = "body: |2\n   more indented\n  regular\n";
    let node = parse_str(input).expect("parse");
    let emitted = to_string(&node).expect("emit");
    assert!(emitted.contains("body: |2\n"));
    assert!(parse_str(&emitted).expect("reparse").equivalent(&node));
}

#[test]
fn emitter_preserves_block_string_trailing_blank_lines() {
    let input = include_str!("fixtures/yaml-test-suite/data/F8F9/in.yaml");
    let node = parse_str(input).expect("parse chomping fixture");
    let emitted = to_string(&node).expect("emit chomping fixture");

    assert!(emitted.contains("clip: |"));
    assert!(emitted.contains("keep: |+"));
    assert!(parse_str(&emitted).expect("reparse").equivalent(&node));

    let empty_input = include_str!("fixtures/yaml-test-suite/data/K858/in.yaml");
    let node = parse_str(empty_input).expect("parse empty chomping fixture");
    let emitted = to_string(&node).expect("emit empty chomping fixture");
    assert!(emitted.contains("keep: |+"));
    assert!(parse_str(&emitted).expect("reparse").equivalent(&node));
}

#[test]
fn emitter_preserves_number_kinds() {
    let node = parse_str("int: 7\nfloat: 7.5\n").expect("parse");
    let Value::Mapping(entries) = &node.value else {
        panic!("expected mapping");
    };
    assert!(matches!(
        entries[0].1.value,
        Value::Number(Number::Integer(7))
    ));
    assert!(matches!(
        entries[1].1.value,
        Value::Number(Number::Float(value)) if value == 7.5
    ));
    assert!(
        parse_str(&to_string(&node).unwrap())
            .unwrap()
            .equivalent(&node)
    );
}

#[test]
fn emitter_rejects_signed_unsigned_duplicate_numeric_keys() {
    let node = Node::new(
        Value::Mapping(vec![
            (
                Node::new(Value::Number(Number::Integer(1)), Span::default()),
                Node::new(Value::String("signed".to_string()), Span::default()),
            ),
            (
                Node::new(Value::Number(Number::Unsigned(1)), Span::default()),
                Node::new(Value::String("unsigned".to_string()), Span::default()),
            ),
        ]),
        Span::default(),
    );

    let error = to_string(&node).expect_err("duplicate numeric keys are rejected");
    let message = error.to_string();
    assert!(
        message.contains("duplicate key") || message.contains("duplicate mapping key `1`"),
        "{message}"
    );
}

#[test]
fn emitter_rejects_signed_zero_duplicate_float_keys() {
    let node = Node::new(
        Value::Mapping(vec![
            (
                float_node(0.0),
                Node::new(Value::String("positive".to_string()), Span::default()),
            ),
            (
                float_node(-0.0),
                Node::new(Value::String("negative".to_string()), Span::default()),
            ),
        ]),
        Span::default(),
    );

    let error = to_string(&node).expect_err("signed zero float keys are rejected");
    let message = error.to_string();
    assert!(
        message.contains("duplicate key") || message.contains("duplicate mapping key"),
        "{message}"
    );
}

#[test]
fn emitter_rejects_permuted_duplicate_mapping_keys() {
    let node = Node::new(
        Value::Mapping(vec![
            (
                permuted_mapping_key(true),
                Node::new(Value::String("first".to_string()), Span::default()),
            ),
            (
                permuted_mapping_key(false),
                Node::new(Value::String("second".to_string()), Span::default()),
            ),
        ]),
        Span::default(),
    );

    let error = to_string(&node).expect_err("permuted duplicate mapping keys are rejected");
    let message = error.to_string();
    assert!(
        message.contains("duplicate key") || message.contains("duplicate mapping key"),
        "{message}"
    );
}

#[test]
fn emitter_round_trips_special_floats() {
    for input in [".nan\n", ".inf\n", "+.inf\n", "-.inf\n"] {
        let node = parse_str(input).expect("parse special float");
        let emitted = to_string(&node).expect("emit special float");
        let reparsed = parse_str(&emitted).expect("parse emitted special float");
        assert!(
            reparsed.equivalent(&node),
            "emitted YAML did not preserve {input:?}: {emitted}"
        );
    }

    let node = parse_str("nan: .nan\ninf: .inf\nplus_inf: +.inf\nneg_inf: -.inf\n")
        .expect("parse special float mapping");
    let emitted = to_string(&node).expect("emit special float mapping");
    assert!(emitted.contains("nan: .nan"));
    assert!(emitted.contains("inf: .inf"));
    assert!(emitted.contains("plus_inf: .inf"));
    assert!(emitted.contains("neg_inf: -.inf"));
    let reparsed = parse_str(&emitted).expect("parse emitted special float mapping");
    assert!(reparsed.equivalent(&node));
}

#[test]
fn emitter_quotes_strings_that_look_like_special_floats() {
    for input in [
        "\".nan\"\n",
        "\".NaN\"\n",
        "\".inf\"\n",
        "\"+.inf\"\n",
        "\"+.INF\"\n",
        "\"-.inf\"\n",
    ] {
        let node = parse_str(input).expect("parse special-float-looking string");
        let emitted = to_string(&node).expect("emit special-float-looking string");
        let reparsed = parse_str(&emitted).expect("parse emitted special-float-looking string");
        assert!(
            reparsed.equivalent(&node),
            "emitted YAML did not preserve {input:?}: {emitted}"
        );
    }

    let node = parse_str("{\".nan\": \".inf\", key: \"+.inf\"}\n")
        .expect("parse special-float-looking mapping strings");
    let emitted = to_string(&node).expect("emit special-float-looking mapping strings");
    assert!(emitted.contains("\".nan\""));
    assert!(emitted.contains("\".inf\""));
    assert!(emitted.contains("\"+.inf\""));
    let reparsed = parse_str(&emitted).expect("parse emitted mapping");
    assert!(reparsed.equivalent(&node));
}

#[test]
fn emitter_round_trips_collection_mapping_keys() {
    for input in [
        "{[a]: value}\n",
        "{[\"a, b\"]: value}\n",
        "? [\"a]b\"]\n: value\n",
        "{{name: app}: {enabled: true}}\n",
        "{[a, b]: b, [c, b]: [c, b, d]}\n",
        include_str!("fixtures/yaml-test-suite/data/6BFJ/in.yaml"),
        "- A:\n    - null\n",
    ] {
        let node = parse_str(input).expect("parse collection-key mapping");
        let emitted = to_string(&node).expect("emit");
        let reparsed = parse_str(&emitted).expect("parse emitted collection-key mapping");
        assert!(
            reparsed.equivalent(&node),
            "emitted YAML did not preserve {input:?}: {emitted}"
        );
    }
}

#[test]
fn emitter_quotes_root_document_marker_and_directive_like_scalars() {
    for input in [
        "\"...\"\n",
        "\"... marker\"\n",
        "\"%YAML 1.2\"\n",
        "\"a:\\tb\"\n",
        "\"a\\t#b\"\n",
    ] {
        let node = parse_str(input).expect("parse marker-like root scalar");
        let emitted = to_string(&node).expect("emit marker-like root scalar");
        let reparsed = parse_str(&emitted).expect("parse emitted marker-like root scalar");
        assert!(
            reparsed.equivalent(&node),
            "emitted YAML did not preserve {input:?}: {emitted}"
        );
    }
}
