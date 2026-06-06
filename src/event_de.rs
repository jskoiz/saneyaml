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

struct EventDocumentFrames {
    events: crate::parse::EventStream,
    started: bool,
    finished: bool,
    index: usize,
}

impl EventDocumentFrames {
    fn from_str_with_options(input: &str, options: LoadOptions) -> Result<Self> {
        Ok(Self {
            events: crate::parse::EventStream::from_str_with_options(input, options)?,
            started: false,
            finished: false,
            index: 0,
        })
    }

    fn next_frame(&mut self) -> Option<(usize, Result<Vec<Event>>)> {
        if self.finished {
            return None;
        }
        let index = self.index;
        if let Err(error) = self.enter_stream() {
            self.finished = true;
            return Some((index, Err(error)));
        }

        match self.events.next() {
            Some(Ok(Event::StreamEnd)) => {
                self.finished = true;
                None
            }
            Some(Ok(start @ Event::DocumentStart { .. })) => {
                Some((index, self.collect_document_frame(start)))
            }
            Some(Ok(event)) => {
                self.finished = true;
                Some((
                    index,
                    Err(unexpected_event("document start or stream end", &event)),
                ))
            }
            Some(Err(error)) => {
                self.finished = true;
                Some((index, Err(error)))
            }
            None => {
                self.finished = true;
                None
            }
        }
    }

    fn enter_stream(&mut self) -> Result<()> {
        if self.started {
            return Ok(());
        }
        self.started = true;
        match self.events.next() {
            Some(Ok(Event::StreamStart)) => Ok(()),
            Some(Ok(event)) => Err(unexpected_event("stream start", &event)),
            Some(Err(error)) => Err(error),
            None => Err(Error::data("unexpected end of YAML event stream", None)),
        }
    }

    fn collect_document_frame(&mut self, start: Event) -> Result<Vec<Event>> {
        let mut frame = Vec::new();
        frame.push(Event::StreamStart);
        frame.push(start);
        loop {
            match self.events.next() {
                Some(Ok(event)) => {
                    let end = matches!(event, Event::DocumentEnd { .. });
                    frame.push(event);
                    if end {
                        frame.push(Event::StreamEnd);
                        self.index += 1;
                        return Ok(frame);
                    }
                }
                Some(Err(error)) => {
                    self.finished = true;
                    return Err(error);
                }
                None => {
                    self.finished = true;
                    return Err(Error::data("unexpected end of YAML event stream", None));
                }
            }
        }
    }
}

fn deserialize_document_frame<'de, T>(
    input: &'de str,
    events: Vec<Event>,
    configured_schema: Schema,
    replay_budget: usize,
    max_nesting_depth: Option<usize>,
) -> Result<T>
where
    T: serde::Deserialize<'de>,
{
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
        Some(event) => Err(unexpected_event("stream end", event)),
        None => Err(Error::data("unexpected end of YAML event stream", None)),
    }
}

struct EventSource<'de> {
    input: &'de str,
    events: Vec<Event>,
    pos: usize,
    configured_schema: Schema,
    schema: Schema,
    anchors: HashMap<String, Vec<Event>>,
    inject: Vec<InjectedEvents>,
    replayed_events: usize,
    replay_budget: usize,
    max_nesting_depth: Option<usize>,
    depth: usize,
}

struct InjectedEvents {
    anchor: String,
    events: Vec<Event>,
    pos: usize,
}

impl<'de> EventSource<'de> {
    fn new(
        input: &'de str,
        events: Vec<Event>,
        configured_schema: Schema,
        replay_budget: usize,
        max_nesting_depth: Option<usize>,
    ) -> Self {
        Self {
            input,
            events,
            pos: 0,
            configured_schema,
            schema: configured_schema,
            anchors: HashMap::new(),
            inject: Vec::new(),
            replayed_events: 0,
            replay_budget,
            max_nesting_depth,
            depth: 0,
        }
    }

    /// Records descent into a nested collection and enforces the configured
    /// nesting-depth ceiling. The event-backed path expands aliases lazily as
    /// it walks, so — unlike the tree-backed path's `AnchorTable::resolve` — the
    /// parser's literal-depth check does not bound the *expanded* depth. Without
    /// this guard a literally shallow document with a long alias chain recurses
    /// until the stack overflows. Mirrors the tree-backed `depth > max` check.
    fn enter_depth(&mut self, span: Span) -> Result<()> {
        self.depth = self.depth.saturating_add(1);
        if self.max_nesting_depth.is_some_and(|max| self.depth > max) {
            return Err(Error::limit(
                "maximum YAML nesting depth exceeded while expanding alias",
                span,
            ));
        }
        Ok(())
    }

    fn exit_depth(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Same ceiling as [`enter_depth`], but for the read-only key/merge
    /// materialization walk in [`node_at_for_key`], which threads an explicit
    /// `depth` because it borrows `self` immutably.
    fn check_depth(&self, depth: usize, span: impl Into<Option<Span>>) -> Result<()> {
        if self.max_nesting_depth.is_some_and(|max| depth > max) {
            return Err(Error::limit(
                "maximum YAML nesting depth exceeded while expanding alias",
                span,
            ));
        }
        Ok(())
    }

    fn peek(&self) -> Option<&Event> {
        if let Some(frame) = self.inject.last()
            && frame.pos < frame.events.len()
        {
            return frame.events.get(frame.pos);
        }
        self.events.get(self.pos)
    }

    fn next(&mut self) -> Result<Event> {
        loop {
            let event = self.next_raw()?;
            if let Event::Alias { anchor } = event {
                self.inject_alias(anchor.name, anchor.span)?;
                continue;
            }
            return Ok(event);
        }
    }

    fn next_raw(&mut self) -> Result<Event> {
        if let Some(event) = self.next_injected_event() {
            return Ok(event);
        }

        let pos = self.pos;
        let event = self
            .events
            .get(pos)
            .cloned()
            .ok_or_else(|| Error::data("unexpected end of YAML event stream", None))?;
        self.record_anchor_at(pos, &event)?;
        self.pos += 1;
        Ok(event)
    }

    fn resolve_aliases_until_non_alias(&mut self) -> Result<()> {
        while matches!(self.peek(), Some(Event::Alias { .. })) {
            let Event::Alias { anchor } = self.next_raw()? else {
                unreachable!("peek observed an alias");
            };
            self.inject_alias(anchor.name, anchor.span)?;
        }
        Ok(())
    }

    fn next_injected_event(&mut self) -> Option<Event> {
        loop {
            let frame = self.inject.last_mut()?;
            if frame.pos < frame.events.len() {
                let event = frame.events[frame.pos].clone();
                frame.pos += 1;
                if frame.pos == frame.events.len() {
                    self.inject.pop();
                }
                return Some(event);
            }
            self.inject.pop();
        }
    }

    fn record_anchor_at(&mut self, pos: usize, event: &Event) -> Result<()> {
        let Some(name) = event_anchor_name(event) else {
            return Ok(());
        };
        let end = skip_node_in(&self.events, pos)?;
        self.anchors
            .insert(name.to_string(), self.events[pos..end].to_vec());
        Ok(())
    }

    fn inject_alias(&mut self, name: String, span: Span) -> Result<()> {
        if self.inject.iter().any(|frame| frame.anchor == name) {
            return Err(Error::reference(
                format!("recursive alias `{name}` is not supported"),
                span,
            ));
        }
        let events = self
            .anchors
            .get(&name)
            .cloned()
            .ok_or_else(|| Error::reference(format!("unknown anchor `{name}`"), span))?;
        self.replayed_events = self.replayed_events.saturating_add(events.len());
        if self.replayed_events > self.replay_budget {
            return Err(Error::limit("alias event replay limit exceeded", span));
        }
        self.inject.push(InjectedEvents {
            anchor: name,
            events,
            pos: 0,
        });
        Ok(())
    }

    fn enter_stream(&mut self) -> Result<()> {
        match self.next()? {
            Event::StreamStart => Ok(()),
            event => Err(unexpected_event("stream start", &event)),
        }
    }

    fn enter_document(&mut self) -> Result<()> {
        match self.next()? {
            Event::DocumentStart { directives, .. } => {
                self.anchors.clear();
                self.inject.clear();
                self.replayed_events = 0;
                self.depth = 0;
                self.schema = schema_for_directives(self.configured_schema, &directives);
                Ok(())
            }
            event => Err(unexpected_event("document start", &event)),
        }
    }

    fn finish_document(&mut self) -> Result<()> {
        match self.next()? {
            Event::DocumentEnd { .. } => Ok(()),
            event => Err(unexpected_event("document end", &event)),
        }
    }

    fn scalar_from_event(
        &self,
        value: String,
        style: ScalarStyle,
        meta: &EventMeta,
        span: Span,
    ) -> Result<Node> {
        if let Some(tag) = &meta.tag {
            let tag = &tag.tag;
            let tag_span = meta.tag.as_ref().expect("tag checked").span;
            if tag.is_yaml_core("str") {
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::String(value), span),
                ));
            }
            if tag.is_yaml_core("int") {
                let number = crate::de::parse_explicit_core_int_text(&value, Some(span))?;
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::Number(number), span).with_scalar_source(value),
                ));
            }
            if tag.is_yaml_core("float") {
                let number = crate::de::parse_explicit_core_float_text(&value, Some(span))?;
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::Number(number), span).with_scalar_source(value),
                ));
            }
            if tag.is_yaml_core("bool") {
                let value = crate::de::parse_explicit_core_bool_text(&value, Some(span))?;
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::Bool(value), span),
                ));
            }
            if tag.is_yaml_core("null") {
                crate::de::parse_explicit_core_null_text(&value, Some(span))?;
                return Ok(tagged_key_node(
                    tag.clone(),
                    tag_span,
                    Node::new(NodeValue::Null, span),
                ));
            }
            let inner = self.untagged_scalar_from_event(value, style, span)?;
            if tag.is_non_specific() {
                return Ok(non_specific_event_node(span_union(tag_span, span), inner));
            }
            return Ok(Node::new(
                NodeValue::Tagged(Box::new(TaggedNode {
                    tag: tag.clone(),
                    tag_span,
                    value: inner,
                })),
                span_union(tag_span, span),
            ));
        }
        self.untagged_scalar_from_event(value, style, span)
    }

    fn untagged_scalar_from_event(
        &self,
        value: String,
        style: ScalarStyle,
        span: Span,
    ) -> Result<Node> {
        match style {
            ScalarStyle::Plain => parse_scalar_with_schema(&value, span, self.schema),
            ScalarStyle::SingleQuoted
            | ScalarStyle::DoubleQuoted
            | ScalarStyle::Literal
            | ScalarStyle::Folded => Ok(Node::new(NodeValue::String(value), span)),
        }
    }

    fn take_scalar(&mut self) -> Result<Node> {
        match self.next()? {
            Event::Scalar {
                value,
                style,
                meta,
                span,
            } => self.scalar_from_event(value, style, &meta, span),
            Event::Alias { anchor } => Err(Error::reference(
                "event-backed alias replay is not implemented",
                anchor.span,
            )),
            event => Err(unexpected_event("scalar", &event)),
        }
    }

    fn skip_node(&mut self) -> Result<()> {
        self.resolve_aliases_until_non_alias()?;
        match self.peek().cloned() {
            Some(Event::Scalar { .. }) => {
                self.next()?;
                Ok(())
            }
            Some(Event::Alias { anchor }) => Err(Error::reference(
                "event-backed alias replay is not implemented",
                anchor.span,
            )),
            Some(Event::SequenceStart { span, .. }) => {
                self.enter_depth(span)?;
                self.next()?;
                loop {
                    if matches!(self.peek(), Some(Event::SequenceEnd { .. })) {
                        self.next()?;
                        self.exit_depth();
                        return Ok(());
                    }
                    self.skip_node()?;
                }
            }
            Some(Event::MappingStart { span, .. }) => {
                if self.next_mapping_has_merge_key()? {
                    let mut node = self.materialize_current_node_for_merge()?;
                    node.apply_merge_keys_with_policy(merge_policy_for_schema(self.schema))?;
                    self.skip_node_raw()?;
                    return Ok(());
                }
                self.validate_next_mapping_duplicates()?;
                self.enter_depth(span)?;
                self.next()?;
                loop {
                    if matches!(self.peek(), Some(Event::MappingEnd { .. })) {
                        self.next()?;
                        self.exit_depth();
                        return Ok(());
                    }
                    self.skip_node()?;
                    self.skip_node()?;
                }
            }
            Some(event) => Err(unexpected_event("node", &event)),
            None => Err(Error::data("unexpected end of YAML event stream", None)),
        }
    }

    fn skip_node_raw(&mut self) -> Result<()> {
        match self.next()? {
            Event::Scalar { .. } => Ok(()),
            Event::Alias { anchor } => Err(Error::reference(
                "event-backed alias replay is not implemented",
                anchor.span,
            )),
            Event::SequenceStart { span, .. } => {
                self.enter_depth(span)?;
                loop {
                    if matches!(self.peek(), Some(Event::SequenceEnd { .. })) {
                        self.next()?;
                        self.exit_depth();
                        return Ok(());
                    }
                    self.skip_node_raw()?;
                }
            }
            Event::MappingStart { span, .. } => {
                self.enter_depth(span)?;
                loop {
                    if matches!(self.peek(), Some(Event::MappingEnd { .. })) {
                        self.next()?;
                        self.exit_depth();
                        return Ok(());
                    }
                    self.skip_node_raw()?;
                    self.skip_node_raw()?;
                }
            }
            event => Err(unexpected_event("node", &event)),
        }
    }

    fn materialize_current_node_for_merge(&self) -> Result<Node> {
        let (events, pos) = self.current_events_and_pos();
        let mut scan_anchors = self.anchors.clone();
        let mut replayed_events = 0usize;
        let (node, next) = self.node_at_for_key(
            events,
            pos,
            &mut scan_anchors,
            &mut Vec::new(),
            &mut replayed_events,
            true,
            self.depth,
        )?;
        let expected = skip_node_in(events, pos)?;
        if next != expected {
            return Err(Error::data(
                "unterminated merge materialization event stream",
                None,
            ));
        }
        Ok(node)
    }

    fn next_mapping_has_merge_key(&self) -> Result<bool> {
        let (events, start) = self.current_events_and_pos();
        let Some(Event::MappingStart { .. }) = events.get(start) else {
            return Ok(false);
        };
        let mut pos = start + 1;
        let mut scan_anchors = self.anchors.clone();
        let mut replayed_events = 0usize;
        while let Some(event) = events.get(pos) {
            if matches!(event, Event::MappingEnd { .. }) {
                return Ok(false);
            }
            let (key, next_pos) = self.node_at_for_key(
                events,
                pos,
                &mut scan_anchors,
                &mut Vec::new(),
                &mut replayed_events,
                true,
                self.depth,
            )?;
            if node_is_merge_key(&key) {
                return Ok(true);
            }
            pos = next_pos;
            pos = scan_anchors_in(events, pos, &mut scan_anchors)?;
        }
        Err(Error::data("unterminated mapping event stream", None))
    }

    fn validate_next_mapping_duplicates(&self) -> Result<()> {
        let (events, start) = self.current_events_and_pos();
        let Some(Event::MappingStart { .. }) = events.get(start) else {
            return Ok(());
        };
        let mut pos = start + 1;
        let mut seen = DuplicateKeyTracker::new();
        let mut scan_anchors = self.anchors.clone();
        let mut replayed_events = 0usize;
        while let Some(event) = events.get(pos) {
            if matches!(event, Event::MappingEnd { .. }) {
                return Ok(());
            }
            if let Some((key, next_pos)) = self.mapping_key_at(
                events,
                pos,
                &mut scan_anchors,
                &mut replayed_events,
                self.depth,
            )? {
                if node_is_merge_key(&key) {
                    return Err(Error::data(
                        "event-backed merge-key expansion is not implemented",
                        Some(key.span),
                    ));
                }
                check_duplicate_with_tracker_at_depth_limit(&mut seen, &key, 1, None)?;
                pos = next_pos;
            } else {
                pos = scan_anchors_in(events, pos, &mut scan_anchors)?;
            }
            pos = scan_anchors_in(events, pos, &mut scan_anchors)?;
        }
        Err(Error::data("unterminated mapping event stream", None))
    }

    fn current_events_and_pos(&self) -> (&[Event], usize) {
        if let Some(frame) = self.inject.last()
            && frame.pos < frame.events.len()
        {
            return (&frame.events, frame.pos);
        }
        (&self.events, self.pos)
    }

    fn mapping_key_at(
        &self,
        events: &[Event],
        pos: usize,
        scan_anchors: &mut HashMap<String, Vec<Event>>,
        replayed_events: &mut usize,
        depth: usize,
    ) -> Result<Option<(Node, usize)>> {
        if let Some(name) = events.get(pos).and_then(event_anchor_name) {
            let end = skip_node_in(events, pos)?;
            scan_anchors.insert(name.to_string(), events[pos..end].to_vec());
        }
        match events.get(pos) {
            Some(Event::Scalar { .. })
            | Some(Event::Alias { .. })
            | Some(Event::SequenceStart { .. })
            | Some(Event::MappingStart { .. }) => self
                .node_at_for_key(
                    events,
                    pos,
                    scan_anchors,
                    &mut Vec::new(),
                    replayed_events,
                    false,
                    depth,
                )
                .map(|(node, next)| Some((node, next))),
            Some(_) | None => Ok(None),
        }
    }

    fn scalar_key_at(&self, pos: usize) -> Result<Option<(Node, usize)>> {
        self.scalar_key_at_in(&self.events, pos)
    }

    fn scalar_key_at_in(&self, events: &[Event], pos: usize) -> Result<Option<(Node, usize)>> {
        let Some(Event::Scalar {
            value,
            style,
            meta,
            span,
        }) = events.get(pos)
        else {
            return Ok(None);
        };
        self.scalar_from_event(value.clone(), *style, meta, *span)
            .map(|node| Some((node, pos + 1)))
    }

    fn scalar_key_node_from_event(
        &self,
        value: String,
        style: ScalarStyle,
        meta: &EventMeta,
        span: Span,
    ) -> Result<Node> {
        let Some(tag) = &meta.tag else {
            return self.scalar_from_event(value, style, meta, span);
        };
        let inner = if tag.tag.is_yaml_core("int") {
            Node::new(
                NodeValue::Number(crate::de::parse_explicit_core_int_text(&value, Some(span))?),
                span,
            )
        } else if tag.tag.is_yaml_core("float") {
            Node::new(
                NodeValue::Number(crate::de::parse_explicit_core_float_text(
                    &value,
                    Some(span),
                )?),
                span,
            )
        } else if tag.tag.is_yaml_core("bool") {
            Node::new(
                NodeValue::Bool(crate::de::parse_explicit_core_bool_text(
                    &value,
                    Some(span),
                )?),
                span,
            )
        } else if tag.tag.is_yaml_core("null") {
            crate::de::parse_explicit_core_null_text(&value, Some(span))?;
            Node::new(NodeValue::Null, span)
        } else {
            let _ = style;
            Node::new(NodeValue::String(value), span)
        };
        Ok(tagged_key_node(tag.tag.clone(), tag.span, inner))
    }

    #[allow(clippy::too_many_arguments)]
    fn node_at_for_key(
        &self,
        events: &[Event],
        pos: usize,
        scan_anchors: &mut HashMap<String, Vec<Event>>,
        active_aliases: &mut Vec<String>,
        replayed_events: &mut usize,
        allow_merge_key: bool,
        depth: usize,
    ) -> Result<(Node, usize)> {
        let Some(event) = events.get(pos) else {
            return Err(Error::data("unexpected end of YAML event stream", None));
        };
        self.check_depth(depth, event_span(event))?;
        if let Some(name) = event_anchor_name(event) {
            let end = skip_node_in(events, pos)?;
            scan_anchors.insert(name.to_string(), events[pos..end].to_vec());
        }

        match event {
            Event::Scalar {
                value,
                style,
                meta,
                span,
            } => self
                .scalar_key_node_from_event(value.clone(), *style, meta, *span)
                .map(|node| (node, pos + 1)),
            Event::Alias { anchor } => {
                let name = &anchor.name;
                if active_aliases.iter().any(|active| active == name) {
                    return Err(Error::reference(
                        format!("recursive alias `{name}` is not supported"),
                        anchor.span,
                    ));
                }
                let target = scan_anchors.get(name).cloned().ok_or_else(|| {
                    Error::reference(format!("unknown anchor `{name}`"), anchor.span)
                })?;
                *replayed_events = replayed_events.saturating_add(target.len());
                if *replayed_events > self.replay_budget {
                    return Err(Error::limit(
                        "alias event replay limit exceeded",
                        anchor.span,
                    ));
                }
                active_aliases.push(name.clone());
                let (mut node, end) = self.node_at_for_key(
                    &target,
                    0,
                    scan_anchors,
                    active_aliases,
                    replayed_events,
                    allow_merge_key,
                    depth,
                )?;
                active_aliases.pop();
                if end != target.len() {
                    return Err(Error::data("unterminated alias key event subtree", None));
                }
                node.span = anchor.span;
                Ok((node, pos + 1))
            }
            Event::SequenceStart { meta, span, .. } => {
                let mut items = Vec::new();
                let mut next = pos + 1;
                loop {
                    match events.get(next) {
                        Some(Event::SequenceEnd { span: end_span }) => {
                            let node =
                                Node::new(NodeValue::Sequence(items), span_union(*span, *end_span));
                            return Ok((apply_event_tag(meta, node), next + 1));
                        }
                        Some(_) => {
                            let (item, after_item) = self.node_at_for_key(
                                events,
                                next,
                                scan_anchors,
                                active_aliases,
                                replayed_events,
                                allow_merge_key,
                                depth + 1,
                            )?;
                            items.push(item);
                            next = after_item;
                        }
                        None => {
                            return Err(Error::data("unterminated sequence event stream", None));
                        }
                    }
                }
            }
            Event::MappingStart { meta, span, .. } => {
                let mut entries = Vec::new();
                let mut seen = DuplicateKeyTracker::new();
                let mut next = pos + 1;
                loop {
                    match events.get(next) {
                        Some(Event::MappingEnd { span: end_span }) => {
                            let node = Node::new(
                                NodeValue::Mapping(entries),
                                span_union(*span, *end_span),
                            );
                            return Ok((apply_event_tag(meta, node), next + 1));
                        }
                        Some(_) => {
                            let (key, after_key) = self.node_at_for_key(
                                events,
                                next,
                                scan_anchors,
                                active_aliases,
                                replayed_events,
                                allow_merge_key,
                                depth + 1,
                            )?;
                            if !allow_merge_key && node_is_merge_key(&key) {
                                return Err(Error::data(
                                    "event-backed merge-key expansion is not implemented",
                                    Some(key.span),
                                ));
                            }
                            if !(allow_merge_key
                                && self.schema.is_legacy_compatible()
                                && node_is_merge_key(&key))
                            {
                                check_duplicate_with_tracker_at_depth_limit(
                                    &mut seen, &key, 1, None,
                                )?;
                            }
                            let (value, after_value) = self.node_at_for_key(
                                events,
                                after_key,
                                scan_anchors,
                                active_aliases,
                                replayed_events,
                                allow_merge_key,
                                depth + 1,
                            )?;
                            entries.push((key, value));
                            next = after_value;
                        }
                        None => return Err(Error::data("unterminated mapping event stream", None)),
                    }
                }
            }
            event => Err(unexpected_event("node", event)),
        }
    }
}

fn skip_node_in(events: &[Event], pos: usize) -> Result<usize> {
    match events
        .get(pos)
        .ok_or_else(|| Error::data("unexpected end of YAML event stream", None))?
    {
        Event::Scalar { .. } | Event::Alias { .. } => Ok(pos + 1),
        Event::SequenceStart { .. } => {
            let mut next = pos + 1;
            loop {
                match events.get(next) {
                    Some(Event::SequenceEnd { .. }) => return Ok(next + 1),
                    Some(_) => next = skip_node_in(events, next)?,
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
                        next = skip_node_in(events, next)?;
                        next = skip_node_in(events, next)?;
                    }
                    None => return Err(Error::data("unterminated mapping event stream", None)),
                }
            }
        }
        event => Err(unexpected_event("node", event)),
    }
}

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

struct EventNodeDeserializer<'a, 'de> {
    source: &'a mut EventSource<'de>,
}

impl<'de> EventNodeDeserializer<'_, 'de> {
    fn deserialize_prepared_current_node<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut node = self.source.materialize_current_node_for_merge()?;
        node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
        self.source.skip_node_raw()?;
        de::Deserializer::deserialize_any(PreparedNodeDeserializer { node }, visitor)
    }

    fn deserialize_prepared_current_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut node = self.source.materialize_current_node_for_merge()?;
        node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
        self.source.skip_node_raw()?;
        de::Deserializer::deserialize_seq(PreparedNodeDeserializer { node }, visitor)
    }

    fn deserialize_prepared_current_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut node = self.source.materialize_current_node_for_merge()?;
        node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
        self.source.skip_node_raw()?;
        de::Deserializer::deserialize_map(PreparedNodeDeserializer { node }, visitor)
    }
}

impl<'de> de::Deserializer<'de> for EventNodeDeserializer<'_, 'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.source.resolve_aliases_until_non_alias()?;
        match self.source.peek() {
            Some(Event::Scalar { .. }) => {
                let node = self.source.take_scalar()?;
                visit_scalar_any(&node, self.source.input, visitor)
            }
            Some(Event::SequenceStart { meta, .. }) | Some(Event::MappingStart { meta, .. })
                if meta.tag.is_some() =>
            {
                self.deserialize_prepared_current_node(visitor)
            }
            Some(Event::SequenceStart { .. }) => self.deserialize_seq(visitor),
            Some(Event::MappingStart { .. }) => self.deserialize_map(visitor),
            Some(Event::Alias { anchor }) => Err(Error::reference(
                "event-backed alias replay is not implemented",
                anchor.span,
            )),
            Some(event) => Err(unexpected_event("node", event)),
            None => Err(Error::data("unexpected end of YAML event stream", None)),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Bool(value) => with_span(visitor.visit_bool(value), node.span),
            _ => Err(type_error("bool", &node)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Number(number) => visit_i64_number(number, node.span, visitor),
            _ => Err(type_error("integer", &node)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Number(number) => visit_u64_number(number, node.span, visitor),
            _ => Err(type_error("unsigned integer", &node)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Number(number) => visit_i128_number(number, node.span, visitor),
            _ => Err(type_error("integer", &node)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Number(number) => visit_u128_number(number, node.span, visitor),
            _ => Err(type_error("unsigned integer", &node)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Number(number) => visit_f64_number(number, node.span, visitor),
            _ => Err(type_error("number", &node)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self.source.take_scalar()?;
        let value = prepared_string_target_text(&node).ok_or_else(|| type_error("char", &node))?;
        let mut chars = value.chars();
        match (chars.next(), chars.next()) {
            (Some(ch), None) => with_span(visitor.visit_char(ch), node.span),
            _ => Err(type_error("char", &node)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self.source.take_scalar()?;
        let value = string_target_text(&node).ok_or_else(|| type_error("string", &node))?;
        if let Some(borrowed) = borrowed_event_str(self.source.input, node.span, value) {
            return visitor.visit_borrowed_str(borrowed);
        }
        visitor.visit_str(value)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self.source.take_scalar()?;
        let value = string_target_text(&node).ok_or_else(|| type_error("string", &node))?;
        visitor.visit_string(value.to_string())
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self.source.take_scalar()?;
        Err(type_error("bytes", &node))
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.source.resolve_aliases_until_non_alias()?;
        if self.source.peek_is_null_scalar()? {
            self.source.take_scalar()?;
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.source.take_scalar()?);
        match node.value {
            NodeValue::Null => visitor.visit_unit(),
            _ => Err(type_error("unit/null", &node)),
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.source.resolve_aliases_until_non_alias()?;
        if self
            .source
            .peek_has_yaml_core_tag(&["set", "omap", "pairs"])
        {
            return self.deserialize_prepared_current_seq(visitor);
        }
        match self.source.next()? {
            Event::SequenceStart { span, .. } => {
                self.source.enter_depth(span)?;
                let value = visitor.visit_seq(EventSeqAccess {
                    source: &mut *self.source,
                    index: 0,
                });
                self.source.exit_depth();
                value
            }
            event => Err(unexpected_event("sequence", &event)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
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
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.source.resolve_aliases_until_non_alias()?;
        if self.source.peek_has_yaml_core_tag(&["omap"]) {
            return self.deserialize_prepared_current_map(visitor);
        }
        if self.source.next_mapping_has_merge_key()? {
            let mut node = self.source.materialize_current_node_for_merge()?;
            node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
            self.source.skip_node_raw()?;
            return de::Deserializer::deserialize_map(PreparedNodeDeserializer { node }, visitor);
        }
        self.source.validate_next_mapping_duplicates()?;
        match self.source.next()? {
            Event::MappingStart { span, .. } => {
                self.source.enter_depth(span)?;
                let value = visitor.visit_map(EventMapAccess {
                    source: &mut *self.source,
                    value: None,
                });
                self.source.exit_depth();
                value
            }
            event => Err(unexpected_event("mapping", &event)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // Materialize the current node and reuse the tree-backed enum logic so
        // the event path accepts the same forms as `de.rs`: bare-scalar unit
        // variants, single-key `{Variant: payload}` mappings (newtype/tuple/
        // struct variants), and tag-shorthand variants. The previous
        // scalar-only path rejected every externally-tagged variant that
        // carried a payload.
        self.source.resolve_aliases_until_non_alias()?;
        let mut node = self.source.materialize_current_node_for_merge()?;
        node.apply_merge_keys_with_policy(merge_policy_for_schema(self.source.schema))?;
        self.source.skip_node_raw()?;
        de::Deserializer::deserialize_enum(
            PreparedNodeDeserializer { node },
            name,
            variants,
            visitor,
        )
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.source.skip_node()?;
        visitor.visit_unit()
    }
}

impl EventSource<'_> {
    fn peek_has_yaml_core_tag(&self, suffixes: &[&str]) -> bool {
        match self.peek() {
            Some(Event::SequenceStart { meta, .. }) | Some(Event::MappingStart { meta, .. }) => {
                meta.tag
                    .as_ref()
                    .is_some_and(|tag| suffixes.iter().any(|suffix| tag.tag.is_yaml_core(suffix)))
            }
            _ => false,
        }
    }

    fn peek_is_null_scalar(&self) -> Result<bool> {
        let Some(Event::Scalar {
            value,
            style,
            meta,
            span,
        }) = self.peek()
        else {
            return Ok(false);
        };
        let node = self.scalar_from_event(value.clone(), *style, meta, *span)?;
        Ok(prepared_is_null_node(&node))
    }
}

struct EventSeqAccess<'a, 'de> {
    source: &'a mut EventSource<'de>,
    index: usize,
}

impl<'de> SeqAccess<'de> for EventSeqAccess<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        if matches!(self.source.peek(), Some(Event::SequenceEnd { .. })) {
            self.source.next()?;
            return Ok(None);
        }
        let index = self.index;
        self.index += 1;
        seed.deserialize(EventNodeDeserializer {
            source: self.source,
        })
        .map(Some)
        .map_err(|error| error.prepend_path_segment(ErrorPathSegment::Index(index)))
    }
}

struct EventMapAccess<'a, 'de> {
    source: &'a mut EventSource<'de>,
    value: Option<ErrorPathSegment>,
}

impl<'de> MapAccess<'de> for EventMapAccess<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: DeserializeSeed<'de>,
    {
        if matches!(self.source.peek(), Some(Event::MappingEnd { .. })) {
            self.source.next()?;
            return Ok(None);
        }
        let depth = self.source.depth;
        let (events, pos) = self.source.current_events_and_pos();
        let mut scan_anchors = self.source.anchors.clone();
        let mut replayed_events = 0usize;
        let segment = self
            .source
            .mapping_key_at(events, pos, &mut scan_anchors, &mut replayed_events, depth)?
            .map(|(node, _)| path_segment_for_node(&node))
            .unwrap_or(ErrorPathSegment::ComplexKey);
        self.value = Some(segment.clone());
        seed.deserialize(EventNodeDeserializer {
            source: self.source,
        })
        .map(Some)
        .map_err(|error| error.with_path_segment_if_empty(segment))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: DeserializeSeed<'de>,
    {
        let segment = self
            .value
            .take()
            .ok_or_else(|| Error::data("value requested before key", None))?;
        seed.deserialize(EventNodeDeserializer {
            source: self.source,
        })
        .map_err(|error| error.prepend_path_segment(segment))
    }
}

struct PreparedNodeDeserializer {
    node: Node,
}

struct PreparedSeqAccess {
    items: std::vec::IntoIter<Node>,
    index: usize,
}

impl<'de> SeqAccess<'de> for PreparedSeqAccess {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        let Some(node) = self.items.next() else {
            return Ok(None);
        };
        let index = self.index;
        self.index += 1;
        seed.deserialize(PreparedNodeDeserializer { node })
            .map(Some)
            .map_err(|error| error.prepend_path_segment(ErrorPathSegment::Index(index)))
    }
}

struct PreparedMapAccess {
    entries: std::vec::IntoIter<(Node, Node)>,
    value: Option<(Node, ErrorPathSegment)>,
}

impl<'de> MapAccess<'de> for PreparedMapAccess {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: DeserializeSeed<'de>,
    {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        let segment = path_segment_for_node(&key);
        self.value = Some((value, segment.clone()));
        seed.deserialize(PreparedNodeDeserializer { node: key })
            .map(Some)
            .map_err(|error| error.with_path_segment_if_empty(segment))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: DeserializeSeed<'de>,
    {
        let (node, segment) = self
            .value
            .take()
            .ok_or_else(|| Error::data("value requested before key", None))?;
        seed.deserialize(PreparedNodeDeserializer { node })
            .map_err(|error| error.prepend_path_segment(segment))
    }
}

impl<'de> de::Deserializer<'de> for PreparedNodeDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let span = self.node.span;
        match self.node.value {
            NodeValue::Null => visitor.visit_unit(),
            NodeValue::Bool(value) => visitor.visit_bool(value),
            NodeValue::Number(number) => visit_any_number(number, span, visitor),
            NodeValue::String(value) => visitor.visit_string(value),
            NodeValue::Sequence(items) => visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            }),
            NodeValue::Mapping(entries) => visitor.visit_map(PreparedMapAccess {
                entries: entries.into_iter(),
                value: None,
            }),
            NodeValue::Tagged(tagged) => visitor.visit_enum(PreparedTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Bool(value) => with_span(visitor.visit_bool(value), node.span),
            _ => Err(type_error("bool", &node)),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_i64_number(number, node.span, visitor),
            _ => Err(type_error("integer", &node)),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_u64_number(number, node.span, visitor),
            _ => Err(type_error("unsigned integer", &node)),
        }
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_i128_number(number, node.span, visitor),
            _ => Err(type_error("integer", &node)),
        }
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_u128_number(number, node.span, visitor),
            _ => Err(type_error("unsigned integer", &node)),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Number(number) => visit_f64_number(number, node.span, visitor),
            _ => Err(type_error("number", &node)),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        let value = prepared_string_target_text(&node).ok_or_else(|| type_error("char", &node))?;
        let mut chars = value.chars();
        match (chars.next(), chars.next()) {
            (Some(ch), None) => with_span(visitor.visit_char(ch), node.span),
            _ => Err(type_error("char", &node)),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        let value =
            prepared_string_target_text(&node).ok_or_else(|| type_error("string", &node))?;
        visitor.visit_string(value.to_string())
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        Err(type_error("bytes", &node))
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if prepared_is_null_node(&self.node) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Null => visitor.visit_unit(),
            _ => Err(type_error("unit/null", &node)),
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if yaml11_set_entries_node(&self.node)?.is_some() {
            let entries = take_yaml11_set_entries_node(self.node).expect("checked explicit !!set");
            let items = yaml11_set_key_nodes(entries)?;
            return visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            });
        }
        if yaml11_pair_items_node(&self.node, "omap")?.is_some() {
            let items =
                take_yaml11_pair_items_node(self.node, "omap").expect("checked explicit !!omap");
            let items = yaml11_pair_sequence_nodes(items, "omap")?;
            return visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            });
        }
        if yaml11_pair_items_node(&self.node, "pairs")?.is_some() {
            let items =
                take_yaml11_pair_items_node(self.node, "pairs").expect("checked explicit !!pairs");
            let items = yaml11_pair_sequence_nodes(items, "pairs")?;
            return visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            });
        }
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Sequence(items) => visitor.visit_seq(PreparedSeqAccess {
                items: items.into_iter(),
                index: 0,
            }),
            _ => Err(type_error("sequence", &node)),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
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
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        if let Some(items) = yaml11_pair_items_node(&self.node, "omap")? {
            validate_yaml11_omap_node_keys(items)?;
            let items =
                take_yaml11_pair_items_node(self.node, "omap").expect("checked explicit !!omap");
            let entries = yaml11_pair_entries(items, "omap")?;
            return visitor.visit_map(PreparedMapAccess {
                entries: entries.into_iter(),
                value: None,
            });
        }
        let node = prepared_untag_node_owned(self.node);
        match node.value {
            NodeValue::Mapping(entries) => visitor.visit_map(PreparedMapAccess {
                entries: entries.into_iter(),
                value: None,
            }),
            _ => Err(type_error("mapping", &node)),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
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
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.node.value {
            NodeValue::String(variant) => visitor.visit_enum(variant.into_deserializer()),
            NodeValue::Mapping(entries) if entries.len() == 1 => {
                let mut entries = entries.into_iter();
                let (key, value) = entries.next().expect("length checked");
                visitor.visit_enum(PreparedEnumDeserializer {
                    key,
                    value: Some(value),
                })
            }
            NodeValue::Tagged(tagged) => visitor.visit_enum(PreparedTaggedEnumDeserializer {
                tag: tagged.tag,
                value: tagged.value,
            }),
            _ => Err(type_error("enum string or single-key mapping", &self.node)),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

struct PreparedEnumDeserializer {
    key: Node,
    value: Option<Node>,
}

impl<'de> EnumAccess<'de> for PreparedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(PreparedNodeDeserializer {
            node: self.key.clone(),
        })?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for PreparedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        match self.value {
            None => Ok(()),
            Some(node) if matches!(node.value, NodeValue::Null) => Ok(()),
            Some(node) => Err(type_error("unit enum variant", &node)),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: DeserializeSeed<'de>,
    {
        let node = self
            .value
            .ok_or_else(|| Error::data("newtype variant requires a value", None))?;
        seed.deserialize(PreparedNodeDeserializer { node })
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self
            .value
            .ok_or_else(|| Error::data("tuple variant requires a value", None))?;
        de::Deserializer::deserialize_seq(PreparedNodeDeserializer { node }, visitor)
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let node = self
            .value
            .ok_or_else(|| Error::data("struct variant requires a value", None))?;
        de::Deserializer::deserialize_map(PreparedNodeDeserializer { node }, visitor)
    }
}

struct PreparedTaggedEnumDeserializer {
    tag: Tag,
    value: Node,
}

impl<'de> EnumAccess<'de> for PreparedTaggedEnumDeserializer {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: DeserializeSeed<'de>,
    {
        let variant =
            seed.deserialize(self.tag.serde_variant().into_owned().into_deserializer())?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for PreparedTaggedEnumDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        if prepared_is_null_node(&self.value) {
            Ok(())
        } else {
            Err(type_error("unit enum variant", &self.value))
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(PreparedNodeDeserializer { node: self.value })
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(PreparedNodeDeserializer { node: self.value }, visitor)
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(PreparedNodeDeserializer { node: self.value }, visitor)
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

fn yaml11_set_entries_node(node: &Node) -> Result<Option<&[(Node, Node)]>> {
    let Some(value) = explicit_core_tagged_node(node, "set") else {
        return Ok(None);
    };
    match &value.value {
        NodeValue::Mapping(entries) => Ok(Some(entries)),
        _ => Err(type_error("mapping for explicit !!set", value)),
    }
}

fn take_yaml11_set_entries_node(node: Node) -> Option<Vec<(Node, Node)>> {
    let value = take_explicit_core_tagged_node(node, "set")?;
    match value.value {
        NodeValue::Mapping(entries) => Some(entries),
        _ => None,
    }
}

fn yaml11_pair_items_node<'a>(node: &'a Node, suffix: &'static str) -> Result<Option<&'a [Node]>> {
    let Some(value) = explicit_core_tagged_node(node, suffix) else {
        return Ok(None);
    };
    match &value.value {
        NodeValue::Sequence(items) => Ok(Some(items)),
        _ => Err(Error::data(
            format!("expected sequence for explicit !!{suffix}"),
            Some(value.span),
        )),
    }
}

fn take_yaml11_pair_items_node(node: Node, suffix: &'static str) -> Option<Vec<Node>> {
    let value = take_explicit_core_tagged_node(node, suffix)?;
    match value.value {
        NodeValue::Sequence(items) => Some(items),
        _ => None,
    }
}

fn validate_yaml11_omap_node_keys(items: &[Node]) -> Result<()> {
    let mut seen = DuplicateKeyTracker::new();
    for item in items {
        let (key, _) = yaml11_singleton_pair_node(item, "omap")?;
        check_duplicate_with_tracker_at_depth_limit(
            &mut seen,
            key,
            1,
            Some(crate::schema::DEFAULT_MAX_NESTING_DEPTH),
        )?;
    }
    Ok(())
}

fn yaml11_set_key_nodes(entries: Vec<(Node, Node)>) -> Result<Vec<Node>> {
    entries
        .into_iter()
        .map(|(key, value)| {
            ensure_yaml11_set_null_node(&value)?;
            Ok(key)
        })
        .collect()
}

fn ensure_yaml11_set_null_node(value: &Node) -> Result<()> {
    if prepared_is_null_node(value) {
        Ok(())
    } else {
        Err(Error::data(
            "expected explicit !!set entry value to be null",
            Some(value.span),
        ))
    }
}

fn yaml11_pair_sequence_nodes(items: Vec<Node>, suffix: &'static str) -> Result<Vec<Node>> {
    items
        .into_iter()
        .map(|item| {
            let span = item.span;
            let (key, value) = take_yaml11_singleton_pair_node(item, suffix)?;
            Ok(Node::new(NodeValue::Sequence(vec![key, value]), span))
        })
        .collect()
}

fn yaml11_pair_entries(items: Vec<Node>, suffix: &'static str) -> Result<Vec<(Node, Node)>> {
    items
        .into_iter()
        .map(|item| take_yaml11_singleton_pair_node(item, suffix))
        .collect()
}

fn yaml11_singleton_pair_node<'a>(
    node: &'a Node,
    suffix: &'static str,
) -> Result<(&'a Node, &'a Node)> {
    let node = prepared_untag_node(node);
    match &node.value {
        NodeValue::Mapping(entries) if entries.len() == 1 => Ok((&entries[0].0, &entries[0].1)),
        NodeValue::Mapping(_) => Err(Error::data(
            format!("expected explicit !!{suffix} entry to contain exactly one pair"),
            Some(node.span),
        )),
        _ => Err(Error::data(
            format!("expected single-pair mapping entry for explicit !!{suffix}"),
            Some(node.span),
        )),
    }
}

fn take_yaml11_singleton_pair_node(node: Node, suffix: &'static str) -> Result<(Node, Node)> {
    let node = prepared_untag_node_owned(node);
    match node.value {
        NodeValue::Mapping(entries) if entries.len() == 1 => {
            let mut entries = entries.into_iter();
            entries.next().ok_or_else(|| {
                Error::data(
                    "internal: singleton mapping lost its entry",
                    Some(node.span),
                )
            })
        }
        NodeValue::Mapping(_) => Err(Error::data(
            format!("expected explicit !!{suffix} entry to contain exactly one pair"),
            Some(node.span),
        )),
        _ => Err(Error::data(
            format!("expected single-pair mapping entry for explicit !!{suffix}"),
            Some(node.span),
        )),
    }
}

fn prepared_untag_node(mut node: &Node) -> &Node {
    while let NodeValue::Tagged(tagged) = &node.value {
        node = &tagged.value;
    }
    node
}

fn prepared_untag_node_owned(node: Node) -> Node {
    let Node {
        value,
        span,
        source,
    } = node;
    match value {
        NodeValue::Tagged(tagged) => prepared_untag_node_owned(tagged.value),
        value => Node {
            value,
            span,
            source,
        },
    }
}

fn prepared_is_null_node(node: &Node) -> bool {
    match &node.value {
        NodeValue::Null => true,
        NodeValue::Tagged(tagged) => prepared_is_null_node(&tagged.value),
        _ => false,
    }
}

fn prepared_string_target_text(node: &Node) -> Option<&str> {
    match &node.value {
        NodeValue::Tagged(tagged) => prepared_string_target_text(&tagged.value),
        _ => string_target_text(node),
    }
}

fn visit_scalar_any<'de, V>(node: &Node, input: &'de str, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match &node.value {
        NodeValue::Null => visitor.visit_unit(),
        NodeValue::Bool(value) => visitor.visit_bool(*value),
        NodeValue::Number(number) => visit_any_number(*number, node.span, visitor),
        NodeValue::String(value) => {
            if let Some(borrowed) = borrowed_event_str(input, node.span, value) {
                visitor.visit_borrowed_str(borrowed)
            } else {
                visitor.visit_str(value)
            }
        }
        NodeValue::Tagged(tagged) => visitor.visit_enum(PreparedTaggedEnumDeserializer {
            tag: tagged.tag.clone(),
            value: tagged.value.clone(),
        }),
        NodeValue::Sequence(_) | NodeValue::Mapping(_) => Err(type_error("scalar", node)),
    }
}

fn string_target_text(node: &Node) -> Option<&str> {
    match &node.value {
        NodeValue::String(value) => Some(value),
        NodeValue::Null => Some("null"),
        NodeValue::Bool(value) => Some(if *value { "true" } else { "false" }),
        NodeValue::Number(_) => node.scalar_source().map(|source| source.raw()),
        NodeValue::Tagged(tagged) => string_target_text(&tagged.value),
        NodeValue::Sequence(_) | NodeValue::Mapping(_) => None,
    }
}

fn borrowed_event_str<'de>(input: &'de str, span: Span, value: &str) -> Option<&'de str> {
    let raw = input.get(span.start..span.end)?;
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

fn path_segment_for_node(node: &Node) -> ErrorPathSegment {
    match &node.value {
        NodeValue::String(value) => ErrorPathSegment::Key(value.clone()),
        NodeValue::Bool(value) => ErrorPathSegment::ScalarKey(value.to_string()),
        NodeValue::Number(number) => ErrorPathSegment::ScalarKey(number.to_string()),
        NodeValue::Null => ErrorPathSegment::ScalarKey("null".to_string()),
        NodeValue::Sequence(_) | NodeValue::Mapping(_) | NodeValue::Tagged(_) => {
            ErrorPathSegment::ComplexKey
        }
    }
}

fn with_span<T>(result: Result<T>, span: Span) -> Result<T> {
    result.map_err(|error| error.with_span_if_missing(span))
}

fn visit_i64_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => match i64::try_from(value) {
            Ok(value) => with_span(visitor.visit_i64(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for i64",
                Some(span),
            )),
        },
        Number::Unsigned(value) => match i64::try_from(value) {
            Ok(value) => with_span(visitor.visit_i64(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for i64",
                Some(span),
            )),
        },
        Number::Float(_) => Err(Error::data("expected integer, found float", Some(span))),
    }
}

fn visit_u64_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) if value >= 0 => match u64::try_from(value) {
            Ok(value) => with_span(visitor.visit_u64(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for u64",
                Some(span),
            )),
        },
        Number::Unsigned(value) => match u64::try_from(value) {
            Ok(value) => with_span(visitor.visit_u64(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for u64",
                Some(span),
            )),
        },
        Number::Integer(_) => Err(Error::data(
            "expected unsigned integer, found integer",
            Some(span),
        )),
        Number::Float(_) => Err(Error::data(
            "expected unsigned integer, found float",
            Some(span),
        )),
    }
}

fn visit_i128_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => with_span(visitor.visit_i128(value), span),
        Number::Unsigned(value) => match i128::try_from(value) {
            Ok(value) => with_span(visitor.visit_i128(value), span),
            Err(_) => Err(Error::data(
                "integer scalar is out of range for i128",
                Some(span),
            )),
        },
        Number::Float(_) => Err(Error::data("expected integer, found float", Some(span))),
    }
}

fn visit_u128_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) if value >= 0 => {
            let value = u128::try_from(value).expect("non-negative i128 fits u128");
            with_span(visitor.visit_u128(value), span)
        }
        Number::Unsigned(value) => with_span(visitor.visit_u128(value), span),
        Number::Integer(_) => Err(Error::data(
            "expected unsigned integer, found integer",
            Some(span),
        )),
        Number::Float(_) => Err(Error::data(
            "expected unsigned integer, found float",
            Some(span),
        )),
    }
}

fn visit_f64_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => with_span(visitor.visit_f64(value as f64), span),
        Number::Unsigned(value) => with_span(visitor.visit_f64(value as f64), span),
        Number::Float(value) => with_span(visitor.visit_f64(value), span),
    }
}

fn visit_any_number<'de, V>(number: Number, span: Span, visitor: V) -> Result<V::Value>
where
    V: Visitor<'de>,
{
    match number {
        Number::Integer(value) => match i64::try_from(value) {
            Ok(value) => with_span(visitor.visit_i64(value), span),
            Err(_) => with_span(visitor.visit_i128(value), span),
        },
        Number::Unsigned(value) => match u64::try_from(value) {
            Ok(value) => with_span(visitor.visit_u64(value), span),
            Err(_) => with_span(visitor.visit_u128(value), span),
        },
        Number::Float(value) => with_span(visitor.visit_f64(value), span),
    }
}

fn type_error(expected: &'static str, node: &Node) -> Error {
    Error::data(
        format!("expected {expected}, found {}", kind_name(&node.value)),
        Some(node.span),
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

fn unexpected_event(expected: &'static str, event: &Event) -> Error {
    Error::data(
        format!("expected {expected}, found {}", event_kind(event)),
        event_span(event),
    )
}

fn event_kind(event: &Event) -> &'static str {
    match event {
        Event::StreamStart => "stream start",
        Event::StreamEnd => "stream end",
        Event::DocumentStart { .. } => "document start",
        Event::DocumentEnd { .. } => "document end",
        Event::SequenceStart { .. } => "sequence start",
        Event::SequenceEnd { .. } => "sequence end",
        Event::MappingStart { .. } => "mapping start",
        Event::MappingEnd { .. } => "mapping end",
        Event::Alias { .. } => "alias",
        Event::Scalar { .. } => "scalar",
    }
}

fn event_span(event: &Event) -> Option<Span> {
    match event {
        Event::DocumentStart { span, .. }
        | Event::DocumentEnd { span, .. }
        | Event::SequenceStart { span, .. }
        | Event::SequenceEnd { span }
        | Event::MappingStart { span, .. }
        | Event::MappingEnd { span }
        | Event::Scalar { span, .. } => Some(*span),
        Event::Alias { anchor } => Some(anchor.span),
        Event::StreamStart | Event::StreamEnd => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, de::IgnoredAny};
    use std::collections::{BTreeMap, BTreeSet};
    use std::io::{self, Cursor, Read};

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

    #[derive(Debug, Deserialize, PartialEq)]
    struct EventConfig<'a> {
        name: &'a str,
        ports: Vec<u16>,
        enabled: bool,
        labels: BTreeMap<String, String>,
        optional: Option<String>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct OwnedEventConfig {
        name: String,
        ports: Vec<u16>,
        enabled: bool,
        labels: BTreeMap<String, String>,
        optional: Option<String>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ExplicitCoreScalars {
        string_null: String,
        optional_string_null: Option<String>,
        string_bool: String,
        yes: bool,
        off: bool,
        maybe: Option<String>,
        unit: (),
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ExplicitCoreNumbers {
        integer: i64,
        unsigned: u64,
        float: f64,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TargetMap {
        target: BTreeMap<String, String>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TargetValueMap {
        target: BTreeMap<String, crate::Value>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct KnownOnly {
        name: String,
    }

    fn assert_value_tagged_key(
        mapping: &crate::Value,
        expected_tag: crate::Tag,
        expected_key: &str,
        expected_value: &str,
    ) {
        let mapping = mapping.as_mapping().expect("mapping value");
        assert!(
            mapping.iter().any(|(key, value)| {
                matches!(key, crate::Value::Tagged(tagged)
                    if tagged.tag == expected_tag
                        && tagged.value.as_str() == Some(expected_key)
                        && value.as_str() == Some(expected_value))
            }),
            "expected tagged key {expected_tag:?} {expected_key:?}: {expected_value:?}"
        );
    }

    #[test]
    fn event_deserializer_reads_typed_structs() {
        let input = "\
name: api
ports: [80, 443]
enabled: true
labels:
  tier: backend
  release: stable
optional: null
";

        let parsed: EventConfig<'_> =
            from_str_with_options(input, LoadOptions::new()).expect("event-backed typed config");
        assert_eq!(parsed.name, "api");
        assert!(std::ptr::eq(parsed.name.as_ptr(), input[6..9].as_ptr()));
        assert_eq!(parsed.ports, vec![80, 443]);
        assert!(parsed.enabled);
        assert_eq!(parsed.labels["tier"], "backend");
        assert_eq!(parsed.labels["release"], "stable");
        assert_eq!(parsed.optional, None);
    }

    #[test]
    fn event_deserializer_rejects_duplicate_scalar_keys() {
        let input = "labels:\n  tier: backend\n  tier: worker\n";
        let error = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
            input,
            LoadOptions::new(),
        )
        .expect_err("event-backed duplicate keys reject");
        assert!(error.to_string().contains("duplicate mapping key"));
    }

    #[test]
    fn event_deserializer_rejects_duplicate_sequence_alias_mapping_keys() {
        let input = "seq: &seq [a, b]\nroot: {? *seq : first, ? [a, b] : second}\n";
        let error = from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
            .expect_err("event-backed alias-expanded sequence keys reject");

        assert!(error.to_string().contains("duplicate mapping key"));
    }

    #[test]
    fn event_deserializer_rejects_duplicate_mapping_alias_keys_order_insensitively() {
        let input = "base: &base {a: 1, b: 2}\nroot: {? *base : first, ? {b: 2, a: 1} : second}\n";
        let error = from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
            .expect_err("event-backed alias-expanded mapping keys reject");

        assert!(error.to_string().contains("duplicate mapping key"));
    }

    #[test]
    fn event_deserializer_accepts_distinct_complex_alias_mapping_keys() {
        let input = "seq: &seq [a, b]\nroot: {? *seq : first, ? [a, c] : second}\n";

        from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
            .expect("distinct complex alias keys pass duplicate preflight");
    }

    #[test]
    fn event_deserializer_rejects_recursive_alias_mapping_keys() {
        let input = "root: {? &self [*self] : value}\n";
        let error = from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
            .expect_err("recursive alias key rejects");

        assert!(error.to_string().contains("recursive alias"));
    }

    #[test]
    fn event_deserializer_rejects_complex_alias_mapping_keys_over_budget() {
        let input = "seq: &seq [a, b]\nroot: {? *seq : first}\n";
        let error = from_str_with_options::<IgnoredAny>(
            input,
            LoadOptions::new().max_alias_expansion_nodes(1),
        )
        .expect_err("complex alias key replay budget rejects");

        assert!(
            error
                .to_string()
                .contains("alias event replay limit exceeded")
        );
    }

    #[test]
    fn event_deserializer_expands_merge_keys() {
        let input = "\
base: &base
  retries: 3
  command: deploy
target:
  <<: *base
  command: smoke
";
        let parsed =
            from_str_with_options::<TargetMap>(input, LoadOptions::new()).expect("merge keys");

        assert_eq!(parsed.target["retries"], "3");
        assert_eq!(parsed.target["command"], "smoke");
    }

    #[test]
    fn event_deserializer_expands_merge_lists_with_earlier_sources_winning() {
        let input = "\
base1: &base1 {a: one, shared: first}
base2: &base2 {b: two, shared: second}
target: {<<: [*base1, *base2], local: ok}
";
        let parsed =
            from_str_with_options::<TargetMap>(input, LoadOptions::new()).expect("merge list");

        assert_eq!(parsed.target["a"], "one");
        assert_eq!(parsed.target["b"], "two");
        assert_eq!(parsed.target["shared"], "first");
        assert_eq!(parsed.target["local"], "ok");
    }

    #[test]
    fn event_deserializer_expands_explicit_merge_tag_keys() {
        let input = "\
%TAG !m! tag:yaml.org,2002:
---
base: &base {a: one, shared: base}
tagged: {!!merge <<: *base, shared: tagged}
canonical: {!<tag:yaml.org,2002:merge> <<: *base, shared: canonical}
handle: {!m!merge <<: *base, shared: handle}
";
        let parsed = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
            input,
            LoadOptions::new(),
        )
        .expect("explicit merge tag keys");

        for (key, expected_shared) in [
            ("tagged", "tagged"),
            ("canonical", "canonical"),
            ("handle", "handle"),
        ] {
            assert_eq!(parsed[key]["a"], "one");
            assert_eq!(parsed[key]["shared"], expected_shared);
        }
    }

    #[test]
    fn event_deserializer_keeps_explicit_string_merge_key_literal() {
        let input = "base: &base {!!str <<: literal, a: one}\ntarget: {<<: *base}\n";
        let parsed = from_str_with_options::<TargetMap>(input, LoadOptions::new())
            .expect("explicit string merge key stays literal");

        assert_eq!(parsed.target["a"], "one");
        assert_eq!(parsed.target["<<"], "literal");
    }

    #[test]
    fn event_deserializer_reports_invalid_merge_payloads() {
        let input = "target: {<<: scalar}\n";
        let error = from_str_with_options::<TargetMap>(input, LoadOptions::new())
            .expect_err("invalid merge payload rejects");

        assert!(
            error
                .to_string()
                .contains("expected a mapping or list of mappings for merging"),
            "{error}"
        );
    }

    #[test]
    fn event_deserializer_skips_valid_merge_maps_for_ignored_values() {
        let input = "base: &base {a: one}\nname: app\nignored: {<<: *base, b: two}\n";
        let parsed = from_str_with_options::<KnownOnly>(input, LoadOptions::new())
            .expect("unknown merge-bearing field is skipped");

        assert_eq!(parsed.name, "app");
        from_str_with_options::<IgnoredAny>(input, LoadOptions::new())
            .expect("ignored-any skips merge-bearing maps");
    }

    #[test]
    fn event_deserializer_rejects_invalid_merge_payloads_in_ignored_values() {
        let input = "name: app\nignored: {<<: scalar}\n";
        let error = from_str_with_options::<KnownOnly>(input, LoadOptions::new())
            .expect_err("strict invalid merge payload rejects while skipping");

        assert!(
            error
                .to_string()
                .contains("expected a mapping or list of mappings for merging"),
            "{error}"
        );
    }

    #[test]
    fn event_deserializer_yaml11_skips_literal_merge_payload_in_ignored_value() {
        let input = "%YAML 1.1\n---\nname: app\nignored: {<<: scalar, keep: value}\n";
        let parsed =
            from_str_with_options::<KnownOnly>(input, LoadOptions::yaml_version_directive())
                .expect("directive-driven YAML 1.1 literal merge payload is skipped");

        assert_eq!(parsed.name, "app");
    }

    #[test]
    fn event_deserializer_rejects_repeated_merge_keys_by_default() {
        let input = "\
first: &first {shared: first}
second: &second {shared: second}
target:
  <<: *first
  !!merge <<: *second
";
        let error = from_str_with_options::<TargetMap>(input, LoadOptions::new())
            .expect_err("default repeated merge keys reject");

        assert!(error.to_string().contains("duplicate mapping key `<<`"));
    }

    #[test]
    fn event_deserializer_yaml11_recovers_repeated_merge_keys() {
        let input = "\
first: &first {shared: first, retries: 3}
second: &second {shared: second, timeout: 10}
target:
  <<: *first
  !<tag:yaml.org,2002:merge> <<: *second
  keep: value
";
        let parsed = from_str_with_options::<TargetMap>(input, LoadOptions::yaml_1_1())
            .expect("YAML 1.1 repeated merge keys recover");

        assert_eq!(parsed.target["shared"], "second");
        assert_eq!(parsed.target["retries"], "3");
        assert_eq!(parsed.target["timeout"], "10");
        assert_eq!(parsed.target["keep"], "value");
    }

    #[test]
    fn event_deserializer_yaml11_keeps_scalar_merge_payload_literal() {
        let input = "\
target:
  <<: scalar
  keep: value
";
        let parsed = from_str_with_options::<TargetMap>(input, LoadOptions::yaml_1_1())
            .expect("YAML 1.1 scalar merge payload stays literal");

        assert_eq!(parsed.target["<<"], "scalar");
        assert_eq!(parsed.target["keep"], "value");
    }

    #[test]
    fn event_deserializer_yaml11_keeps_mixed_invalid_merge_list_literal() {
        let input = "\
base: &base {a: 1}
target:
  <<: [*base, scalar]
  keep: value
";
        let parsed = from_str_with_options::<TargetValueMap>(input, LoadOptions::yaml_1_1())
            .expect("YAML 1.1 mixed invalid merge list stays literal");
        let merge = &parsed.target["<<"];
        let merge = merge.as_sequence().expect("literal merge list");

        assert_eq!(merge[0]["a"].as_u64(), Some(1));
        assert_eq!(merge[1].as_str(), Some("scalar"));
        assert_eq!(parsed.target["keep"].as_str(), Some("value"));
    }

    #[test]
    fn event_deserializer_reads_explicit_core_scalar_tags() {
        let input = "\
string_null: !!str null
optional_string_null: !!str null
string_bool: !!str true
yes: !!bool YES
off: !!bool off
maybe: !!null null
unit: !!null ~
";
        let parsed =
            from_str_with_options::<ExplicitCoreScalars>(input, LoadOptions::new()).unwrap();

        assert_eq!(
            parsed,
            ExplicitCoreScalars {
                string_null: "null".to_string(),
                optional_string_null: Some("null".to_string()),
                string_bool: "true".to_string(),
                yes: true,
                off: false,
                maybe: None,
                unit: (),
            }
        );
    }

    #[test]
    fn event_deserializer_reads_explicit_core_numeric_tags() {
        let input = "integer: !!int \"42\"\nunsigned: !!int 0x2A\nfloat: !!float \"1.5\"\n";
        let parsed =
            from_str_with_options::<ExplicitCoreNumbers>(input, LoadOptions::new()).unwrap();

        assert_eq!(
            parsed,
            ExplicitCoreNumbers {
                integer: 42,
                unsigned: 42,
                float: 1.5,
            }
        );
    }

    #[test]
    fn event_deserializer_explicit_tags_follow_directive_schema() {
        let parsed = from_str_with_options::<bool>(
            "%YAML 1.1\n--- !!bool YES\n",
            LoadOptions::yaml_version_directive(),
        )
        .expect("directive-driven explicit bool");

        assert!(parsed);
    }

    #[test]
    fn event_deserializer_rejects_invalid_explicit_core_scalar_tags() {
        let bool_error = from_str_with_options::<bool>("!!bool maybe\n", LoadOptions::new())
            .expect_err("invalid explicit bool");
        assert!(
            bool_error
                .to_string()
                .contains("failed to parse explicit !!bool scalar"),
            "{bool_error}"
        );

        let str_error = from_str_with_options::<i64>("!!str 7\n", LoadOptions::new())
            .expect_err("explicit string does not coerce to integer");
        assert!(str_error.to_string().contains("expected integer"));
    }

    #[test]
    fn event_deserializer_retains_tagged_scalars_for_value_and_unwraps_typed_strings() {
        let value = from_str_with_options::<crate::Value>("!Thing tagged\n", LoadOptions::new())
            .expect("custom tagged scalar value");
        let tagged = value.as_tagged().expect("custom tag retained");

        assert_eq!(tagged.tag, crate::Tag::new("Thing"));
        assert_eq!(tagged.value.as_str(), Some("tagged"));

        let typed = from_str_with_options::<String>("!Thing tagged\n", LoadOptions::new())
            .expect("typed string unwraps custom tag");
        assert_eq!(typed, "tagged");

        let explicit = from_str_with_options::<crate::Value>("!!str null\n", LoadOptions::new())
            .expect("explicit core string tag value");
        let tagged = explicit.as_tagged().expect("explicit core tag retained");
        assert_eq!(tagged.tag, crate::Tag::new("!!str"));
        assert_eq!(tagged.value.as_str(), Some("null"));
    }

    #[test]
    fn event_deserializer_retains_tagged_collections_for_value_and_unwraps_typed_targets() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct TaggedCollections {
            seq: Vec<String>,
            map: BTreeMap<String, String>,
        }

        let input = "seq: !Seq [a, b]\nmap: !Map {k: v}\n";
        let value =
            from_str_with_options::<crate::Value>(input, LoadOptions::new()).expect("value");

        let sequence = value["seq"].as_tagged().expect("sequence tag retained");
        assert_eq!(sequence.tag, crate::Tag::new("Seq"));
        assert_eq!(
            sequence
                .value
                .as_sequence()
                .expect("sequence payload")
                .len(),
            2
        );
        assert_eq!(sequence.value[0].as_str(), Some("a"));
        assert_eq!(sequence.value[1].as_str(), Some("b"));

        let mapping = value["map"].as_tagged().expect("mapping tag retained");
        assert_eq!(mapping.tag, crate::Tag::new("Map"));
        assert_eq!(mapping.value["k"].as_str(), Some("v"));

        let typed = from_str_with_options::<TaggedCollections>(input, LoadOptions::new())
            .expect("typed collections unwrap tags");
        assert_eq!(
            typed,
            TaggedCollections {
                seq: vec!["a".to_string(), "b".to_string()],
                map: BTreeMap::from([("k".to_string(), "v".to_string())]),
            }
        );

        let top_value = from_str_with_options::<crate::Value>("!Seq [a, b]\n", LoadOptions::new())
            .expect("top-level tagged sequence value");
        let tagged = top_value.as_tagged().expect("top-level tag retained");
        assert_eq!(tagged.tag, crate::Tag::new("Seq"));
        assert_eq!(tagged.value[1].as_str(), Some("b"));

        let top_typed = from_str_with_options::<Vec<String>>("!Seq [a, b]\n", LoadOptions::new())
            .expect("top-level typed sequence unwraps tag");
        assert_eq!(top_typed, ["a", "b"]);
    }

    #[test]
    fn event_deserializer_projects_yaml11_collection_tags_for_typed_targets() {
        let set = from_str_with_options::<BTreeSet<String>>(
            "!!set\n? alpha\n? beta\n",
            LoadOptions::new(),
        )
        .expect("typed !!set");
        assert_eq!(
            set,
            BTreeSet::from(["alpha".to_string(), "beta".to_string()])
        );

        let omap_pairs = from_str_with_options::<Vec<(String, i64)>>(
            "!!omap\n- first: 1\n- second: 2\n",
            LoadOptions::new(),
        )
        .expect("typed !!omap pair sequence");
        assert_eq!(
            omap_pairs,
            vec![("first".to_string(), 1), ("second".to_string(), 2)]
        );

        let omap_map = from_str_with_options::<BTreeMap<String, i64>>(
            "!!omap\n- second: 2\n- first: 1\n",
            LoadOptions::new(),
        )
        .expect("typed !!omap map");
        assert_eq!(
            omap_map,
            BTreeMap::from([("first".to_string(), 1), ("second".to_string(), 2)])
        );

        let pairs = from_str_with_options::<Vec<(String, i64)>>(
            "!!pairs\n- repeat: 1\n- repeat: 2\n",
            LoadOptions::new(),
        )
        .expect("typed !!pairs preserves duplicate keys");
        assert_eq!(
            pairs,
            vec![("repeat".to_string(), 1), ("repeat".to_string(), 2)]
        );
    }

    #[test]
    fn event_deserializer_rejects_lossy_yaml11_collection_tag_shapes() {
        let duplicate = from_str_with_options::<BTreeMap<String, i64>>(
            "!!omap\n- z: 1\n- a: 2\n- z: 3\n",
            LoadOptions::new(),
        )
        .expect_err("typed !!omap map rejects duplicate keys");
        assert!(duplicate.to_string().contains("duplicate mapping key `z`"));

        let set_error =
            from_str_with_options::<BTreeSet<String>>("!!set {alpha: true}\n", LoadOptions::new())
                .expect_err("typed !!set rejects non-null values");
        assert!(
            set_error
                .to_string()
                .contains("expected explicit !!set entry value to be null"),
            "{set_error}"
        );

        let omap_error = from_str_with_options::<Vec<(String, i64)>>(
            "!!omap\n- {a: 1, b: 2}\n",
            LoadOptions::new(),
        )
        .expect_err("typed !!omap rejects multi-pair entries");
        assert!(
            omap_error
                .to_string()
                .contains("expected explicit !!omap entry to contain exactly one pair"),
            "{omap_error}"
        );

        let pairs_error =
            from_str_with_options::<Vec<(String, i64)>>("!!pairs\n- scalar\n", LoadOptions::new())
                .expect_err("typed !!pairs rejects scalar entries");
        assert!(
            pairs_error
                .to_string()
                .contains("expected single-pair mapping entry for explicit !!pairs"),
            "{pairs_error}"
        );
    }

    #[test]
    fn event_deserializer_retains_tagged_merge_maps_for_value_and_unwraps_typed_targets() {
        let input = "base: &base {a: one}\ntarget: !Thing {<<: *base, b: two}\n";
        let value = from_str_with_options::<crate::Value>(input, LoadOptions::new())
            .expect("tagged merge map value");
        let tagged = value["target"].as_tagged().expect("target tag retained");

        assert_eq!(tagged.tag, crate::Tag::new("Thing"));
        assert_eq!(tagged.value["a"].as_str(), Some("one"));
        assert_eq!(tagged.value["b"].as_str(), Some("two"));

        let typed = from_str_with_options::<TargetMap>(input, LoadOptions::new())
            .expect("typed tagged merge map unwraps tag");
        assert_eq!(typed.target["a"], "one");
        assert_eq!(typed.target["b"], "two");
    }

    #[test]
    fn event_deserializer_retains_tagged_literal_merge_keys_without_expansion() {
        let input = "\
custom: {!Thing <<: literal, image: app:custom}
string: {!!str <<: literal, image: app:string}
";
        let value =
            from_str_with_options::<crate::Value>(input, LoadOptions::new()).expect("tagged keys");

        assert_value_tagged_key(&value["custom"], crate::Tag::new("Thing"), "<<", "literal");
        assert_value_tagged_key(&value["string"], crate::Tag::new("!!str"), "<<", "literal");
        assert_eq!(value["custom"]["image"].as_str(), Some("app:custom"));
        assert_eq!(value["string"]["image"].as_str(), Some("app:string"));

        let typed = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
            input,
            LoadOptions::new(),
        )
        .expect("typed maps unwrap tagged literal keys");
        assert_eq!(typed["custom"]["<<"], "literal");
        assert_eq!(typed["string"]["<<"], "literal");
    }

    #[test]
    fn event_deserializer_replays_acyclic_scalar_aliases() {
        let input = "base: &base api\nservice: *base\n";
        let parsed = from_str_with_options::<BTreeMap<String, String>>(input, LoadOptions::new())
            .expect("event-backed scalar alias replay");

        assert_eq!(parsed["base"], "api");
        assert_eq!(parsed["service"], "api");
    }

    #[test]
    fn event_deserializer_replays_acyclic_sequence_aliases() {
        let input = "base: &base [api, worker]\nservice: *base\n";
        let parsed =
            from_str_with_options::<BTreeMap<String, Vec<String>>>(input, LoadOptions::new())
                .expect("event-backed sequence alias replay");

        assert_eq!(parsed["base"], ["api", "worker"]);
        assert_eq!(parsed["service"], ["api", "worker"]);
    }

    #[test]
    fn event_deserializer_validates_alias_expanded_mapping_values() {
        let input = "base: &base {a: one, b: two}\ntarget: *base\n";
        let parsed =
            from_str_with_options::<TargetMap>(input, LoadOptions::new()).expect("mapping alias");

        assert_eq!(parsed.target["a"], "one");
        assert_eq!(parsed.target["b"], "two");
    }

    #[test]
    fn event_deserializer_replays_scalar_alias_mapping_keys() {
        let input = "root: {anchor: &svc service, ? *svc : api}\n";
        let parsed = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
            input,
            LoadOptions::new(),
        )
        .expect("event-backed scalar alias mapping key replay");

        assert_eq!(parsed["root"]["anchor"], "service");
        assert_eq!(parsed["root"]["service"], "api");
    }

    #[test]
    fn event_deserializer_rejects_duplicate_alias_mapping_keys() {
        let input = "root: {? &name name : api, ? *name : worker}\n";
        let error = from_str_with_options::<BTreeMap<String, BTreeMap<String, String>>>(
            input,
            LoadOptions::new(),
        )
        .expect_err("event-backed alias-expanded duplicate keys reject");
        assert!(error.to_string().contains("duplicate mapping key"));
    }

    #[test]
    fn event_deserializer_rejects_alias_replay_over_budget() {
        let input = "base: &base api\nservice: *base\n";
        let error = from_str_with_options::<BTreeMap<String, String>>(
            input,
            LoadOptions::new().max_alias_expansion_nodes(0),
        )
        .expect_err("event-backed alias replay budget rejects");

        assert!(
            error
                .to_string()
                .contains("alias event replay limit exceeded")
        );
    }

    #[test]
    fn event_deserializer_rejects_duplicate_keys_in_ignored_mappings() {
        let input = "base: &base {a: one, a: two}\ntarget: *base\n";
        let error = from_str_with_options::<TargetMap>(input, LoadOptions::new())
            .expect_err("ignored anchor source duplicate keys reject");

        assert!(error.to_string().contains("duplicate mapping key"));
    }

    #[test]
    fn event_deserializer_reads_multiple_documents() {
        let input = "---\nname: api\nports: [80]\nenabled: true\nlabels: {}\noptional: null\n---\nname: worker\nports: [8080]\nenabled: false\nlabels:\n  tier: job\noptional: note\n";
        let parsed: Vec<OwnedEventConfig> =
            from_documents_str_with_options(input, LoadOptions::new())
                .expect("event-backed document stream");

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "api");
        assert_eq!(parsed[0].ports, vec![80]);
        assert_eq!(parsed[1].name, "worker");
        assert_eq!(parsed[1].ports, vec![8080]);
        assert_eq!(parsed[1].labels["tier"], "job");
        assert_eq!(parsed[1].optional.as_deref(), Some("note"));
    }

    #[test]
    fn event_document_iterator_yields_borrowed_typed_documents() {
        let input = "---\nname: api\nports: [80]\nenabled: true\nlabels: {}\noptional: null\n---\nname: worker\nports: [8080]\nenabled: false\nlabels: {}\noptional: null\n";
        let mut iter = document_iter_str_with_options::<EventConfig<'_>>(input, LoadOptions::new())
            .expect("event-backed document iterator");

        let first = iter.next().expect("first document").expect("first parses");
        assert_eq!(first.name, "api");
        assert!(std::ptr::eq(first.name.as_ptr(), input[10..13].as_ptr()));

        let second = iter
            .next()
            .expect("second document")
            .expect("second parses");
        assert_eq!(second.name, "worker");
        let worker_offset = input.find("worker").expect("worker text in input");
        assert!(std::ptr::eq(
            second.name.as_ptr(),
            input[worker_offset..worker_offset + "worker".len()].as_ptr()
        ));
        assert!(iter.next().is_none());
    }

    #[test]
    fn event_document_iterator_continues_after_typed_document_error() {
        let input = "\
---
name: api
ports: [80]
enabled: true
labels: {}
optional: null
---
name: bad
ports: [70000]
enabled: true
labels: {}
optional: null
---
name: worker
ports: [8080]
enabled: false
labels: {}
optional: null
";
        let mut iter =
            document_iter_str_with_options::<OwnedEventConfig>(input, LoadOptions::new())
                .expect("event-backed document iterator");

        let first = iter.next().expect("first document").expect("first parses");
        assert_eq!(first.name, "api");

        let error = iter
            .next()
            .expect("second document")
            .expect_err("second document has typed range error");
        assert_eq!(error.document_index(), Some(1));
        assert!(error.to_string().contains("70000"), "{error}");

        let third = iter.next().expect("third document").expect("third parses");
        assert_eq!(third.name, "worker");
        assert!(iter.next().is_none());
    }

    #[test]
    fn event_document_iterator_defers_later_parse_error_and_then_stops() {
        let input = "---\nname: one\n---\n:\tbad\n---\nname: never\n";
        let mut iter = document_iter_str_with_options::<KnownOnly>(input, LoadOptions::new())
            .expect("event-backed document iterator");

        let first = iter.next().expect("first document").expect("first parses");
        assert_eq!(first.name, "one");

        let error = iter
            .next()
            .expect("second document item")
            .expect_err("later parser error");
        assert_eq!(error.document_index(), Some(1));
        assert_eq!(error.line(), Some(4));
        assert_eq!(error.column(), Some(2));
        assert!(iter.next().is_none());
    }

    #[test]
    fn event_document_iterator_empty_stream_yields_no_documents() {
        let mut iter = document_iter_str_with_options::<crate::Value>("", LoadOptions::new())
            .expect("empty event-backed document iterator");

        assert!(iter.next().is_none());
        let collected = from_documents_str_with_options::<crate::Value>("", LoadOptions::new())
            .expect("empty document collection");
        assert!(collected.is_empty());
    }

    #[test]
    fn event_document_iterator_slice_checks_utf8_and_input_limits() {
        let invalid = match document_iter_slice_with_options::<crate::Value>(
            b"name: \xFF\n",
            LoadOptions::new(),
        ) {
            Ok(_) => panic!("invalid UTF-8 should fail"),
            Err(error) => error,
        };
        assert!(invalid.to_string().contains("input is not valid UTF-8"));

        let limited = match document_iter_slice_with_options::<crate::Value>(
            b"name: app\n",
            LoadOptions::new().max_input_bytes(4),
        ) {
            Ok(_) => panic!("input limit should fail"),
            Err(error) => error,
        };
        assert!(
            limited
                .to_string()
                .contains("YAML input exceeds configured limit of 4 bytes")
        );
    }

    #[test]
    fn event_document_reader_iterator_uses_owned_input_and_preserves_merge_alias_semantics() {
        let input = "\
---
base: &base {a: one}
target: {<<: *base, b: two}
---
base: &base {a: three}
target: *base
";
        let docs = document_iter_reader_with_options::<TargetMap, _>(
            Cursor::new(input.as_bytes()),
            LoadOptions::new(),
        )
        .expect("reader-backed event iterator")
        .collect::<Result<Vec<_>>>()
        .expect("reader-backed documents");

        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].target["a"], "one");
        assert_eq!(docs[0].target["b"], "two");
        assert_eq!(docs[1].target["a"], "three");
    }

    #[test]
    fn event_document_reader_iterator_reports_read_errors_before_iteration() {
        let error = match document_iter_reader_with_options::<OwnedEventConfig, _>(
            FailingAfterPrefixReader::new(b"name: api\n"),
            LoadOptions::new(),
        ) {
            Ok(_) => panic!("reader failure should reject iterator construction"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("failed to read YAML input"));
        assert_eq!(error.location(), None);
    }

    #[test]
    fn event_deserializer_document_errors_carry_document_index() {
        let input = "---\nname: api\nports: [80]\nenabled: true\nlabels: {}\noptional: null\n---\nname: worker\nports: [70000]\nenabled: true\nlabels: {}\noptional: null\n";
        let error = from_documents_str_with_options::<OwnedEventConfig>(input, LoadOptions::new())
            .expect_err("event-backed stream reports second document error");
        assert_eq!(error.document_index(), Some(1));
    }

    #[test]
    fn event_deserializer_skips_ignored_any_without_materializing_values() {
        let input = "root:\n  - name: api\n    ports: [80, 443]\n  - nested:\n      ok: true\n";
        IgnoredAny::deserialize(EventNodeDeserializer {
            source: &mut EventSource::new(
                input,
                crate::parse::EventStream::from_str(input)
                    .expect("event stream")
                    .collect::<Result<Vec<_>>>()
                    .expect("events"),
                Schema::Yaml12,
                LoadOptions::new().alias_expansion_budget(input.len()),
                LoadOptions::new().selected_max_nesting_depth(),
            ),
        })
        .expect_err("raw stream markers must still be explicit");

        from_str_with_options::<IgnoredAny>(input, LoadOptions::new()).expect("ignored any");
    }

    fn alias_depth_chain(levels: usize) -> String {
        // A literally shallow document (max nesting depth 2) whose final anchor
        // expands, via the alias chain, to a structure `levels` deep.
        let mut input = String::from("- &n0 0\n");
        for k in 1..levels {
            input.push_str(&format!("- &n{k} [*n{prev}]\n", prev = k - 1));
        }
        input
    }

    #[test]
    fn event_deserializer_bounds_alias_expansion_depth() {
        // The event-backed path expands aliases lazily while walking, so the
        // parser's literal-depth check does not bound the expanded depth. Without
        // an explicit ceiling this recurses until the stack overflows; it must
        // instead reject, matching the tree-backed `AnchorTable::resolve` guard.
        let input = alias_depth_chain(400);
        let error = from_str_with_options::<Vec<crate::Value>>(&input, LoadOptions::new())
            .expect_err("deep alias chain must hit the nesting-depth ceiling");
        assert!(
            error.to_string().contains("nesting depth"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn event_deserializer_allows_alias_chain_within_depth_limit() {
        let input = alias_depth_chain(8);
        let parsed = from_str_with_options::<Vec<crate::Value>>(&input, LoadOptions::new())
            .expect("alias chain within the depth limit deserializes");
        assert_eq!(parsed.len(), 8);
    }

    #[test]
    fn event_deserializer_reads_map_form_enum_variants() {
        // Externally-tagged enum variants carrying a payload — the forms the
        // earlier scalar-only path rejected. Covers unit, newtype, tuple, and
        // struct variants in one sequence.
        #[derive(Debug, Deserialize, PartialEq)]
        enum EventEnum {
            Unit,
            Newtype(u32),
            Tuple(u8, u8),
            Struct { width: u32, height: u32 },
        }

        let input = "\
- Unit
- Newtype: 7
- Tuple: [1, 2]
- Struct:
    width: 3
    height: 4
";
        let parsed: Vec<EventEnum> =
            from_str_with_options(input, LoadOptions::new()).expect("event-backed enum variants");
        assert_eq!(
            parsed,
            vec![
                EventEnum::Unit,
                EventEnum::Newtype(7),
                EventEnum::Tuple(1, 2),
                EventEnum::Struct {
                    width: 3,
                    height: 4,
                },
            ]
        );
    }

    #[test]
    fn event_deserializer_reads_map_form_enum_variant_through_alias() {
        #[derive(Debug, Deserialize, PartialEq)]
        enum Mode {
            Tuned { level: u8 },
        }

        // The anchored definition and the alias must both resolve to the same
        // map-form variant.
        let parsed = from_str_with_options::<Vec<Mode>>(
            "- &m {Tuned: {level: 9}}\n- *m\n",
            LoadOptions::new(),
        )
        .expect("aliased map-form enum variant");
        assert_eq!(
            parsed,
            vec![Mode::Tuned { level: 9 }, Mode::Tuned { level: 9 }]
        );
    }
}
