use saneyaml::{
    DEFAULT_MAX_INPUT_BYTES, LoadOptions, Node, NodeValue, Value, parse_bytes,
    parse_lossless_bytes, parse_str,
};
use serde::Deserialize;
use std::cell::Cell;
use std::io::{self, Cursor, Read};
use std::rc::Rc;

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

struct InfiniteReader {
    bytes_read: Rc<Cell<usize>>,
}

impl InfiniteReader {
    fn new() -> (Self, Rc<Cell<usize>>) {
        let bytes_read = Rc::new(Cell::new(0));
        (
            Self {
                bytes_read: Rc::clone(&bytes_read),
            },
            bytes_read,
        )
    }
}

impl Read for InfiniteReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        buf.fill(b'a');
        self.bytes_read
            .set(self.bytes_read.get().saturating_add(buf.len()));
        Ok(buf.len())
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

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct NestedServerConfig {
    server: ServerConfig,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ServerConfig {
    port: u16,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RootServersConfig(Vec<ServerConfig>);

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
        saneyaml::from_slice::<Value>(input).expect_err("from_slice invalid UTF-8"),
        saneyaml::from_documents_slice::<Value>(input)
            .expect_err("from_documents_slice invalid UTF-8"),
        Value::deserialize(
            saneyaml::Deserializer::from_slice(input)
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
        saneyaml::from_reader::<_, Value>(FailingReader).expect_err("from_reader read failure"),
        saneyaml::from_documents_reader::<Value, _>(FailingReader)
            .expect_err("from_documents_reader read failure"),
        Value::deserialize(
            saneyaml::Deserializer::from_reader(FailingReader)
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
fn reader_input_limit_stops_non_eof_readers_after_configured_bound() {
    let options = LoadOptions::new().max_input_bytes(4);

    let (reader, bytes_read) = InfiniteReader::new();
    let error = options
        .from_reader::<_, Value>(reader)
        .expect_err("from_reader input limit");
    assert_reader_limit_error(error, bytes_read);

    let (reader, bytes_read) = InfiniteReader::new();
    let error = options
        .from_documents_reader::<Value, _>(reader)
        .expect_err("from_documents_reader input limit");
    assert_reader_limit_error(error, bytes_read);

    let (reader, bytes_read) = InfiniteReader::new();
    let error = Value::deserialize(
        options
            .deserializer_from_reader(reader)
            .next()
            .expect("document"),
    )
    .expect_err("deserializer_from_reader input limit");
    assert_reader_limit_error(error, bytes_read);
}

fn assert_reader_limit_error(error: saneyaml::Error, bytes_read: Rc<Cell<usize>>) {
    let display = error.to_string();
    assert!(
        display.contains("YAML input exceeds configured limit of 4 bytes"),
        "{display}"
    );
    assert_eq!(error.location(), None);
    assert_eq!(bytes_read.get(), 5);
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
fn parser_spans_tab_separated_document_start_inline_scalar() {
    let input = include_str!("fixtures/yaml-test-suite/data/K54U/in.yaml");
    let events = saneyaml::parse_events(input).expect("K54U parses as raw events");
    let Some(saneyaml::Event::DocumentStart { explicit, span, .. }) = events.get(1) else {
        panic!("K54U should emit an explicit document start event");
    };
    assert!(*explicit);
    assert_exact_span(
        span,
        input,
        &ExpectedSpan {
            line: 1,
            column: 1,
            source: "---",
        },
        "K54U",
        "parse_events",
    );

    let (value, span) = events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Scalar { value, span, .. } = event {
                (value == "scalar").then_some((value, span))
            } else {
                None
            }
        })
        .expect("K54U should emit the scalar after the tab separator");
    assert_eq!(value, "scalar");
    assert_exact_span(
        span,
        input,
        &ExpectedSpan {
            line: 1,
            column: 5,
            source: "scalar",
        },
        "K54U",
        "parse_events",
    );
}

#[test]
fn parser_spans_q9wf_flow_key_and_block_value_mapping() {
    let input = include_str!("fixtures/yaml-test-suite/data/Q9WF/in.yaml");
    let events = saneyaml::parse_events(input).expect("Q9WF parses as raw events");
    let mappings = events
        .iter()
        .filter_map(|event| {
            if let saneyaml::Event::MappingStart { style, span, .. } = event {
                Some((*style, span))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(
        mappings.iter().map(|(style, _)| *style).collect::<Vec<_>>(),
        [
            saneyaml::CollectionStyle::Block,
            saneyaml::CollectionStyle::Flow,
            saneyaml::CollectionStyle::Block,
        ]
    );
    assert_exact_span(
        mappings[1].1,
        input,
        &ExpectedSpan {
            line: 1,
            column: 1,
            source: "{",
        },
        "Q9WF",
        "parse_events",
    );
    assert_exact_span(
        mappings[2].1,
        input,
        &ExpectedSpan {
            line: 3,
            column: 3,
            source: "hr:",
        },
        "Q9WF",
        "parse_events",
    );
}

#[test]
fn parser_spans_remaining_tree_deferral_properties() {
    let pw8x = include_str!("fixtures/yaml-test-suite/data/PW8X/in.yaml");
    let pw8x_events = saneyaml::parse_events(pw8x).expect("PW8X parses as raw events");
    let anchors = pw8x_events
        .iter()
        .filter_map(|event| {
            if let saneyaml::Event::Scalar { meta, .. } = event {
                meta.anchor.as_ref()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for (anchor, expected) in [
        (
            "d",
            ExpectedSpan {
                line: 9,
                column: 5,
                source: "&d",
            },
        ),
        (
            "e",
            ExpectedSpan {
                line: 11,
                column: 5,
                source: "&e",
            },
        ),
    ] {
        let span = anchors
            .iter()
            .find(|anchor_meta| anchor_meta.name == anchor)
            .unwrap_or_else(|| panic!("PW8X should emit anchor {anchor}"));
        assert_exact_span(&span.span, pw8x, &expected, "PW8X", "parse_events");
    }

    let six_kgn = include_str!("fixtures/yaml-test-suite/data/6KGN/in.yaml");
    let six_kgn_events = saneyaml::parse_events(six_kgn).expect("6KGN parses as raw events");
    let six_kgn_anchor = six_kgn_events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Scalar { meta, .. } = event {
                meta.anchor.as_ref()
            } else {
                None
            }
        })
        .expect("6KGN should emit the empty scalar anchor");
    assert_exact_span(
        &six_kgn_anchor.span,
        six_kgn,
        &ExpectedSpan {
            line: 2,
            column: 4,
            source: "&anchor",
        },
        "6KGN",
        "parse_events",
    );
    let six_kgn_alias = six_kgn_events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Alias { anchor } = event {
                Some(anchor)
            } else {
                None
            }
        })
        .expect("6KGN should emit the alias");
    assert_exact_span(
        &six_kgn_alias.span,
        six_kgn,
        &ExpectedSpan {
            line: 3,
            column: 4,
            source: "*anchor",
        },
        "6KGN",
        "parse_events",
    );

    let s4jq = include_str!("fixtures/yaml-test-suite/data/S4JQ/in.yaml");
    let s4jq_events = saneyaml::parse_events(s4jq).expect("S4JQ parses as raw events");
    let tagged = s4jq_events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Scalar {
                value, span, meta, ..
            } = event
                && value == "12"
                && meta
                    .tag
                    .as_ref()
                    .is_some_and(|tag| tag.tag == saneyaml::Tag::new("!"))
            {
                Some((span, meta.tag.as_ref().expect("explicit tag")))
            } else {
                None
            }
        })
        .expect("S4JQ should retain the explicit non-specific tag");
    assert_exact_span(
        tagged.0,
        s4jq,
        &ExpectedSpan {
            line: 3,
            column: 5,
            source: "12",
        },
        "S4JQ",
        "parse_events",
    );
    assert_exact_span(
        &tagged.1.span,
        s4jq,
        &ExpectedSpan {
            line: 3,
            column: 3,
            source: "!",
        },
        "S4JQ",
        "parse_events",
    );
}

#[test]
fn parser_spans_tagged_tree_deferral_properties() {
    let f2c7 = include_str!("fixtures/yaml-test-suite/data/F2C7/in.yaml");
    let f2c7_events = saneyaml::parse_events(f2c7).expect("F2C7 parses as raw events");
    let anchored_int = f2c7_events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Scalar {
                value, span, meta, ..
            } = event
                && value == "4"
                && meta
                    .anchor
                    .as_ref()
                    .is_some_and(|anchor| anchor.name == "c")
            {
                Some((
                    span,
                    meta.tag.as_ref().expect("explicit int tag"),
                    meta.anchor.as_ref().expect("anchor c"),
                ))
            } else {
                None
            }
        })
        .expect("F2C7 should emit the tagged anchored scalar");
    assert_exact_span(
        anchored_int.0,
        f2c7,
        &ExpectedSpan {
            line: 3,
            column: 13,
            source: "4",
        },
        "F2C7",
        "parse_events",
    );
    assert_exact_span(
        &anchored_int.1.span,
        f2c7,
        &ExpectedSpan {
            line: 3,
            column: 4,
            source: "!!int",
        },
        "F2C7",
        "parse_events",
    );
    assert_exact_span(
        &anchored_int.2.span,
        f2c7,
        &ExpectedSpan {
            line: 3,
            column: 10,
            source: "&c",
        },
        "F2C7",
        "parse_events",
    );

    for case in [
        (
            "2AUY",
            include_str!("fixtures/yaml-test-suite/data/2AUY/in.yaml"),
            "42",
            ExpectedSpan {
                line: 3,
                column: 10,
                source: "42",
            },
            ExpectedSpan {
                line: 3,
                column: 4,
                source: "!!int",
            },
        ),
        (
            "33X3",
            include_str!("fixtures/yaml-test-suite/data/33X3/in.yaml"),
            "-2",
            ExpectedSpan {
                line: 3,
                column: 9,
                source: "-2",
            },
            ExpectedSpan {
                line: 3,
                column: 3,
                source: "!!int",
            },
        ),
        (
            "74H7",
            include_str!("fixtures/yaml-test-suite/data/74H7/in.yaml"),
            "false",
            ExpectedSpan {
                line: 5,
                column: 18,
                source: "false",
            },
            ExpectedSpan {
                line: 5,
                column: 11,
                source: "!!bool",
            },
        ),
        (
            "L94M",
            include_str!("fixtures/yaml-test-suite/data/L94M/in.yaml"),
            "47",
            ExpectedSpan {
                line: 2,
                column: 9,
                source: "47",
            },
            ExpectedSpan {
                line: 2,
                column: 3,
                source: "!!int",
            },
        ),
    ] {
        let (name, input, value, scalar_span, tag_span) = case;
        let events = saneyaml::parse_events(input).unwrap_or_else(|_| panic!("{name} raw events"));
        let tagged_scalar = events
            .iter()
            .find_map(|event| {
                if let saneyaml::Event::Scalar {
                    value: scalar_value,
                    span,
                    meta,
                    ..
                } = event
                    && scalar_value == value
                {
                    Some((span, meta.tag.as_ref().expect("explicit tag")))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("{name} should emit tagged scalar {value}"));
        assert_exact_span(tagged_scalar.0, input, &scalar_span, name, "parse_events");
        assert_exact_span(
            &tagged_scalar.1.span,
            input,
            &tag_span,
            name,
            "parse_events",
        );
    }

    let fh7j = include_str!("fixtures/yaml-test-suite/data/FH7J/in.yaml");
    let fh7j_events = saneyaml::parse_events(fh7j).expect("FH7J parses as raw events");
    let null_tag = fh7j_events
        .iter()
        .filter_map(|event| {
            if let saneyaml::Event::Scalar { meta, .. } = event {
                meta.tag.as_ref()
            } else {
                None
            }
        })
        .find(|tag| &fh7j[tag.span.start..tag.span.end] == "!!null")
        .expect("FH7J should retain explicit null tag metadata");
    assert_exact_span(
        &null_tag.span,
        fh7j,
        &ExpectedSpan {
            line: 3,
            column: 3,
            source: "!!null",
        },
        "FH7J",
        "parse_events",
    );

    let c4hz = include_str!("fixtures/yaml-test-suite/data/C4HZ/in.yaml");
    let c4hz_events = saneyaml::parse_events(c4hz).expect("C4HZ parses as raw events");
    let color = c4hz_events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Scalar { value, span, .. } = event
                && value == "0xFFEEBB"
            {
                Some(span)
            } else {
                None
            }
        })
        .expect("C4HZ should emit the color scalar");
    assert_exact_span(
        color,
        c4hz,
        &ExpectedSpan {
            line: 13,
            column: 10,
            source: "0xFFEEBB",
        },
        "C4HZ",
        "parse_events",
    );
}

#[test]
fn parser_spans_remaining_shared_reference_deferral_properties() {
    let hwv9 = include_str!("fixtures/yaml-test-suite/data/HWV9/in.yaml");
    let hwv9_events = saneyaml::parse_events(hwv9).expect("HWV9 parses as raw events");
    assert_eq!(
        hwv9_events,
        vec![saneyaml::Event::StreamStart, saneyaml::Event::StreamEnd],
        "HWV9 document-end-only stream stays accepted without manufacturing a document event",
    );

    let qt73 = include_str!("fixtures/yaml-test-suite/data/QT73/in.yaml");
    let qt73_events = saneyaml::parse_events(qt73).expect("QT73 parses as raw events");
    assert_eq!(
        qt73_events,
        vec![saneyaml::Event::StreamStart, saneyaml::Event::StreamEnd],
        "QT73 comment plus document-end marker stays accepted without manufacturing a document event",
    );

    let four_abk = include_str!("fixtures/yaml-test-suite/data/4ABK/in.yaml");
    let four_abk_events = saneyaml::parse_events(four_abk).expect("4ABK parses as raw events");
    let flow_uri = four_abk_events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Scalar { value, span, .. } = event
                && value == "http://foo.com"
            {
                Some(span)
            } else {
                None
            }
        })
        .expect("4ABK should emit the plain URI scalar");
    assert_exact_span(
        flow_uri,
        four_abk,
        &ExpectedSpan {
            line: 3,
            column: 1,
            source: "http://foo.com",
        },
        "4ABK",
        "parse_events",
    );

    let eight_xyn = include_str!("fixtures/yaml-test-suite/data/8XYN/in.yaml");
    let eight_xyn_events = saneyaml::parse_events(eight_xyn).expect("8XYN parses as raw events");
    let unicode_anchor = eight_xyn_events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Scalar { meta, .. } = event {
                meta.anchor.as_ref()
            } else {
                None
            }
        })
        .expect("8XYN should emit Unicode anchor metadata");
    assert_exact_span(
        &unicode_anchor.span,
        eight_xyn,
        &ExpectedSpan {
            line: 2,
            column: 3,
            source: "&😁",
        },
        "8XYN",
        "parse_events",
    );

    let w5vh = include_str!("fixtures/yaml-test-suite/data/W5VH/in.yaml");
    let w5vh_events = saneyaml::parse_events(w5vh).expect("W5VH parses as raw events");
    let w5vh_anchor = w5vh_events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Scalar { meta, .. } = event {
                meta.anchor.as_ref()
            } else {
                None
            }
        })
        .expect("W5VH should emit punctuation-heavy anchor metadata");
    assert_exact_span(
        &w5vh_anchor.span,
        w5vh,
        &ExpectedSpan {
            line: 1,
            column: 4,
            source: "&:@*!$\"<foo>:",
        },
        "W5VH",
        "parse_events",
    );
    let w5vh_alias = w5vh_events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::Alias { anchor } = event {
                Some(anchor)
            } else {
                None
            }
        })
        .expect("W5VH should emit punctuation-heavy alias metadata");
    assert_exact_span(
        &w5vh_alias.span,
        w5vh,
        &ExpectedSpan {
            line: 2,
            column: 4,
            source: "*:@*!$\"<foo>:",
        },
        "W5VH",
        "parse_events",
    );
}

#[test]
fn parser_spans_empty_and_comment_only_stream_deferrals() {
    for (name, input, expected_docs) in [
        (
            "AVM7",
            include_str!("fixtures/yaml-test-suite/data/AVM7/in.yaml"),
            0usize,
        ),
        (
            "8G76",
            include_str!("fixtures/yaml-test-suite/data/8G76/in.yaml"),
            0usize,
        ),
        (
            "98YD",
            include_str!("fixtures/yaml-test-suite/data/98YD/in.yaml"),
            0usize,
        ),
    ] {
        let docs =
            saneyaml::parse_documents(input).expect("document-count deferral parses locally");
        assert_eq!(docs.len(), expected_docs, "{name} local document count");
    }

    let stream = include_str!("fixtures/yaml-test-suite/data/7Z25/in.yaml");
    let events = saneyaml::parse_events(stream).expect("7Z25 parses as raw events");
    let end = events
        .iter()
        .find_map(|event| {
            if let saneyaml::Event::DocumentEnd { span, .. } = event {
                Some(span)
            } else {
                None
            }
        })
        .expect("7Z25 should emit a document-end marker");
    assert_exact_span(
        end,
        stream,
        &ExpectedSpan {
            line: 3,
            column: 1,
            source: "...",
        },
        "7Z25",
        "parse_events",
    );
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
fn diagnostics_expose_category_path_and_rendered_source_context() {
    let alias_input = "service: *missing\n";
    let alias_error = parse_str(alias_input).expect_err("undefined alias");
    assert_eq!(alias_error.category(), saneyaml::ErrorCategory::Reference);
    assert_eq!(
        alias_error.to_string(),
        "unknown anchor `missing` at line 1, column 10"
    );
    assert_eq!(
        alias_error.render_source(alias_input).to_string(),
        "unknown anchor `missing` at line 1, column 10\n  |\n1 | service: *missing\n  |          ^^^^^^^^"
    );

    let duplicate_input = "root:\n  true: first\n  true: second\n";
    let duplicate_error = parse_str(duplicate_input).expect_err("duplicate key");
    assert_eq!(
        duplicate_error.category(),
        saneyaml::ErrorCategory::DuplicateKey
    );
    assert_eq!(
        duplicate_error.render_source(duplicate_input).to_string(),
        "duplicate mapping key `true` at line 3, column 3\n  |\n3 |   true: second\n  |   ^^^^\nprevious key is here\n  |\n2 |   true: first\n  |   ^^^^"
    );

    let input_limit = LoadOptions::new()
        .max_input_bytes(4)
        .parse_str("name: app\n")
        .expect_err("input limit");
    assert_eq!(input_limit.category(), saneyaml::ErrorCategory::Limit);
    assert_eq!(
        input_limit.render_source("name: app\n").to_string(),
        input_limit.to_string()
    );

    let utf8 = saneyaml::from_slice::<Value>(b"bad: \xFF").expect_err("invalid UTF-8");
    assert_eq!(utf8.category(), saneyaml::ErrorCategory::Encoding);

    let reader = saneyaml::from_reader::<_, Value>(FailingReader).expect_err("reader error");
    assert_eq!(reader.category(), saneyaml::ErrorCategory::Io);

    let syntax = parse_str("\tbad: true\n").expect_err("tab indentation");
    assert_eq!(syntax.category(), saneyaml::ErrorCategory::Syntax);
}

#[test]
fn source_render_options_context_lines_control_primary_and_related_spans() {
    let input = "root:\n  true: first\n  true: second\n";
    let error = parse_str(input).expect_err("duplicate key");
    let compact = error.render_source(input).to_string();

    let mut options = saneyaml::SourceRenderOptions::default();
    options.context_lines = 0;
    assert_eq!(
        error.render_source_with_options(input, options).to_string(),
        compact
    );
    assert_eq!(
        error
            .diagnostic()
            .render_source_with_options(input, options)
            .to_string(),
        compact
    );

    options.context_lines = 1;
    assert_eq!(
        error.render_source_with_options(input, options).to_string(),
        "duplicate mapping key `true` at line 3, column 3\n  |\n2 |   true: first\n3 |   true: second\n  |   ^^^^\nprevious key is here\n  |\n1 | root:\n2 |   true: first\n  |   ^^^^\n3 |   true: second"
    );
    assert_eq!(
        error
            .diagnostic()
            .render_source_with_options(input, options)
            .to_string(),
        error.render_source_with_options(input, options).to_string()
    );
}

#[test]
fn serde_diagnostics_include_key_paths_across_entrypoints() {
    for (shape, input, expected_path, expected_source) in [
        (
            SerdeShape::NestedServer,
            "server:\n  port: nope\n",
            "server.port",
            "nope",
        ),
        (
            SerdeShape::Ports,
            "ports:\n  - 80\n  - nope\n",
            "ports[1]",
            "nope",
        ),
        (
            SerdeShape::RootServers,
            "- port: 80\n- port: nope\n",
            "[1].port",
            "nope",
        ),
        (
            SerdeShape::Strict,
            "name: app\nextra: true\n",
            "extra",
            "extra",
        ),
    ] {
        let mut root_previous = None;
        let mut direct_previous = None;
        for entrypoint in SerdeEntrypoint::ALL {
            let error = entrypoint.error(shape, input);
            assert_eq!(error.category(), saneyaml::ErrorCategory::Data);
            assert_eq!(
                error.path().map(ToString::to_string).as_deref(),
                Some(expected_path),
                "{} path",
                entrypoint.name()
            );
            assert_eq!(
                &input[error.span().start..error.span().end],
                expected_source
            );
            let record = diagnostic_record(&error, input);
            let previous = if entrypoint.is_direct_deserializer() {
                &mut direct_previous
            } else {
                &mut root_previous
            };
            if let Some(previous) = previous {
                assert_eq!(previous, &record, "{} parity", entrypoint.name());
            }
            *previous = Some(record);
        }
    }
}

#[test]
fn value_deserialization_paths_do_not_require_source_locations() {
    let value = saneyaml::from_str::<Value>("ports:\n  - 80\n  - nope\n").expect("value parses");
    let owned_error =
        saneyaml::from_value::<PortsMatrixConfig>(value.clone()).expect_err("owned value");
    assert_eq!(owned_error.location(), None);
    assert_eq!(
        owned_error.path().map(ToString::to_string).as_deref(),
        Some("ports[1]")
    );

    let borrowed_error =
        PortsMatrixConfig::deserialize(&value).expect_err("borrowed value deserialization");
    assert_eq!(borrowed_error.location(), None);
    assert_eq!(
        borrowed_error.path().map(ToString::to_string).as_deref(),
        Some("ports[1]")
    );
}

#[test]
fn document_diagnostics_include_zero_based_document_index() {
    let input = "---\nports: [80]\n---\nports: no\n";
    let errors = [
        saneyaml::from_documents_str::<PortsMatrixConfig>(input).expect_err("documents str"),
        saneyaml::from_documents_slice::<PortsMatrixConfig>(input.as_bytes())
            .expect_err("documents slice"),
        saneyaml::from_documents_reader::<PortsMatrixConfig, _>(Cursor::new(input.as_bytes()))
            .expect_err("documents reader"),
    ];
    let first = diagnostic_record(&errors[0], input);
    for error in &errors {
        assert_eq!(error.document_index(), Some(1));
        assert_eq!(
            error.path().map(ToString::to_string).as_deref(),
            Some("ports")
        );
        assert_eq!(diagnostic_record(error, input), first);
    }

    let streamed_error = saneyaml::Deserializer::from_str(input)
        .enumerate()
        .find_map(|(index, document)| {
            PortsMatrixConfig::deserialize(document)
                .err()
                .map(|error| (index, error))
        })
        .expect("streamed document error");
    assert_eq!(streamed_error.0, 1);
    assert_eq!(streamed_error.1.document_index(), Some(1));
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
fn rendered_source_caret_uses_reported_byte_column_after_multibyte() {
    let input = "éé: [x\n";
    let error = parse_str(input).expect_err("unterminated flow sequence");

    assert_eq!(error.span().line, 1);
    assert_eq!(error.span().column, 9);
    let rendered = error.render_source(input).to_string();
    assert!(rendered.contains("line 1, column 9"), "{rendered}");
    let marker = rendered.lines().last().expect("marker line");
    assert_eq!(marker.strip_prefix("  | "), Some("        ^"));
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
        saneyaml::from_str::<Value>(&input)
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

        let error = saneyaml::from_str::<Value>(&input)
            .expect_err(&format!("{name} serde entrypoint should reject"));
        assert_depth_limit_error(&error);
    }
}

#[test]
fn alias_expansion_boundary_keeps_raw_events_safe() {
    let below = alias_expansion_chain(4);
    parse_str(&below).expect("four-level alias expansion chain stays below budget");
    saneyaml::from_str::<Value>(&below).expect("serde reads below-budget alias chain");
    assert!(
        saneyaml::parse_events(&below)
            .expect("raw events expose below-budget aliases")
            .iter()
            .any(|event| matches!(event, saneyaml::Event::Alias { anchor } if anchor.name == "d"))
    );

    let above = alias_expansion_chain(5);
    let error = parse_str(&above).expect_err("five-level alias expansion chain crosses budget");
    assert!(error.to_string().contains("alias expansion limit exceeded"));
    assert!(error.location().is_some());
    let error =
        saneyaml::from_str::<Value>(&above).expect_err("serde rejects above-budget alias chain");
    assert!(error.to_string().contains("alias expansion limit exceeded"));
    assert!(error.location().is_some());
    assert!(
        saneyaml::parse_events(&above)
            .expect("raw events remain safe because aliases are not expanded")
            .iter()
            .any(|event| matches!(event, saneyaml::Event::Alias { anchor } if anchor.name == "e"))
    );
}

#[test]
fn load_options_alias_expansion_limit_rejects_across_entrypoints() {
    let input = "base: &base {a: 1}\ntarget: *base\n";
    let options = LoadOptions::new().max_alias_expansion_nodes(2);
    let mut errors = vec![
        options
            .parse_str(input)
            .expect_err("parse_str alias expansion budget"),
        options
            .parse_bytes(input.as_bytes())
            .expect_err("parse_bytes alias expansion budget"),
        options
            .from_str::<Value>(input)
            .expect_err("from_str alias expansion budget"),
        options
            .from_slice::<Value>(input.as_bytes())
            .expect_err("from_slice alias expansion budget"),
        options
            .from_reader::<_, Value>(Cursor::new(input.as_bytes()))
            .expect_err("from_reader alias expansion budget"),
        options
            .from_documents_str::<Value>(input)
            .expect_err("from_documents_str alias expansion budget"),
        options
            .from_documents_slice::<Value>(input.as_bytes())
            .expect_err("from_documents_slice alias expansion budget"),
        options
            .from_documents_reader::<Value, _>(Cursor::new(input.as_bytes()))
            .expect_err("from_documents_reader alias expansion budget"),
    ];
    errors.push(
        Value::deserialize(options.deserializer_from_str(input))
            .expect_err("deserializer_from_str alias expansion budget"),
    );
    errors.push(
        Value::deserialize(options.deserializer_from_slice(input.as_bytes()))
            .expect_err("deserializer_from_slice alias expansion budget"),
    );
    errors.push(
        Value::deserialize(options.deserializer_from_reader(Cursor::new(input.as_bytes())))
            .expect_err("deserializer_from_reader alias expansion budget"),
    );

    for error in errors {
        assert_configured_alias_expansion_error(&error, input);
    }

    let relaxed: Value = LoadOptions::new()
        .max_alias_expansion_nodes(3)
        .from_str(input)
        .expect("caller-selected alias budget permits this expansion");
    assert_eq!(relaxed["target"]["a"].as_i64(), Some(1));
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

    saneyaml::parse_events(&input)
        .expect("raw events stay available without duplicate-key expansion");
}

#[test]
fn parser_rejects_alias_expansion_bomb_with_alias_span() {
    let error = parse_str(ALIAS_EXPANSION_BOMB).expect_err("alias expansion limit");
    assert_alias_expansion_error(&error);

    let from_str_error =
        saneyaml::from_str::<Value>(ALIAS_EXPANSION_BOMB).expect_err("serde alias expansion limit");
    assert_alias_expansion_error(&from_str_error);

    let documents_error = saneyaml::from_documents_str::<Value>(ALIAS_EXPANSION_BOMB)
        .expect_err("document serde alias expansion limit");
    assert_alias_expansion_error(&documents_error);

    let events = saneyaml::parse_events(ALIAS_EXPANSION_BOMB)
        .expect("raw events do not expand aliases when validating stream safety");
    assert!(events.iter().any(|event| {
        matches!(
            event,
            saneyaml::Event::Alias { anchor }
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

fn assert_alias_expansion_error(error: &saneyaml::Error) {
    assert!(error.to_string().contains("alias expansion limit exceeded"));
    assert_eq!(error.span().line, 5);
    assert_eq!(error.span().column, 12);
    assert_eq!(
        &ALIAS_EXPANSION_BOMB[error.span().start..error.span().end],
        "*d"
    );
}

fn assert_configured_alias_expansion_error(error: &saneyaml::Error, input: &str) {
    assert!(error.to_string().contains("alias expansion limit exceeded"));
    assert_eq!(error.span().line, 2);
    assert_eq!(error.span().column, 9);
    assert_eq!(&input[error.span().start..error.span().end], "*base");
}

fn assert_depth_limit_error(error: &saneyaml::Error) {
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

    fn error(self, input: &str) -> saneyaml::Error {
        match self {
            Self::ParseStr => parse_str(input).expect_err("parse_str should reject"),
            Self::ParseBytes => {
                parse_bytes(input.as_bytes()).expect_err("parse_bytes should reject")
            }
            Self::FromStr => {
                saneyaml::from_str::<Value>(input).expect_err("from_str should reject")
            }
            Self::FromSlice => saneyaml::from_slice::<Value>(input.as_bytes())
                .expect_err("from_slice should reject"),
            Self::FromReader => saneyaml::from_reader::<_, Value>(Cursor::new(input.as_bytes()))
                .expect_err("from_reader should reject"),
            Self::DirectDeserializerStr => {
                Value::deserialize(saneyaml::Deserializer::from_str(input))
                    .expect_err("direct string deserializer should reject")
            }
            Self::DirectDeserializerSlice => {
                Value::deserialize(saneyaml::Deserializer::from_slice(input.as_bytes()))
                    .expect_err("direct slice deserializer should reject")
            }
            Self::DirectDeserializerReader => Value::deserialize(
                saneyaml::Deserializer::from_reader(Cursor::new(input.as_bytes())),
            )
            .expect_err("direct reader deserializer should reject"),
        }
    }
}

#[derive(Clone, Copy)]
enum SerdeShape {
    Strict,
    Ports,
    Port,
    NestedServer,
    RootServers,
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

    fn is_direct_deserializer(self) -> bool {
        matches!(
            self,
            Self::DirectDeserializerStr
                | Self::DirectDeserializerSlice
                | Self::DirectDeserializerReader
        )
    }

    fn error(self, shape: SerdeShape, input: &str) -> saneyaml::Error {
        match (self, shape) {
            (Self::FromStr, SerdeShape::Strict) => {
                saneyaml::from_str::<StrictMatrixConfig>(input).expect_err("from_str should reject")
            }
            (Self::FromStr, SerdeShape::Ports) => {
                saneyaml::from_str::<PortsMatrixConfig>(input).expect_err("from_str should reject")
            }
            (Self::FromStr, SerdeShape::Port) => {
                saneyaml::from_str::<PortMatrixConfig>(input).expect_err("from_str should reject")
            }
            (Self::FromStr, SerdeShape::NestedServer) => {
                saneyaml::from_str::<NestedServerConfig>(input).expect_err("from_str should reject")
            }
            (Self::FromStr, SerdeShape::RootServers) => {
                saneyaml::from_str::<RootServersConfig>(input).expect_err("from_str should reject")
            }
            (Self::FromSlice, SerdeShape::Strict) => {
                saneyaml::from_slice::<StrictMatrixConfig>(input.as_bytes())
                    .expect_err("from_slice should reject")
            }
            (Self::FromSlice, SerdeShape::Ports) => {
                saneyaml::from_slice::<PortsMatrixConfig>(input.as_bytes())
                    .expect_err("from_slice should reject")
            }
            (Self::FromSlice, SerdeShape::Port) => {
                saneyaml::from_slice::<PortMatrixConfig>(input.as_bytes())
                    .expect_err("from_slice should reject")
            }
            (Self::FromSlice, SerdeShape::NestedServer) => {
                saneyaml::from_slice::<NestedServerConfig>(input.as_bytes())
                    .expect_err("from_slice should reject")
            }
            (Self::FromSlice, SerdeShape::RootServers) => {
                saneyaml::from_slice::<RootServersConfig>(input.as_bytes())
                    .expect_err("from_slice should reject")
            }
            (Self::FromReader, SerdeShape::Strict) => {
                saneyaml::from_reader::<_, StrictMatrixConfig>(Cursor::new(input.as_bytes()))
                    .expect_err("from_reader should reject")
            }
            (Self::FromReader, SerdeShape::Ports) => {
                saneyaml::from_reader::<_, PortsMatrixConfig>(Cursor::new(input.as_bytes()))
                    .expect_err("from_reader should reject")
            }
            (Self::FromReader, SerdeShape::Port) => {
                saneyaml::from_reader::<_, PortMatrixConfig>(Cursor::new(input.as_bytes()))
                    .expect_err("from_reader should reject")
            }
            (Self::FromReader, SerdeShape::NestedServer) => {
                saneyaml::from_reader::<_, NestedServerConfig>(Cursor::new(input.as_bytes()))
                    .expect_err("from_reader should reject")
            }
            (Self::FromReader, SerdeShape::RootServers) => {
                saneyaml::from_reader::<_, RootServersConfig>(Cursor::new(input.as_bytes()))
                    .expect_err("from_reader should reject")
            }
            (Self::DirectDeserializerStr, SerdeShape::Strict) => {
                StrictMatrixConfig::deserialize(saneyaml::Deserializer::from_str(input))
                    .expect_err("direct string deserializer should reject")
            }
            (Self::DirectDeserializerStr, SerdeShape::Ports) => {
                PortsMatrixConfig::deserialize(saneyaml::Deserializer::from_str(input))
                    .expect_err("direct string deserializer should reject")
            }
            (Self::DirectDeserializerStr, SerdeShape::Port) => {
                PortMatrixConfig::deserialize(saneyaml::Deserializer::from_str(input))
                    .expect_err("direct string deserializer should reject")
            }
            (Self::DirectDeserializerStr, SerdeShape::NestedServer) => {
                NestedServerConfig::deserialize(saneyaml::Deserializer::from_str(input))
                    .expect_err("direct string deserializer should reject")
            }
            (Self::DirectDeserializerStr, SerdeShape::RootServers) => {
                RootServersConfig::deserialize(saneyaml::Deserializer::from_str(input))
                    .expect_err("direct string deserializer should reject")
            }
            (Self::DirectDeserializerSlice, SerdeShape::Strict) => StrictMatrixConfig::deserialize(
                saneyaml::Deserializer::from_slice(input.as_bytes()),
            )
            .expect_err("direct slice deserializer should reject"),
            (Self::DirectDeserializerSlice, SerdeShape::Ports) => {
                PortsMatrixConfig::deserialize(saneyaml::Deserializer::from_slice(input.as_bytes()))
                    .expect_err("direct slice deserializer should reject")
            }
            (Self::DirectDeserializerSlice, SerdeShape::Port) => {
                PortMatrixConfig::deserialize(saneyaml::Deserializer::from_slice(input.as_bytes()))
                    .expect_err("direct slice deserializer should reject")
            }
            (Self::DirectDeserializerSlice, SerdeShape::NestedServer) => {
                NestedServerConfig::deserialize(saneyaml::Deserializer::from_slice(
                    input.as_bytes(),
                ))
                .expect_err("direct slice deserializer should reject")
            }
            (Self::DirectDeserializerSlice, SerdeShape::RootServers) => {
                RootServersConfig::deserialize(saneyaml::Deserializer::from_slice(input.as_bytes()))
                    .expect_err("direct slice deserializer should reject")
            }
            (Self::DirectDeserializerReader, SerdeShape::Strict) => {
                StrictMatrixConfig::deserialize(saneyaml::Deserializer::from_reader(Cursor::new(
                    input.as_bytes(),
                )))
                .expect_err("direct reader deserializer should reject")
            }
            (Self::DirectDeserializerReader, SerdeShape::Ports) => PortsMatrixConfig::deserialize(
                saneyaml::Deserializer::from_reader(Cursor::new(input.as_bytes())),
            )
            .expect_err("direct reader deserializer should reject"),
            (Self::DirectDeserializerReader, SerdeShape::Port) => PortMatrixConfig::deserialize(
                saneyaml::Deserializer::from_reader(Cursor::new(input.as_bytes())),
            )
            .expect_err("direct reader deserializer should reject"),
            (Self::DirectDeserializerReader, SerdeShape::NestedServer) => {
                NestedServerConfig::deserialize(saneyaml::Deserializer::from_reader(Cursor::new(
                    input.as_bytes(),
                )))
                .expect_err("direct reader deserializer should reject")
            }
            (Self::DirectDeserializerReader, SerdeShape::RootServers) => {
                RootServersConfig::deserialize(saneyaml::Deserializer::from_reader(Cursor::new(
                    input.as_bytes(),
                )))
                .expect_err("direct reader deserializer should reject")
            }
        }
    }
}

fn diagnostic_record(error: &saneyaml::Error, input: &str) -> String {
    format!(
        "category={:?}\ndisplay={}\nspan={}:{}:{}:{}\nsource={:?}\npath={}\ndocument={:?}\nrendered={}",
        error.category(),
        error,
        error.span().start,
        error.span().end,
        error.span().line,
        error.span().column,
        input
            .get(error.span().start..error.span().end)
            .unwrap_or(""),
        error
            .path()
            .map(ToString::to_string)
            .unwrap_or_else(|| "<none>".to_string()),
        error.document_index(),
        error.render_source(input),
    )
}

fn assert_exact_diagnostic(
    error: &saneyaml::Error,
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
    actual: &saneyaml::Span,
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
