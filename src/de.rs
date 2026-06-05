//! Serde deserialization entrypoints for YAML strings, bytes, readers, nodes,
//! and values.
//!
//! ```rust
//! use serde::Deserialize;
//! use std::io::Cursor;
//!
//! #[derive(Deserialize)]
//! struct Config {
//!     enabled: bool,
//! }
//!
//! let config: Config = saneyaml::from_reader(Cursor::new(b"enabled: true\n"))?;
//! assert!(config.enabled);
//!
//! let value: saneyaml::Value = saneyaml::from_slice(b"name: api\n")?;
//! assert_eq!(value.get("name").and_then(saneyaml::Value::as_str), Some("api"));
//! # Ok::<(), saneyaml::Error>(())
//! ```

use crate::parse::parse_document_results_with_options;
use crate::{
    Error, ErrorPathSegment, Mapping, Node, NodeValue, Number, Span, Tag, TaggedValue, Value,
    ast::compact_decimal_number_text, error::utf8_error_span, schema::LoadOptions, yaml11,
};
use serde::de::{
    self, DeserializeOwned, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde::forward_to_deserialize_any;
use std::io::Read;

/// Deserializes a single YAML document from a string.
pub fn from_str<'de, T>(input: &'de str) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    from_str_with_options(input, LoadOptions::new())
}

pub(crate) fn from_str_with_options<'de, T>(
    input: &'de str,
    options: LoadOptions,
) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    let node = options.parse_str(input)?;
    from_input_node(&node, input)
}

/// Deserializes a single UTF-8 YAML document from bytes.
pub fn from_slice<'de, T>(input: &'de [u8]) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    from_slice_with_options(input, LoadOptions::new())
}

pub(crate) fn from_slice_with_options<'de, T>(
    input: &'de [u8],
    options: LoadOptions,
) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    options.check_input_len(input.len())?;
    let input = std::str::from_utf8(input)
        .map_err(|err| Error::encoding("input is not valid UTF-8", utf8_error_span(input, err)))?;
    from_str_with_options(input, options)
}

/// Reads all bytes from a reader and deserializes one YAML document.
pub fn from_reader<R, T>(reader: R) -> crate::Result<T>
where
    R: Read,
    T: DeserializeOwned,
{
    from_reader_with_options(reader, LoadOptions::new())
}

pub(crate) fn from_reader_with_options<R, T>(reader: R, options: LoadOptions) -> crate::Result<T>
where
    R: Read,
    T: DeserializeOwned,
{
    let input = read_to_end_with_options(reader, options)?;
    from_slice_with_options(&input, options)
}

/// Deserializes from an already parsed spanful [`Node`].
pub fn from_node<'de, T>(node: &'de Node) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    T::deserialize(node).map_err(|error| error.with_span_if_missing(node.span))
}

fn from_input_node<'de, T>(node: &Node, input: &'de str) -> crate::Result<T>
where
    T: serde::Deserialize<'de>,
{
    T::deserialize(InputNode { node, input }).map_err(|error| error.with_span_if_missing(node.span))
}

/// Deserializes from a spanless YAML [`Value`].
pub fn from_value<T>(value: Value) -> crate::Result<T>
where
    T: DeserializeOwned,
{
    T::deserialize(apply_default_value_merges(value)?)
}

/// Deserializes every document in a YAML stream from a string.
pub fn from_documents_str<T>(input: &str) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
{
    from_documents_str_with_options(input, LoadOptions::new())
}

pub(crate) fn from_documents_str_with_options<T>(
    input: &str,
    options: LoadOptions,
) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
{
    options.check_input_len(input.len())?;
    let results = parse_document_results_with_options(input, options);
    let mut documents = Vec::with_capacity(results.len());
    for (index, result) in results.into_iter().enumerate() {
        let node = result.map_err(|error| error.with_document_index(index))?;
        let value =
            from_input_node(&node, input).map_err(|error| error.with_document_index(index))?;
        documents.push(value);
    }
    Ok(documents)
}

/// Deserializes every document in a UTF-8 YAML stream from bytes.
pub fn from_documents_slice<T>(input: &[u8]) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
{
    from_documents_slice_with_options(input, LoadOptions::new())
}

pub(crate) fn from_documents_slice_with_options<T>(
    input: &[u8],
    options: LoadOptions,
) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
{
    options.check_input_len(input.len())?;
    let input = std::str::from_utf8(input)
        .map_err(|err| Error::encoding("input is not valid UTF-8", utf8_error_span(input, err)))?;
    from_documents_str_with_options(input, options)
}

/// Reads all bytes from a reader and deserializes every YAML document.
pub fn from_documents_reader<T, R>(reader: R) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
    R: Read,
{
    from_documents_reader_with_options(reader, LoadOptions::new())
}

pub(crate) fn from_documents_reader_with_options<T, R>(
    reader: R,
    options: LoadOptions,
) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
    R: Read,
{
    let input = read_to_end_with_options(reader, options)?;
    from_documents_slice_with_options(&input, options)
}

pub(crate) fn read_to_end_with_options<R>(reader: R, options: LoadOptions) -> crate::Result<Vec<u8>>
where
    R: Read,
{
    let mut input = Vec::new();
    if let Some(max) = options.selected_max_input_bytes() {
        let limit = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
        let mut reader = reader.take(limit);
        reader.read_to_end(&mut input).map_err(read_error)?;
        if input.len() > max {
            return Err(options.input_limit_error());
        }
    } else {
        let mut reader = reader;
        reader.read_to_end(&mut input).map_err(read_error)?;
    }
    Ok(input)
}

fn read_error(err: std::io::Error) -> Error {
    Error::io(format!("failed to read YAML input: {err}"), Span::default())
}

/// Streaming Serde deserializer over one or more YAML documents.
#[derive(Debug)]
pub struct Deserializer<'de> {
    documents: std::vec::IntoIter<Document<'de>>,
}

impl<'de> Deserializer<'de> {
    #[allow(clippy::should_implement_trait)]
    /// Creates a streaming deserializer from a YAML string.
    pub fn from_str(input: &'de str) -> Self {
        Self::from_str_with_options(input, LoadOptions::new())
    }

    /// Creates a streaming deserializer from a YAML string using load options.
    pub fn from_str_with_options(input: &'de str, options: LoadOptions) -> Self {
        Self::from_document_results(
            parse_document_results_with_options(input, options),
            Some(input),
        )
    }

    /// Creates a streaming deserializer from UTF-8 YAML bytes.
    pub fn from_slice(input: &'de [u8]) -> Self {
        Self::from_slice_with_options(input, LoadOptions::new())
    }

    /// Creates a streaming deserializer from UTF-8 YAML bytes using load options.
    pub fn from_slice_with_options(input: &'de [u8], options: LoadOptions) -> Self {
        if let Err(error) = options.check_input_len(input.len()) {
            return Self::from_parse_result(Err(error));
        }
        match std::str::from_utf8(input) {
            Ok(input) => Self::from_str_with_options(input, options),
            Err(err) => Self::from_parse_result(Err(Error::encoding(
                "input is not valid UTF-8",
                utf8_error_span(input, err),
            ))),
        }
    }

    /// Reads a YAML stream and creates a streaming deserializer.
    pub fn from_reader<R>(reader: R) -> Self
    where
        R: Read,
    {
        Self::from_reader_with_options(reader, LoadOptions::new())
    }

    /// Reads a YAML stream and creates a streaming deserializer using load options.
    pub fn from_reader_with_options<R>(reader: R, options: LoadOptions) -> Self
    where
        R: Read,
    {
        match read_to_end_with_options(reader, options) {
            Ok(input) => match std::str::from_utf8(&input) {
                Ok(input) => Self::from_document_results(
                    parse_document_results_with_options(input, options),
                    None,
                ),
                Err(err) => Self::from_parse_result(Err(Error::encoding(
                    "input is not valid UTF-8",
                    utf8_error_span(&input, err),
                ))),
            },
            Err(error) => Self::from_parse_result(Err(error)),
        }
    }

    fn from_parse_result(result: crate::Result<Vec<Node>>) -> Self {
        let documents = match result {
            Ok(documents) => documents
                .into_iter()
                .enumerate()
                .map(|(index, node)| Document {
                    node: Ok(node),
                    input: None,
                    index,
                })
                .collect(),
            Err(error) => vec![Document {
                node: Err(error.with_document_index(0)),
                input: None,
                index: 0,
            }],
        };
        Self {
            documents: documents.into_iter(),
        }
    }

    fn from_document_results(results: Vec<crate::Result<Node>>, input: Option<&'de str>) -> Self {
        if results.is_empty() {
            return Self {
                documents: vec![Document {
                    node: Ok(Node::null(Span::point(0, 1, 1))),
                    input,
                    index: 0,
                }]
                .into_iter(),
            };
        }

        Self {
            documents: results
                .into_iter()
                .enumerate()
                .map(|(index, node)| Document {
                    node: node.map_err(|error| error.with_document_index(index)),
                    input,
                    index,
                })
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }

    fn into_single_document(mut self) -> Result<Document<'de>, Error> {
        let Some(document) = self.documents.next() else {
            return Ok(Document {
                node: Ok(Node::null(Span::point(0, 1, 1))),
                input: None,
                index: 0,
            });
        };
        if let Some(extra) = self.documents.next() {
            let span = extra
                .node
                .as_ref()
                .map(|node| node.span)
                .unwrap_or_else(|error| error.span());
            return Err(Error::new(
                "expected a single YAML document; use the iterator API for streams",
                Some(span),
            )
            .with_document_index(extra.index));
        }
        Ok(document)
    }
}

impl<'de> Iterator for Deserializer<'de> {
    type Item = Deserializer<'de>;

    fn next(&mut self) -> Option<Self::Item> {
        self.documents.next().map(|document| Deserializer {
            documents: vec![document].into_iter(),
        })
    }
}

#[derive(Debug)]
struct Document<'de> {
    node: crate::Result<Node>,
    input: Option<&'de str>,
    index: usize,
}

impl<'de> Document<'de> {
    fn as_node(&self) -> Result<&Node, Error> {
        self.node
            .as_ref()
            .map_err(Clone::clone)
            .map_err(|error| error.with_document_index(self.index))
    }

    fn into_node(self) -> Result<Node, Error> {
        self.node
            .map_err(|error| error.with_document_index(self.index))
    }

    fn into_node_and_input(self) -> Result<(Node, Option<&'de str>, usize), Error> {
        let input = self.input;
        let index = self.index;
        self.into_node().map(|node| (node, input, index))
    }
}

fn with_span<T>(result: Result<T, Error>, span: Span) -> Result<T, Error> {
    result.map_err(|error| error.with_span_if_missing(span))
}

fn with_index<T>(result: Result<T, Error>, index: usize) -> Result<T, Error> {
    result.map_err(|error| error.prepend_path_segment(ErrorPathSegment::Index(index)))
}

fn with_key_path<T>(result: Result<T, Error>, segment: ErrorPathSegment) -> Result<T, Error> {
    result.map_err(|error| error.with_path_segment_if_empty(segment))
}

fn with_value_path<T>(result: Result<T, Error>, segment: ErrorPathSegment) -> Result<T, Error> {
    result.map_err(|error| error.prepend_path_segment(segment))
}

fn with_optional_span<T>(result: Result<T, Error>, span: Option<Span>) -> Result<T, Error> {
    match span {
        Some(span) => with_span(result, span),
        None => result,
    }
}

fn node_path_segment(node: &Node) -> ErrorPathSegment {
    value_path_segment(&node.value.clone().into())
}

fn value_path_segment(value: &Value) -> ErrorPathSegment {
    match untag_value(value) {
        Value::String(value) => ErrorPathSegment::Key(value.clone()),
        Value::Bool(value) => ErrorPathSegment::ScalarKey(value.to_string()),
        Value::Number(number) => ErrorPathSegment::ScalarKey(number.to_string()),
        Value::Null => ErrorPathSegment::ScalarKey("null".to_string()),
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_) => ErrorPathSegment::ComplexKey,
    }
}

fn value_key_is_merge_key(key: &Value) -> bool {
    match key {
        Value::String(value) => value == "<<",
        Value::Tagged(tagged) if tagged.tag.is_yaml_core("merge") => {
            tagged.value.as_str() == Some("<<")
        }
        _ => false,
    }
}

fn value_needs_default_merge(value: &Value) -> bool {
    let mut values = vec![value];
    while let Some(value) = values.pop() {
        match value {
            Value::Mapping(mapping) => {
                for (key, value) in mapping.as_slice() {
                    if value_key_is_merge_key(key) {
                        return true;
                    }
                    values.push(value);
                }
            }
            Value::Sequence(sequence) => values.extend(sequence),
            Value::Tagged(tagged) => values.push(&tagged.value),
            _ => {}
        }
    }
    false
}

fn apply_default_value_merges(mut value: Value) -> Result<Value, Error> {
    if value_needs_default_merge(&value) {
        value.apply_merge()?;
    }
    Ok(value)
}

fn merged_value_ref_entries(mapping: &Mapping) -> Result<Option<Vec<(&Value, &Value)>>, Error> {
    let Some(merge_index) = mapping
        .as_slice()
        .iter()
        .position(|(key, _)| value_key_is_merge_key(key))
    else {
        return Ok(None);
    };

    let mut entries = mapping
        .as_slice()
        .iter()
        .enumerate()
        .filter_map(|(index, (key, value))| (index != merge_index).then_some((key, value)))
        .collect::<Vec<_>>();
    let merge = &mapping.as_slice()[merge_index].1;
    let merge_entries = value_ref_merge_entries(merge)?;
    insert_missing_value_ref_entries(&mut entries, merge_entries);
    Ok(Some(entries))
}

fn value_ref_merge_entries(merge: &Value) -> Result<Vec<(&Value, &Value)>, Error> {
    match merge {
        Value::Mapping(mapping) => Ok(merged_value_ref_entries(mapping)?.unwrap_or_else(|| {
            mapping
                .as_slice()
                .iter()
                .map(|(key, value)| (key, value))
                .collect()
        })),
        Value::Sequence(sequence) => {
            let mut entries = Vec::new();
            for value in sequence {
                match value {
                    Value::Mapping(mapping) => {
                        let merge_entries =
                            merged_value_ref_entries(mapping)?.unwrap_or_else(|| {
                                mapping
                                    .as_slice()
                                    .iter()
                                    .map(|(key, value)| (key, value))
                                    .collect()
                            });
                        insert_missing_value_ref_entries(&mut entries, merge_entries);
                    }
                    Value::Sequence(_) => {
                        return Err(value_merge_error(
                            "expected a mapping for merging, but found sequence",
                        ));
                    }
                    Value::Tagged(_) => {
                        return Err(value_merge_error("unexpected tagged value in merge"));
                    }
                    _ => {
                        return Err(value_merge_error(
                            "expected a mapping for merging, but found scalar",
                        ));
                    }
                }
            }
            Ok(entries)
        }
        Value::Tagged(_) => Err(value_merge_error("unexpected tagged value in merge")),
        _ => Err(value_merge_error(
            "expected a mapping or list of mappings for merging, but found scalar",
        )),
    }
}

fn insert_missing_value_ref_entries<'a>(
    entries: &mut Vec<(&'a Value, &'a Value)>,
    merge_entries: Vec<(&'a Value, &'a Value)>,
) {
    for (key, value) in merge_entries {
        if entries.iter().all(|(existing_key, _)| *existing_key != key) {
            entries.push((key, value));
        }
    }
}

fn value_merge_error(message: &'static str) -> Error {
    Error::new(message, None)
}

fn is_empty_null_node(node: &Node) -> bool {
    matches!(node.value, NodeValue::Null)
        && node
            .scalar_source()
            .is_none_or(|source| source.raw().is_empty())
}

fn string_source_for_scalar(node: &Node) -> Option<&str> {
    match node.value {
        NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) => {
            node.scalar_source().map(|source| source.raw())
        }
        _ => None,
    }
}

fn integer_source_for_scalar(node: &Node) -> Option<&str> {
    match node.value {
        NodeValue::String(_) => node.scalar_source().map(|source| source.raw()),
        _ => None,
    }
}

fn parse_i128_source(raw: &str, span: Span) -> Result<i128, Error> {
    compact_decimal_number_text(raw)
        .ok_or_else(|| Error::new("integer scalar is out of range for i128", Some(span)))?
        .parse::<i128>()
        .map_err(|_| Error::new("integer scalar is out of range for i128", Some(span)))
}

fn parse_u128_source(raw: &str, span: Span) -> Result<u128, Error> {
    compact_decimal_number_text(raw)
        .ok_or_else(|| {
            Error::new(
                "integer scalar is out of range for unsigned integer",
                Some(span),
            )
        })?
        .parse::<u128>()
        .map_err(|_| {
            Error::new(
                "integer scalar is out of range for unsigned integer",
                Some(span),
            )
        })
}

#[derive(Clone, Copy)]
struct CoercedNumber {
    number: Number,
    span: Option<Span>,
}

#[derive(Clone, Copy)]
struct CoercedBool {
    value: bool,
    span: Option<Span>,
}

fn explicit_core_int_number_node(node: &Node) -> Result<Option<CoercedNumber>, Error> {
    let Some(node) = explicit_core_tagged_node(node, "int") else {
        return Ok(None);
    };
    match &node.value {
        NodeValue::Number(number) => Ok(Some(CoercedNumber {
            number: *number,
            span: Some(node.span),
        })),
        NodeValue::String(value) => {
            let raw = node
                .scalar_source()
                .map(|source| source.raw())
                .unwrap_or(value);
            parse_explicit_core_int_text(raw, Some(node.span)).map(|number| {
                Some(CoercedNumber {
                    number,
                    span: Some(node.span),
                })
            })
        }
        _ => Err(type_error("integer", node)),
    }
}

fn explicit_core_float_number_node(node: &Node) -> Result<Option<CoercedNumber>, Error> {
    let Some(node) = explicit_core_tagged_node(node, "float") else {
        return Ok(None);
    };
    match &node.value {
        NodeValue::Number(number) => Ok(Some(CoercedNumber {
            number: *number,
            span: Some(node.span),
        })),
        NodeValue::String(value) => {
            let raw = node
                .scalar_source()
                .map(|source| source.raw())
                .unwrap_or(value);
            parse_explicit_core_float_text(raw, Some(node.span)).map(|number| {
                Some(CoercedNumber {
                    number,
                    span: Some(node.span),
                })
            })
        }
        _ => Err(type_error("number", node)),
    }
}

fn explicit_core_binary_bytes_node(node: &Node) -> Result<Option<Vec<u8>>, Error> {
    let Some(node) = explicit_core_tagged_node(node, "binary") else {
        return Ok(None);
    };
    match &node.value {
        NodeValue::String(value) => {
            let raw = node
                .scalar_source()
                .map(|source| source.raw())
                .unwrap_or(value);
            decode_yaml_binary(raw, Some(node.span)).map(Some)
        }
        _ => Err(type_error("binary scalar", node)),
    }
}

fn explicit_core_bool_node(node: &Node) -> Result<Option<CoercedBool>, Error> {
    let Some(node) = explicit_core_tagged_node(node, "bool") else {
        return Ok(None);
    };
    match &node.value {
        NodeValue::Bool(value) => Ok(Some(CoercedBool {
            value: *value,
            span: Some(node.span),
        })),
        NodeValue::String(value) => {
            let raw = node
                .scalar_source()
                .map(|source| source.raw())
                .unwrap_or(value);
            parse_explicit_core_bool_text(raw, Some(node.span)).map(|value| {
                Some(CoercedBool {
                    value,
                    span: Some(node.span),
                })
            })
        }
        _ => Err(type_error("bool", node)),
    }
}

fn explicit_core_null_node(node: &Node) -> Result<bool, Error> {
    let Some(node) = explicit_core_tagged_node(node, "null") else {
        return Ok(false);
    };
    match &node.value {
        NodeValue::Null => Ok(true),
        NodeValue::String(value) => {
            let raw = node
                .scalar_source()
                .map(|source| source.raw())
                .unwrap_or(value);
            parse_explicit_core_null_text(raw, Some(node.span)).map(|()| true)
        }
        _ => Err(type_error("unit/null", node)),
    }
}

fn explicit_core_tagged_node<'a>(mut node: &'a Node, suffix: &str) -> Option<&'a Node> {
    while let NodeValue::Tagged(tagged) = &node.value {
        if tagged.tag.is_yaml_core(suffix) {
            return Some(&tagged.value);
        }
        node = &tagged.value;
    }
    None
}

fn explicit_core_int_number_value(value: &Value) -> Result<Option<CoercedNumber>, Error> {
    let Some(value) = explicit_core_tagged_value(value, "int") else {
        return Ok(None);
    };
    match value {
        Value::Number(number) => Ok(Some(CoercedNumber {
            number: *number,
            span: None,
        })),
        Value::String(value) => parse_explicit_core_int_text(value, None)
            .map(|number| Some(CoercedNumber { number, span: None })),
        other => Err(type_error_value("integer", other)),
    }
}

fn explicit_core_float_number_value(value: &Value) -> Result<Option<CoercedNumber>, Error> {
    let Some(value) = explicit_core_tagged_value(value, "float") else {
        return Ok(None);
    };
    match value {
        Value::Number(number) => Ok(Some(CoercedNumber {
            number: *number,
            span: None,
        })),
        Value::String(value) => parse_explicit_core_float_text(value, None)
            .map(|number| Some(CoercedNumber { number, span: None })),
        other => Err(type_error_value("number", other)),
    }
}

fn explicit_core_binary_bytes_value(value: &Value) -> Result<Option<Vec<u8>>, Error> {
    let Some(value) = explicit_core_tagged_value(value, "binary") else {
        return Ok(None);
    };
    match value {
        Value::String(value) => decode_yaml_binary(value, None).map(Some),
        other => Err(type_error_value("binary scalar", other)),
    }
}

fn explicit_core_bool_value(value: &Value) -> Result<Option<CoercedBool>, Error> {
    let Some(value) = explicit_core_tagged_value(value, "bool") else {
        return Ok(None);
    };
    match value {
        Value::Bool(value) => Ok(Some(CoercedBool {
            value: *value,
            span: None,
        })),
        Value::String(value) => parse_explicit_core_bool_text(value, None)
            .map(|value| Some(CoercedBool { value, span: None })),
        other => Err(type_error_value("bool", other)),
    }
}

fn explicit_core_null_value(value: &Value) -> Result<bool, Error> {
    let Some(value) = explicit_core_tagged_value(value, "null") else {
        return Ok(false);
    };
    match value {
        Value::Null => Ok(true),
        Value::String(value) => parse_explicit_core_null_text(value, None).map(|()| true),
        other => Err(type_error_value("unit/null", other)),
    }
}

fn explicit_core_tagged_value<'a>(mut value: &'a Value, suffix: &str) -> Option<&'a Value> {
    while let Value::Tagged(tagged) = value {
        if tagged.tag.is_yaml_core(suffix) {
            return Some(&tagged.value);
        }
        value = &tagged.value;
    }
    None
}

fn take_explicit_core_tagged_node(mut node: Node, suffix: &str) -> Option<Node> {
    loop {
        match node.value {
            NodeValue::Tagged(tagged) if tagged.tag.is_yaml_core(suffix) => {
                return Some(tagged.value);
            }
            NodeValue::Tagged(tagged) => node = tagged.value,
            _ => return None,
        }
    }
}

fn take_explicit_core_tagged_value(mut value: Value, suffix: &str) -> Option<Value> {
    loop {
        match value {
            Value::Tagged(tagged) if tagged.tag.is_yaml_core(suffix) => return Some(tagged.value),
            Value::Tagged(tagged) => value = tagged.value,
            _ => return None,
        }
    }
}

fn yaml11_set_entries_node(node: &Node) -> Result<Option<&[(Node, Node)]>, Error> {
    let Some(value) = explicit_core_tagged_node(node, "set") else {
        return Ok(None);
    };
    match &value.value {
        NodeValue::Mapping(entries) => Ok(Some(entries)),
        _ => Err(type_error("mapping for explicit !!set", value)),
    }
}

fn yaml11_set_entries_value(value: &Value) -> Result<Option<&[(Value, Value)]>, Error> {
    let Some(value) = explicit_core_tagged_value(value, "set") else {
        return Ok(None);
    };
    match value {
        Value::Mapping(entries) => Ok(Some(entries.as_slice())),
        other => Err(type_error_value("mapping for explicit !!set", other)),
    }
}

fn yaml11_pair_items_node<'a>(
    node: &'a Node,
    suffix: &'static str,
) -> Result<Option<&'a [Node]>, Error> {
    let Some(value) = explicit_core_tagged_node(node, suffix) else {
        return Ok(None);
    };
    match &value.value {
        NodeValue::Sequence(items) => Ok(Some(items)),
        _ => Err(Error::new(
            format!("expected sequence for explicit !!{suffix}"),
            Some(value.span),
        )),
    }
}

fn yaml11_pair_items_value<'a>(
    value: &'a Value,
    suffix: &'static str,
) -> Result<Option<&'a [Value]>, Error> {
    let Some(value) = explicit_core_tagged_value(value, suffix) else {
        return Ok(None);
    };
    match value {
        Value::Sequence(items) => Ok(Some(items)),
        _ => Err(Error::new(
            format!("expected sequence for explicit !!{suffix}"),
            None,
        )),
    }
}

fn take_yaml11_set_entries_node(node: Node) -> Option<Vec<(Node, Node)>> {
    let value = take_explicit_core_tagged_node(node, "set")?;
    match value.value {
        NodeValue::Mapping(entries) => Some(entries),
        _ => None,
    }
}

fn take_yaml11_set_entries_value(value: Value) -> Option<Mapping> {
    match take_explicit_core_tagged_value(value, "set")? {
        Value::Mapping(entries) => Some(entries),
        _ => None,
    }
}

fn take_yaml11_pair_items_node(node: Node, suffix: &'static str) -> Option<Vec<Node>> {
    let value = take_explicit_core_tagged_node(node, suffix)?;
    match value.value {
        NodeValue::Sequence(items) => Some(items),
        _ => None,
    }
}

fn take_yaml11_pair_items_value(value: Value, suffix: &'static str) -> Option<Vec<Value>> {
    match take_explicit_core_tagged_value(value, suffix)? {
        Value::Sequence(items) => Some(items),
        _ => None,
    }
}

fn ensure_yaml11_set_null_node(value: &Node) -> Result<(), Error> {
    if explicit_core_null_node(value)? || matches!(untag_node(value).value, NodeValue::Null) {
        Ok(())
    } else {
        Err(Error::new(
            "expected explicit !!set entry value to be null",
            Some(value.span),
        ))
    }
}

fn ensure_yaml11_set_null_value(value: &Value) -> Result<(), Error> {
    if explicit_core_null_value(value)? || matches!(untag_value(value), Value::Null) {
        Ok(())
    } else {
        Err(Error::new(
            "expected explicit !!set entry value to be null",
            None,
        ))
    }
}

fn yaml11_singleton_pair_node<'a>(
    node: &'a Node,
    suffix: &'static str,
) -> Result<(&'a Node, &'a Node), Error> {
    let node = untag_node(node);
    match &node.value {
        NodeValue::Mapping(entries) if entries.len() == 1 => Ok((&entries[0].0, &entries[0].1)),
        NodeValue::Mapping(_) => Err(Error::new(
            format!("expected explicit !!{suffix} entry to contain exactly one pair"),
            Some(node.span),
        )),
        _ => Err(Error::new(
            format!("expected single-pair mapping entry for explicit !!{suffix}"),
            Some(node.span),
        )),
    }
}

fn yaml11_singleton_pair_value<'a>(
    value: &'a Value,
    suffix: &'static str,
) -> Result<(&'a Value, &'a Value), Error> {
    let value = untag_value(value);
    match value {
        Value::Mapping(entries) if entries.len() == 1 => {
            let entries = entries.as_slice();
            Ok((&entries[0].0, &entries[0].1))
        }
        Value::Mapping(_) => Err(Error::new(
            format!("expected explicit !!{suffix} entry to contain exactly one pair"),
            None,
        )),
        _ => Err(Error::new(
            format!("expected single-pair mapping entry for explicit !!{suffix}"),
            None,
        )),
    }
}

fn take_yaml11_singleton_pair_node(
    node: Node,
    suffix: &'static str,
) -> Result<(Node, Node), Error> {
    let node = untag_node_owned(node);
    match node.value {
        NodeValue::Mapping(entries) if entries.len() == 1 => {
            let mut entries = entries.into_iter();
            entries.next().ok_or_else(|| {
                Error::new(
                    "internal: singleton mapping lost its entry",
                    Some(node.span),
                )
            })
        }
        NodeValue::Mapping(_) => Err(Error::new(
            format!("expected explicit !!{suffix} entry to contain exactly one pair"),
            Some(node.span),
        )),
        _ => Err(Error::new(
            format!("expected single-pair mapping entry for explicit !!{suffix}"),
            Some(node.span),
        )),
    }
}

fn take_yaml11_singleton_pair_value(
    value: Value,
    suffix: &'static str,
) -> Result<(Value, Value), Error> {
    match untag_value_owned(value) {
        Value::Mapping(entries) if entries.len() == 1 => {
            let mut entries = entries.into_iter();
            entries
                .next()
                .ok_or_else(|| Error::new("internal: singleton mapping lost its entry", None))
        }
        Value::Mapping(_) => Err(Error::new(
            format!("expected explicit !!{suffix} entry to contain exactly one pair"),
            None,
        )),
        _ => Err(Error::new(
            format!("expected single-pair mapping entry for explicit !!{suffix}"),
            None,
        )),
    }
}

fn parse_explicit_core_int_text(raw: &str, span: Option<Span>) -> Result<Number, Error> {
    yaml11::parse_explicit_int_number(raw)
        .ok_or_else(|| Error::new("failed to parse explicit !!int scalar", span))
}

fn parse_explicit_core_float_text(raw: &str, span: Option<Span>) -> Result<Number, Error> {
    yaml11::parse_explicit_float_number(raw)
        .ok_or_else(|| Error::new("failed to parse explicit !!float scalar", span))
}

fn parse_explicit_core_bool_text(raw: &str, span: Option<Span>) -> Result<bool, Error> {
    yaml11::parse_bool(raw)
        .ok_or_else(|| Error::new("failed to parse explicit !!bool scalar", span))
}

fn parse_explicit_core_null_text(raw: &str, span: Option<Span>) -> Result<(), Error> {
    yaml11::is_null(raw)
        .then_some(())
        .ok_or_else(|| Error::new("failed to parse explicit !!null scalar", span))
}

fn decode_yaml_binary(raw: &str, span: Option<Span>) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    let mut quartet = [0u8; 4];
    let mut len = 0usize;
    let mut saw_padding = false;

    for byte in raw.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        let value = match byte {
            b'=' => {
                saw_padding = true;
                64
            }
            _ if saw_padding => return Err(Error::new("invalid explicit !!binary scalar", span)),
            _ => base64_value(byte)
                .ok_or_else(|| Error::new("invalid explicit !!binary scalar", span))?,
        };
        quartet[len] = value;
        len += 1;
        if len == 4 {
            if quartet[0] == 64 || quartet[1] == 64 || (quartet[2] == 64 && quartet[3] != 64) {
                return Err(Error::new("invalid explicit !!binary scalar", span));
            }
            output.push((quartet[0] << 2) | (quartet[1] >> 4));
            if quartet[2] == 64 {
                if quartet[1] & 0x0f != 0 {
                    return Err(Error::new("invalid explicit !!binary scalar", span));
                }
            } else {
                output.push(((quartet[1] & 0x0f) << 4) | (quartet[2] >> 2));
                if quartet[3] == 64 {
                    if quartet[2] & 0x03 != 0 {
                        return Err(Error::new("invalid explicit !!binary scalar", span));
                    }
                } else {
                    output.push(((quartet[2] & 0x03) << 6) | quartet[3]);
                }
            }
            len = 0;
        }
    }

    match len {
        0 => Ok(output),
        2 if !saw_padding => {
            if quartet[1] & 0x0f != 0 {
                return Err(Error::new("invalid explicit !!binary scalar", span));
            }
            output.push((quartet[0] << 2) | (quartet[1] >> 4));
            Ok(output)
        }
        3 if !saw_padding => {
            if quartet[2] & 0x03 != 0 {
                return Err(Error::new("invalid explicit !!binary scalar", span));
            }
            output.push((quartet[0] << 2) | (quartet[1] >> 4));
            output.push(((quartet[1] & 0x0f) << 4) | (quartet[2] >> 2));
            Ok(output)
        }
        _ => Err(Error::new("invalid explicit !!binary scalar", span)),
    }
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn visit_i64_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => {
            let value = i64::try_from(value)
                .map_err(|_| Error::new("integer scalar is out of range for i64", span))?;
            with_optional_span(visitor.visit_i64(value), span)
        }
        Number::Unsigned(value) => match i64::try_from(value) {
            Ok(value) => with_optional_span(visitor.visit_i64(value), span),
            Err(_) => Err(Error::new(
                format!("expected integer, found unsigned integer: {value}"),
                span,
            )),
        },
        Number::Float(f) => Err(Error::new(
            format!("expected integer, found float: {f}"),
            span,
        )),
    }
}

fn visit_u64_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) if value >= 0 => {
            let value = u64::try_from(value)
                .map_err(|_| Error::new("integer scalar is out of range for u64", span))?;
            with_optional_span(visitor.visit_u64(value), span)
        }
        Number::Unsigned(value) => {
            let value = u64::try_from(value)
                .map_err(|_| Error::new("integer scalar is out of range for u64", span))?;
            with_optional_span(visitor.visit_u64(value), span)
        }
        Number::Integer(value) => Err(Error::new(
            format!("expected unsigned integer, found integer: {value}"),
            span,
        )),
        Number::Float(f) => Err(Error::new(
            format!("expected unsigned integer, found float: {f}"),
            span,
        )),
    }
}

fn visit_i128_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => with_optional_span(visitor.visit_i128(value), span),
        Number::Unsigned(value) => match i128::try_from(value) {
            Ok(value) => with_optional_span(visitor.visit_i128(value), span),
            Err(_) => Err(Error::new("integer scalar is out of range for i128", span)),
        },
        Number::Float(f) => Err(Error::new(
            format!("expected integer, found float: {f}"),
            span,
        )),
    }
}

fn visit_u128_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) if value >= 0 => with_optional_span(
            visitor.visit_u128(u128::try_from(value).expect("non-negative i128 fits u128")),
            span,
        ),
        Number::Unsigned(value) => with_optional_span(visitor.visit_u128(value), span),
        Number::Integer(value) => Err(Error::new(
            format!("expected unsigned integer, found integer: {value}"),
            span,
        )),
        Number::Float(f) => Err(Error::new(
            format!("expected unsigned integer, found float: {f}"),
            span,
        )),
    }
}

fn visit_f64_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    let result = match number {
        Number::Integer(value) => visitor.visit_f64(value as f64),
        Number::Unsigned(value) => visitor.visit_f64(value as f64),
        Number::Float(value) => visitor.visit_f64(value),
    };
    with_optional_span(result, span)
}

fn visit_any_number<'de, V>(
    number: Number,
    span: Option<Span>,
    visitor: V,
) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    let result = match number {
        Number::Integer(value) => match i64::try_from(value) {
            Ok(value) => visitor.visit_i64(value),
            Err(_) => visitor.visit_i128(value),
        },
        Number::Unsigned(value) => match u64::try_from(value) {
            Ok(value) => visitor.visit_u64(value),
            Err(_) => visitor.visit_u128(value),
        },
        Number::Float(value) => visitor.visit_f64(value),
    };
    with_optional_span(result, span)
}

#[derive(Clone, Copy)]
struct InputNode<'tree, 'de> {
    node: &'tree Node,
    input: &'de str,
}

impl<'tree, 'de> InputNode<'tree, 'de> {
    fn untag(self) -> Self {
        let mut node = self.node;
        while let NodeValue::Tagged(tagged) = &node.value {
            node = &tagged.value;
        }
        Self { node, ..self }
    }

    fn borrowed_str(self) -> Option<&'de str> {
        let node = self.untag().node;
        let raw = self.input.get(node.span.start..node.span.end)?;
        match &node.value {
            NodeValue::String(value) => borrowable_string(raw, value),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) => {
                let source = string_source_for_scalar(node)?;
                (raw == source).then_some(raw)
            }
            _ => None,
        }
    }

    fn transient_str(self) -> Option<&'tree str> {
        let node = self.untag().node;
        match &node.value {
            NodeValue::String(value) => Some(value),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) => {
                string_source_for_scalar(node)
            }
            _ => None,
        }
    }
}

fn borrowable_string<'de>(raw: &'de str, value: &str) -> Option<&'de str> {
    if raw == value {
        return Some(raw);
    }

    let quote = raw.chars().next()?;
    if !matches!(quote, '"' | '\'') || !raw.ends_with(quote) || raw.len() < 2 {
        return None;
    }
    let inner = &raw[quote.len_utf8()..raw.len() - quote.len_utf8()];
    (inner == value).then_some(inner)
}

impl<'de, 'tree> de::Deserializer<'de> for InputNode<'tree, 'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match &self.node.value {
            NodeValue::Null => visitor.visit_unit(),
            NodeValue::Bool(value) => visitor.visit_bool(*value),
            NodeValue::Number(number) => visit_any_number(*number, Some(self.node.span), visitor),
            NodeValue::String(_) => match self.borrowed_str() {
                Some(value) => visitor.visit_borrowed_str(value),
                None => visitor.visit_str(
                    self.transient_str()
                        .expect("string node has transient string value"),
                ),
            },
            NodeValue::Sequence(items) => visitor.visit_seq(InputSeqDeserializer {
                items,
                input: self.input,
                index: 0,
            }),
            NodeValue::Mapping(entries) => visitor.visit_map(InputMapDeserializer {
                entries,
                input: self.input,
                index: 0,
                value: None,
            }),
            NodeValue::Tagged(tagged) => visitor.visit_enum(InputTaggedEnumDeserializer {
                tag: &tagged.tag,
                value: InputNode {
                    node: &tagged.value,
                    input: self.input,
                },
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(value) = explicit_core_bool_node(self.node)? {
            return with_optional_span(visitor.visit_bool(value.value), value.span);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Bool(value) => visitor.visit_bool(*value),
            _ => Err(type_error("bool", node)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                let value = i64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for i64", Some(node.span))
                })?;
                with_span(visitor.visit_i64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i64::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i64(value), node.span),
                Err(_) => Err(type_error("integer", node)),
            },
            _ => Err(type_error("integer", node)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            _ => Err(type_error("unsigned integer", node)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_i128(*value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i128::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i128(value), node.span),
                Err(_) => Err(Error::new(
                    "integer scalar is out of range for i128",
                    Some(node.span),
                )),
            },
            NodeValue::String(_) if integer_source_for_scalar(node).is_some() => {
                let value = parse_i128_source(
                    integer_source_for_scalar(node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_i128(value), node.span)
            }
            _ => Err(type_error("integer", node)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u128::try_from(*value).expect("non-negative i128 fits u128");
                with_span(visitor.visit_u128(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_u128(*value), node.span)
            }
            NodeValue::String(_) if integer_source_for_scalar(node).is_some() => {
                let value = parse_u128_source(
                    integer_source_for_scalar(node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_u128(value), node.span)
            }
            _ => Err(type_error("unsigned integer", node)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_node(self.node)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_node(self.node)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_f64(*value as f64), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_f64(*value as f64), node.span)
            }
            NodeValue::Number(Number::Float(value)) => {
                with_span(visitor.visit_f64(*value), node.span)
            }
            _ => Err(type_error("number", node)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag().node;
        match &node.value {
            NodeValue::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(type_error("char", node)),
                }
            }
            _ => Err(type_error("char", node)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(value) = self.borrowed_str() {
            return visitor.visit_borrowed_str(value);
        }
        if let Some(value) = self.transient_str() {
            return visitor.visit_str(value);
        }
        Err(type_error("string", self.untag().node))
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = self.untag().node;
        match &node.value {
            NodeValue::String(value) => visitor.visit_string(value.clone()),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_)
                if string_source_for_scalar(node).is_some() =>
            {
                visitor.visit_string(
                    string_source_for_scalar(node)
                        .expect("scalar source checked")
                        .to_string(),
                )
            }
            _ => Err(type_error("string", node)),
        }
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_node(self.node)? {
            return _visitor.visit_byte_buf(bytes);
        }
        Err(type_error("bytes", self.untag().node))
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_node(self.node)? {
            return _visitor.visit_byte_buf(bytes);
        }
        Err(type_error("bytes", self.untag().node))
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if explicit_core_null_node(self.node)? {
            return visitor.visit_none();
        }
        let node = self.untag();
        match &node.node.value {
            NodeValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(node),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if explicit_core_null_node(self.node)? {
            return visitor.visit_unit();
        }
        let node = self.untag().node;
        match &node.value {
            NodeValue::Null => visitor.visit_unit(),
            _ => Err(type_error("unit/null", node)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self.untag())
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_node(self.node)? {
            return visitor.visit_seq(ByteBufSeqDeserializer::new(bytes));
        }
        if let Some(entries) = yaml11_set_entries_node(self.node)? {
            return visitor.visit_seq(InputSetKeySeqDeserializer {
                entries,
                input: self.input,
                index: 0,
            });
        }
        if let Some(items) = yaml11_pair_items_node(self.node, "omap")? {
            return visitor.visit_seq(InputYaml11PairSeqDeserializer {
                items,
                input: self.input,
                index: 0,
                suffix: "omap",
            });
        }
        if let Some(items) = yaml11_pair_items_node(self.node, "pairs")? {
            return visitor.visit_seq(InputYaml11PairSeqDeserializer {
                items,
                input: self.input,
                index: 0,
                suffix: "pairs",
            });
        }
        let node = self.untag();
        if is_empty_null_node(node.node) {
            return visitor.visit_seq(InputSeqDeserializer {
                items: &[],
                input: node.input,
                index: 0,
            });
        }
        match &node.node.value {
            NodeValue::Sequence(items) => visitor.visit_seq(InputSeqDeserializer {
                items,
                input: node.input,
                index: 0,
            }),
            _ => Err(type_error("sequence", node.node)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(items) = yaml11_pair_items_node(self.node, "omap")? {
            return visitor.visit_map(InputYaml11OmapDeserializer {
                items,
                input: self.input,
                index: 0,
                value: None,
            });
        }
        let node = self.untag();
        if is_empty_null_node(node.node) {
            return visitor.visit_map(InputMapDeserializer {
                entries: &[],
                input: node.input,
                index: 0,
                value: None,
            });
        }
        match &node.node.value {
            NodeValue::Mapping(entries) => visitor.visit_map(InputMapDeserializer {
                entries,
                input: node.input,
                index: 0,
                value: None,
            }),
            _ => Err(type_error("mapping", node.node)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match &self.node.value {
            NodeValue::String(variant) => {
                if let Some(variant) = self.borrowed_str() {
                    visitor.visit_enum(variant.into_deserializer())
                } else {
                    visitor.visit_enum(variant.clone().into_deserializer())
                }
            }
            NodeValue::Mapping(entries) if entries.len() == 1 => {
                visitor.visit_enum(InputEnumDeserializer {
                    key: InputNode {
                        node: &entries[0].0,
                        input: self.input,
                    },
                    value: Some(InputNode {
                        node: &entries[0].1,
                        input: self.input,
                    }),
                })
            }
            NodeValue::Tagged(tagged) => visitor.visit_enum(InputTaggedEnumDeserializer {
                tag: &tagged.tag,
                value: InputNode {
                    node: &tagged.value,
                    input: self.input,
                },
            }),
            _ => Err(type_error("enum string or single-key mapping", self.node)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

impl<'de> de::Deserializer<'de> for &'de Node {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match &self.value {
            NodeValue::Null => visitor.visit_unit(),
            NodeValue::Bool(value) => visitor.visit_bool(*value),
            NodeValue::Number(number) => visit_any_number(*number, Some(self.span), visitor),
            NodeValue::String(value) => visitor.visit_borrowed_str(value),
            NodeValue::Sequence(items) => visitor.visit_seq(SeqDeserializer { items, index: 0 }),
            NodeValue::Mapping(entries) => visitor.visit_map(MapDeserializer {
                entries,
                index: 0,
                value: None,
            }),
            NodeValue::Tagged(tagged) => visitor.visit_enum(TaggedEnumDeserializer {
                tag: &tagged.tag,
                value: &tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(value) = explicit_core_bool_node(self)? {
            return with_optional_span(visitor.visit_bool(value.value), value.span);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Bool(value) => visitor.visit_bool(*value),
            _ => Err(type_error("bool", node)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                let value = i64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for i64", Some(node.span))
                })?;
                with_span(visitor.visit_i64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i64::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i64(value), node.span),
                Err(_) => Err(type_error("integer", node)),
            },
            _ => Err(type_error("integer", node)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(*value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            _ => Err(type_error("unsigned integer", node)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_i128(*value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i128::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i128(value), node.span),
                Err(_) => Err(Error::new(
                    "integer scalar is out of range for i128",
                    Some(node.span),
                )),
            },
            NodeValue::String(_) if integer_source_for_scalar(node).is_some() => {
                let value = parse_i128_source(
                    integer_source_for_scalar(node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_i128(value), node.span)
            }
            _ => Err(type_error("integer", node)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u128::try_from(*value).expect("non-negative i128 fits u128");
                with_span(visitor.visit_u128(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_u128(*value), node.span)
            }
            NodeValue::String(_) if integer_source_for_scalar(node).is_some() => {
                let value = parse_u128_source(
                    integer_source_for_scalar(node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_u128(value), node.span)
            }
            _ => Err(type_error("unsigned integer", node)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_node(self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_node(self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_f64(*value as f64), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_f64(*value as f64), node.span)
            }
            NodeValue::Number(Number::Float(value)) => {
                with_span(visitor.visit_f64(*value), node.span)
            }
            _ => Err(type_error("number", node)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(type_error("char", node)),
                }
            }
            _ => Err(type_error("char", node)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::String(value) => visitor.visit_borrowed_str(value),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_)
                if string_source_for_scalar(node).is_some() =>
            {
                visitor.visit_borrowed_str(
                    string_source_for_scalar(node).expect("scalar source checked"),
                )
            }
            _ => Err(type_error("string", node)),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node(self);
        match &node.value {
            NodeValue::String(value) => visitor.visit_string(value.clone()),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_)
                if string_source_for_scalar(node).is_some() =>
            {
                visitor.visit_string(
                    string_source_for_scalar(node)
                        .expect("scalar source checked")
                        .to_string(),
                )
            }
            _ => Err(type_error("string", node)),
        }
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_node(self)? {
            return _visitor.visit_byte_buf(bytes);
        }
        Err(type_error("bytes", untag_node(self)))
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_node(self)? {
            return _visitor.visit_byte_buf(bytes);
        }
        Err(type_error("bytes", untag_node(self)))
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if explicit_core_null_node(self)? {
            return visitor.visit_none();
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(node),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if explicit_core_null_node(self)? {
            return visitor.visit_unit();
        }
        let node = untag_node(self);
        match &node.value {
            NodeValue::Null => visitor.visit_unit(),
            _ => Err(type_error("unit/null", node)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(untag_node(self))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_node(self)? {
            return visitor.visit_seq(ByteBufSeqDeserializer::new(bytes));
        }
        if let Some(entries) = yaml11_set_entries_node(self)? {
            return visitor.visit_seq(SetKeySeqDeserializer { entries, index: 0 });
        }
        if let Some(items) = yaml11_pair_items_node(self, "omap")? {
            return visitor.visit_seq(Yaml11PairSeqDeserializer {
                items,
                index: 0,
                suffix: "omap",
            });
        }
        if let Some(items) = yaml11_pair_items_node(self, "pairs")? {
            return visitor.visit_seq(Yaml11PairSeqDeserializer {
                items,
                index: 0,
                suffix: "pairs",
            });
        }
        let node = untag_node(self);
        if is_empty_null_node(node) {
            return visitor.visit_seq(SeqDeserializer {
                items: &[],
                index: 0,
            });
        }
        match &node.value {
            NodeValue::Sequence(items) => visitor.visit_seq(SeqDeserializer { items, index: 0 }),
            _ => Err(type_error("sequence", node)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(items) = yaml11_pair_items_node(self, "omap")? {
            return visitor.visit_map(Yaml11OmapDeserializer {
                items,
                index: 0,
                value: None,
            });
        }
        let node = untag_node(self);
        if is_empty_null_node(node) {
            return visitor.visit_map(MapDeserializer {
                entries: &[],
                index: 0,
                value: None,
            });
        }
        match &node.value {
            NodeValue::Mapping(entries) => visitor.visit_map(MapDeserializer {
                entries,
                index: 0,
                value: None,
            }),
            _ => Err(type_error("mapping", node)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match &self.value {
            NodeValue::String(variant) => visitor.visit_enum(variant.as_str().into_deserializer()),
            NodeValue::Mapping(entries) if entries.len() == 1 => {
                visitor.visit_enum(EnumDeserializer {
                    key: &entries[0].0,
                    value: Some(&entries[0].1),
                })
            }
            NodeValue::Tagged(tagged) => visitor.visit_enum(TaggedEnumDeserializer {
                tag: &tagged.tag,
                value: &tagged.value,
            }),
            _ => Err(type_error("enum string or single-key mapping", self)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

impl<'de> de::Deserializer<'de> for Node {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let span = self.span;
        match self.value {
            NodeValue::Null => visitor.visit_unit(),
            NodeValue::Bool(value) => visitor.visit_bool(value),
            NodeValue::Number(number) => visit_any_number(number, Some(span), visitor),
            NodeValue::String(value) => visitor.visit_string(value),
            NodeValue::Sequence(items) => visitor.visit_seq(OwnedSeqDeserializer {
                items: items.into_iter(),
                index: 0,
            }),
            NodeValue::Mapping(entries) => visitor.visit_map(OwnedMapDeserializer {
                entries: entries.into_iter(),
                value: None,
            }),
            NodeValue::Tagged(tagged) => visitor.visit_enum(OwnedTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(value) = explicit_core_bool_node(&self)? {
            return with_optional_span(visitor.visit_bool(value.value), value.span);
        }
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Bool(value) => visitor.visit_bool(value),
            other => Err(type_error_owned("bool", &other, node.span)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Number(Number::Integer(value)) => {
                let value = i64::try_from(value).map_err(|_| {
                    Error::new("integer scalar is out of range for i64", Some(node.span))
                })?;
                with_span(visitor.visit_i64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i64::try_from(value) {
                Ok(value) => with_span(visitor.visit_i64(value), node.span),
                Err(_) => Err(Error::new(
                    format!("expected integer, found unsigned integer: {value}"),
                    Some(node.span),
                )),
            },
            other => Err(type_error_owned("integer", &other, node.span)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Number(Number::Integer(value)) if value >= 0 => {
                let value = u64::try_from(value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(value).map_err(|_| {
                    Error::new("integer scalar is out of range for u64", Some(node.span))
                })?;
                with_span(visitor.visit_u64(value), node.span)
            }
            other => Err(type_error_owned("unsigned integer", &other, node.span)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_i128(*value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => match i128::try_from(*value) {
                Ok(value) => with_span(visitor.visit_i128(value), node.span),
                Err(_) => Err(Error::new(
                    "integer scalar is out of range for i128",
                    Some(node.span),
                )),
            },
            NodeValue::String(_) if integer_source_for_scalar(&node).is_some() => {
                let value = parse_i128_source(
                    integer_source_for_scalar(&node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_i128(value), node.span)
            }
            other => Err(type_error_owned("integer", other, node.span)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match &node.value {
            NodeValue::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u128::try_from(*value).expect("non-negative i128 fits u128");
                with_span(visitor.visit_u128(value), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_u128(*value), node.span)
            }
            NodeValue::String(_) if integer_source_for_scalar(&node).is_some() => {
                let value = parse_u128_source(
                    integer_source_for_scalar(&node).expect("integer source checked"),
                    node.span,
                )?;
                with_span(visitor.visit_u128(value), node.span)
            }
            other => Err(type_error_owned("unsigned integer", other, node.span)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_node(&self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_node(&self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Number(Number::Integer(value)) => {
                with_span(visitor.visit_f64(value as f64), node.span)
            }
            NodeValue::Number(Number::Unsigned(value)) => {
                with_span(visitor.visit_f64(value as f64), node.span)
            }
            NodeValue::Number(Number::Float(value)) => {
                with_span(visitor.visit_f64(value), node.span)
            }
            other => Err(type_error_owned("number", &other, node.span)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(Error::new("expected char, found string", Some(node.span))),
                }
            }
            other => Err(type_error_owned("char", &other, node.span)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::String(value) => visitor.visit_string(value),
            NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_)
                if node.source.is_some() =>
            {
                visitor.visit_string(
                    node.source
                        .expect("scalar source checked")
                        .raw()
                        .to_string(),
                )
            }
            other => Err(type_error_owned("string", &other, node.span)),
        }
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_node(&self)? {
            return _visitor.visit_byte_buf(bytes);
        }
        let node = untag_node_owned(self);
        Err(type_error_owned("bytes", &node.value, node.span))
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if explicit_core_null_node(&self)? {
            return visitor.visit_none();
        }
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Null => visitor.visit_none(),
            value => visitor.visit_some(Node {
                value,
                span: node.span,
                source: node.source,
            }),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if explicit_core_null_node(&self)? {
            return visitor.visit_unit();
        }
        let node = untag_node_owned(self);
        match node.value {
            NodeValue::Null => visitor.visit_unit(),
            other => Err(type_error_owned("unit/null", &other, node.span)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(untag_node_owned(self))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_node(&self)? {
            return visitor.visit_seq(ByteBufSeqDeserializer::new(bytes));
        }
        if yaml11_set_entries_node(&self)?.is_some() {
            let entries = take_yaml11_set_entries_node(self).expect("checked explicit !!set");
            return visitor.visit_seq(OwnedSetKeySeqDeserializer {
                entries: entries.into_iter(),
            });
        }
        if yaml11_pair_items_node(&self, "omap")?.is_some() {
            let items = take_yaml11_pair_items_node(self, "omap").expect("checked explicit !!omap");
            return visitor.visit_seq(OwnedYaml11PairSeqDeserializer {
                items: items.into_iter(),
                suffix: "omap",
            });
        }
        if yaml11_pair_items_node(&self, "pairs")?.is_some() {
            let items =
                take_yaml11_pair_items_node(self, "pairs").expect("checked explicit !!pairs");
            return visitor.visit_seq(OwnedYaml11PairSeqDeserializer {
                items: items.into_iter(),
                suffix: "pairs",
            });
        }
        let node = untag_node_owned(self);
        if is_empty_null_node(&node) {
            return visitor.visit_seq(OwnedSeqDeserializer {
                items: Vec::<Node>::new().into_iter(),
                index: 0,
            });
        }
        match node.value {
            NodeValue::Sequence(items) => visitor.visit_seq(OwnedSeqDeserializer {
                items: items.into_iter(),
                index: 0,
            }),
            other => Err(type_error_owned("sequence", &other, node.span)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if yaml11_pair_items_node(&self, "omap")?.is_some() {
            let items = take_yaml11_pair_items_node(self, "omap").expect("checked explicit !!omap");
            return visitor.visit_map(OwnedYaml11OmapDeserializer {
                items: items.into_iter(),
                value: None,
            });
        }
        let node = untag_node_owned(self);
        if is_empty_null_node(&node) {
            return visitor.visit_map(OwnedMapDeserializer {
                entries: Vec::<(Node, Node)>::new().into_iter(),
                value: None,
            });
        }
        match node.value {
            NodeValue::Mapping(entries) => visitor.visit_map(OwnedMapDeserializer {
                entries: entries.into_iter(),
                value: None,
            }),
            other => Err(type_error_owned("mapping", &other, node.span)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            NodeValue::String(variant) => visitor.visit_enum(variant.into_deserializer()),
            NodeValue::Mapping(entries) if entries.len() == 1 => {
                let mut entries = entries.into_iter();
                let (key, value) = entries.next().expect("length checked");
                visitor.visit_enum(OwnedEnumDeserializer {
                    key,
                    value: Some(value),
                })
            }
            NodeValue::Tagged(tagged) => visitor.visit_enum(OwnedTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
            other => Err(type_error_owned(
                "enum string or single-key mapping",
                &other,
                self.span,
            )),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        untag_node_owned(self).deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

impl<'de> de::Deserializer<'de> for Value {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match apply_default_value_merges(self)? {
            Value::Null => visitor.visit_unit(),
            Value::Bool(value) => visitor.visit_bool(value),
            Value::Number(number) => visit_any_number(number, None, visitor),
            Value::String(value) => visitor.visit_string(value),
            Value::Sequence(items) => visitor.visit_seq(ValueSeqDeserializer {
                items: items.into_iter(),
                index: 0,
            }),
            Value::Mapping(entries) => visitor.visit_map(ValueMapDeserializer {
                entries: entries.into_iter(),
                value: None,
            }),
            Value::Tagged(tagged) => visitor.visit_enum(ValueTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(value) = explicit_core_bool_value(&self)? {
            return with_optional_span(visitor.visit_bool(value.value), value.span);
        }
        match untag_value_owned(self) {
            Value::Bool(value) => visitor.visit_bool(value),
            other => Err(type_error_value("bool", &other)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) => {
                let value = i64::try_from(value)
                    .map_err(|_| Error::new("integer scalar is out of range for i64", None))?;
                visitor.visit_i64(value)
            }
            Value::Number(Number::Unsigned(value)) => match i64::try_from(value) {
                Ok(value) => visitor.visit_i64(value),
                Err(_) => Err(Error::new(
                    format!("expected integer, found unsigned integer: {value}"),
                    None,
                )),
            },
            other => Err(type_error_value("integer", &other)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) if value >= 0 => {
                let value = u64::try_from(value)
                    .map_err(|_| Error::new("integer scalar is out of range for u64", None))?;
                visitor.visit_u64(value)
            }
            Value::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(value)
                    .map_err(|_| Error::new("integer scalar is out of range for u64", None))?;
                visitor.visit_u64(value)
            }
            other => Err(type_error_value("unsigned integer", &other)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) => visitor.visit_i128(value),
            Value::Number(Number::Unsigned(value)) => match i128::try_from(value) {
                Ok(value) => visitor.visit_i128(value),
                Err(_) => Err(Error::new("integer scalar is out of range for i128", None)),
            },
            other => Err(type_error_value("integer", &other)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) if value >= 0 => {
                let value = u128::try_from(value).expect("non-negative i128 fits u128");
                visitor.visit_u128(value)
            }
            Value::Number(Number::Unsigned(value)) => visitor.visit_u128(value),
            other => Err(type_error_value("unsigned integer", &other)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_value(&self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_value(&self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        match untag_value_owned(self) {
            Value::Number(Number::Integer(value)) => visitor.visit_f64(value as f64),
            Value::Number(Number::Unsigned(value)) => visitor.visit_f64(value as f64),
            Value::Number(Number::Float(value)) => visitor.visit_f64(value),
            other => Err(type_error_value("number", &other)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(Error::new("expected char, found string", None)),
                }
            }
            other => Err(type_error_value("char", &other)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match untag_value_owned(self) {
            Value::String(value) => visitor.visit_string(value),
            other => Err(type_error_value("string", &other)),
        }
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_value(&self)? {
            return _visitor.visit_byte_buf(bytes);
        }
        let value = untag_value_owned(self);
        Err(type_error_value("bytes", &value))
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = apply_default_value_merges(self)?;
        if explicit_core_null_value(&value)? {
            return visitor.visit_none();
        }
        match untag_value_owned(value) {
            Value::Null => visitor.visit_none(),
            other => visitor.visit_some(other),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if explicit_core_null_value(&self)? {
            return visitor.visit_unit();
        }
        match untag_value_owned(self) {
            Value::Null => visitor.visit_unit(),
            other => Err(type_error_value("unit/null", &other)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(untag_value_owned(apply_default_value_merges(self)?))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = apply_default_value_merges(self)?;
        if let Some(bytes) = explicit_core_binary_bytes_value(&value)? {
            return visitor.visit_seq(ByteBufSeqDeserializer::new(bytes));
        }
        if yaml11_set_entries_value(&value)?.is_some() {
            let entries = take_yaml11_set_entries_value(value).expect("checked explicit !!set");
            return visitor.visit_seq(ValueSetKeySeqDeserializer {
                entries: entries.into_iter(),
            });
        }
        if yaml11_pair_items_value(&value, "omap")?.is_some() {
            let items =
                take_yaml11_pair_items_value(value, "omap").expect("checked explicit !!omap");
            return visitor.visit_seq(ValueYaml11PairSeqDeserializer {
                items: items.into_iter(),
                suffix: "omap",
            });
        }
        if yaml11_pair_items_value(&value, "pairs")?.is_some() {
            let items =
                take_yaml11_pair_items_value(value, "pairs").expect("checked explicit !!pairs");
            return visitor.visit_seq(ValueYaml11PairSeqDeserializer {
                items: items.into_iter(),
                suffix: "pairs",
            });
        }
        match untag_value_owned(value) {
            Value::Null => visitor.visit_seq(ValueSeqDeserializer {
                items: Vec::new().into_iter(),
                index: 0,
            }),
            Value::Sequence(items) => visitor.visit_seq(ValueSeqDeserializer {
                items: items.into_iter(),
                index: 0,
            }),
            other => Err(type_error_value("sequence", &other)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = apply_default_value_merges(self)?;
        if yaml11_pair_items_value(&value, "omap")?.is_some() {
            let items =
                take_yaml11_pair_items_value(value, "omap").expect("checked explicit !!omap");
            return visitor.visit_map(ValueYaml11OmapDeserializer {
                items: items.into_iter(),
                value: None,
            });
        }
        match untag_value_owned(value) {
            Value::Null => visitor.visit_map(ValueMapDeserializer {
                entries: Mapping::new().into_iter(),
                value: None,
            }),
            Value::Mapping(entries) => visitor.visit_map(ValueMapDeserializer {
                entries: entries.into_iter(),
                value: None,
            }),
            other => Err(type_error_value("mapping", &other)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match apply_default_value_merges(self)? {
            Value::String(variant) => visitor.visit_enum(variant.into_deserializer()),
            Value::Mapping(entries) if entries.len() == 1 => {
                let mut entries = entries.into_iter();
                let (key, value) = entries.next().expect("length checked");
                visitor.visit_enum(ValueEnumDeserializer {
                    key,
                    value: Some(value),
                })
            }
            Value::Tagged(tagged) => visitor.visit_enum(ValueTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
            other => Err(type_error_value(
                "enum string or single-key mapping",
                &other,
            )),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

struct ValueSeqDeserializer {
    items: std::vec::IntoIter<Value>,
    index: usize,
}

impl<'de> SeqAccess<'de> for ValueSeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.next() else {
            return Ok(None);
        };
        let index = self.index;
        self.index += 1;
        with_index(seed.deserialize(item), index).map(Some)
    }
}

struct ValueMapDeserializer {
    entries: crate::ast::IntoIter,
    value: Option<(Value, ErrorPathSegment)>,
}

impl<'de> MapAccess<'de> for ValueMapDeserializer {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        let segment = value_path_segment(&key);
        self.value = Some((value, segment.clone()));
        with_key_path(seed.deserialize(key), segment).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let (value, segment) = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        with_value_path(seed.deserialize(value), segment)
    }
}

struct ValueEnumDeserializer {
    key: Value,
    value: Option<Value>,
}

impl<'de> EnumAccess<'de> for ValueEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key.clone())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for ValueEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(Value::Null) => Ok(()),
            Some(value) => Err(type_error_value("unit enum variant", &value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

struct OwnedSeqDeserializer {
    items: std::vec::IntoIter<Node>,
    index: usize,
}

impl<'de> SeqAccess<'de> for OwnedSeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.next() else {
            return Ok(None);
        };
        let index = self.index;
        self.index += 1;
        let span = item.span;
        with_index(seed.deserialize(item), index)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(span))
    }
}

struct OwnedMapDeserializer {
    entries: std::vec::IntoIter<(Node, Node)>,
    value: Option<(Node, ErrorPathSegment)>,
}

impl<'de> MapAccess<'de> for OwnedMapDeserializer {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        let key_span = key.span;
        let segment = node_path_segment(&key);
        self.value = Some((value, segment.clone()));
        with_key_path(seed.deserialize(key), segment)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(key_span))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let (value, segment) = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        let span = value.span;
        with_value_path(seed.deserialize(value), segment)
            .map_err(|error| error.with_span_if_missing(span))
    }
}

struct OwnedEnumDeserializer {
    key: Node,
    value: Option<Node>,
}

impl<'de> EnumAccess<'de> for OwnedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key.clone())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for OwnedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(node) if matches!(node.value, NodeValue::Null) => Ok(()),
            Some(node) => Err(type_error_owned(
                "unit enum variant",
                &node.value,
                node.span,
            )),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

impl<'de, 'a, 'src> de::Deserializer<'de> for &'a Document<'src>
where
    'a: 'de,
{
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_any(node, visitor)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_bool(node, visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_i64(node, visitor)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_u64(node, visitor)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_f64(node, visitor)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_char(node, visitor)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_str(node, visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_string(node, visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_bytes(node, visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_byte_buf(node, visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_option(node, visitor)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_unit(node, visitor)
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_unit_struct(node, name, visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_newtype_struct(node, name, visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_seq(node, visitor)
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_tuple(node, len, visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_tuple_struct(node, name, len, visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_map(node, visitor)
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_struct(node, name, fields, visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_enum(node, name, variants, visitor)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let node: &'de Node = self.as_node()?;
        de::Deserializer::deserialize_identifier(node, visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

macro_rules! document_forward {
    ($method:ident ( $($arg:ident : $arg_ty:ty),* ; $visitor:ident )) => {
        fn $method<V>(self, $($arg: $arg_ty,)* $visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let (node, input, document_index) = self.into_node_and_input()?;
            match input {
                Some(input) => {
                    let span = node.span;
                    de::Deserializer::$method(
                        InputNode {
                            node: &node,
                            input,
                        },
                        $($arg,)*
                        $visitor,
                    )
                    .map_err(|error| error.with_span_if_missing(span).with_document_index(document_index))
                }
                None => de::Deserializer::$method(node, $($arg,)* $visitor)
                    .map_err(|error| error.with_document_index(document_index)),
            }
        }
    };
}

impl<'de> de::Deserializer<'de> for Document<'de> {
    type Error = Error;

    document_forward!(deserialize_any(; visitor));
    document_forward!(deserialize_bool(; visitor));
    document_forward!(deserialize_i64(; visitor));

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    document_forward!(deserialize_u64(; visitor));

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    document_forward!(deserialize_f64(; visitor));

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    document_forward!(deserialize_char(; visitor));
    document_forward!(deserialize_str(; visitor));
    document_forward!(deserialize_string(; visitor));
    document_forward!(deserialize_bytes(; visitor));
    document_forward!(deserialize_byte_buf(; visitor));
    document_forward!(deserialize_option(; visitor));
    document_forward!(deserialize_unit(; visitor));
    document_forward!(deserialize_unit_struct(name: &'static str; visitor));
    document_forward!(deserialize_newtype_struct(name: &'static str; visitor));
    document_forward!(deserialize_seq(; visitor));
    document_forward!(deserialize_tuple(len: usize; visitor));
    document_forward!(deserialize_tuple_struct(name: &'static str, len: usize; visitor));
    document_forward!(deserialize_map(; visitor));
    document_forward!(deserialize_struct(
        name: &'static str,
        fields: &'static [&'static str];
        visitor
    ));
    document_forward!(deserialize_enum(
        name: &'static str,
        variants: &'static [&'static str];
        visitor
    ));
    document_forward!(deserialize_identifier(; visitor));

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = self.into_node_and_input()?;
        visitor.visit_unit()
    }
}

impl<'de> de::Deserializer<'de> for Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_any(self.into_single_document()?, visitor)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_bool(self.into_single_document()?, visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_i64(self.into_single_document()?, visitor)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_u64(self.into_single_document()?, visitor)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_f64(self.into_single_document()?, visitor)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_char(self.into_single_document()?, visitor)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_str(self.into_single_document()?, visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_string(self.into_single_document()?, visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_bytes(self.into_single_document()?, visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_byte_buf(self.into_single_document()?, visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_option(self.into_single_document()?, visitor)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_unit(self.into_single_document()?, visitor)
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_unit_struct(self.into_single_document()?, name, visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_newtype_struct(self.into_single_document()?, name, visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.into_single_document()?, visitor)
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_tuple(self.into_single_document()?, len, visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_tuple_struct(self.into_single_document()?, name, len, visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.into_single_document()?, visitor)
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_struct(self.into_single_document()?, name, fields, visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_enum(self.into_single_document()?, name, variants, visitor)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_identifier(self.into_single_document()?, visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_ignored_any(self.into_single_document()?, visitor)
    }
}

impl<'de> IntoDeserializer<'de, Error> for Value {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> de::Deserializer<'de> for TaggedValue {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(ValueTaggedEnumDeserializer {
            tag: self.tag,
            value: self.value,
        })
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        drop(self);
        visitor.visit_unit()
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier
    }
}

impl<'de> IntoDeserializer<'de, Error> for TaggedValue {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> de::Deserializer<'de> for &'de TaggedValue {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(ValueRefTaggedEnumDeserializer {
            tag: &self.tag,
            value: &self.value,
        })
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier
    }
}

impl<'de> IntoDeserializer<'de, Error> for &'de TaggedValue {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> de::Deserializer<'de> for Number {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        deserialize_number(self, visitor)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

impl<'de> de::Deserializer<'de> for &Number {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        deserialize_number(*self, visitor)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

fn deserialize_number<'de, V>(number: Number, visitor: V) -> Result<V::Value, Error>
where
    V: Visitor<'de>,
{
    visit_any_number(number, None, visitor)
}

impl<'de> de::Deserializer<'de> for &'de Value {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Value::Null => visitor.visit_unit(),
            Value::Bool(value) => visitor.visit_bool(*value),
            Value::Number(number) => visit_any_number(*number, None, visitor),
            Value::String(value) => visitor.visit_borrowed_str(value),
            Value::Sequence(items) => {
                visitor.visit_seq(ValueRefSeqDeserializer { items, index: 0 })
            }
            Value::Mapping(mapping) => match merged_value_ref_entries(mapping)? {
                Some(entries) => visitor.visit_map(ValueRefMergedMapDeserializer {
                    entries,
                    index: 0,
                    value: None,
                }),
                None => visitor.visit_map(ValueRefMapDeserializer {
                    entries: mapping.as_slice(),
                    index: 0,
                    value: None,
                }),
            },
            Value::Tagged(tagged) => visitor.visit_enum(ValueRefTaggedEnumDeserializer {
                tag: &tagged.tag,
                value: &tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(value) = explicit_core_bool_value(self)? {
            return with_optional_span(visitor.visit_bool(value.value), value.span);
        }
        let value = untag_value(self);
        match value {
            Value::Bool(value) => visitor.visit_bool(*value),
            other => Err(type_error_value("bool", other)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_i64_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) => {
                let value = i64::try_from(*value)
                    .map_err(|_| Error::new("integer scalar is out of range for i64", None))?;
                visitor.visit_i64(value)
            }
            Value::Number(Number::Unsigned(value)) => match i64::try_from(*value) {
                Ok(value) => visitor.visit_i64(value),
                Err(_) => Err(Error::new(
                    format!("expected integer, found unsigned integer: {value}"),
                    None,
                )),
            },
            other => Err(type_error_value("integer", other)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_u64_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u64::try_from(*value)
                    .map_err(|_| Error::new("integer scalar is out of range for u64", None))?;
                visitor.visit_u64(value)
            }
            Value::Number(Number::Unsigned(value)) => {
                let value = u64::try_from(*value)
                    .map_err(|_| Error::new("integer scalar is out of range for u64", None))?;
                visitor.visit_u64(value)
            }
            other => Err(type_error_value("unsigned integer", other)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_i128_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) => visitor.visit_i128(*value),
            Value::Number(Number::Unsigned(value)) => match i128::try_from(*value) {
                Ok(value) => visitor.visit_i128(value),
                Err(_) => Err(Error::new("integer scalar is out of range for i128", None)),
            },
            other => Err(type_error_value("integer", other)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_u128_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) if *value >= 0 => {
                let value = u128::try_from(*value).expect("non-negative i128 fits u128");
                visitor.visit_u128(value)
            }
            Value::Number(Number::Unsigned(value)) => visitor.visit_u128(*value),
            other => Err(type_error_value("unsigned integer", other)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(number) = explicit_core_float_number_value(self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        if let Some(number) = explicit_core_int_number_value(self)? {
            return visit_f64_number(number.number, number.span, visitor);
        }
        let value = untag_value(self);
        match value {
            Value::Number(Number::Integer(value)) => visitor.visit_f64(*value as f64),
            Value::Number(Number::Unsigned(value)) => visitor.visit_f64(*value as f64),
            Value::Number(Number::Float(value)) => visitor.visit_f64(*value),
            other => Err(type_error_value("number", other)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(Error::new("expected char, found string", None)),
                }
            }
            other => Err(type_error_value("char", other)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::String(value) => visitor.visit_borrowed_str(value),
            other => Err(type_error_value("string", other)),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = untag_value(self);
        match value {
            Value::String(value) => visitor.visit_string(value.clone()),
            other => Err(type_error_value("string", other)),
        }
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_value(self)? {
            return _visitor.visit_byte_buf(bytes);
        }
        let value = untag_value(self);
        Err(type_error_value("bytes", value))
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_value(self)? {
            return _visitor.visit_byte_buf(bytes);
        }
        let value = untag_value(self);
        Err(type_error_value("bytes", value))
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if explicit_core_null_value(self)? {
            return visitor.visit_none();
        }
        let value = untag_value(self);
        match value {
            Value::Null => visitor.visit_none(),
            other => visitor.visit_some(other),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if explicit_core_null_value(self)? {
            return visitor.visit_unit();
        }
        let value = untag_value(self);
        match value {
            Value::Null => visitor.visit_unit(),
            other => Err(type_error_value("unit/null", other)),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(untag_value(self))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(bytes) = explicit_core_binary_bytes_value(self)? {
            return visitor.visit_seq(ByteBufSeqDeserializer::new(bytes));
        }
        if let Some(entries) = yaml11_set_entries_value(self)? {
            return visitor.visit_seq(ValueRefSetKeySeqDeserializer { entries, index: 0 });
        }
        if let Some(items) = yaml11_pair_items_value(self, "omap")? {
            return visitor.visit_seq(ValueRefYaml11PairSeqDeserializer {
                items,
                index: 0,
                suffix: "omap",
            });
        }
        if let Some(items) = yaml11_pair_items_value(self, "pairs")? {
            return visitor.visit_seq(ValueRefYaml11PairSeqDeserializer {
                items,
                index: 0,
                suffix: "pairs",
            });
        }
        let value = untag_value(self);
        match value {
            Value::Null => visitor.visit_seq(ValueRefSeqDeserializer {
                items: &[],
                index: 0,
            }),
            Value::Sequence(items) => {
                visitor.visit_seq(ValueRefSeqDeserializer { items, index: 0 })
            }
            other => Err(type_error_value("sequence", other)),
        }
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(items) = yaml11_pair_items_value(self, "omap")? {
            return visitor.visit_map(ValueRefYaml11OmapDeserializer {
                items,
                index: 0,
                value: None,
            });
        }
        let value = untag_value(self);
        match value {
            Value::Null => visitor.visit_map(ValueRefMapDeserializer {
                entries: &[],
                index: 0,
                value: None,
            }),
            Value::Mapping(mapping) => match merged_value_ref_entries(mapping)? {
                Some(entries) => visitor.visit_map(ValueRefMergedMapDeserializer {
                    entries,
                    index: 0,
                    value: None,
                }),
                None => visitor.visit_map(ValueRefMapDeserializer {
                    entries: mapping.as_slice(),
                    index: 0,
                    value: None,
                }),
            },
            other => Err(type_error_value("mapping", other)),
        }
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = (name, fields);
        self.deserialize_map(visitor)
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = (name, variants);
        match self {
            Value::String(variant) => visitor.visit_enum(variant.as_str().into_deserializer()),
            Value::Mapping(mapping) => match merged_value_ref_entries(mapping)? {
                Some(entries) if entries.len() == 1 => {
                    let (key, value) = entries[0];
                    visitor.visit_enum(ValueRefEnumDeserializer {
                        key,
                        value: Some(value),
                    })
                }
                Some(_) => Err(type_error_value("enum string or single-key mapping", self)),
                None if mapping.len() == 1 => {
                    let entries = mapping.as_slice();
                    visitor.visit_enum(ValueRefEnumDeserializer {
                        key: &entries[0].0,
                        value: Some(&entries[0].1),
                    })
                }
                None => Err(type_error_value("enum string or single-key mapping", self)),
            },
            Value::Tagged(tagged) => visitor.visit_enum(ValueRefTaggedEnumDeserializer {
                tag: &tagged.tag,
                value: &tagged.value,
            }),
            other => Err(type_error_value("enum string or single-key mapping", other)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

struct ValueRefSeqDeserializer<'a> {
    items: &'a [Value],
    index: usize,
}

impl<'de> SeqAccess<'de> for ValueRefSeqDeserializer<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        let index = self.index;
        self.index += 1;
        with_index(seed.deserialize(item), index).map(Some)
    }
}

struct ValueRefMapDeserializer<'a> {
    entries: &'a [(Value, Value)],
    index: usize,
    value: Option<(&'a Value, ErrorPathSegment)>,
}

impl<'de> MapAccess<'de> for ValueRefMapDeserializer<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let segment = value_path_segment(key);
        self.value = Some((value, segment.clone()));
        with_key_path(seed.deserialize(key), segment).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let (value, segment) = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        with_value_path(seed.deserialize(value), segment)
    }
}

struct ValueRefMergedMapDeserializer<'a> {
    entries: Vec<(&'a Value, &'a Value)>,
    index: usize,
    value: Option<(&'a Value, ErrorPathSegment)>,
}

impl<'de> MapAccess<'de> for ValueRefMergedMapDeserializer<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let segment = value_path_segment(key);
        self.value = Some((value, segment.clone()));
        with_key_path(seed.deserialize(*key), segment).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let (value, segment) = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        with_value_path(seed.deserialize(value), segment)
    }
}

struct ValueRefEnumDeserializer<'a> {
    key: &'a Value,
    value: Option<&'a Value>,
}

struct ValueTaggedEnumDeserializer {
    tag: Tag,
    value: Value,
}

impl<'de> EnumAccess<'de> for ValueTaggedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for ValueTaggedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            Value::Null => Ok(()),
            value => Err(type_error_value("unit enum variant", &value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

struct ValueRefTaggedEnumDeserializer<'a> {
    tag: &'a Tag,
    value: &'a Value,
}

impl<'de> EnumAccess<'de> for ValueRefTaggedEnumDeserializer<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for ValueRefTaggedEnumDeserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            Value::Null => Ok(()),
            value => Err(type_error_value("unit enum variant", value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

impl<'de> EnumAccess<'de> for ValueRefEnumDeserializer<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key)?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for ValueRefEnumDeserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(Value::Null) => Ok(()),
            Some(value) => Err(type_error_value("unit enum variant", value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

struct SeqDeserializer<'a> {
    items: &'a [Node],
    index: usize,
}

struct InputSeqDeserializer<'tree, 'de> {
    items: &'tree [Node],
    input: &'de str,
    index: usize,
}

struct ByteBufSeqDeserializer {
    bytes: std::vec::IntoIter<u8>,
}

impl ByteBufSeqDeserializer {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: bytes.into_iter(),
        }
    }
}

struct InputSetKeySeqDeserializer<'tree, 'de> {
    entries: &'tree [(Node, Node)],
    input: &'de str,
    index: usize,
}

struct SetKeySeqDeserializer<'a> {
    entries: &'a [(Node, Node)],
    index: usize,
}

struct OwnedSetKeySeqDeserializer {
    entries: std::vec::IntoIter<(Node, Node)>,
}

struct ValueSetKeySeqDeserializer {
    entries: crate::ast::IntoIter,
}

struct ValueRefSetKeySeqDeserializer<'a> {
    entries: &'a [(Value, Value)],
    index: usize,
}

struct InputYaml11PairSeqDeserializer<'tree, 'de> {
    items: &'tree [Node],
    input: &'de str,
    index: usize,
    suffix: &'static str,
}

struct Yaml11PairSeqDeserializer<'a> {
    items: &'a [Node],
    index: usize,
    suffix: &'static str,
}

struct OwnedYaml11PairSeqDeserializer {
    items: std::vec::IntoIter<Node>,
    suffix: &'static str,
}

struct ValueYaml11PairSeqDeserializer {
    items: std::vec::IntoIter<Value>,
    suffix: &'static str,
}

struct ValueRefYaml11PairSeqDeserializer<'a> {
    items: &'a [Value],
    index: usize,
    suffix: &'static str,
}

struct InputYaml11OmapDeserializer<'tree, 'de> {
    items: &'tree [Node],
    input: &'de str,
    index: usize,
    value: Option<InputNode<'tree, 'de>>,
}

struct Yaml11OmapDeserializer<'a> {
    items: &'a [Node],
    index: usize,
    value: Option<&'a Node>,
}

struct OwnedYaml11OmapDeserializer {
    items: std::vec::IntoIter<Node>,
    value: Option<Node>,
}

struct ValueYaml11OmapDeserializer {
    items: std::vec::IntoIter<Value>,
    value: Option<Value>,
}

struct ValueRefYaml11OmapDeserializer<'a> {
    items: &'a [Value],
    index: usize,
    value: Option<&'a Value>,
}

impl<'de> SeqAccess<'de> for ByteBufSeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(byte) = self.bytes.next() else {
            return Ok(None);
        };
        seed.deserialize(byte.into_deserializer()).map(Some)
    }
}

impl<'de, 'tree> SeqAccess<'de> for InputSetKeySeqDeserializer<'tree, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        ensure_yaml11_set_null_node(value)?;
        seed.deserialize(InputNode {
            node: key,
            input: self.input,
        })
        .map(Some)
        .map_err(|error| error.with_span_if_missing(key.span))
    }
}

impl<'de> SeqAccess<'de> for SetKeySeqDeserializer<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        ensure_yaml11_set_null_node(value)?;
        seed.deserialize(key)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(key.span))
    }
}

impl<'de> SeqAccess<'de> for OwnedSetKeySeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        ensure_yaml11_set_null_node(&value)?;
        let span = key.span;
        seed.deserialize(key)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(span))
    }
}

impl<'de> SeqAccess<'de> for ValueSetKeySeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        ensure_yaml11_set_null_value(&value)?;
        seed.deserialize(key).map(Some)
    }
}

impl<'de> SeqAccess<'de> for ValueRefSetKeySeqDeserializer<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        ensure_yaml11_set_null_value(value)?;
        seed.deserialize(key).map(Some)
    }
}

impl<'de, 'tree> SeqAccess<'de> for InputSeqDeserializer<'tree, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        let index = self.index;
        self.index += 1;
        with_index(
            seed.deserialize(InputNode {
                node: item,
                input: self.input,
            }),
            index,
        )
        .map(Some)
        .map_err(|error| error.with_span_if_missing(item.span))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len().saturating_sub(self.index))
    }
}

impl<'de> SeqAccess<'de> for SeqDeserializer<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        let index = self.index;
        self.index += 1;
        with_index(seed.deserialize(item), index)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(item.span))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len().saturating_sub(self.index))
    }
}

impl<'de, 'tree> SeqAccess<'de> for InputYaml11PairSeqDeserializer<'tree, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let (key, value) = yaml11_singleton_pair_node(item, self.suffix)?;
        seed.deserialize(InputYaml11PairDeserializer {
            key: InputNode {
                node: key,
                input: self.input,
            },
            value: InputNode {
                node: value,
                input: self.input,
            },
        })
        .map(Some)
        .map_err(|error| error.with_span_if_missing(item.span))
    }
}

impl<'de> SeqAccess<'de> for Yaml11PairSeqDeserializer<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let (key, value) = yaml11_singleton_pair_node(item, self.suffix)?;
        seed.deserialize(Yaml11PairDeserializer { key, value })
            .map(Some)
            .map_err(|error| error.with_span_if_missing(item.span))
    }
}

impl<'de> SeqAccess<'de> for OwnedYaml11PairSeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.next() else {
            return Ok(None);
        };
        let span = item.span;
        let (key, value) = take_yaml11_singleton_pair_node(item, self.suffix)?;
        seed.deserialize(OwnedYaml11PairDeserializer {
            key: Some(key),
            value: Some(value),
        })
        .map(Some)
        .map_err(|error| error.with_span_if_missing(span))
    }
}

impl<'de> SeqAccess<'de> for ValueYaml11PairSeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.next() else {
            return Ok(None);
        };
        let (key, value) = take_yaml11_singleton_pair_value(item, self.suffix)?;
        seed.deserialize(ValueYaml11PairDeserializer {
            key: Some(key),
            value: Some(value),
        })
        .map(Some)
    }
}

impl<'de> SeqAccess<'de> for ValueRefYaml11PairSeqDeserializer<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let (key, value) = yaml11_singleton_pair_value(item, self.suffix)?;
        seed.deserialize(ValueRefYaml11PairDeserializer { key, value })
            .map(Some)
    }
}

struct InputYaml11PairDeserializer<'tree, 'de> {
    key: InputNode<'tree, 'de>,
    value: InputNode<'tree, 'de>,
}

struct Yaml11PairDeserializer<'a> {
    key: &'a Node,
    value: &'a Node,
}

struct OwnedYaml11PairDeserializer {
    key: Option<Node>,
    value: Option<Node>,
}

struct ValueYaml11PairDeserializer {
    key: Option<Value>,
    value: Option<Value>,
}

struct ValueRefYaml11PairDeserializer<'a> {
    key: &'a Value,
    value: &'a Value,
}

struct InputYaml11PairAccess<'tree, 'de> {
    key: Option<InputNode<'tree, 'de>>,
    value: Option<InputNode<'tree, 'de>>,
}

struct Yaml11PairAccess<'a> {
    key: Option<&'a Node>,
    value: Option<&'a Node>,
}

struct OwnedYaml11PairAccess {
    key: Option<Node>,
    value: Option<Node>,
}

struct ValueYaml11PairAccess {
    key: Option<Value>,
    value: Option<Value>,
}

struct ValueRefYaml11PairAccess<'a> {
    key: Option<&'a Value>,
    value: Option<&'a Value>,
}

macro_rules! pair_deserializer_forward {
    () => {
        forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct map struct enum identifier ignored_any
        }
    };
}

impl<'de, 'tree> de::Deserializer<'de> for InputYaml11PairDeserializer<'tree, 'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(InputYaml11PairAccess {
            key: Some(self.key),
            value: Some(self.value),
        })
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    pair_deserializer_forward!();
}

impl<'de> de::Deserializer<'de> for Yaml11PairDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(Yaml11PairAccess {
            key: Some(self.key),
            value: Some(self.value),
        })
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    pair_deserializer_forward!();
}

impl<'de> de::Deserializer<'de> for OwnedYaml11PairDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(OwnedYaml11PairAccess {
            key: self.key,
            value: self.value,
        })
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    pair_deserializer_forward!();
}

impl<'de> de::Deserializer<'de> for ValueYaml11PairDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(ValueYaml11PairAccess {
            key: self.key,
            value: self.value,
        })
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    pair_deserializer_forward!();
}

impl<'de> de::Deserializer<'de> for ValueRefYaml11PairDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(ValueRefYaml11PairAccess {
            key: Some(self.key),
            value: Some(self.value),
        })
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    pair_deserializer_forward!();
}

impl<'de, 'tree> SeqAccess<'de> for InputYaml11PairAccess<'tree, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if let Some(key) = self.key.take() {
            return seed.deserialize(key).map(Some);
        }
        if let Some(value) = self.value.take() {
            return seed.deserialize(value).map(Some);
        }
        Ok(None)
    }
}

impl<'de> SeqAccess<'de> for Yaml11PairAccess<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if let Some(key) = self.key.take() {
            return seed.deserialize(key).map(Some);
        }
        if let Some(value) = self.value.take() {
            return seed.deserialize(value).map(Some);
        }
        Ok(None)
    }
}

impl<'de> SeqAccess<'de> for OwnedYaml11PairAccess {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if let Some(key) = self.key.take() {
            let span = key.span;
            return seed
                .deserialize(key)
                .map(Some)
                .map_err(|error| error.with_span_if_missing(span));
        }
        if let Some(value) = self.value.take() {
            let span = value.span;
            return seed
                .deserialize(value)
                .map(Some)
                .map_err(|error| error.with_span_if_missing(span));
        }
        Ok(None)
    }
}

impl<'de> SeqAccess<'de> for ValueYaml11PairAccess {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if let Some(key) = self.key.take() {
            return seed.deserialize(key).map(Some);
        }
        if let Some(value) = self.value.take() {
            return seed.deserialize(value).map(Some);
        }
        Ok(None)
    }
}

impl<'de> SeqAccess<'de> for ValueRefYaml11PairAccess<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if let Some(key) = self.key.take() {
            return seed.deserialize(key).map(Some);
        }
        if let Some(value) = self.value.take() {
            return seed.deserialize(value).map(Some);
        }
        Ok(None)
    }
}

struct MapDeserializer<'a> {
    entries: &'a [(Node, Node)],
    index: usize,
    value: Option<(&'a Node, ErrorPathSegment)>,
}

struct InputMapDeserializer<'tree, 'de> {
    entries: &'tree [(Node, Node)],
    input: &'de str,
    index: usize,
    value: Option<(InputNode<'tree, 'de>, ErrorPathSegment)>,
}

impl<'de, 'tree> MapAccess<'de> for InputMapDeserializer<'tree, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let segment = node_path_segment(key);
        self.value = Some((
            InputNode {
                node: value,
                input: self.input,
            },
            segment.clone(),
        ));
        with_key_path(
            seed.deserialize(InputNode {
                node: key,
                input: self.input,
            }),
            segment,
        )
        .map(Some)
        .map_err(|error| error.with_span_if_missing(key.span))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let (value, segment) = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        with_value_path(seed.deserialize(value), segment)
            .map_err(|error| error.with_span_if_missing(value.node.span))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len().saturating_sub(self.index))
    }
}

impl<'de> MapAccess<'de> for MapDeserializer<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let segment = node_path_segment(key);
        self.value = Some((value, segment.clone()));
        with_key_path(seed.deserialize(key), segment)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(key.span))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let (value, segment) = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        with_value_path(seed.deserialize(value), segment)
            .map_err(|error| error.with_span_if_missing(value.span))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len().saturating_sub(self.index))
    }
}

impl<'de, 'tree> MapAccess<'de> for InputYaml11OmapDeserializer<'tree, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let (key, value) = yaml11_singleton_pair_node(item, "omap")?;
        self.value = Some(InputNode {
            node: value,
            input: self.input,
        });
        seed.deserialize(InputNode {
            node: key,
            input: self.input,
        })
        .map(Some)
        .map_err(|error| error.with_span_if_missing(key.span))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        seed.deserialize(value)
            .map_err(|error| error.with_span_if_missing(value.node.span))
    }
}

impl<'de> MapAccess<'de> for Yaml11OmapDeserializer<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let (key, value) = yaml11_singleton_pair_node(item, "omap")?;
        self.value = Some(value);
        seed.deserialize(key)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(key.span))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        seed.deserialize(value)
            .map_err(|error| error.with_span_if_missing(value.span))
    }
}

impl<'de> MapAccess<'de> for OwnedYaml11OmapDeserializer {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.next() else {
            return Ok(None);
        };
        let (key, value) = take_yaml11_singleton_pair_node(item, "omap")?;
        let key_span = key.span;
        self.value = Some(value);
        seed.deserialize(key)
            .map(Some)
            .map_err(|error| error.with_span_if_missing(key_span))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        let span = value.span;
        seed.deserialize(value)
            .map_err(|error| error.with_span_if_missing(span))
    }
}

impl<'de> MapAccess<'de> for ValueYaml11OmapDeserializer {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.next() else {
            return Ok(None);
        };
        let (key, value) = take_yaml11_singleton_pair_value(item, "omap")?;
        self.value = Some(value);
        seed.deserialize(key).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        seed.deserialize(value)
    }
}

impl<'de> MapAccess<'de> for ValueRefYaml11OmapDeserializer<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some(item) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        let (key, value) = yaml11_singleton_pair_value(item, "omap")?;
        self.value = Some(value);
        seed.deserialize(key).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::new("value requested before key", None))?;
        seed.deserialize(value)
    }
}

struct EnumDeserializer<'a> {
    key: &'a Node,
    value: Option<&'a Node>,
}

struct TaggedEnumDeserializer<'a> {
    tag: &'a Tag,
    value: &'a Node,
}

struct InputEnumDeserializer<'tree, 'de> {
    key: InputNode<'tree, 'de>,
    value: Option<InputNode<'tree, 'de>>,
}

struct InputTaggedEnumDeserializer<'tree, 'de> {
    tag: &'tree Tag,
    value: InputNode<'tree, 'de>,
}

impl<'de, 'tree> EnumAccess<'de> for InputTaggedEnumDeserializer<'tree, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de, 'tree> VariantAccess<'de> for InputTaggedEnumDeserializer<'tree, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value.node.value {
            NodeValue::Null => Ok(()),
            _ => Err(type_error("unit enum variant", self.value.node)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

impl<'de, 'tree> EnumAccess<'de> for InputEnumDeserializer<'tree, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key)?;
        Ok((variant, self))
    }
}

impl<'de, 'tree> VariantAccess<'de> for InputEnumDeserializer<'tree, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(node) if matches!(node.node.value, NodeValue::Null) => Ok(()),
            Some(node) => Err(type_error("unit enum variant", node.node)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

impl<'de> EnumAccess<'de> for TaggedEnumDeserializer<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for TaggedEnumDeserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value.value {
            NodeValue::Null => Ok(()),
            _ => Err(type_error("unit enum variant", self.value)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

struct OwnedTaggedEnumDeserializer {
    tag: Tag,
    value: Node,
}

impl<'de> EnumAccess<'de> for OwnedTaggedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for OwnedTaggedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value.value {
            NodeValue::Null => Ok(()),
            other => Err(type_error_owned(
                "unit enum variant",
                &other,
                self.value.span,
            )),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.value, visitor)
    }
}

impl<'de> EnumAccess<'de> for EnumDeserializer<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(self.key)?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for EnumDeserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(node) if matches!(node.value, NodeValue::Null) => Ok(()),
            Some(node) => Err(type_error("unit enum variant", node)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("newtype variant requires a value", None))?;
        seed.deserialize(value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(value, visitor)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::new("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(value, visitor)
    }
}

fn untag_node(mut node: &Node) -> &Node {
    while let NodeValue::Tagged(tagged) = &node.value {
        node = &tagged.value;
    }
    node
}

fn untag_node_owned(node: Node) -> Node {
    let Node {
        value,
        span,
        source,
    } = node;
    match value {
        NodeValue::Tagged(tagged) => untag_node_owned(tagged.value),
        value => Node {
            value,
            span,
            source,
        },
    }
}

fn untag_value(mut value: &Value) -> &Value {
    while let Value::Tagged(tagged) = value {
        value = &tagged.value;
    }
    value
}

fn untag_value_owned(value: Value) -> Value {
    match value {
        Value::Tagged(tagged) => untag_value_owned(tagged.value),
        value => value,
    }
}

fn type_error(expected: &'static str, node: &Node) -> Error {
    Error::data(
        format!("expected {expected}, found {}", kind_name(&node.value)),
        Some(node.span),
    )
}

fn type_error_owned(expected: &'static str, value: &NodeValue, span: Span) -> Error {
    Error::data(
        format!("expected {expected}, found {}", kind_name(value)),
        Some(span),
    )
}

fn kind_name(value: &NodeValue) -> &'static str {
    match value {
        NodeValue::Null => "null",
        NodeValue::Bool(_) => "bool",
        NodeValue::Number(Number::Integer(_)) => "integer",
        NodeValue::Number(Number::Unsigned(_)) => "unsigned integer",
        NodeValue::Number(Number::Float(_)) => "float",
        NodeValue::String(_) => "string",
        NodeValue::Sequence(_) => "sequence",
        NodeValue::Mapping(_) => "mapping",
        NodeValue::Tagged(_) => "tagged value",
    }
}

fn type_error_value(expected: &'static str, value: &Value) -> Error {
    Error::data(
        format!("expected {expected}, found {}", kind_name_value(value)),
        None,
    )
}

fn kind_name_value(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(Number::Integer(_)) => "integer",
        Value::Number(Number::Unsigned(_)) => "unsigned integer",
        Value::Number(Number::Float(_)) => "float",
        Value::String(_) => "string",
        Value::Sequence(_) => "sequence",
        Value::Mapping(_) => "mapping",
        Value::Tagged(_) => "tagged value",
    }
}
