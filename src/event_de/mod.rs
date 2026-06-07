#![allow(dead_code)]
// Compiled work-in-progress: this module is exercised by unit tests before it is
// wired into public Serde entrypoints.

use crate::{
    Error, ErrorPathSegment, Node, NodeValue, Number, Result, Span, Tag, TaggedNode,
    error::utf8_error_span,
    key_identity::{DuplicateKeyTracker, check_duplicate_with_tracker_at_depth_limit},
    parse::{
        Event, EventMeta, ScalarStyle, merge_policy_for_schema, parse_scalar_with_schema,
        schema_for_directives,
    },
    schema::{LoadOptions, Schema},
};
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};
use std::{collections::HashMap, io::Read, marker::PhantomData};

pub(crate) fn from_str_with_options<'de, T>(input: &'de str, options: LoadOptions) -> Result<T>
where
    T: serde::Deserialize<'de>,
{
    let configured_schema = options.selected_schema();
    let replay_budget = options.alias_expansion_budget(input.len());
    let max_nesting_depth = options.selected_max_nesting_depth();
    let events = crate::parse::EventStream::from_str_with_options(input, options)?
        .collect::<Result<Vec<_>>>()?;
    let mut source = EventSource::new(
        input,
        events,
        configured_schema,
        replay_budget,
        max_nesting_depth,
    );
    source.enter_stream()?;
    source.enter_document()?;
    let value = T::deserialize(EventNodeDeserializer {
        source: &mut source,
    })?;
    source.finish_document()?;
    match source.peek() {
        Some(Event::StreamEnd) => Ok(value),
        Some(Event::DocumentStart { .. }) => Err(Error::data(
            "expected single YAML document, found multiple documents",
            None,
        )),
        Some(event) => Err(unexpected_event("stream end", event)),
        None => Err(Error::data("unexpected end of YAML event stream", None)),
    }
}

pub(crate) fn from_documents_str_with_options<T>(
    input: &str,
    options: LoadOptions,
) -> Result<Vec<T>>
where
    T: DeserializeOwned,
{
    document_iter_str_with_options(input, options)?.collect()
}

pub(crate) fn document_iter_str_with_options<'de, T>(
    input: &'de str,
    options: LoadOptions,
) -> Result<EventDocumentIter<'de, T>>
where
    T: serde::Deserialize<'de>,
{
    let configured_schema = options.selected_schema();
    let replay_budget = options.alias_expansion_budget(input.len());
    let max_nesting_depth = options.selected_max_nesting_depth();
    Ok(EventDocumentIter {
        input,
        frames: EventDocumentFrames::from_str_with_options(input, options)?,
        configured_schema,
        replay_budget,
        max_nesting_depth,
        _marker: PhantomData,
    })
}

pub(crate) fn document_iter_slice_with_options<'de, T>(
    input: &'de [u8],
    options: LoadOptions,
) -> Result<EventDocumentIter<'de, T>>
where
    T: serde::Deserialize<'de>,
{
    options.check_input_len(input.len())?;
    let input = std::str::from_utf8(input)
        .map_err(|err| Error::encoding("input is not valid UTF-8", utf8_error_span(input, err)))?;
    document_iter_str_with_options(input, options)
}

pub(crate) fn document_iter_reader_with_options<T, R>(
    reader: R,
    options: LoadOptions,
) -> Result<OwnedEventDocumentIter<T>>
where
    T: DeserializeOwned,
    R: Read,
{
    let bytes = crate::de::read_to_end_with_options(reader, options)?;
    let input = String::from_utf8(bytes).map_err(|err| {
        Error::encoding(
            "input is not valid UTF-8",
            utf8_error_span(err.as_bytes(), err.utf8_error()),
        )
    })?;
    let configured_schema = options.selected_schema();
    let replay_budget = options.alias_expansion_budget(input.len());
    let max_nesting_depth = options.selected_max_nesting_depth();
    let frames = EventDocumentFrames::from_str_with_options(&input, options)?;
    Ok(OwnedEventDocumentIter {
        input,
        frames,
        configured_schema,
        replay_budget,
        max_nesting_depth,
        _marker: PhantomData,
    })
}

pub(crate) struct EventDocumentIter<'de, T> {
    input: &'de str,
    frames: EventDocumentFrames,
    configured_schema: Schema,
    replay_budget: usize,
    max_nesting_depth: Option<usize>,
    _marker: PhantomData<T>,
}

impl<'de, T> Iterator for EventDocumentIter<'de, T>
where
    T: serde::Deserialize<'de>,
{
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let (index, frame) = self.frames.next_frame()?;
        Some(
            frame
                .and_then(|events| {
                    deserialize_document_frame(
                        self.input,
                        events,
                        self.configured_schema,
                        self.replay_budget,
                        self.max_nesting_depth,
                    )
                })
                .map_err(|error| error.with_document_index(index)),
        )
    }
}

pub(crate) struct OwnedEventDocumentIter<T> {
    input: String,
    frames: EventDocumentFrames,
    configured_schema: Schema,
    replay_budget: usize,
    max_nesting_depth: Option<usize>,
    _marker: PhantomData<T>,
}

impl<T> Iterator for OwnedEventDocumentIter<T>
where
    T: DeserializeOwned,
{
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let (index, frame) = self.frames.next_frame()?;
        Some(
            frame
                .and_then(|events| {
                    deserialize_document_frame(
                        &self.input,
                        events,
                        self.configured_schema,
                        self.replay_budget,
                        self.max_nesting_depth,
                    )
                })
                .map_err(|error| error.with_document_index(index)),
        )
    }
}

mod prepared;
mod serde_impl;
mod source;

#[cfg(test)]
mod tests;

use self::prepared::{event_span, unexpected_event};
use self::serde_impl::EventNodeDeserializer;
use self::source::{EventDocumentFrames, EventSource, deserialize_document_frame, skip_node_in};

fn span_union(start: Span, end: Span) -> Span {
    Span::new(start.start, end.end, start.line, start.column)
}

fn tagged_key_node(tag: crate::Tag, tag_span: Span, value: Node) -> Node {
    let span = span_union(tag_span, value.span);
    Node::new(
        NodeValue::Tagged(Box::new(TaggedNode {
            tag,
            tag_span,
            value,
        })),
        span,
    )
}

fn apply_event_tag(meta: &EventMeta, node: Node) -> Node {
    let Some(tag) = &meta.tag else {
        return node;
    };
    if tag.tag.is_non_specific() {
        non_specific_event_node(span_union(tag.span, node.span), node)
    } else {
        tagged_key_node(tag.tag.clone(), tag.span, node)
    }
}

fn non_specific_event_node(span: Span, mut node: Node) -> Node {
    node.span = span;
    match &node.value {
        NodeValue::Sequence(_)
        | NodeValue::Mapping(_)
        | NodeValue::String(_)
        | NodeValue::Tagged(_) => node,
        NodeValue::Null | NodeValue::Bool(_) | NodeValue::Number(_) => {
            let source = node
                .scalar_source()
                .map(|source| source.raw().to_string())
                .unwrap_or_default();
            Node::new(NodeValue::String(source.clone()), span).with_scalar_source(source)
        }
    }
}

fn node_is_merge_key(key: &Node) -> bool {
    match &key.value {
        NodeValue::String(_) => key.as_str() == Some("<<"),
        NodeValue::Tagged(tagged) if tagged.tag.is_yaml_core("merge") => {
            tagged.value.as_str() == Some("<<")
        }
        _ => false,
    }
}

fn scan_anchors_in(
    events: &[Event],
    pos: usize,
    anchors: &mut HashMap<String, Vec<Event>>,
) -> Result<usize> {
    let Some(event) = events.get(pos) else {
        return Err(Error::data("unexpected end of YAML event stream", None));
    };
    if let Some(name) = event_anchor_name(event) {
        let end = skip_node_in(events, pos)?;
        anchors.insert(name.to_string(), events[pos..end].to_vec());
    }
    match event {
        Event::Scalar { .. } | Event::Alias { .. } => Ok(pos + 1),
        Event::SequenceStart { .. } => {
            let mut next = pos + 1;
            loop {
                match events.get(next) {
                    Some(Event::SequenceEnd { .. }) => return Ok(next + 1),
                    Some(_) => next = scan_anchors_in(events, next, anchors)?,
                    None => return Err(Error::data("unterminated sequence event stream", None)),
                }
            }
        }
        Event::MappingStart { .. } => {
            let mut next = pos + 1;
            loop {
                match events.get(next) {
                    Some(Event::MappingEnd { .. }) => return Ok(next + 1),
                    Some(_) => {
                        next = scan_anchors_in(events, next, anchors)?;
                        next = scan_anchors_in(events, next, anchors)?;
                    }
                    None => return Err(Error::data("unterminated mapping event stream", None)),
                }
            }
        }
        event => Err(unexpected_event("node", event)),
    }
}

fn event_anchor_name(event: &Event) -> Option<&str> {
    match event {
        Event::Scalar { meta, .. }
        | Event::SequenceStart { meta, .. }
        | Event::MappingStart { meta, .. } => {
            meta.anchor.as_ref().map(|anchor| anchor.name.as_str())
        }
        Event::StreamStart
        | Event::StreamEnd
        | Event::DocumentStart { .. }
        | Event::DocumentEnd { .. }
        | Event::SequenceEnd { .. }
        | Event::MappingEnd { .. }
        | Event::Alias { .. } => None,
    }
}
