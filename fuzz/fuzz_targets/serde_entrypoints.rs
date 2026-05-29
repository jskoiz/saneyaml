#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::{
    Deserialize,
    de::{self, DeserializeOwned, Visitor},
};
use std::fmt;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};
use yaml::{Error, LoadOptions, Span, Value};

fuzz_target!(|input: &[u8]| {
    assert_single_document_entrypoint(input);
    assert_document_stream_entrypoints(input);
    assert_reader_backed_entrypoints(input);
    assert_config_string_map_entrypoints(input);
    assert_numeric_map_entrypoints(input);
    assert_typed_reader_entrypoints(input);
    assert_yaml11_collection_tag_entrypoints(input);
    assert_borrowed_entrypoints(input);
    assert_byte_entrypoints(input);
});

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

#[derive(Debug, Deserialize, PartialEq)]
struct OwnedReaderConfig {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    vars: BTreeMap<String, String>,
    #[serde(default)]
    ints: BTreeMap<String, i128>,
    #[serde(default)]
    uints: BTreeMap<String, u128>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TaggedAliasValues {
    first: String,
    second: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct RootStringConfig {
    root: BTreeMap<String, String>,
    #[serde(default)]
    alias_value: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ValueStructuralTags {
    value_key: String,
    #[serde(default)]
    value_mapping: BTreeMap<String, String>,
}

#[derive(Debug)]
struct FuzzBytes;

struct FuzzByteVisitor;

impl<'de> Visitor<'de> for FuzzByteVisitor {
    type Value = FuzzBytes;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bytes")
    }

    fn visit_bytes<E>(self, _value: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(FuzzBytes)
    }

    fn visit_borrowed_bytes<E>(self, _value: &'de [u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(FuzzBytes)
    }

    fn visit_byte_buf<E>(self, _value: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(FuzzBytes)
    }
}

impl<'de> Deserialize<'de> for FuzzBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_bytes(FuzzByteVisitor)
    }
}

fn assert_single_document_entrypoint(input: &[u8]) {
    let result = yaml::from_slice::<Value>(input);
    match result {
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
            let input = std::str::from_utf8(input).expect("document success must be UTF-8");
            let nodes = yaml::parse_documents(input).expect("document success must parse");
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
        Ok(expected) => assert_stream_results_match_document_values(
            stream_results,
            expected,
            "stream document",
        ),
        Err(_) => {
            assert!(
                stream_results.iter().any(Result::is_err),
                "stream deserializer should surface parse errors"
            );
            for error in stream_results.iter().filter_map(|result| result.as_ref().err()) {
                assert_error_invariants(input, error);
            }
        }
    }
}

fn assert_reader_backed_entrypoints(input: &[u8]) {
    match (
        yaml::from_slice::<Value>(input),
        yaml::from_reader::<_, Value>(Cursor::new(input)),
    ) {
        (Ok(from_slice), Ok(from_reader)) => assert!(from_slice.equivalent(&from_reader)),
        (Err(slice_error), Err(reader_error)) => {
            assert_error_invariants(input, &slice_error);
            assert_error_invariants(input, &reader_error);
        }
        (from_slice, from_reader) => panic!(
            "from_reader drifted from from_slice: from_slice={from_slice:?}, from_reader={from_reader:?}"
        ),
    }

    let from_documents_slice = yaml::from_documents_slice::<Value>(input);
    let from_documents_reader = yaml::from_documents_reader::<Value, _>(Cursor::new(input));
    match (from_documents_slice, from_documents_reader) {
        (Ok(from_slice), Ok(from_reader)) => {
            assert_eq!(from_slice.len(), from_reader.len());
            for (from_slice, from_reader) in from_slice.iter().zip(from_reader.iter()) {
                assert!(from_slice.equivalent(from_reader));
            }
        }
        (Err(slice_error), Err(reader_error)) => {
            assert_error_invariants(input, &slice_error);
            assert_error_invariants(input, &reader_error);
        }
        (from_slice, from_reader) => panic!(
            "from_documents_reader drifted from from_documents_slice: from_slice={from_slice:?}, from_reader={from_reader:?}"
        ),
    }

    let reader_stream_results = yaml::Deserializer::from_reader(Cursor::new(input))
        .map(Value::deserialize)
        .collect::<Vec<_>>();
    match yaml::from_documents_reader::<Value, _>(Cursor::new(input)) {
        Ok(expected) => assert_stream_results_match_document_values(
            reader_stream_results,
            expected,
            "reader stream document",
        ),
        Err(_) => {
            assert!(
                reader_stream_results.iter().any(Result::is_err),
                "reader stream deserializer should surface parse errors"
            );
            for error in reader_stream_results
                .iter()
                .filter_map(|result| result.as_ref().err())
            {
                assert_error_invariants(input, error);
            }
        }
    }
}

fn assert_stream_results_match_document_values(
    stream_results: Vec<Result<Value, Error>>,
    expected: Vec<Value>,
    context: &str,
) {
    if expected.is_empty() {
        assert_eq!(stream_results.len(), 1);
        let actual = stream_results
            .into_iter()
            .next()
            .expect("empty stream should yield one document")
            .expect("empty stream document should deserialize");
        assert!(actual.is_null(), "{context} should be an empty null document");
        return;
    }

    assert_eq!(stream_results.len(), expected.len());
    for (actual, expected) in stream_results.into_iter().zip(expected) {
        let actual = actual.expect("stream document should deserialize");
        assert!(actual.equivalent(&expected));
    }
}

fn assert_config_string_map_entrypoints(input: &[u8]) {
    assert_owned_reader_entrypoint::<BTreeMap<String, String>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, Option<String>>>(input);
}

fn assert_numeric_map_entrypoints(input: &[u8]) {
    assert_owned_reader_entrypoint::<BTreeMap<String, i128>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, u128>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, i64>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, u64>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, i8>>(input);
    assert_owned_reader_entrypoint::<BTreeMap<String, u8>>(input);
}

fn assert_typed_reader_entrypoints(input: &[u8]) {
    assert_owned_reader_entrypoint::<OwnedReaderConfig>(input);
    assert_owned_reader_entrypoint::<TaggedAliasValues>(input);
    assert_owned_reader_entrypoint::<RootStringConfig>(input);
}

fn assert_yaml11_collection_tag_entrypoints(input: &[u8]) {
    for options in [
        LoadOptions::new(),
        LoadOptions::yaml_1_1(),
        LoadOptions::yaml_version_directive(),
    ] {
        assert_load_options_reader_pair::<BTreeSet<String>>(input, options);
        assert_load_options_reader_pair::<Vec<(String, i64)>>(input, options);
        assert_load_options_reader_pair::<ValueStructuralTags>(input, options);
    }
}

fn assert_load_options_reader_pair<T>(input: &[u8], options: LoadOptions)
where
    T: DeserializeOwned + fmt::Debug + PartialEq,
{
    assert_entrypoint_pair(
        input,
        "LoadOptions from_slice/from_reader",
        options.from_slice::<T>(input),
        options.from_reader::<_, T>(Cursor::new(input)),
    );
}

fn assert_owned_reader_entrypoint<T>(input: &[u8])
where
    T: DeserializeOwned + fmt::Debug + PartialEq,
{
    assert_entrypoint_pair(
        input,
        "from_slice/from_reader",
        yaml::from_slice::<T>(input),
        yaml::from_reader::<_, T>(Cursor::new(input)),
    );

    assert_entrypoint_pair(
        input,
        "direct slice/reader deserializer",
        T::deserialize(yaml::Deserializer::from_slice(input)),
        T::deserialize(yaml::Deserializer::from_reader(Cursor::new(input))),
    );

    assert_entrypoint_pair(
        input,
        "from_documents_slice/from_documents_reader",
        yaml::from_documents_slice::<T>(input),
        yaml::from_documents_reader::<T, _>(Cursor::new(input)),
    );

    assert_stream_results_match(
        input,
        yaml::Deserializer::from_slice(input)
            .map(T::deserialize)
            .collect::<Vec<_>>(),
        yaml::Deserializer::from_reader(Cursor::new(input))
            .map(T::deserialize)
            .collect::<Vec<_>>(),
    );
}

fn assert_entrypoint_pair<T>(
    input: &[u8],
    label: &str,
    left: yaml::Result<T>,
    right: yaml::Result<T>,
) where
    T: fmt::Debug + PartialEq,
{
    match (left, right) {
        (Ok(left), Ok(right)) => assert_eq!(left, right, "{label} drifted"),
        (Err(left), Err(right)) => {
            assert_error_invariants_allowing_unspanned(input, &left);
            assert_error_invariants_allowing_unspanned(input, &right);
        }
        (left, right) => panic!("{label} drifted: left={left:?}, right={right:?}"),
    }
}

fn assert_stream_results_match<T>(
    input: &[u8],
    left: Vec<yaml::Result<T>>,
    right: Vec<yaml::Result<T>>,
) where
    T: fmt::Debug + PartialEq,
{
    assert_eq!(left.len(), right.len(), "typed stream length drifted");
    for (left, right) in left.into_iter().zip(right) {
        assert_entrypoint_pair(input, "typed stream slice/reader", left, right);
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
        Err(error) => {
            assert_error_invariants_allowing_unspanned(input, &error);
        }
    }

    for result in yaml::Deserializer::from_slice(input)
        .map(BorrowedConfig::deserialize)
    {
        match result {
            Ok(config) => {
                assert_borrowed_from_input(input, config.name);
                assert_borrowed_from_input(input, config.path);
            }
            Err(error) => assert_error_invariants(input, &error),
        }
    }
}

fn assert_byte_entrypoints(input: &[u8]) {
    if let Err(error) = yaml::from_slice::<FuzzBytes>(input) {
        assert_error_invariants(input, &error);
    }

    if let Err(error) = FuzzBytes::deserialize(yaml::Deserializer::from_slice(input)) {
        assert_error_invariants_allowing_unspanned(input, &error);
    }

    for result in yaml::Deserializer::from_slice(input).map(FuzzBytes::deserialize) {
        if let Err(error) = result {
            assert_error_invariants(input, &error);
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

fn assert_error_invariants(input: &[u8], error: &Error) {
    let diagnostic = error.diagnostic();
    assert!(!diagnostic.message.is_empty());
    assert_span_invariants(input, diagnostic.span);
    for related in &diagnostic.related {
        assert!(!related.message.is_empty());
        assert_span_invariants(input, related.span);
    }
}

fn assert_error_invariants_allowing_unspanned(input: &[u8], error: &Error) {
    let diagnostic = error.diagnostic();
    assert!(!diagnostic.message.is_empty());
    if error.location().is_some() {
        assert_span_invariants(input, diagnostic.span);
    } else {
        assert_eq!(diagnostic.span, Span::default());
    }
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
