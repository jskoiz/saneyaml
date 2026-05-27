use serde::Deserialize;
use std::io::{self, Read};
use yaml::{Node, NodeValue, Value, parse_bytes, parse_str};

const ALIAS_EXPANSION_BOMB: &str = "\
a: &a [lol, lol, lol, lol, lol, lol, lol, lol]
b: &b [*a, *a, *a, *a, *a, *a, *a, *a]
c: &c [*b, *b, *b, *b, *b, *b, *b, *b]
d: &d [*c, *c, *c, *c, *c, *c, *c, *c]
e: &e [*d, *d, *d, *d, *d, *d, *d, *d]
boom: *e
";
const TEST_MAX_DEPTH: usize = 128;

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("read exploded"))
    }
}

#[test]
fn successful_tree_spans_are_in_bounds() {
    let input = include_str!("fixtures/real-world/helm/values.yaml");
    let node = parse_str(input).expect("parse");
    assert_node_spans(&node, input.len());
}

#[test]
fn invalid_utf8_reports_byte_offset_location() {
    let error = parse_bytes(b"ok: true\nbad: \xFF").expect_err("invalid UTF-8");
    assert!(error.to_string().contains("input is not valid UTF-8"));
    assert_eq!(error.span().start, 14);
    assert_eq!(error.span().line, 2);
    assert_eq!(error.span().column, 6);
}

#[test]
fn invalid_utf8_reports_consistent_locations_for_byte_entry_apis() {
    let input = b"ok: true\nbad: \xFF";
    for error in [
        yaml::from_slice::<Value>(input).expect_err("from_slice invalid UTF-8"),
        yaml::from_documents_slice::<Value>(input).expect_err("from_documents_slice invalid UTF-8"),
        Value::deserialize(
            yaml::Deserializer::from_slice(input)
                .next()
                .expect("invalid UTF-8 document"),
        )
        .expect_err("deserializer invalid UTF-8"),
    ] {
        assert_eq!(error.span().start, 14);
        assert_eq!(error.span().line, 2);
        assert_eq!(error.span().column, 6);
    }
}

#[test]
fn reader_failures_without_source_spans_do_not_render_zero_location() {
    let errors = [
        yaml::from_reader::<_, Value>(FailingReader).expect_err("from_reader read failure"),
        yaml::from_documents_reader::<Value, _>(FailingReader)
            .expect_err("from_documents_reader read failure"),
        Value::deserialize(
            yaml::Deserializer::from_reader(FailingReader)
                .next()
                .expect("reader failure document"),
        )
        .expect_err("deserializer reader failure"),
    ];

    for error in errors {
        let display = error.to_string();
        assert!(display.contains("failed to read YAML input"));
        assert_eq!(error.location(), None);
        assert!(!display.contains("line 0"));
        assert!(!display.contains("column 0"));
    }
}

#[test]
fn invalid_utf8_after_non_ascii_reports_line_and_byte_column() {
    let input = b"emoji: \xF0\x9F\x98\x80\nbad: \xFF";
    let error = parse_bytes(input).expect_err("invalid UTF-8 after non-ASCII");
    assert_eq!(error.span().start, 17);
    assert_eq!(error.span().line, 2);
    assert_eq!(error.span().column, 6);
}

#[test]
fn diagnostics_after_non_ascii_stay_in_bounds() {
    let input = "emoji: 😀\nvalue: *missing\n";
    let error = parse_str(input).expect_err("undefined alias");
    assert!(error.to_string().contains("unknown anchor `missing`"));
    assert!(error.span().start <= input.len());
    assert!(error.span().end <= input.len());
    assert_eq!(error.span().line, 2);
}

#[test]
fn undefined_alias_reports_alias_span() {
    let error = parse_str("service: *missing\n").expect_err("undefined alias");
    assert!(error.to_string().contains("unknown anchor `missing`"));
    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 10);
}

#[test]
fn compose_merge_alias_diagnostic_points_to_missing_alias() {
    let input = "services:\n  web:\n    <<: *service-defaults\n";
    let error = parse_str(input).expect_err("undefined merge alias");
    assert!(
        error
            .to_string()
            .contains("unknown anchor `service-defaults`")
    );
    assert_eq!(error.span().line, 3);
    assert_eq!(error.span().column, 9);
}

#[test]
fn flow_mapping_key_alias_diagnostics_point_to_alias_span() {
    let input = "emoji: 😀\nroot: {? *missing : value}\n";
    let error = parse_str(input).expect_err("undefined flow key alias");
    assert!(error.to_string().contains("unknown anchor `missing`"));
    assert!(error.span().start <= input.len());
    assert_eq!(error.span().line, 2);
    assert_eq!(error.span().column, 10);
}

#[test]
fn flow_alias_colon_name_diagnostics_keep_alias_span() {
    let input = "root: [*missing:]\n";
    let error = parse_str(input).expect_err("undefined colon alias");
    assert!(error.to_string().contains("unknown anchor `missing:`"));
    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 8);
}

#[test]
fn recursive_alias_reports_alias_and_anchor_spans() {
    for input in ["self: &self [*self]\n", "root: &root\n  self: *root\n"] {
        let error = parse_str(input).expect_err("recursive alias rejected");
        assert!(error.to_string().contains("recursive alias"));
        assert_eq!(error.diagnostic().related.len(), 1);
        assert!(
            error.diagnostic().related[0]
                .message
                .contains("anchor is still being parsed")
        );
    }
}

#[test]
fn recursive_flow_mapping_key_alias_reports_alias_and_anchor_spans() {
    let error = parse_str("root: &root {? *root : value}\n")
        .expect_err("recursive flow key alias rejected");
    assert!(error.to_string().contains("recursive alias"));
    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 16);
    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].span.line, 1);
    assert_eq!(diagnostic.related[0].span.column, 7);
}

#[test]
fn duplicate_key_related_span_points_to_original() {
    let error = parse_str("root:\n  true: first\n  true: second\n").expect_err("duplicate key");
    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.span.line, 3);
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].span.line, 2);
}

#[test]
fn compose_duplicate_service_key_diagnostic_points_to_duplicate_and_original() {
    let input = "services:\n  web:\n    image: nginx\n    image: redis\n";
    let error = parse_str(input).expect_err("duplicate nested compose key");
    assert!(error.to_string().contains("duplicate mapping key `image`"));
    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.span.line, 4);
    assert_eq!(diagnostic.span.column, 5);
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].span.line, 3);
    assert_eq!(diagnostic.related[0].span.column, 5);
}

#[test]
fn duplicate_flow_mapping_alias_key_related_span_points_to_original() {
    let error = parse_str("key: &key dup\nroot: {? *key : first, dup: second}\n")
        .expect_err("duplicate key");
    assert!(error.to_string().contains("duplicate mapping key `dup`"));
    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.span.line, 2);
    assert_eq!(diagnostic.span.column, 24);
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].span.line, 2);
    assert_eq!(diagnostic.related[0].span.column, 10);
}

#[test]
fn tagged_duplicate_keys_report_duplicate_and_original_spans() {
    for (input, label) in [
        ("root: {!Tag dup: first, dup: second}\n", "dup"),
        ("root: {!Tag [a, b]: first, [a, b]: second}\n", "[a, b]"),
        ("root: {!Tag {x: y}: first, {x: y}: second}\n", "{x: y}"),
    ] {
        let error = parse_str(input).expect_err("tagged duplicate key");
        let display = error.to_string();
        assert!(display.contains("duplicate mapping key"));
        assert!(display.contains(label), "{display}");
        let diagnostic = error.diagnostic();
        assert_eq!(diagnostic.span.line, 1);
        assert_eq!(diagnostic.related.len(), 1);
        assert_eq!(diagnostic.related[0].span.line, 1);
    }
}

#[test]
fn parser_rejects_excessive_nesting_with_span() {
    let mut input = String::new();
    for depth in 0..130 {
        input.push_str(&"  ".repeat(depth));
        input.push_str("-\n");
    }
    let error = parse_str(&input).expect_err("depth limit");
    assert!(
        error
            .to_string()
            .contains("maximum YAML nesting depth exceeded")
    );
    assert!(error.location().is_some());
}

#[test]
fn parser_rejects_excessive_flow_nesting_with_span() {
    let mut input = "[".repeat(130);
    input.push_str(&"]".repeat(130));
    let error = parse_str(&input).expect_err("flow depth limit");
    assert!(
        error
            .to_string()
            .contains("maximum YAML nesting depth exceeded")
    );
    assert!(error.location().is_some());
}

#[test]
fn parser_depth_boundaries_cover_block_and_flow_shapes() {
    for (name, input) in [
        (
            "block sequence below depth limit",
            nested_block_sequence(TEST_MAX_DEPTH - 1),
        ),
        (
            "flow sequence below depth limit",
            nested_flow_sequence(TEST_MAX_DEPTH - 1),
        ),
    ] {
        parse_str(&input).unwrap_or_else(|error| panic!("{name} should parse: {error}"));
        yaml::from_str::<Value>(&input)
            .unwrap_or_else(|error| panic!("{name} should deserialize: {error}"));
    }

    for (name, input) in [
        (
            "block sequence above depth limit",
            nested_block_sequence(TEST_MAX_DEPTH + 1),
        ),
        (
            "flow sequence above depth limit",
            nested_flow_sequence(TEST_MAX_DEPTH + 1),
        ),
    ] {
        let error = parse_str(&input).expect_err(&format!("{name} should reject"));
        assert_depth_limit_error(&error);

        let error = yaml::from_str::<Value>(&input)
            .expect_err(&format!("{name} serde entrypoint should reject"));
        assert_depth_limit_error(&error);
    }
}

#[test]
fn alias_expansion_boundary_keeps_raw_events_safe() {
    let below = alias_expansion_chain(4);
    parse_str(&below).expect("four-level alias expansion chain stays below budget");
    yaml::from_str::<Value>(&below).expect("serde reads below-budget alias chain");
    assert!(
        yaml::parse_events(&below)
            .expect("raw events expose below-budget aliases")
            .iter()
            .any(|event| matches!(event, yaml::Event::Alias { anchor } if anchor.name == "d"))
    );

    let above = alias_expansion_chain(5);
    let error = parse_str(&above).expect_err("five-level alias expansion chain crosses budget");
    assert!(error.to_string().contains("alias expansion limit exceeded"));
    assert!(error.location().is_some());
    let error =
        yaml::from_str::<Value>(&above).expect_err("serde rejects above-budget alias chain");
    assert!(error.to_string().contains("alias expansion limit exceeded"));
    assert!(error.location().is_some());
    assert!(
        yaml::parse_events(&above)
            .expect("raw events remain safe because aliases are not expanded")
            .iter()
            .any(|event| matches!(event, yaml::Event::Alias { anchor } if anchor.name == "e"))
    );
}

#[test]
fn alias_expanded_duplicate_key_boundaries_report_errors() {
    let key = nested_flow_sequence(TEST_MAX_DEPTH / 2);
    let input = format!("key: &key {key}\nroot:\n  ? *key\n  : first\n  ? {key}\n  : second\n");
    let error = parse_str(&input).expect_err("alias-expanded duplicate key should reject safely");
    let display = error.to_string();
    assert!(
        display.contains("duplicate mapping key")
            || display.contains("maximum YAML nesting depth exceeded"),
        "{display}"
    );
    assert!(error.location().is_some());

    yaml::parse_events(&input).expect("raw events stay available without duplicate-key expansion");
}

#[test]
fn parser_rejects_alias_expansion_bomb_with_alias_span() {
    let error = parse_str(ALIAS_EXPANSION_BOMB).expect_err("alias expansion limit");
    assert_alias_expansion_error(&error);

    let from_str_error =
        yaml::from_str::<Value>(ALIAS_EXPANSION_BOMB).expect_err("serde alias expansion limit");
    assert_alias_expansion_error(&from_str_error);

    let documents_error = yaml::from_documents_str::<Value>(ALIAS_EXPANSION_BOMB)
        .expect_err("document serde alias expansion limit");
    assert_alias_expansion_error(&documents_error);

    let events = yaml::parse_events(ALIAS_EXPANSION_BOMB)
        .expect("raw events do not expand aliases when validating stream safety");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            yaml::Event::Alias { anchor }
                if anchor.name == "e"
                    && anchor.span.line == 6
                    && anchor.span.column == 7
                    && &ALIAS_EXPANSION_BOMB[anchor.span.start..anchor.span.end] == "*e"
        )
    }));
}

fn assert_node_spans(node: &Node, input_len: usize) {
    assert!(node.span.start <= node.span.end);
    assert!(node.span.end <= input_len);
    assert!(node.span.line >= 1);
    assert!(node.span.column >= 1);
    match &node.value {
        NodeValue::Sequence(items) => {
            for item in items {
                assert_node_spans(item, input_len);
            }
        }
        NodeValue::Mapping(entries) => {
            for (key, value) in entries {
                assert_node_spans(key, input_len);
                assert_node_spans(value, input_len);
            }
        }
        NodeValue::Tagged(tagged) => {
            assert!(tagged.tag_span.start <= tagged.tag_span.end);
            assert!(tagged.tag_span.end <= input_len);
            assert!(tagged.tag_span.line >= 1);
            assert!(tagged.tag_span.column >= 1);
            assert_node_spans(&tagged.value, input_len);
        }
        NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) | NodeValue::String(_) => {}
    }
}

fn assert_alias_expansion_error(error: &yaml::Error) {
    assert!(error.to_string().contains("alias expansion limit exceeded"));
    assert_eq!(error.span().line, 5);
    assert_eq!(error.span().column, 12);
    assert_eq!(
        &ALIAS_EXPANSION_BOMB[error.span().start..error.span().end],
        "*d"
    );
}

fn assert_depth_limit_error(error: &yaml::Error) {
    assert!(
        error
            .to_string()
            .contains("maximum YAML nesting depth exceeded")
    );
    assert!(error.location().is_some());
}

fn nested_block_sequence(depth: usize) -> String {
    let mut input = String::new();
    for level in 0..depth {
        input.push_str(&"  ".repeat(level));
        input.push_str("-\n");
    }
    input
}

fn nested_flow_sequence(depth: usize) -> String {
    let mut input = "[".repeat(depth);
    input.push('0');
    input.push_str(&"]".repeat(depth));
    input
}

fn alias_expansion_chain(levels: usize) -> String {
    let names = ["a", "b", "c", "d", "e", "f"];
    assert!(levels > 0 && levels <= names.len());
    let mut input = "a: &a [lol, lol, lol, lol, lol, lol, lol, lol]\n".to_string();
    for index in 1..levels {
        let name = names[index];
        let previous = names[index - 1];
        let aliases = (0..8)
            .map(|_| format!("*{previous}"))
            .collect::<Vec<_>>()
            .join(", ");
        input.push_str(&format!("{name}: &{name} [{aliases}]\n"));
    }
    input.push_str(&format!("boom: *{}\n", names[levels - 1]));
    input
}
