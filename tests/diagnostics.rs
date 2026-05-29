use serde::Deserialize;
use std::io::{self, Cursor, Read};
use yaml::{
    DEFAULT_MAX_INPUT_BYTES, LoadOptions, Node, NodeValue, Value, parse_bytes,
    parse_lossless_bytes, parse_str,
};

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

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictMatrixConfig {
    name: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct PortsMatrixConfig {
    ports: Vec<u16>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct PortMatrixConfig {
    port: u16,
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
fn load_options_input_limit_rejects_oversized_inputs_before_parsing() {
    let input = "name: app\n";
    let options = LoadOptions::new().max_input_bytes(4);
    let mut errors = vec![
        options.parse_str(input).expect_err("parse_str input limit"),
        options
            .parse_bytes(input.as_bytes())
            .expect_err("parse_bytes input limit"),
        options
            .from_str::<Value>(input)
            .expect_err("from_str input limit"),
        options
            .from_slice::<Value>(input.as_bytes())
            .expect_err("from_slice input limit"),
        options
            .from_reader::<_, Value>(Cursor::new(input.as_bytes()))
            .expect_err("from_reader input limit"),
        options
            .from_documents_str::<Value>(input)
            .expect_err("from_documents_str input limit"),
        options
            .from_documents_slice::<Value>(input.as_bytes())
            .expect_err("from_documents_slice input limit"),
        options
            .from_documents_reader::<Value, _>(Cursor::new(input.as_bytes()))
            .expect_err("from_documents_reader input limit"),
    ];
    errors.push(
        Value::deserialize(options.deserializer_from_str(input))
            .expect_err("deserializer_from_str input limit"),
    );
    errors.push(
        Value::deserialize(options.deserializer_from_slice(input.as_bytes()))
            .expect_err("deserializer_from_slice input limit"),
    );
    errors.push(
        Value::deserialize(options.deserializer_from_reader(Cursor::new(input.as_bytes())))
            .expect_err("deserializer_from_reader input limit"),
    );

    for error in errors {
        let display = error.to_string();
        assert!(
            display.contains("YAML input exceeds configured limit of 4 bytes"),
            "{display}"
        );
        assert_eq!(error.location(), None);
        assert!(!display.contains("line 0"));
        assert!(!display.contains("column 0"));
    }
}

#[test]
fn lossless_bytes_rejects_default_input_limit_before_utf8_validation() {
    let mut input = vec![b'a'; DEFAULT_MAX_INPUT_BYTES + 1];
    *input.last_mut().expect("non-empty input") = 0xFF;

    let error = parse_lossless_bytes(&input).expect_err("lossless input limit");
    let display = error.to_string();
    assert!(
        display.contains(&format!(
            "YAML input exceeds configured limit of {DEFAULT_MAX_INPUT_BYTES} bytes"
        )),
        "{display}"
    );
    assert!(!display.contains("valid UTF-8"), "{display}");
    assert_eq!(error.location(), None);
}

#[test]
fn load_options_input_limit_allows_exact_bound_and_can_be_removed() {
    let input = "name: app\n";
    let exact: Value = LoadOptions::new()
        .max_input_bytes(input.len())
        .from_reader(Cursor::new(input.as_bytes()))
        .expect("exact input limit");
    let unlimited: Value = LoadOptions::new()
        .max_input_bytes(4)
        .without_input_limit()
        .from_str(input)
        .expect("input limit removed");

    assert_eq!(exact["name"].as_str(), Some("app"));
    assert_eq!(unlimited["name"].as_str(), Some("app"));
}

#[test]
fn parser_diagnostics_have_exact_spans_across_entrypoints() {
    for case in [
        ParserDiagnosticCase {
            name: "undefined alias",
            input: "service: *missing\n",
            message: "unknown anchor `missing`",
            span: ExpectedSpan {
                line: 1,
                column: 10,
                source: "*missing",
            },
            related: &[],
        },
        ParserDiagnosticCase {
            name: "recursive flow alias",
            input: "root: &root {? *root : value}\n",
            message: "recursive alias",
            span: ExpectedSpan {
                line: 1,
                column: 16,
                source: "*root",
            },
            related: &[ExpectedRelated {
                message: "anchor is still being parsed here",
                span: ExpectedSpan {
                    line: 1,
                    column: 7,
                    source: "&root",
                },
            }],
        },
        ParserDiagnosticCase {
            name: "duplicate scalar key",
            input: "root:\n  true: first\n  true: second\n",
            message: "duplicate mapping key `true`",
            span: ExpectedSpan {
                line: 3,
                column: 3,
                source: "true",
            },
            related: &[ExpectedRelated {
                message: "previous key is here",
                span: ExpectedSpan {
                    line: 2,
                    column: 3,
                    source: "true",
                },
            }],
        },
    ] {
        for entrypoint in ParserEntrypoint::ALL {
            let error = entrypoint.error(case.input);
            assert_exact_diagnostic(&error, case.input, case.name, entrypoint.name(), &case);
        }
    }
}

#[test]
fn yaml_11_schema_duplicate_key_collisions_keep_related_spans() {
    let error = LoadOptions::yaml_1_1()
        .parse_str("root:\n  on: push\n  yes: deploy\n")
        .expect_err("YAML 1.1 boolean aliases collide as keys");

    assert!(error.to_string().contains("duplicate mapping key `true`"));
    assert_eq!(error.span().line, 3);
    assert_eq!(error.span().column, 3);
    let related = &error.diagnostic().related;
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].span.line, 2);
    assert_eq!(related[0].span.column, 3);
}

#[test]
fn serde_diagnostics_have_exact_spans_across_entrypoints() {
    for case in [
        SerdeDiagnosticCase {
            name: "unknown field",
            shape: SerdeShape::Strict,
            input: "name: app\nextra: true\n",
            message: "unknown field `extra`",
            span: ExpectedSpan {
                line: 2,
                column: 1,
                source: "extra",
            },
        },
        SerdeDiagnosticCase {
            name: "sequence type error",
            shape: SerdeShape::Ports,
            input: "ports: no\n",
            message: "expected sequence",
            span: ExpectedSpan {
                line: 1,
                column: 8,
                source: "no",
            },
        },
        SerdeDiagnosticCase {
            name: "integer range error",
            shape: SerdeShape::Port,
            input: "port: 70000\n",
            message: "invalid value",
            span: ExpectedSpan {
                line: 1,
                column: 7,
                source: "70000",
            },
        },
    ] {
        for entrypoint in SerdeEntrypoint::ALL {
            let error = entrypoint.error(case.shape, case.input);
            assert_exact_span(
                &error.span(),
                case.input,
                &case.span,
                case.name,
                entrypoint.name(),
            );
            assert!(
                error.to_string().contains(case.message),
                "{} via {} should contain {:?}: {}",
                case.name,
                entrypoint.name(),
                case.message,
                error
            );
            assert!(
                error.diagnostic().related.is_empty(),
                "{} via {} should not report related spans",
                case.name,
                entrypoint.name(),
            );
        }
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

#[derive(Clone, Copy)]
struct ExpectedSpan {
    line: usize,
    column: usize,
    source: &'static str,
}

struct ExpectedRelated {
    message: &'static str,
    span: ExpectedSpan,
}

struct ParserDiagnosticCase {
    name: &'static str,
    input: &'static str,
    message: &'static str,
    span: ExpectedSpan,
    related: &'static [ExpectedRelated],
}

#[derive(Clone, Copy)]
enum ParserEntrypoint {
    ParseStr,
    ParseBytes,
    FromStr,
    FromSlice,
    FromReader,
    DirectDeserializerStr,
    DirectDeserializerSlice,
    DirectDeserializerReader,
}

impl ParserEntrypoint {
    const ALL: [Self; 8] = [
        Self::ParseStr,
        Self::ParseBytes,
        Self::FromStr,
        Self::FromSlice,
        Self::FromReader,
        Self::DirectDeserializerStr,
        Self::DirectDeserializerSlice,
        Self::DirectDeserializerReader,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::ParseStr => "parse_str",
            Self::ParseBytes => "parse_bytes",
            Self::FromStr => "from_str",
            Self::FromSlice => "from_slice",
            Self::FromReader => "from_reader",
            Self::DirectDeserializerStr => "Deserializer::from_str",
            Self::DirectDeserializerSlice => "Deserializer::from_slice",
            Self::DirectDeserializerReader => "Deserializer::from_reader",
        }
    }

    fn error(self, input: &str) -> yaml::Error {
        match self {
            Self::ParseStr => parse_str(input).expect_err("parse_str should reject"),
            Self::ParseBytes => {
                parse_bytes(input.as_bytes()).expect_err("parse_bytes should reject")
            }
            Self::FromStr => yaml::from_str::<Value>(input).expect_err("from_str should reject"),
            Self::FromSlice => {
                yaml::from_slice::<Value>(input.as_bytes()).expect_err("from_slice should reject")
            }
            Self::FromReader => yaml::from_reader::<_, Value>(Cursor::new(input.as_bytes()))
                .expect_err("from_reader should reject"),
            Self::DirectDeserializerStr => Value::deserialize(yaml::Deserializer::from_str(input))
                .expect_err("direct string deserializer should reject"),
            Self::DirectDeserializerSlice => {
                Value::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
                    .expect_err("direct slice deserializer should reject")
            }
            Self::DirectDeserializerReader => Value::deserialize(yaml::Deserializer::from_reader(
                Cursor::new(input.as_bytes()),
            ))
            .expect_err("direct reader deserializer should reject"),
        }
    }
}

#[derive(Clone, Copy)]
enum SerdeShape {
    Strict,
    Ports,
    Port,
}

struct SerdeDiagnosticCase {
    name: &'static str,
    shape: SerdeShape,
    input: &'static str,
    message: &'static str,
    span: ExpectedSpan,
}

#[derive(Clone, Copy)]
enum SerdeEntrypoint {
    FromStr,
    FromSlice,
    FromReader,
    DirectDeserializerStr,
    DirectDeserializerSlice,
    DirectDeserializerReader,
}

impl SerdeEntrypoint {
    const ALL: [Self; 6] = [
        Self::FromStr,
        Self::FromSlice,
        Self::FromReader,
        Self::DirectDeserializerStr,
        Self::DirectDeserializerSlice,
        Self::DirectDeserializerReader,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::FromStr => "from_str",
            Self::FromSlice => "from_slice",
            Self::FromReader => "from_reader",
            Self::DirectDeserializerStr => "Deserializer::from_str",
            Self::DirectDeserializerSlice => "Deserializer::from_slice",
            Self::DirectDeserializerReader => "Deserializer::from_reader",
        }
    }

    fn error(self, shape: SerdeShape, input: &str) -> yaml::Error {
        match (self, shape) {
            (Self::FromStr, SerdeShape::Strict) => {
                yaml::from_str::<StrictMatrixConfig>(input).expect_err("from_str should reject")
            }
            (Self::FromStr, SerdeShape::Ports) => {
                yaml::from_str::<PortsMatrixConfig>(input).expect_err("from_str should reject")
            }
            (Self::FromStr, SerdeShape::Port) => {
                yaml::from_str::<PortMatrixConfig>(input).expect_err("from_str should reject")
            }
            (Self::FromSlice, SerdeShape::Strict) => {
                yaml::from_slice::<StrictMatrixConfig>(input.as_bytes())
                    .expect_err("from_slice should reject")
            }
            (Self::FromSlice, SerdeShape::Ports) => {
                yaml::from_slice::<PortsMatrixConfig>(input.as_bytes())
                    .expect_err("from_slice should reject")
            }
            (Self::FromSlice, SerdeShape::Port) => {
                yaml::from_slice::<PortMatrixConfig>(input.as_bytes())
                    .expect_err("from_slice should reject")
            }
            (Self::FromReader, SerdeShape::Strict) => {
                yaml::from_reader::<_, StrictMatrixConfig>(Cursor::new(input.as_bytes()))
                    .expect_err("from_reader should reject")
            }
            (Self::FromReader, SerdeShape::Ports) => {
                yaml::from_reader::<_, PortsMatrixConfig>(Cursor::new(input.as_bytes()))
                    .expect_err("from_reader should reject")
            }
            (Self::FromReader, SerdeShape::Port) => {
                yaml::from_reader::<_, PortMatrixConfig>(Cursor::new(input.as_bytes()))
                    .expect_err("from_reader should reject")
            }
            (Self::DirectDeserializerStr, SerdeShape::Strict) => {
                StrictMatrixConfig::deserialize(yaml::Deserializer::from_str(input))
                    .expect_err("direct string deserializer should reject")
            }
            (Self::DirectDeserializerStr, SerdeShape::Ports) => {
                PortsMatrixConfig::deserialize(yaml::Deserializer::from_str(input))
                    .expect_err("direct string deserializer should reject")
            }
            (Self::DirectDeserializerStr, SerdeShape::Port) => {
                PortMatrixConfig::deserialize(yaml::Deserializer::from_str(input))
                    .expect_err("direct string deserializer should reject")
            }
            (Self::DirectDeserializerSlice, SerdeShape::Strict) => {
                StrictMatrixConfig::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
                    .expect_err("direct slice deserializer should reject")
            }
            (Self::DirectDeserializerSlice, SerdeShape::Ports) => {
                PortsMatrixConfig::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
                    .expect_err("direct slice deserializer should reject")
            }
            (Self::DirectDeserializerSlice, SerdeShape::Port) => {
                PortMatrixConfig::deserialize(yaml::Deserializer::from_slice(input.as_bytes()))
                    .expect_err("direct slice deserializer should reject")
            }
            (Self::DirectDeserializerReader, SerdeShape::Strict) => {
                StrictMatrixConfig::deserialize(yaml::Deserializer::from_reader(Cursor::new(
                    input.as_bytes(),
                )))
                .expect_err("direct reader deserializer should reject")
            }
            (Self::DirectDeserializerReader, SerdeShape::Ports) => PortsMatrixConfig::deserialize(
                yaml::Deserializer::from_reader(Cursor::new(input.as_bytes())),
            )
            .expect_err("direct reader deserializer should reject"),
            (Self::DirectDeserializerReader, SerdeShape::Port) => PortMatrixConfig::deserialize(
                yaml::Deserializer::from_reader(Cursor::new(input.as_bytes())),
            )
            .expect_err("direct reader deserializer should reject"),
        }
    }
}

fn assert_exact_diagnostic(
    error: &yaml::Error,
    input: &str,
    case_name: &str,
    entrypoint: &str,
    expected: &ParserDiagnosticCase,
) {
    assert!(
        error.to_string().contains(expected.message),
        "{case_name} via {entrypoint} should contain {:?}: {error}",
        expected.message,
    );
    assert_exact_span(&error.span(), input, &expected.span, case_name, entrypoint);
    let diagnostic = error.diagnostic();
    assert_eq!(
        diagnostic.related.len(),
        expected.related.len(),
        "{case_name} via {entrypoint} related span count",
    );
    for (actual, expected) in diagnostic.related.iter().zip(expected.related) {
        assert!(
            actual.message.contains(expected.message),
            "{case_name} via {entrypoint} related diagnostic should contain {:?}: {:?}",
            expected.message,
            actual.message,
        );
        assert_exact_span(&actual.span, input, &expected.span, case_name, entrypoint);
    }
}

fn assert_exact_span(
    actual: &yaml::Span,
    input: &str,
    expected: &ExpectedSpan,
    case_name: &str,
    entrypoint: &str,
) {
    let expected_start = byte_offset(input, expected.line, expected.column);
    let expected_end = expected_start + expected.source.len();
    assert_eq!(
        actual.line, expected.line,
        "{case_name} via {entrypoint} line"
    );
    assert_eq!(
        actual.column, expected.column,
        "{case_name} via {entrypoint} column",
    );
    assert_eq!(
        actual.start, expected_start,
        "{case_name} via {entrypoint} start",
    );
    assert_eq!(actual.end, expected_end, "{case_name} via {entrypoint} end");
    assert_eq!(
        &input[actual.start..actual.end],
        expected.source,
        "{case_name} via {entrypoint} source slice",
    );
}

fn byte_offset(input: &str, line: usize, column: usize) -> usize {
    let mut current_line = 1usize;
    let mut current_column = 1usize;
    for (offset, byte) in input.bytes().enumerate() {
        if current_line == line && current_column == column {
            return offset;
        }
        if byte == b'\n' {
            current_line += 1;
            current_column = 1;
        } else {
            current_column += 1;
        }
    }
    if current_line == line && current_column == column {
        input.len()
    } else {
        panic!("line {line}, column {column} is outside input {input:?}");
    }
}
