#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;
use yaml::{Error, Event, LoadOptions, Node, NodeValue, Schema, Span, Value};

fuzz_target!(|input: &[u8]| {
    for options in [
        LoadOptions::new(),
        LoadOptions::core(),
        LoadOptions::json(),
        LoadOptions::failsafe(),
        LoadOptions::new().schema(Schema::Yaml11),
        LoadOptions::legacy_serde_yaml(),
        LoadOptions::yaml_version_directive(),
    ] {
        assert_parse_invariants(input, options);
        assert_serde_invariants(input, options);
    }
    assert_limit_option_invariants(input);
});

fn assert_limit_option_invariants(input: &[u8]) {
    let Some((&control, yaml_input)) = input.split_first() else {
        return;
    };
    let base = match control & 0b11 {
        0 => LoadOptions::new(),
        1 => LoadOptions::yaml_1_1(),
        2 => LoadOptions::yaml_version_directive(),
        _ => LoadOptions::new().without_input_limit(),
    };
    let options = match (control >> 2) & 0b111 {
        0 => base.max_input_bytes(yaml_input.len().saturating_sub(1)),
        1 => base.max_alias_expansion_nodes(8),
        2 => base.max_nesting_depth(4),
        3 => base.max_scalar_bytes(8),
        4 => base.max_collection_items(4),
        5 => base.without_nesting_depth_limit(),
        6 => base.without_scalar_limit(),
        _ => base.without_collection_limit(),
    };

    assert_parse_invariants(yaml_input, options);
    assert_serde_invariants(yaml_input, options);
    assert_stream_invariants(yaml_input, options);
    assert_lossless_invariants(yaml_input, options);
}

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

fn assert_stream_invariants(input: &[u8], options: LoadOptions) {
    let event_stream = options.stream_events_slice(input);
    match event_stream {
        Ok(stream) => {
            for result in stream {
                match result {
                    Ok(event) => assert_event_invariants(input, &event),
                    Err(error) => assert_error_invariants(input, &error),
                }
            }
        }
        Err(error) => assert_error_invariants(input, &error),
    }

    match options.stream_events_reader(Cursor::new(input)) {
        Ok(stream) => {
            for result in stream {
                match result {
                    Ok(event) => assert_event_invariants(input, &event),
                    Err(error) => assert_error_invariants(input, &error),
                }
            }
        }
        Err(error) => assert_error_invariants(input, &error),
    }

    match options.stream_documents_slice(input) {
        Ok(stream) => {
            for result in stream {
                match result {
                    Ok(node) => assert_node_invariants(input, &node),
                    Err(error) => assert_error_invariants(input, &error),
                }
            }
        }
        Err(error) => assert_error_invariants(input, &error),
    }
}

fn assert_lossless_invariants(input: &[u8], options: LoadOptions) {
    match yaml::parse_lossless_bytes_with_options(input, options) {
        Ok(_) => {}
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

fn assert_event_invariants(input: &[u8], event: &Event) {
    match event {
        Event::DocumentStart { span, directives, .. } => {
            assert_span_invariants(input, *span);
            if let Some(version) = &directives.yaml_version {
                assert_span_invariants(input, version.span);
            }
            for directive in &directives.tag_directives {
                assert_span_invariants(input, directive.handle_span);
                assert_span_invariants(input, directive.prefix_span);
                assert_span_invariants(input, directive.span);
            }
        }
        Event::DocumentEnd { span, .. }
        | Event::SequenceStart { span, .. }
        | Event::SequenceEnd { span }
        | Event::MappingStart { span, .. }
        | Event::MappingEnd { span }
        | Event::Scalar { span, .. } => assert_span_invariants(input, *span),
        Event::Alias { anchor } => assert_span_invariants(input, anchor.span),
        Event::StreamStart | Event::StreamEnd => {}
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
    if span == Span::default() {
        return;
    }
    assert!(span.start <= span.end, "span starts after it ends: {span:?}");
    assert!(
        span.end <= input.len(),
        "span exceeds input length {}: {span:?}",
        input.len()
    );
    assert!(span.line >= 1, "span line must be one-based: {span:?}");
    assert!(span.column >= 1, "span column must be one-based: {span:?}");
}
