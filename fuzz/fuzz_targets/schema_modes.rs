#![no_main]

use libfuzzer_sys::fuzz_target;
use yaml::{Error, LoadOptions, Node, NodeValue, Schema, Span, Value};

fuzz_target!(|input: &[u8]| {
    for options in [
        LoadOptions::new(),
        LoadOptions::new().schema(Schema::Yaml11),
        LoadOptions::yaml_version_directive(),
    ] {
        assert_parse_invariants(input, options);
        assert_serde_invariants(input, options);
    }
});

fn assert_parse_invariants(input: &[u8], options: LoadOptions) {
    match options.parse_bytes(input) {
        Ok(node) => assert_node_invariants(input, &node),
        Err(error) => assert_error_invariants(input, &error),
    }
}

fn assert_serde_invariants(input: &[u8], options: LoadOptions) {
    match options.from_slice::<Value>(input) {
        Ok(value) => {
            let node = options
                .parse_bytes(input)
                .expect("from_slice success must parse with same options");
            assert!(Value::from(&node).equivalent(&value));
        }
        Err(error) => assert_error_invariants(input, &error),
    }
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
}
