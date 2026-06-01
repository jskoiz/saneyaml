#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::Serialize;
use yaml::{BlockScalarStyle, EmitCollectionStyle, EmitOptions, Node, ScalarQuoteStyle, Value};

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
    for options in emit_option_roundtrip_matrix() {
        assert_optioned_value_roundtrip(&value, options);
    }

    let byte_value_emitted =
        yaml::to_string_with_options(&value, EmitOptions::byte_compatible())
            .expect("emit parsed value in byte-compatible mode");
    let mut byte_value_written = Vec::new();
    yaml::to_writer_with_options(&mut byte_value_written, &value, EmitOptions::byte_compatible())
        .expect("write parsed value in byte-compatible mode");
    assert_eq!(byte_value_written, byte_value_emitted.as_bytes());
    let byte_reparsed_value: Value =
        yaml::from_str(&byte_value_emitted).expect("parse byte-compatible emitted value");
    assert!(byte_reparsed_value.equivalent(&value));

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

    let mut byte_stream =
        yaml::Serializer::with_options(Vec::new(), EmitOptions::byte_compatible());
    value
        .serialize(&mut byte_stream)
        .expect("byte-compatible stream first value");
    byte_reparsed_value
        .serialize(&mut byte_stream)
        .expect("byte-compatible stream second value");
    let byte_stream_output =
        String::from_utf8(byte_stream.into_inner().expect("byte stream into inner"))
            .expect("byte stream output is utf8");
    let byte_stream_values =
        yaml::from_documents_str::<Value>(&byte_stream_output)
            .expect("parse byte-compatible streamed values");
    assert_eq!(byte_stream_values.len(), 2);
    assert!(byte_stream_values[0].equivalent(&value));
    assert!(byte_stream_values[1].equivalent(&byte_reparsed_value));
}

fn emit_option_roundtrip_matrix() -> [EmitOptions; 4] {
    [
        EmitOptions::structural().with_scalar_quote_style(ScalarQuoteStyle::SingleQuoted),
        EmitOptions::structural().with_scalar_quote_style(ScalarQuoteStyle::DoubleQuoted),
        EmitOptions::structural().with_block_scalar_style(BlockScalarStyle::Folded),
        EmitOptions::structural().with_collection_style(EmitCollectionStyle::Flow),
    ]
}

fn assert_optioned_value_roundtrip(value: &Value, options: EmitOptions) {
    let emitted =
        yaml::to_string_with_options(value, options).expect("emit optioned parsed value");
    let mut written = Vec::new();
    yaml::to_writer_with_options(&mut written, value, options)
        .expect("write optioned parsed value");
    assert_eq!(written, emitted.as_bytes());

    let reparsed: Value = yaml::from_str(&emitted).expect("parse optioned emitted value");
    assert!(reparsed.equivalent(value));
    let emitted_again =
        yaml::to_string_with_options(&reparsed, options).expect("re-emit optioned parsed value");
    assert_eq!(emitted_again, emitted);
}
