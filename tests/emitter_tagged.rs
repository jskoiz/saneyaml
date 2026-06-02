use saneyaml::{Node, NodeValue, Span, Tag, TaggedNode, parse_bytes, parse_str, to_string};

fn string_node(value: &str) -> Node {
    Node::new(NodeValue::String(value.to_string()), Span::default())
}

fn tagged_node(tag: &str, value: Node) -> Node {
    Node::new(
        NodeValue::Tagged(Box::new(TaggedNode {
            tag: Tag::new(tag),
            tag_span: Span::default(),
            value,
        })),
        Span::default(),
    )
}

#[test]
fn emitter_rejects_duplicate_effective_tagged_mapping_keys() {
    let node = Node::new(
        NodeValue::Mapping(vec![
            (string_node("_"), string_node("plain")),
            (
                Node::new(
                    NodeValue::Tagged(Box::new(TaggedNode {
                        tag: Tag::new("Thing"),
                        tag_span: Span::default(),
                        value: string_node("_"),
                    })),
                    Span::default(),
                ),
                string_node("tagged"),
            ),
        ]),
        Span::default(),
    );

    let error = to_string(&node).expect_err("duplicate effective keys are rejected");
    let message = error.to_string();
    assert!(message.contains("duplicate mapping key"), "{message}");
    assert!(message.contains("_"), "{message}");
}

#[test]
fn emitter_rejects_nested_tags_before_writing_invalid_yaml() {
    let node = tagged_node("Outer", tagged_node("Inner", string_node("value")));

    let error = to_string(&node).expect_err("nested tags are not directly emittable");
    let message = error.to_string();
    assert!(
        message.contains("nested YAML tags cannot be emitted directly"),
        "{message}"
    );
}

#[test]
fn emitter_quotes_flow_scalars_inside_tagged_nested_mappings() {
    let node = Node::new(
        NodeValue::Mapping(vec![
            (
                string_node("A"),
                Node::new(
                    NodeValue::Tagged(Box::new(TaggedNode {
                        tag: Tag::new("Thing"),
                        tag_span: Span::default(),
                        value: Node::new(
                            NodeValue::Mapping(vec![(
                                string_node("_"),
                                string_node("quote \" slash \\\\"),
                            )]),
                            Span::default(),
                        ),
                    })),
                    Span::default(),
                ),
            ),
            (string_node("B"), Node::null(Span::default())),
        ]),
        Span::default(),
    );

    let emitted = to_string(&node).expect("emit tagged mapping");
    let reparsed = parse_str(&emitted).expect("parse emitted tagged mapping");

    assert!(reparsed.equivalent(&node), "{emitted}");
    assert_eq!(to_string(&reparsed).expect("emit again"), emitted);
}

#[test]
fn emitter_round_trips_tagged_quoted_scalar_with_colon() {
    for tag in [
        Tag::new("Thing"),
        Tag::new("comma,tag"),
        Tag::new("flow[bracket]"),
    ] {
        let node = Node::new(
            NodeValue::Tagged(Box::new(TaggedNode {
                tag,
                tag_span: Span::default(),
                value: string_node("a: b"),
            })),
            Span::default(),
        );

        let emitted = to_string(&node).expect("emit tagged scalar");
        let reparsed = parse_str(&emitted)
            .unwrap_or_else(|error| panic!("parse emitted tagged scalar: {error:?}\n{emitted}"));

        assert!(reparsed.equivalent(&node), "{emitted}");
        assert_eq!(to_string(&reparsed).expect("emit again"), emitted);
    }
}

#[test]
fn emitter_round_trips_tags_with_literal_control_bytes() {
    let input = b"!R\0\0\0\"items:\n  - !Thing {name:\x14first, avl, value: 2}\n";
    let node = parse_bytes(input).expect("parse fuzz-discovered tagged input");
    let emitted = to_string(&node).expect("emit fuzz-discovered tagged input");
    assert!(emitted.contains("R%00%00%00"), "{emitted:?}");
    assert!(!emitted.as_bytes().contains(&0), "{emitted:?}");
    let reparsed = parse_str(&emitted)
        .unwrap_or_else(|error| panic!("parse emitted tagged input: {error:?}\n{emitted:?}"));

    assert!(reparsed.equivalent(&node), "{emitted:?}");
    assert_eq!(to_string(&reparsed).expect("emit again"), emitted);
}

#[test]
fn emitter_round_trips_tagged_mapping_keys_with_colon_rich_tags() {
    for input in [
        "\
%TAG !e! :example.com,2026:
---
root: {? !e!Thing key : tagged-v}
",
        "\
%TAG !e! tag:example.com,2026:
---
root: {? !e!Th>ing key : tagged-v}
",
    ] {
        let node = parse_str(input).expect("parse tagged key");
        let emitted = to_string(&node).expect("emit tagged key");
        let reparsed = parse_str(&emitted)
            .unwrap_or_else(|error| panic!("parse emitted tagged key: {error:?}\n{emitted}"));

        assert!(reparsed.equivalent(&node), "{emitted}");
        assert_eq!(to_string(&reparsed).expect("emit again"), emitted);
    }
}

#[test]
fn emitter_round_trips_explicit_core_tag_empty_scalars_in_flow_mapping() {
    let input = include_str!("fixtures/yaml-test-suite/data/WZ62/in.yaml");
    let node = parse_str(input).expect("parse tagged empty content");
    let emitted = to_string(&node).expect("emit tagged empty content");
    let reparsed = parse_str(&emitted).expect("parse emitted tagged empty content");

    assert!(reparsed.equivalent(&node), "{emitted}");
    assert_eq!(to_string(&reparsed).expect("emit again"), emitted);
}

#[test]
fn emitter_round_trips_verbatim_tags_with_leading_less_than_suffix() {
    let input = "!<<abc> value\n";
    let node = parse_str(input).expect("parse verbatim tag");
    let emitted = to_string(&node).expect("emit verbatim tag");
    let reparsed = parse_str(&emitted)
        .unwrap_or_else(|error| panic!("parse emitted verbatim tag: {error:?}\n{emitted}"));

    assert!(emitted.contains("!<<abc> value"), "{emitted}");
    assert!(reparsed.equivalent(&node), "{emitted}");
    assert_eq!(to_string(&reparsed).expect("emit again"), emitted);
}

#[test]
fn emitter_round_trips_verbatim_tags_with_mapping_and_handle_like_suffixes() {
    for input in ["!<abc:> value\n", "!<a!b> value\n"] {
        let node = parse_str(input).expect("parse verbatim tag");
        let emitted = to_string(&node).expect("emit verbatim tag");
        let reparsed = parse_str(&emitted)
            .unwrap_or_else(|error| panic!("parse emitted verbatim tag: {error:?}\n{emitted}"));

        assert!(emitted.contains(input.trim_end()), "{emitted}");
        assert!(reparsed.equivalent(&node), "{emitted}");
        assert_eq!(to_string(&reparsed).expect("emit again"), emitted);
    }
}
