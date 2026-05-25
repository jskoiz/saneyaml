#![no_main]

use libfuzzer_sys::fuzz_target;
use yaml::{Error, Node, NodeValue, Span};

fuzz_target!(|input: &[u8]| {
    let result = yaml::parse_bytes(input);
    assert_parse_result_invariants(input, &result);
});

fn assert_parse_result_invariants(input: &[u8], result: &yaml::Result<Node>) {
    match result {
        Ok(node) => {
            assert_node_invariants(input, node);
            assert_success_invariants(node);
        }
        Err(error) => assert_error_invariants(input, error),
    }
}

fn assert_success_invariants(node: &Node) {
    let emitted = yaml::to_string(node).expect("emit parsed tree");
    let reparsed = yaml::parse_str(&emitted).expect("parse emitted tree");
    assert!(reparsed.equivalent(node));

    let emitted_again = yaml::to_string(&reparsed).expect("emit reparsed tree");
    assert_eq!(emitted_again, emitted);

    let direct = yaml::Value::from(node);
    let from_node: yaml::Value = yaml::from_node(node).expect("from_node value");
    let from_value: yaml::Value = yaml::from_value(direct.clone()).expect("from_value value");
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
    assert!(span.start <= span.end, "span starts after it ends: {span:?}");
    assert!(
        span.end <= input.len(),
        "span exceeds input length {}: {span:?}",
        input.len()
    );
    assert!(span.line >= 1, "span line must be one-based: {span:?}");
    assert!(span.column >= 1, "span column must be one-based: {span:?}");
    assert_eq!(
        (span.line, span.column),
        byte_location(input, span.start),
        "span location does not match byte offset for {span:?}"
    );
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
