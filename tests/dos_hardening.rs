use serde::Deserialize;
use std::io::Cursor;
use yaml::{Error, Event, LoadOptions, Node, Value};

const DOS_MANIFEST: &str = include_str!("fixtures/dos/manifest.toml");
const CLEAN_SMALL: &str = include_str!("fixtures/dos/clean/small.yaml");

const ALIAS_BOMB: &str = "\
a: &a [lol, lol, lol, lol, lol, lol, lol, lol]
b: &b [*a, *a, *a, *a, *a, *a, *a, *a]
c: &c [*b, *b, *b, *b, *b, *b, *b, *b]
d: &d [*c, *c, *c, *c, *c, *c, *c, *c]
boom: *d
";

#[test]
fn adversarial_manifest_documents_goal_06_cases() {
    for required in [
        "deep-flow-nesting",
        "oversized-plain-scalar",
        "oversized-block-scalar",
        "wide-flow-sequence",
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
            .collect::<yaml::Result<Vec<_>>>()
            .expect("events clean fixture");
        options
            .stream_documents(CLEAN_SMALL)
            .expect("document stream")
            .collect::<yaml::Result<Vec<_>>>()
            .expect("documents clean fixture");
        options
            .from_str::<Value>(CLEAN_SMALL)
            .expect("Serde clean fixture");
        yaml::parse_lossless_with_options(CLEAN_SMALL, options).expect("lossless clean fixture");
    }
}

#[test]
fn nesting_limit_rejects_every_load_shape_with_spans() {
    let input = "root: [[[[[0]]]]]\n";
    let options = LoadOptions::new().max_nesting_depth(4);
    assert_all_expanding_entrypoints_reject(input, options, "maximum YAML nesting depth exceeded");
    assert_event_entrypoints_reject(input, options, "maximum YAML nesting depth exceeded");
    assert_lossless_rejects(input, options, "maximum YAML nesting depth exceeded");
}

#[test]
fn scalar_limit_rejects_plain_and_block_scalars_with_spans() {
    let options = LoadOptions::new().max_scalar_bytes(8);
    for input in [
        "value: aaaaaaaaa\n",
        "value: |\n  aaaaaaaaa\n",
        "value: \"aaaaaaaaa\"\n",
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
    for input in ["items: [1, 2, 3, 4]\n", "{a: 1, b: 2, c: 3, d: 4}\n"] {
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
        .collect::<yaml::Result<Vec<Event>>>()
        .expect("raw events validate aliases without expansion");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Alias { anchor } if anchor.name == "d"))
    );
    yaml::parse_lossless_with_options(ALIAS_BOMB, options)
        .expect("lossless graph validates aliases without semantic expansion");
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
            .collect::<yaml::Result<Vec<Node>>>()
            .expect_err("DocumentStream rejects"),
        options
            .deserializer_from_str(input)
            .map(Value::deserialize)
            .collect::<yaml::Result<Vec<Value>>>()
            .expect_err("Deserializer::from_str rejects"),
        options
            .deserializer_from_slice(input.as_bytes())
            .map(Value::deserialize)
            .collect::<yaml::Result<Vec<Value>>>()
            .expect_err("Deserializer::from_slice rejects"),
        options
            .deserializer_from_reader(Cursor::new(input.as_bytes()))
            .map(Value::deserialize)
            .collect::<yaml::Result<Vec<Value>>>()
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
            .collect::<yaml::Result<Vec<Event>>>()
            .expect_err("EventStream::from_str rejects"),
        options
            .stream_events_slice(input.as_bytes())
            .expect("event slice stream constructs")
            .collect::<yaml::Result<Vec<Event>>>()
            .expect_err("EventStream::from_slice rejects"),
        options
            .stream_events_reader(Cursor::new(input.as_bytes()))
            .expect("event reader stream constructs")
            .collect::<yaml::Result<Vec<Event>>>()
            .expect_err("EventStream::from_reader rejects"),
    ] {
        assert_limit_error(input, &error, expected);
    }
}

fn assert_lossless_rejects(input: &str, options: LoadOptions, expected: &str) {
    for error in [
        yaml::parse_lossless_with_options(input, options).expect_err("lossless str rejects"),
        yaml::parse_lossless_bytes_with_options(input.as_bytes(), options)
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
