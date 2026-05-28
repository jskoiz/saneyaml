#![no_main]

use libfuzzer_sys::fuzz_target;
use yaml::{AnchorId, LoadOptions, LosslessNodeKind, LosslessStream, LosslessTriviaKind, NodeId, Span};

fuzz_target!(|input: &[u8]| {
    let result = yaml::parse_lossless_bytes(input);
    if let Ok(input_str) = std::str::from_utf8(input) {
        if yaml::parse_events(input_str).is_ok() {
            assert!(
                result.is_ok(),
                "parse_lossless rejected YAML accepted by parse_events: {result:?}"
            );
        }
    }
    match result {
        Ok(stream) => assert_stream_invariants(input, &stream),
        Err(error) => {
            if let Some(location) = error.location() {
                assert!(location.index() <= input.len());
            }
        }
    }
});

fn assert_stream_invariants(input: &[u8], stream: &LosslessStream) {
    assert_eq!(stream.as_source().as_bytes(), input);
    assert_eq!(stream.to_string().as_bytes(), input);
    assert_yaml11_schema_probe(input, stream);

    for document in stream.documents() {
        assert_span_invariants(input, document.start_span());
        assert_span_invariants(input, document.end_span());
        if let Some(root) = document.root() {
            assert_node_id(stream, root);
        }
    }

    for node in stream.nodes() {
        assert_eq!(stream.node(node.id()).map(|node| node.id()), Some(node.id()));
        assert_span_invariants(input, node.span());
        assert!(stream.source_fragment(node.span()).is_some());
        if let Some(anchor) = node.anchor() {
            assert_anchor_id(stream, anchor);
            assert_eq!(
                stream.anchor(anchor).map(|anchor| anchor.node()),
                Some(node.id())
            );
        }
        if let Some(tag) = node.tag() {
            assert_span_invariants(input, tag.span);
        }
        match node.kind() {
            LosslessNodeKind::Scalar { .. } => {}
            LosslessNodeKind::Sequence { children, .. } => {
                for child in children {
                    assert_node_id(stream, *child);
                }
            }
            LosslessNodeKind::Mapping { entries, .. } => {
                for (key, value) in entries {
                    assert_node_id(stream, *key);
                    assert_node_id(stream, *value);
                }
            }
            LosslessNodeKind::Alias {
                name,
                alias,
                target,
            } => {
                let alias_ref = stream.alias(*alias).expect("alias id resolves");
                let target_ref = stream.anchor(*target).expect("alias target resolves");
                assert_eq!(alias_ref.name(), name);
                assert_eq!(target_ref.name(), name);
                assert_eq!(alias_ref.node(), node.id());
            }
        }
    }

    for anchor in stream.anchors() {
        assert_eq!(
            stream.anchor(anchor.id()).map(|anchor| anchor.id()),
            Some(anchor.id())
        );
        assert!(!anchor.name().is_empty());
        assert_span_invariants(input, anchor.span());
        assert_node_id(stream, anchor.node());
    }

    for alias in stream.aliases() {
        assert_eq!(
            stream.alias(alias.id()).map(|alias| alias.id()),
            Some(alias.id())
        );
        assert!(!alias.name().is_empty());
        assert_span_invariants(input, alias.span());
        assert_node_id(stream, alias.node());
        assert_anchor_id(stream, alias.target());
    }

    for trivia in stream.trivia() {
        assert_span_invariants(input, trivia.span());
        match trivia.kind() {
            LosslessTriviaKind::Comment => assert!(trivia.text().starts_with('#')),
            LosslessTriviaKind::BlankLine => assert!(trivia.text().trim().is_empty()),
        }
    }
}

fn assert_yaml11_schema_probe(input: &[u8], stream: &LosslessStream) {
    if !stream.as_source().contains("%YAML 1.1") {
        return;
    }
    match LoadOptions::yaml_version_directive().parse_documents(stream.as_source()) {
        Ok(_) => {}
        Err(error) => {
            if let Some(location) = error.location() {
                assert!(location.index() <= input.len());
            }
        }
    }
}

fn assert_node_id(stream: &LosslessStream, id: NodeId) {
    assert!(id.index() < stream.nodes().len());
}

fn assert_anchor_id(stream: &LosslessStream, id: AnchorId) {
    assert!(id.index() < stream.anchors().len());
}

fn assert_span_invariants(input: &[u8], span: Span) {
    assert!(span.start <= span.end, "invalid span ordering: {span:?}");
    assert!(
        span.end <= input.len(),
        "span {span:?} exceeds input length {}",
        input.len()
    );
    assert!(span.line >= 1);
    assert!(span.column >= 1);
}
