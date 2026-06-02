use saneyaml::{Error, Event, LoadOptions, Node, Value};
use serde::Deserialize;
use std::fs;
use std::io::Cursor;
use std::path::Path;

const DOS_MANIFEST: &str = include_str!("fixtures/dos/manifest.toml");
const CLEAN_SMALL: &str = include_str!("fixtures/dos/clean/small.yaml");
const REAL_WORLD_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/real-world");
const REAL_WORLD_SOURCE: &str = include_str!("fixtures/real-world/SOURCE.toml");
const ALIAS_BOMB: &str = include_str!("fixtures/dos/adversarial/alias-expansion-bomb.yaml");
const DEEP_FLOW_NESTING: &str = include_str!("fixtures/dos/adversarial/deep-flow-nesting.yaml");
const OVERSIZED_PLAIN_SCALAR: &str =
    include_str!("fixtures/dos/adversarial/oversized-plain-scalar.yaml");
const OVERSIZED_BLOCK_SCALAR: &str =
    include_str!("fixtures/dos/adversarial/oversized-block-scalar.yaml");
const OVERSIZED_DOUBLE_QUOTED_SCALAR: &str =
    include_str!("fixtures/dos/adversarial/oversized-double-quoted-scalar.yaml");
const WIDE_FLOW_SEQUENCE: &str = include_str!("fixtures/dos/adversarial/wide-flow-sequence.yaml");
const WIDE_FLOW_MAPPING: &str = include_str!("fixtures/dos/adversarial/wide-flow-mapping.yaml");

#[derive(Debug, Deserialize)]
struct RealWorldManifest {
    fixture: Vec<RealWorldFixture>,
}

#[derive(Debug, Deserialize)]
struct RealWorldFixture {
    path: String,
}

#[test]
fn adversarial_manifest_documents_goal_06_cases() {
    for required in [
        "deep-flow-nesting",
        "oversized-plain-scalar",
        "oversized-block-scalar",
        "oversized-double-quoted-scalar",
        "wide-flow-sequence",
        "wide-flow-mapping",
        "alias-expansion-bomb",
        "nesting-depth",
        "scalar-growth",
        "collection-growth",
        "alias-expansion",
    ] {
        assert!(
            DOS_MANIFEST.contains(required),
            "DoS manifest must record {required}"
        );
    }
}

#[test]
fn default_limits_accept_all_real_world_fixtures() {
    let manifest: RealWorldManifest =
        toml::from_str(REAL_WORLD_SOURCE).expect("real-world manifest parses");
    assert_eq!(manifest.fixture.len(), 33);
    let root = Path::new(REAL_WORLD_ROOT);
    let mut documents = 0usize;
    for fixture in manifest.fixture {
        let input = fs::read_to_string(root.join(&fixture.path))
            .unwrap_or_else(|error| panic!("read real-world fixture {}: {error}", fixture.path));
        let parsed = LoadOptions::new()
            .parse_documents(&input)
            .unwrap_or_else(|error| panic!("default limits parse {}: {error}", fixture.path));
        documents += parsed.len();
        LoadOptions::new()
            .stream_events(&input)
            .unwrap_or_else(|error| {
                panic!("default limits stream events {}: {error}", fixture.path)
            })
            .collect::<saneyaml::Result<Vec<Event>>>()
            .unwrap_or_else(|error| {
                panic!("default event stream accepts {}: {error}", fixture.path)
            });
        LoadOptions::new()
            .from_documents_str::<Value>(&input)
            .unwrap_or_else(|error| panic!("default Serde accepts {}: {error}", fixture.path));
    }
    assert_eq!(documents, 39);
}

#[test]
fn clean_inputs_load_with_default_and_tight_limits() {
    for options in [
        LoadOptions::new(),
        LoadOptions::new()
            .max_nesting_depth(8)
            .max_scalar_bytes(16)
            .max_collection_items(4),
    ] {
        options.parse_str(CLEAN_SMALL).expect("parse clean fixture");
        options
            .parse_borrowed_documents(CLEAN_SMALL)
            .expect("borrowed clean fixture");
        options
            .stream_events(CLEAN_SMALL)
            .expect("event stream")
            .collect::<saneyaml::Result<Vec<_>>>()
            .expect("events clean fixture");
        options
            .stream_documents(CLEAN_SMALL)
            .expect("document stream")
            .collect::<saneyaml::Result<Vec<_>>>()
            .expect("documents clean fixture");
        options
            .from_str::<Value>(CLEAN_SMALL)
            .expect("Serde clean fixture");
        saneyaml::parse_lossless_with_options(CLEAN_SMALL, options)
            .expect("lossless clean fixture");
    }
}

#[test]
fn nesting_limit_rejects_every_load_shape_with_spans() {
    let input = DEEP_FLOW_NESTING;
    let options = LoadOptions::new().max_nesting_depth(4);
    assert_all_expanding_entrypoints_reject(input, options, "maximum YAML nesting depth exceeded");
    assert_event_entrypoints_reject(input, options, "maximum YAML nesting depth exceeded");
    assert_lossless_rejects(input, options, "maximum YAML nesting depth exceeded");
}

#[test]
fn scalar_limit_rejects_plain_and_block_scalars_with_spans() {
    let options = LoadOptions::new().max_scalar_bytes(8);
    for input in [
        OVERSIZED_PLAIN_SCALAR,
        OVERSIZED_BLOCK_SCALAR,
        OVERSIZED_DOUBLE_QUOTED_SCALAR,
    ] {
        assert_all_expanding_entrypoints_reject(
            input,
            options,
            "YAML scalar exceeds configured limit",
        );
        assert_event_entrypoints_reject(input, options, "YAML scalar exceeds configured limit");
        assert_lossless_rejects(input, options, "YAML scalar exceeds configured limit");
    }
}

#[test]
fn collection_limit_rejects_wide_sequences_and_mappings_with_spans() {
    let options = LoadOptions::new().max_collection_items(3);
    for input in [WIDE_FLOW_SEQUENCE, WIDE_FLOW_MAPPING] {
        assert_all_expanding_entrypoints_reject(
            input,
            options,
            "YAML collection exceeds configured limit",
        );
        assert_event_entrypoints_reject(input, options, "YAML collection exceeds configured limit");
        assert_lossless_rejects(input, options, "YAML collection exceeds configured limit");
    }
}

#[test]
fn alias_bomb_rejects_semantic_loaders_but_raw_events_do_not_expand() {
    let options = LoadOptions::new().max_alias_expansion_nodes(8);

    assert_all_expanding_entrypoints_reject(ALIAS_BOMB, options, "alias expansion limit exceeded");

    let events = options
        .stream_events(ALIAS_BOMB)
        .expect("raw event stream")
        .collect::<saneyaml::Result<Vec<Event>>>()
        .expect("raw events validate aliases without expansion");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Alias { anchor } if anchor.name == "e"))
    );
    saneyaml::parse_lossless_with_options(ALIAS_BOMB, options)
        .expect("lossless graph validates aliases without semantic expansion");
}

#[test]
fn default_limits_reject_committed_adversarial_fixtures() {
    for (input, expected) in [
        (ALIAS_BOMB, "alias expansion limit exceeded"),
        (DEEP_FLOW_NESTING, "maximum YAML nesting depth exceeded"),
        (
            OVERSIZED_PLAIN_SCALAR,
            "YAML scalar exceeds configured limit",
        ),
        (
            OVERSIZED_BLOCK_SCALAR,
            "YAML scalar exceeds configured limit",
        ),
        (
            OVERSIZED_DOUBLE_QUOTED_SCALAR,
            "YAML scalar exceeds configured limit",
        ),
        (
            WIDE_FLOW_SEQUENCE,
            "YAML collection exceeds configured limit",
        ),
        (
            WIDE_FLOW_MAPPING,
            "YAML collection exceeds configured limit",
        ),
    ] {
        assert_all_expanding_entrypoints_reject(input, LoadOptions::new(), expected);
    }
}

fn assert_all_expanding_entrypoints_reject(input: &str, options: LoadOptions, expected: &str) {
    for error in [
        options.parse_str(input).expect_err("parse_str rejects"),
        options
            .parse_bytes(input.as_bytes())
            .expect_err("parse_bytes rejects"),
        options
            .parse_documents(input)
            .expect_err("parse_documents rejects"),
        options
            .parse_borrowed_documents(input)
            .expect_err("parse_borrowed_documents rejects"),
        options
            .from_str::<Value>(input)
            .expect_err("from_str rejects"),
        options
            .from_slice::<Value>(input.as_bytes())
            .expect_err("from_slice rejects"),
        options
            .from_reader::<_, Value>(Cursor::new(input.as_bytes()))
            .expect_err("from_reader rejects"),
        options
            .from_documents_str::<Value>(input)
            .expect_err("from_documents_str rejects"),
        options
            .from_documents_slice::<Value>(input.as_bytes())
            .expect_err("from_documents_slice rejects"),
        options
            .from_documents_reader::<Value, _>(Cursor::new(input.as_bytes()))
            .expect_err("from_documents_reader rejects"),
        options
            .stream_documents(input)
            .expect("document stream constructs")
            .collect::<saneyaml::Result<Vec<Node>>>()
            .expect_err("DocumentStream rejects"),
        options
            .deserializer_from_str(input)
            .map(Value::deserialize)
            .collect::<saneyaml::Result<Vec<Value>>>()
            .expect_err("Deserializer::from_str rejects"),
        options
            .deserializer_from_slice(input.as_bytes())
            .map(Value::deserialize)
            .collect::<saneyaml::Result<Vec<Value>>>()
            .expect_err("Deserializer::from_slice rejects"),
        options
            .deserializer_from_reader(Cursor::new(input.as_bytes()))
            .map(Value::deserialize)
            .collect::<saneyaml::Result<Vec<Value>>>()
            .expect_err("Deserializer::from_reader rejects"),
    ] {
        assert_limit_error(input, &error, expected);
    }
}

fn assert_event_entrypoints_reject(input: &str, options: LoadOptions, expected: &str) {
    for error in [
        options
            .stream_events(input)
            .expect("event stream constructs")
            .collect::<saneyaml::Result<Vec<Event>>>()
            .expect_err("EventStream::from_str rejects"),
        options
            .stream_events_slice(input.as_bytes())
            .expect("event slice stream constructs")
            .collect::<saneyaml::Result<Vec<Event>>>()
            .expect_err("EventStream::from_slice rejects"),
        options
            .stream_events_reader(Cursor::new(input.as_bytes()))
            .expect("event reader stream constructs")
            .collect::<saneyaml::Result<Vec<Event>>>()
            .expect_err("EventStream::from_reader rejects"),
    ] {
        assert_limit_error(input, &error, expected);
    }
}

fn assert_lossless_rejects(input: &str, options: LoadOptions, expected: &str) {
    for error in [
        saneyaml::parse_lossless_with_options(input, options).expect_err("lossless str rejects"),
        saneyaml::parse_lossless_bytes_with_options(input.as_bytes(), options)
            .expect_err("lossless bytes rejects"),
    ] {
        assert_limit_error(input, &error, expected);
    }
}

fn assert_limit_error(input: &str, error: &Error, expected: &str) {
    let display = error.to_string();
    assert!(
        display.contains(expected),
        "expected {expected:?} in {display:?}"
    );
    let diagnostic = error.diagnostic();
    assert!(
        !diagnostic.message.is_empty(),
        "limit errors keep a diagnostic message"
    );
    let span = diagnostic.span;
    assert!(span.line >= 1, "limit error keeps a line span: {span:?}");
    assert!(
        span.column >= 1,
        "limit error keeps a column span: {span:?}"
    );
    assert!(
        span.start <= span.end && span.end <= input.len(),
        "limit error span stays inside input: {span:?}, len={}",
        input.len()
    );
}
