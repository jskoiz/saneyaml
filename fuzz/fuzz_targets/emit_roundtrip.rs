#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::Serialize;
use yaml::{Node, Value};

fuzz_target!(|input: &[u8]| {
    let Ok(node) = yaml::parse_bytes(input) else {
        return;
    };
    assert_emit_roundtrip_invariants(&node);
});

fn assert_emit_roundtrip_invariants(node: &Node) {
    let emitted = yaml::to_string(node).expect("emit parsed tree");
    let mut written = Vec::new();
    yaml::to_writer(&mut written, node).expect("write parsed tree");
    assert_eq!(written, emitted.as_bytes());

    let reparsed = yaml::parse_str(&emitted).expect("parse emitted tree");
    assert!(reparsed.equivalent(node));
    let emitted_again = yaml::to_string(&reparsed).expect("emit reparsed tree");
    assert_eq!(emitted_again, emitted);

    let value = Value::from(node);
    let value_emitted = yaml::to_string(&value).expect("emit parsed value");
    let mut value_written = Vec::new();
    yaml::to_writer(&mut value_written, &value).expect("write parsed value");
    assert_eq!(value_written, value_emitted.as_bytes());

    let reparsed_value: Value = yaml::from_str(&value_emitted).expect("parse emitted value");
    assert!(reparsed_value.equivalent(&value));

    let mut stream = yaml::Serializer::new(Vec::new());
    value.serialize(&mut stream).expect("stream first value");
    reparsed_value
        .serialize(&mut stream)
        .expect("stream second value");
    let stream_output = String::from_utf8(stream.into_inner().expect("stream into inner"))
        .expect("stream output is utf8");
    let stream_values =
        yaml::from_documents_str::<Value>(&stream_output).expect("parse streamed values");
    assert_eq!(stream_values.len(), 2);
    assert!(stream_values[0].equivalent(&value));
    assert!(stream_values[1].equivalent(&reparsed_value));
}
