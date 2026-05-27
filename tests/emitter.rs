use yaml::{Node, NodeValue as Value, Number, Span, Tag, TaggedNode, parse_str, to_string};

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
