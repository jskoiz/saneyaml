use proptest::prelude::*;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use yaml::{
    Error, Event, EventDocumentDirectives, EventMeta, Node, NodeValue, Number, Span, Tag,
    TaggedNode, Value,
};

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        max_shrink_iters: 1024,
        ..ProptestConfig::default()
    })]

    #[test]
    fn parser_never_panics_on_bytes(input in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let result = std::panic::catch_unwind(|| yaml::parse_bytes(&input))
            .expect("parser must not panic");
        assert_parse_result_invariants(&input, &result);
    }

    #[test]
    fn invalid_utf8_diagnostics_stay_in_bounds(prefix in proptest::collection::vec(prop_oneof![
        Just(b'a'),
        Just(b'\n'),
        Just(b' '),
        Just(b':'),
    ], 0..512)) {
        let mut input = prefix;
        input.push(0xFF);
        std::panic::catch_unwind(|| {
            let error = yaml::parse_bytes(&input).expect_err("inserted invalid UTF-8");
            let span = error.span();
            prop_assert!(span.start <= input.len());
            prop_assert!(span.end <= input.len());
            prop_assert!(span.line >= 1);
            prop_assert!(span.column >= 1);
            assert_error_invariants(&input, &error);
            Ok::<(), TestCaseError>(())
        }).expect("parser must not panic on invalid UTF-8 diagnostics")?;
    }

    #[test]
    fn parser_never_panics_on_utf8(input in "\\PC{0,4096}") {
        let result = std::panic::catch_unwind(|| yaml::parse_str(&input))
            .expect("parser must not panic");
        assert_parse_result_invariants(input.as_bytes(), &result);
    }

    #[test]
    fn event_stream_never_panics_on_utf8(input in "\\PC{0,4096}") {
        let result = std::panic::catch_unwind(|| yaml::parse_events(&input))
            .expect("event parser must not panic");
        assert_event_result_invariants(input.as_bytes(), &result);
    }

    #[test]
    fn emitted_generated_trees_parse_equivalently(node in arb_node(4)) {
        let emitted = match yaml::to_string(&node) {
            Ok(emitted) => emitted,
            Err(error) if error.to_string().contains("duplicate mapping key") => return Ok(()),
            Err(error)
                if error
                    .to_string()
                    .contains("nested YAML tags cannot be emitted directly") =>
            {
                return Ok(());
            }
            Err(error) => {
                return Err(TestCaseError::fail(format!("unexpected emit error: {error}")));
            }
        };
        let reparsed = yaml::parse_str(&emitted).expect("parse emitted node");
        prop_assert!(reparsed.equivalent(&node));
        let emitted_again = yaml::to_string(&reparsed).expect("emit reparsed node");
        prop_assert_eq!(emitted_again, emitted);
    }

    #[test]
    fn from_node_and_from_value_agree_for_generated_values(node in arb_node(4)) {
        let from_node: Value = yaml::from_node(&node).expect("from_node value");
        let from_value: Value = yaml::from_value(Value::from(node.clone())).expect("from_value value");
        prop_assert!(from_node.equivalent(&from_value));
        prop_assert!(from_value.equivalent(&Value::from(&node)));
    }
}

#[test]
fn parser_corpus_does_not_panic() {
    for (name, input) in fixture_inputs()
        .into_iter()
        .chain(synthetic_corpus_inputs())
    {
        std::panic::catch_unwind(|| {
            let result = yaml::parse_bytes(&input);
            assert_parse_result_invariants(&input, &result);
        })
        .unwrap_or_else(|_| panic!("parser must not panic on corpus fixture {name}"));
    }
}

#[test]
fn parse_bytes_fuzz_corpus_does_not_panic() {
    for (name, input) in fuzz_corpus_inputs("parse_bytes") {
        std::panic::catch_unwind(|| {
            let result = yaml::parse_bytes(&input);
            assert_parse_result_invariants(&input, &result);
        })
        .unwrap_or_else(|_| panic!("parser must not panic on parse_bytes fuzz corpus {name}"));
    }
}

#[test]
fn event_corpus_does_not_panic() {
    for (name, input) in fixture_inputs()
        .into_iter()
        .chain(synthetic_corpus_inputs())
    {
        std::panic::catch_unwind(|| {
            if let Ok(text) = std::str::from_utf8(&input) {
                let result = yaml::parse_events(text);
                assert_event_result_invariants(&input, &result);
            }
        })
        .unwrap_or_else(|_| panic!("event parser must not panic on corpus fixture {name}"));
    }
}

#[test]
fn event_stream_fuzz_corpus_does_not_panic() {
    for (name, input) in fuzz_corpus_inputs("event_stream") {
        std::panic::catch_unwind(|| {
            if let Ok(text) = std::str::from_utf8(&input) {
                let result = yaml::parse_events(text);
                assert_event_result_invariants(&input, &result);
            }
        })
        .unwrap_or_else(|_| {
            panic!("event parser must not panic on event_stream fuzz corpus {name}")
        });
    }
}

#[test]
fn serde_corpus_entrypoints_keep_diagnostics_in_bounds() {
    for (name, input) in fixture_inputs()
        .into_iter()
        .chain(synthetic_corpus_inputs())
    {
        std::panic::catch_unwind(|| assert_serde_entrypoint_invariants(&input)).unwrap_or_else(
            |_| panic!("Serde entrypoints must not panic on corpus fixture {name}"),
        );
    }
}

#[test]
fn serde_entrypoints_fuzz_corpus_keeps_diagnostics_in_bounds() {
    for (name, input) in fuzz_corpus_inputs("serde_entrypoints") {
        std::panic::catch_unwind(|| assert_serde_entrypoint_invariants(&input)).unwrap_or_else(
            |_| panic!("Serde entrypoints must not panic on serde_entrypoints fuzz corpus {name}"),
        );
    }
}

#[test]
fn apply_merge_corpus_does_not_panic() {
    for (name, input) in fixture_inputs()
        .into_iter()
        .chain(synthetic_corpus_inputs())
        .chain(apply_merge_synthetic_inputs())
    {
        std::panic::catch_unwind(|| assert_apply_merge_invariants(&input)).unwrap_or_else(|_| {
            panic!("Value::apply_merge must not panic on corpus fixture {name}")
        });
    }
}

#[test]
fn apply_merge_fuzz_corpus_does_not_panic() {
    for (name, input) in fuzz_corpus_inputs("apply_merge") {
        std::panic::catch_unwind(|| assert_apply_merge_invariants(&input)).unwrap_or_else(|_| {
            panic!("Value::apply_merge must not panic on apply_merge fuzz corpus {name}")
        });
    }
}

#[test]
fn malformed_block_scalar_header_corpus_rejects_with_in_bounds_spans() {
    for (name, input) in malformed_block_scalar_header_inputs() {
        for error in [
            yaml::parse_str(input).expect_err("parse_str rejects malformed block scalar header"),
            yaml::parse_events(input)
                .expect_err("parse_events rejects malformed block scalar header"),
            yaml::from_str::<Value>(input)
                .expect_err("from_str rejects malformed block scalar header"),
        ] {
            assert!(
                error.to_string().contains("invalid block scalar header"),
                "{name} unexpected error: {error}"
            );
            assert_error_invariants(input.as_bytes(), &error);
        }
    }
}

#[test]
fn duplicate_tree_key_corpus_rejects_tree_loading_with_in_bounds_spans() {
    for (name, input) in duplicate_tree_key_inputs() {
        yaml::parse_events(input).expect("raw events preserve duplicate collection keys");
        for error in [
            yaml::parse_str(input).expect_err("parse_str rejects duplicate tree key"),
            yaml::from_str::<Value>(input).expect_err("from_str rejects duplicate tree key"),
        ] {
            assert!(
                error.to_string().contains("duplicate mapping key"),
                "{name} unexpected error: {error}"
            );
            assert!(
                !error.diagnostic().related.is_empty(),
                "{name} reports previous key span"
            );
            assert_error_invariants(input.as_bytes(), &error);
        }
    }
}

fn fixture_inputs() -> Vec<(String, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut inputs = Vec::new();
    for relative in ["yaml-test-suite/data", "real-world", "divergences"] {
        collect_yaml_fixtures(&root.join(relative), &root, &mut inputs);
    }
    inputs
}

fn fuzz_corpus_inputs(target: &str) -> Vec<(String, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz/corpus")
        .join(target);
    let mut inputs = Vec::new();
    collect_all_files(&root, &root, &mut inputs);
    inputs
}

fn collect_yaml_fixtures(path: &Path, root: &Path, inputs: &mut Vec<(String, Vec<u8>)>) {
    if path.is_dir() {
        let mut children = fs::read_dir(path)
            .unwrap_or_else(|error| panic!("read fixture directory {}: {error}", path.display()))
            .map(|entry| {
                entry
                    .unwrap_or_else(|error| {
                        panic!("read fixture directory entry {}: {error}", path.display())
                    })
                    .path()
            })
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            collect_yaml_fixtures(&child, root, inputs);
        }
        return;
    }

    if !matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("yaml" | "yml")
    ) {
        return;
    }

    let name = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let input = fs::read(path)
        .unwrap_or_else(|error| panic!("read fixture file {}: {error}", path.display()));
    inputs.push((name, input));
}

fn collect_all_files(path: &Path, root: &Path, inputs: &mut Vec<(String, Vec<u8>)>) {
    if path.is_dir() {
        let mut children = fs::read_dir(path)
            .unwrap_or_else(|error| panic!("read corpus directory {}: {error}", path.display()))
            .map(|entry| {
                entry
                    .unwrap_or_else(|error| {
                        panic!("read corpus directory entry {}: {error}", path.display())
                    })
                    .path()
            })
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            collect_all_files(&child, root, inputs);
        }
        return;
    }

    if !path.is_file() {
        return;
    }

    let name = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let input = fs::read(path)
        .unwrap_or_else(|error| panic!("read corpus file {}: {error}", path.display()));
    inputs.push((name, input));
}

fn synthetic_corpus_inputs() -> impl Iterator<Item = (String, Vec<u8>)> {
    [
        (
            "flow-anchor-only-null-nodes",
            b"root: [&empty, *empty]\nkeyed: {? &key : value}\n".as_slice(),
        ),
        ("root-literal-indented", b"|\n  line\n".as_slice()),
        ("root-folded-indented", b">\n  first\n  second\n".as_slice()),
        ("blank-separated-indicator", b"@\n\n!".as_slice()),
        ("blank-separated-plain", b"@\n\ng".as_slice()),
        ("blank-separated-control", b"\x01\n\n\x01".as_slice()),
        (
            "root-literal-indent-zero-percent",
            b"|\n%!PS-Adobe-2.0 # Not the first line\n".as_slice(),
        ),
        (
            "yaml-double-quoted-escapes",
            b"x: \"\\e\"\nroot: [\"\\a\", \"\\v\", \"\\_\", \"\\N\", \"\\L\", \"\\P\"]\n"
                .as_slice(),
        ),
        (
            "yaml-double-quoted-even-backslash-fold",
            b"value: \"a\\\\\n  b\"\n".as_slice(),
        ),
        (
            "large-integer-widths",
            b"i128_max: 170141183460469231731687303715884105727\nu128_max: 340282366920938463463374607431768211455\nu128_overflow: 340282366920938463463374607431768211456\n".as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, input)| (name.to_string(), input.to_vec()))
}

fn malformed_block_scalar_header_inputs() -> impl Iterator<Item = (&'static str, &'static str)> {
    [
        ("mapping-alpha", "key: |x\n"),
        ("mapping-zero", "key: |0\n"),
        ("mapping-chomping", "key: |+-\n"),
        ("mapping-space", "key: | x\n"),
        ("root-alpha", "|x\n"),
        ("document-indent-zero", "--- |0\n"),
        ("document-two-digit-indent", "--- |10\n"),
        ("sequence-chomping", "- |+-\n"),
    ]
    .into_iter()
}

fn duplicate_tree_key_inputs() -> impl Iterator<Item = (&'static str, &'static str)> {
    [
        (
            "yts-x38w",
            include_str!("fixtures/yaml-test-suite/data/X38W/in.yaml"),
        ),
        (
            "sequence-key",
            "root: {? [a, b]: first, ? [a, b]: second}\n",
        ),
        ("mapping-key", "root: {? {x: y}: first, ? {x: y}: second}\n"),
        (
            "tagged-scalar-key",
            "root: {!Tag dup: first, dup: second}\n",
        ),
        (
            "tagged-sequence-key",
            "root: {!Tag [a, b]: first, [a, b]: second}\n",
        ),
        (
            "tagged-mapping-key",
            "root: {!Tag {x: y}: first, {x: y}: second}\n",
        ),
    ]
    .into_iter()
}

fn apply_merge_synthetic_inputs() -> impl Iterator<Item = (String, Vec<u8>)> {
    [
        (
            "apply-merge-single",
            b"base: &base {a: 1}\ntarget: {<<: *base, b: 2}\n".as_slice(),
        ),
        (
            "apply-merge-list",
            b"first: &first {shared: first}\nsecond: &second {shared: second, b: 2}\ntarget: {<<: [*first, *second]}\n"
                .as_slice(),
        ),
        ("apply-merge-scalar-error", b"target: {<<: scalar}\n".as_slice()),
        (
            "apply-merge-sequence-element-error",
            b"target: {<<: [scalar]}\n".as_slice(),
        ),
        (
            "apply-merge-tagged-error",
            b"target: {<<: !Thing {a: 1}}\n".as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, input)| (name.to_string(), input.to_vec()))
}

#[derive(Debug, Deserialize)]
struct BorrowedConfig<'a> {
    name: &'a str,
    path: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct BorrowedVars<'a> {
    vars: BTreeMap<&'a str, &'a str>,
}

fn assert_serde_entrypoint_invariants(input: &[u8]) {
    assert_single_document_entrypoint(input);
    assert_document_stream_entrypoints(input);
    assert_config_string_map_entrypoints(input);
    assert_numeric_map_entrypoints(input);
    assert_borrowed_entrypoints(input);
}

fn assert_apply_merge_invariants(input: &[u8]) {
    let Ok(mut value) = yaml::from_slice::<Value>(input) else {
        return;
    };

    if let Err(error) = value.apply_merge() {
        assert!(!error.to_string().is_empty());
        assert_eq!(error.location(), None);
    }
}

fn assert_single_document_entrypoint(input: &[u8]) {
    match yaml::from_slice::<Value>(input) {
        Ok(value) => {
            let node = yaml::parse_bytes(input).expect("from_slice success must parse");
            assert!(Value::from(&node).equivalent(&value));
        }
        Err(error) => assert_error_invariants(input, &error),
    }
}

fn assert_document_stream_entrypoints(input: &[u8]) {
    let from_documents = yaml::from_documents_slice::<Value>(input);
    match &from_documents {
        Ok(values) => {
            let input_text = std::str::from_utf8(input).expect("document success must be UTF-8");
            let nodes = yaml::parse_documents(input_text).expect("document success must parse");
            assert_eq!(values.len(), nodes.len());
            for (value, node) in values.iter().zip(nodes.iter()) {
                assert!(Value::from(node).equivalent(value));
            }
        }
        Err(error) => assert_error_invariants(input, error),
    }

    let stream_results = yaml::Deserializer::from_slice(input)
        .map(Value::deserialize)
        .collect::<Vec<_>>();
    match from_documents {
        Ok(expected) => {
            assert_eq!(stream_results.len(), expected.len());
            for (actual, expected) in stream_results.into_iter().zip(expected) {
                let actual = actual.expect("stream document should deserialize");
                assert!(actual.equivalent(&expected));
            }
        }
        Err(_) => {
            assert!(
                stream_results.iter().any(Result::is_err),
                "stream deserializer should surface parse errors"
            );
            for error in stream_results
                .iter()
                .filter_map(|result| result.as_ref().err())
            {
                assert_error_invariants(input, error);
            }
        }
    }
}

fn assert_config_string_map_entrypoints(input: &[u8]) {
    if let Err(error) = yaml::from_slice::<BTreeMap<String, String>>(input) {
        assert_error_invariants(input, &error);
    }
    if let Err(error) = yaml::from_slice::<BTreeMap<String, Option<String>>>(input) {
        assert_error_invariants(input, &error);
    }
}

fn assert_numeric_map_entrypoints(input: &[u8]) {
    assert_owned_entrypoint::<BTreeMap<String, i128>>(input);
    assert_owned_entrypoint::<BTreeMap<String, u128>>(input);
    assert_owned_entrypoint::<BTreeMap<String, i64>>(input);
    assert_owned_entrypoint::<BTreeMap<String, u64>>(input);
    assert_owned_entrypoint::<BTreeMap<String, i8>>(input);
    assert_owned_entrypoint::<BTreeMap<String, u8>>(input);
}

fn assert_owned_entrypoint<T>(input: &[u8])
where
    T: for<'de> Deserialize<'de>,
{
    if let Err(error) = yaml::from_slice::<T>(input) {
        assert_error_invariants(input, &error);
    }
}

fn assert_borrowed_entrypoints(input: &[u8]) {
    match yaml::from_slice::<BorrowedConfig<'_>>(input) {
        Ok(config) => {
            assert_borrowed_from_input(input, config.name);
            assert_borrowed_from_input(input, config.path);
        }
        Err(error) => assert_error_invariants(input, &error),
    }

    match yaml::from_slice::<BorrowedVars<'_>>(input) {
        Ok(config) => {
            for (key, value) in config.vars {
                assert_borrowed_from_input(input, key);
                assert_borrowed_from_input(input, value);
            }
        }
        Err(error) => assert_error_invariants(input, &error),
    }

    match BorrowedConfig::deserialize(yaml::Deserializer::from_slice(input)) {
        Ok(config) => {
            assert_borrowed_from_input(input, config.name);
            assert_borrowed_from_input(input, config.path);
        }
        Err(error) => assert!(!error.to_string().is_empty()),
    }

    for result in yaml::Deserializer::from_slice(input).map(BorrowedConfig::deserialize) {
        match result {
            Ok(config) => {
                assert_borrowed_from_input(input, config.name);
                assert_borrowed_from_input(input, config.path);
            }
            Err(error) => assert_error_invariants(input, &error),
        }
    }
}

fn assert_borrowed_from_input(input: &[u8], value: &str) {
    let input_start = input.as_ptr() as usize;
    let input_end = input_start + input.len();
    let value_start = value.as_ptr() as usize;
    let value_end = value_start + value.len();
    assert!(
        value_start >= input_start && value_end <= input_end,
        "borrowed value should point into input"
    );
    let offset = value_start - input_start;
    assert_eq!(&input[offset..offset + value.len()], value.as_bytes());
}

#[test]
fn event_stream_fuzz_regression_closes_blank_separated_implicit_documents() {
    let input = "a-r--\n\noa: b\0\0\0\0\0\0\0\n";
    let events = yaml::parse_events(input).expect("events");
    assert_event_invariants(input.as_bytes(), &events);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentStart { .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::DocumentEnd { .. }))
            .count(),
        2
    );
}

#[test]
fn event_stream_root_scalar_regressions_stay_structurally_valid() {
    for (name, input) in [
        (
            "M7A3",
            include_str!("fixtures/yaml-test-suite/data/M7A3/in.yaml"),
        ),
        ("root-literal-indented", "|\n  line\n"),
        (
            "root-literal-indent-zero-percent",
            "|\n%!PS-Adobe-2.0 # Not the first line\n",
        ),
        ("blank-separated-plain", "@\n\ng\n"),
    ] {
        let result = yaml::parse_events(input);
        assert_event_result_invariants(input.as_bytes(), &result);
        result.unwrap_or_else(|error| panic!("event stream regression {name} failed: {error}"));
    }
}

fn assert_parse_result_invariants(input: &[u8], result: &yaml::Result<Node>) {
    match result {
        Ok(node) => {
            assert_node_invariants(input, node);
            assert_success_invariants(node);
        }
        Err(error) => assert_error_invariants(input, error),
    }
}

fn assert_event_result_invariants(input: &[u8], result: &yaml::Result<Vec<Event>>) {
    match result {
        Ok(events) => assert_event_invariants(input, events),
        Err(error) => assert_error_invariants(input, error),
    }
}

fn assert_event_invariants(input: &[u8], events: &[Event]) {
    assert!(matches!(events.first(), Some(Event::StreamStart)));
    assert!(matches!(events.last(), Some(Event::StreamEnd)));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::StreamStart))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::StreamEnd))
            .count(),
        1
    );

    let mut docs = 0i32;
    let mut collections = Vec::new();
    for event in events {
        match event {
            Event::StreamStart | Event::StreamEnd => {}
            Event::DocumentStart {
                explicit,
                directives,
                span,
            } => {
                assert_eq!(docs, 0, "nested document start at {event:?}");
                assert!(
                    collections.is_empty(),
                    "document started while collections were open: {collections:?}"
                );
                assert_span_invariants(input, *span);
                if *explicit {
                    assert_eq!(source_slice(input, *span), b"---");
                }
                assert_document_directives_invariants(input, directives);
                docs += 1;
            }
            Event::DocumentEnd { explicit, span } => {
                assert!(
                    collections.is_empty(),
                    "document ended while collections were open: {collections:?}"
                );
                assert_span_invariants(input, *span);
                if *explicit {
                    assert_eq!(source_slice(input, *span), b"...");
                }
                docs -= 1;
            }
            Event::SequenceStart { meta, span, .. } => {
                assert!(docs > 0, "sequence start outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_event_meta_invariants(input, meta);
                collections.push("sequence");
            }
            Event::SequenceEnd { span } => {
                assert!(docs > 0, "sequence end outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_eq!(
                    collections.pop(),
                    Some("sequence"),
                    "crossed collection nesting at {event:?}"
                );
            }
            Event::MappingStart { meta, span, .. } => {
                assert!(docs > 0, "mapping start outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_event_meta_invariants(input, meta);
                collections.push("mapping");
            }
            Event::MappingEnd { span } => {
                assert!(docs > 0, "mapping end outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_eq!(
                    collections.pop(),
                    Some("mapping"),
                    "crossed collection nesting at {event:?}"
                );
            }
            Event::Alias { anchor } => {
                assert!(docs > 0, "alias outside document: {event:?}");
                assert_span_invariants(input, anchor.span);
                assert!(!anchor.name.is_empty());
                assert_eq!(
                    source_slice(input, anchor.span),
                    format!("*{}", anchor.name).as_bytes()
                );
            }
            Event::Scalar { meta, span, .. } => {
                assert!(docs > 0, "scalar outside document: {event:?}");
                assert_span_invariants(input, *span);
                assert_event_meta_invariants(input, meta);
            }
        }
        assert!(docs >= 0, "document stack went negative at {event:?}");
    }
    assert_eq!(docs, 0);
    assert!(
        collections.is_empty(),
        "unclosed collections: {collections:?}"
    );
}

fn assert_event_meta_invariants(input: &[u8], meta: &EventMeta) {
    if let Some(anchor) = &meta.anchor {
        assert_span_invariants(input, anchor.span);
        assert!(!anchor.name.is_empty());
        assert_eq!(
            source_slice(input, anchor.span),
            format!("&{}", anchor.name).as_bytes()
        );
    }
    if let Some(tag) = &meta.tag {
        assert_span_invariants(input, tag.span);
        assert!(
            source_slice(input, tag.span).starts_with(b"!"),
            "tag span should point at tag token: {:?}",
            tag.span
        );
    }
}

fn assert_document_directives_invariants(input: &[u8], directives: &EventDocumentDirectives) {
    if let Some(version) = &directives.yaml_version {
        assert_span_invariants(input, version.span);
        assert!(version.major > 0);
        assert_eq!(
            source_slice(input, version.span),
            format!("{}.{}", version.major, version.minor).as_bytes()
        );
    }
    for directive in &directives.tag_directives {
        assert!(!directive.handle.is_empty());
        assert!(!directive.prefix.is_empty());
        assert_span_invariants(input, directive.span);
        assert_span_invariants(input, directive.handle_span);
        assert_span_invariants(input, directive.prefix_span);
        assert!(
            source_slice(input, directive.span).starts_with(b"%TAG"),
            "TAG directive span should point at directive token: {:?}",
            directive.span
        );
        assert_eq!(
            source_slice(input, directive.handle_span),
            directive.handle.as_bytes()
        );
        assert_eq!(
            source_slice(input, directive.prefix_span),
            directive.prefix.as_bytes()
        );
    }
}

fn assert_success_invariants(node: &Node) {
    let emitted = yaml::to_string(node).expect("emit parsed tree");
    let reparsed = yaml::parse_str(&emitted).expect("parse emitted tree");
    assert!(
        reparsed.equivalent(node),
        "emitted YAML did not parse equivalently:\n{emitted}"
    );

    let emitted_again = yaml::to_string(&reparsed).expect("emit reparsed tree");
    assert_eq!(emitted_again, emitted);

    let direct = Value::from(node);
    let from_node: Value = yaml::from_node(node).expect("from_node value");
    let from_value: Value = yaml::from_value(direct.clone()).expect("from_value value");
    assert!(direct.equivalent(&from_node));
    assert!(direct.equivalent(&from_value));
}

fn assert_node_invariants(input: &[u8], node: &Node) {
    assert_span_invariants(input, node.span);
    match &node.value {
        NodeValue::Sequence(items) => {
            for item in items {
                assert_node_invariants(input, item);
            }
        }
        NodeValue::Mapping(entries) => {
            for (key, value) in entries {
                assert_node_invariants(input, key);
                assert_node_invariants(input, value);
            }
        }
        NodeValue::Tagged(tagged) => {
            assert_span_invariants(input, tagged.tag_span);
            assert_node_invariants(input, &tagged.value);
        }
        NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) | NodeValue::String(_) => {}
    }
}

fn assert_error_invariants(input: &[u8], error: &Error) {
    let diagnostic = error.diagnostic();
    assert!(!diagnostic.message.is_empty());
    assert_span_invariants(input, diagnostic.span);
    for related in &diagnostic.related {
        assert!(!related.message.is_empty());
        assert_span_invariants(input, related.span);
    }
}

fn assert_span_invariants(input: &[u8], span: Span) {
    assert!(
        span.start <= span.end,
        "span starts after it ends: {span:?}"
    );
    assert!(
        span.end <= input.len(),
        "span exceeds input length {}: {span:?}",
        input.len()
    );
    assert!(span.line >= 1, "span line must be one-based: {span:?}");
    assert!(span.column >= 1, "span column must be one-based: {span:?}");
    let (line, column) = byte_location(input, span.start);
    assert_eq!(
        (span.line, span.column),
        (line, column),
        "span location does not match byte offset for {span:?}"
    );
}

fn source_slice(input: &[u8], span: Span) -> &[u8] {
    &input[span.start..span.end]
}

fn byte_location(input: &[u8], offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;
    for byte in &input[..offset] {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn arb_node(max_depth: u32) -> impl Strategy<Value = Node> {
    let leaf = prop_oneof![
        Just(Node::new(NodeValue::Null, Span::default())),
        any::<bool>().prop_map(|value| Node::new(NodeValue::Bool(value), Span::default())),
        (-1_000_000i64..=1_000_000).prop_map(|value| Node::new(
            NodeValue::Number(Number::Integer(i128::from(value))),
            Span::default()
        )),
        (i64::MAX as u64 + 1..=i64::MAX as u64 + 1_000_000).prop_map(|value| Node::new(
            NodeValue::Number(Number::Unsigned(u128::from(value))),
            Span::default()
        )),
        finite_float()
            .prop_map(|value| Node::new(NodeValue::Number(Number::Float(value)), Span::default())),
        prop_oneof![Just(f64::NAN), Just(f64::INFINITY), Just(f64::NEG_INFINITY),]
            .prop_map(|value| Node::new(NodeValue::Number(Number::Float(value)), Span::default())),
        safe_string().prop_map(|value| Node::new(NodeValue::String(value), Span::default())),
    ];

    leaf.prop_recursive(max_depth, 64, 8, |inner| {
        prop_oneof![
            (arb_tag(), inner.clone()).prop_map(|(tag, value)| {
                Node::new(
                    NodeValue::Tagged(Box::new(TaggedNode {
                        tag,
                        tag_span: Span::default(),
                        value,
                    })),
                    Span::default(),
                )
            }),
            prop::collection::vec(inner.clone(), 0..8)
                .prop_map(|items| Node::new(NodeValue::Sequence(items), Span::default())),
            prop::collection::btree_map(safe_key(), inner.clone(), 0..8).prop_map(|entries| {
                Node::new(
                    NodeValue::Mapping(
                        entries
                            .into_iter()
                            .map(|(key, value)| {
                                (Node::new(NodeValue::String(key), Span::default()), value)
                            })
                            .collect(),
                    ),
                    Span::default(),
                )
            }),
            (
                arb_complex_key(),
                inner.clone(),
                prop::collection::btree_map(safe_key(), inner, 0..4)
            )
                .prop_map(|(key, value, entries)| {
                    let mut entries = entries
                        .into_iter()
                        .map(|(key, value)| {
                            (Node::new(NodeValue::String(key), Span::default()), value)
                        })
                        .collect::<Vec<_>>();
                    entries.push((key, value));
                    Node::new(NodeValue::Mapping(entries), Span::default())
                }),
        ]
    })
}

fn arb_tag() -> impl Strategy<Value = Tag> {
    prop_oneof![
        Just(Tag::new("Thing")),
        Just(Tag::new("!!str")),
        Just(Tag::new("!<tag:example.com,2026:Thing>")),
        Just(Tag::new("!<:example.com,2026:Thing>")),
        Just(Tag::new("!<<abc>")),
        Just(Tag::new("!<abc:>")),
        Just(Tag::new("!<a!b>")),
        Just(Tag::new("!<comma,tag>")),
        Just(Tag::new("!<flow[bracket]>")),
        Just(Tag::new("!<space tag>")),
        Just(Tag::new("!<brace{tag}>")),
    ]
}

fn arb_complex_key() -> impl Strategy<Value = Node> {
    prop_oneof![
        prop::collection::vec(safe_string(), 0..4).prop_map(|items| {
            Node::new(
                NodeValue::Sequence(
                    items
                        .into_iter()
                        .map(|value| Node::new(NodeValue::String(value), Span::default()))
                        .collect(),
                ),
                Span::default(),
            )
        }),
        (arb_tag(), safe_key()).prop_map(|(tag, value)| {
            Node::new(
                NodeValue::Tagged(Box::new(TaggedNode {
                    tag,
                    tag_span: Span::default(),
                    value: Node::new(NodeValue::String(value), Span::default()),
                })),
                Span::default(),
            )
        }),
    ]
}

fn finite_float() -> impl Strategy<Value = f64> {
    (-1_000_000f64..1_000_000f64).prop_filter("finite non-integer float", |value| {
        value.is_finite() && value.fract() != 0.0
    })
}

fn safe_key() -> impl Strategy<Value = String> {
    "[A-Za-z_][A-Za-z0-9_.-]{0,16}".prop_map(|value| value)
}

fn safe_string() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        "[A-Za-z0-9 _./:@-]{0,32}".prop_map(|value| value),
        Just("true".to_string()),
        Just("null".to_string()),
        Just(".nan".to_string()),
        Just(".NaN".to_string()),
        Just(".inf".to_string()),
        Just("+.inf".to_string()),
        Just("+.INF".to_string()),
        Just("001".to_string()),
        Just("a: b".to_string()),
        Just("...".to_string()),
        Just("... marker".to_string()),
        Just("%YAML 1.2".to_string()),
        Just("a:\tb".to_string()),
        Just("a\t#b".to_string()),
        Just("line\n".to_string()),
        Just("first\nsecond\n".to_string()),
        Just(" leading\nregular\n".to_string()),
        Just("quote \" slash \\\\".to_string()),
        Just("tab\t nul\0".to_string()),
        Just("a]b".to_string()),
        Just("a[b".to_string()),
        Just("a}b".to_string()),
        Just("a{b".to_string()),
    ]
}
