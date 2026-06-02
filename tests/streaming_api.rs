use saneyaml::{DocumentStream, Event, EventStream, LoadOptions, Node, NodeValue};
use std::io::{self, Cursor, Read};

const ALIAS_EXPANSION_BOMB: &str = "\
a: &a [lol, lol, lol, lol, lol, lol, lol, lol]
b: &b [*a, *a, *a, *a, *a, *a, *a, *a]
c: &c [*b, *b, *b, *b, *b, *b, *b, *b]
d: &d [*c, *c, *c, *c, *c, *c, *c, *c]
boom: *d
";

struct FailingAfterPrefixReader {
    prefix: Cursor<Vec<u8>>,
}

impl FailingAfterPrefixReader {
    fn new(prefix: &[u8]) -> Self {
        Self {
            prefix: Cursor::new(prefix.to_vec()),
        }
    }
}

impl Read for FailingAfterPrefixReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.prefix.read(buf)?;
        if read == 0 {
            Err(io::Error::other("stream interrupted"))
        } else {
            Ok(read)
        }
    }
}

#[test]
fn event_stream_collects_exactly_like_parse_events() {
    for input in [
        "root:\n  items:\n    - one\n    - two\n",
        "%TAG !e! tag:example.com,2026:\n---\nroot: !e!Thing [x, &y y, *y]\n",
        include_str!("fixtures/yaml-test-suite/data/9KAX/in.yaml"),
        include_str!("fixtures/real-world/docker-compose/compose-platform-resources.yaml"),
        include_str!("fixtures/real-world/kubernetes/helm-rendered-stream.yaml"),
    ] {
        let batch = saneyaml::parse_events(input).expect("batch events");
        let streamed = EventStream::from_str(input)
            .expect("stream construction")
            .collect::<saneyaml::Result<Vec<Event>>>()
            .expect("streamed events");
        assert_eq!(streamed, batch);
    }
}

#[test]
fn event_stream_reader_and_slice_match_string_events() {
    let input = "---\nkind: first\n---\nkind: second\n...\n";
    let expected = saneyaml::parse_events(input).expect("batch events");

    let from_slice = EventStream::from_slice(input.as_bytes())
        .expect("slice stream")
        .collect::<saneyaml::Result<Vec<_>>>()
        .expect("slice events");
    let from_reader = EventStream::from_reader(Cursor::new(input.as_bytes()))
        .expect("reader stream")
        .collect::<saneyaml::Result<Vec<_>>>()
        .expect("reader events");

    assert_eq!(from_slice, expected);
    assert_eq!(from_reader, expected);
}

#[test]
fn document_stream_matches_parse_documents_and_applies_merges() {
    let input = "\
base: &base
  image: nginx
service:
  <<: *base
  port: 80
---
flag: true
";
    let batch = saneyaml::parse_documents(input).expect("batch documents");
    let streamed = DocumentStream::from_str(input)
        .expect("document stream")
        .collect::<saneyaml::Result<Vec<Node>>>()
        .expect("streamed documents");

    assert_eq!(streamed, batch);
    assert_eq!(streamed.len(), 2);
    assert_eq!(
        mapping_value(mapping_value(&streamed[0], "service"), "image").as_str(),
        Some("nginx")
    );
}

#[test]
fn streaming_load_options_preserve_input_limit_and_alias_budget_contract() {
    let limited = LoadOptions::new().max_input_bytes(4);
    let event_limit = match limited.stream_events("name: app\n") {
        Ok(_) => panic!("event stream should reject input limit"),
        Err(error) => error,
    };
    let document_limit = match limited.stream_documents("name: app\n") {
        Ok(_) => panic!("document stream should reject input limit"),
        Err(error) => error,
    };
    assert!(
        event_limit
            .to_string()
            .contains("YAML input exceeds configured limit of 4 bytes")
    );
    assert_eq!(event_limit, document_limit);

    let tight_alias_budget = LoadOptions::new().max_alias_expansion_nodes(8);
    let raw_events = tight_alias_budget
        .stream_events(ALIAS_EXPANSION_BOMB)
        .expect("raw event stream")
        .collect::<saneyaml::Result<Vec<_>>>()
        .expect("raw events do not expand aliases");
    assert!(
        raw_events
            .iter()
            .any(|event| matches!(event, Event::Alias { anchor } if anchor.name == "d"))
    );

    let document_error = tight_alias_budget
        .stream_documents(ALIAS_EXPANSION_BOMB)
        .expect("document stream")
        .collect::<saneyaml::Result<Vec<Node>>>()
        .expect_err("semantic document stream enforces alias budget");
    assert!(
        document_error
            .to_string()
            .contains("alias expansion limit exceeded")
    );
}

#[test]
fn document_stream_consumes_large_stream_without_retaining_document_vector() {
    let doc_count = 1024usize;
    let mut input = String::new();
    for idx in 0..doc_count {
        input.push_str("---\nservice:\n  name: app-");
        input.push_str(&idx.to_string());
        input.push_str("\n  image: nginx\n");
    }

    let batch = saneyaml::parse_documents(&input).expect("batch documents");
    assert_eq!(batch.len(), doc_count);

    let mut streamed_count = 0usize;
    let mut max_document_nodes = 0usize;
    for document in DocumentStream::from_str(&input).expect("document stream") {
        let document = document.expect("streamed document");
        max_document_nodes = max_document_nodes.max(count_nodes(&document));
        streamed_count += 1;
    }

    assert_eq!(streamed_count, doc_count);
    assert!(
        max_document_nodes < count_nodes_in_documents(&batch),
        "streaming retains one document at a time instead of the full document vector"
    );
}

#[test]
fn streaming_reader_io_errors_remain_spanless() {
    let event_error = match EventStream::from_reader(FailingAfterPrefixReader::new(b"ok: true\n")) {
        Ok(_) => panic!("event stream should reject reader failure"),
        Err(error) => error,
    };
    let document_error =
        match DocumentStream::from_reader(FailingAfterPrefixReader::new(b"ok: true\n")) {
            Ok(_) => panic!("document stream should reject reader failure"),
            Err(error) => error,
        };

    for error in [event_error, document_error] {
        let display = error.to_string();
        assert!(display.contains("failed to read YAML input"), "{display}");
        assert_eq!(error.location(), None);
        assert!(!display.contains("line 0"));
        assert!(!display.contains("column 0"));
    }
}

#[test]
fn streaming_reader_parse_errors_keep_source_spans() {
    let input = "ok: true\nbad: [unterminated\n";
    let batch_error = saneyaml::parse_events(input).expect_err("batch parse error");
    let event_error = EventStream::from_reader(Cursor::new(input.as_bytes()))
        .expect("event stream")
        .collect::<saneyaml::Result<Vec<_>>>()
        .expect_err("event parse error");
    let document_error = DocumentStream::from_reader(Cursor::new(input.as_bytes()))
        .expect("document stream")
        .collect::<saneyaml::Result<Vec<_>>>()
        .expect_err("document parse error");

    assert_eq!(event_error.span(), batch_error.span());
    assert_eq!(document_error.span(), event_error.span());
}

fn count_nodes_in_documents(documents: &[Node]) -> usize {
    documents.iter().map(count_nodes).sum()
}

fn mapping_value<'a>(node: &'a Node, key: &str) -> &'a Node {
    let NodeValue::Mapping(entries) = &node.value else {
        panic!("expected mapping node for key {key}");
    };
    entries
        .iter()
        .find_map(|(entry_key, value)| (entry_key.as_str() == Some(key)).then_some(value))
        .unwrap_or_else(|| panic!("missing mapping key {key}"))
}

fn count_nodes(node: &Node) -> usize {
    match &node.value {
        NodeValue::Sequence(items) => 1 + items.iter().map(count_nodes).sum::<usize>(),
        NodeValue::Mapping(entries) => {
            1 + entries
                .iter()
                .map(|(key, value)| count_nodes(key) + count_nodes(value))
                .sum::<usize>()
        }
        NodeValue::Tagged(tagged) => 1 + count_nodes(&tagged.value),
        NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) | NodeValue::String(_) => 1,
    }
}
