#![no_main]

use libfuzzer_sys::fuzz_target;
use saneyaml::{Error, Event, EventDocumentDirectives, EventMeta, Span};

fuzz_target!(|input: &[u8]| {
    let Ok(input_str) = std::str::from_utf8(input) else {
        return;
    };
    let result = saneyaml::parse_events(input_str);
    if saneyaml::parse_documents(input_str).is_ok() {
        assert!(
            result.is_ok(),
            "parse_events rejected YAML accepted by parse_documents: {result:?}"
        );
    }
    assert_event_result_invariants(input, &result);
});

fn assert_event_result_invariants(input: &[u8], result: &saneyaml::Result<Vec<Event>>) {
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
                assert!(docs > 0);
                assert_span_invariants(input, *span);
                assert_event_meta_invariants(input, meta);
                collections.push("sequence");
            }
            Event::SequenceEnd { span } => {
                assert!(docs > 0);
                assert_span_invariants(input, *span);
                assert_eq!(
                    collections.pop(),
                    Some("sequence"),
                    "crossed collection nesting at {event:?}"
                );
            }
            Event::MappingStart { meta, span, .. } => {
                assert!(docs > 0);
                assert_span_invariants(input, *span);
                assert_event_meta_invariants(input, meta);
                collections.push("mapping");
            }
            Event::MappingEnd { span } => {
                assert!(docs > 0);
                assert_span_invariants(input, *span);
                assert_eq!(
                    collections.pop(),
                    Some("mapping"),
                    "crossed collection nesting at {event:?}"
                );
            }
            Event::Alias { anchor } => {
                assert!(docs > 0);
                assert_span_invariants(input, anchor.span);
                assert!(!anchor.name.is_empty());
                assert_eq!(
                    source_slice(input, anchor.span),
                    format!("*{}", anchor.name).as_bytes()
                );
            }
            Event::Scalar { meta, span, .. } => {
                assert!(docs > 0);
                assert_span_invariants(input, *span);
                assert_event_meta_invariants(input, meta);
            }
        }
        assert!(docs >= 0);
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
